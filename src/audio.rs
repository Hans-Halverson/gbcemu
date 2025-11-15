use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
};

use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use serde::{Deserialize, Serialize};

use crate::emulator::{REFRESH_RATE, Register, TICKS_PER_FRAME};

/// Rate to sample audio during playback, in Hz
const SAMPLE_RATE: u32 = 44100;

pub const TICKS_PER_SAMPLE: f64 = (TICKS_PER_FRAME as f64 * REFRESH_RATE) / (SAMPLE_RATE as f64);

/// A generic audio output device which can be attached to an emulator
pub trait AudioOutput {
    fn send_frame(&self, samples: VecDeque<TimedSample>);
}

fn shared_audio_channel() -> (SharedAudioSender, SharedAudioReceiver) {
    let (send, recv) = mpsc::channel();
    (
        SharedAudioSender {
            next_frame_send: send,
        },
        SharedAudioReceiver {
            next_frame_recv: recv,
        },
    )
}

pub struct SharedAudioSender {
    /// Batch of samples for the next frame
    next_frame_send: Sender<VecDeque<TimedSample>>,
}

impl SharedAudioSender {
    fn send_frame(&self, samples: VecDeque<TimedSample>) {
        self.next_frame_send.send(samples).unwrap();
    }
}

pub struct SharedAudioReceiver {
    /// Batch of samples available for the next frame
    next_frame_recv: Receiver<VecDeque<TimedSample>>,
}

impl SharedAudioReceiver {
    fn try_next_frame(&self) -> Option<VecDeque<TimedSample>> {
        self.next_frame_recv.try_recv().ok()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TimedSample {
    /// Amplitude of the left channel
    pub left: f32,
    /// Amplitude of the right channel
    pub right: f32,
    /// Tick within the frame for this sample
    pub tick: u32,
}

struct BufferedSource {
    /// Whether the next sample is for the left channel (true) or right channel (false)
    is_next_sample_left: bool,

    /// The current tick in this frame. Fractional since sample rate does not align perfectly with
    /// ticks.
    current_tick: f64,

    /// The current sample
    current_sample: TimedSample,

    /// Samples for the current frame
    current_frame_samples: VecDeque<TimedSample>,

    /// Index of the next sample to read
    next_sample_index: usize,

    /// Receiver for batches of samples for each frame
    receiver: SharedAudioReceiver,
}

impl BufferedSource {
    fn new(receiver: SharedAudioReceiver) -> Self {
        Self {
            is_next_sample_left: true,
            current_tick: 0.0,
            current_sample: TimedSample {
                left: 0.0,
                right: 0.0,
                tick: 0,
            },
            current_frame_samples: VecDeque::new(),
            next_sample_index: 0,
            receiver,
        }
    }
}

impl Source for BufferedSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        2
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Iterator for BufferedSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        // First check if we should skip to the next frame. Make sure not to skip if between left
        // and right samples.
        if self.is_next_sample_left && self.current_tick >= (TICKS_PER_FRAME as f64 - 0.1) {
            if let Some(next_frame_samples) = self.receiver.try_next_frame() {
                self.current_frame_samples = next_frame_samples;
                self.current_tick = 0.0;
                self.next_sample_index = 0;
            } else {
                // No new frame available so loop the last full frame
                self.current_tick = 0.0;
                self.next_sample_index = 0;
            }
        }

        // Find the next sample for the current tick. Remain at the last sample if we reach the end
        // end of the sample buffer.
        while let Some(sample) = self.current_frame_samples.get(self.next_sample_index)
            && (sample.tick as f64) <= self.current_tick
        {
            self.current_sample = *sample;
            self.next_sample_index += 1;
        }

        // Only increment tick after both left and right samples have been read
        if !self.is_next_sample_left {
            self.current_tick += TICKS_PER_SAMPLE;
        }

        // Return the sample for the appropriate channel, interleaving channels
        self.is_next_sample_left = !self.is_next_sample_left;

        if self.is_next_sample_left {
            Some(self.current_sample.left)
        } else {
            Some(self.current_sample.right)
        }
    }
}

pub struct DefaultSystemAudioOutput {
    _output_stream: OutputStream,
    _sink: Sink,
    sender: SharedAudioSender,
}

impl DefaultSystemAudioOutput {
    pub fn new() -> Self {
        let (sender, receiver) = shared_audio_channel();

        let output_stream = OutputStreamBuilder::open_default_stream().unwrap();

        let sink = Sink::connect_new(&output_stream.mixer());
        sink.append(BufferedSource::new(receiver));

        Self {
            _output_stream: output_stream,
            _sink: sink,
            sender,
        }
    }
}

impl AudioOutput for DefaultSystemAudioOutput {
    fn send_frame(&self, samples: VecDeque<TimedSample>) {
        self.sender.send_frame(samples);
    }
}

/// Map digital 0x0-0xF to analog 1.0 to -1.0
fn digital_to_analog(digit: u8) -> f32 {
    1.0 - ((digit as f32) / 7.5)
}

/// Audio processing unit
#[derive(Serialize, Deserialize)]
pub struct Apu {
    channel_1: PulseChannel,
    channel_2: PulseChannel,
    div_apu: u8,

    /// Volume for left output channel. Part of NR51 register.
    left_volume: u8,

    /// Volume for right output channel. Part of NR51 register.
    right_volume: u8,

    /// Full NR51 register value. Each bit controls whether left/right output uses each channel.
    nr51: Register,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            channel_1: PulseChannel::new(),
            channel_2: PulseChannel::new(),
            div_apu: 0,
            left_volume: 0,
            right_volume: 0,
            nr51: 0,
        }
    }

    pub fn channel_1_mut(&mut self) -> &mut PulseChannel {
        &mut self.channel_1
    }

    pub fn channel_2_mut(&mut self) -> &mut PulseChannel {
        &mut self.channel_2
    }

    pub fn advance_div_apu(&mut self) {
        let old_div_apu = self.div_apu;
        self.div_apu = self.div_apu.wrapping_add(1);

        let falling_edges = old_div_apu & !self.div_apu;

        if (falling_edges & 0x1) != 0 {
            self.advance_length_timers();
        }

        if (falling_edges & 0x4) != 0 {
            self.channel_1.advance_envelope_timer();
            self.channel_2.advance_envelope_timer();
        }
    }

    pub fn run_m_tick(&mut self) {
        self.channel_1.run_m_tick();
        self.channel_2.run_m_tick();
    }

    pub fn advance_length_timers(&mut self) {
        self.channel_1.advance_length_timer();
        self.channel_2.advance_length_timer();
    }

    pub fn write_nr50(&mut self, value: Register) {
        self.left_volume = (value & 0x70) >> 4;
        self.right_volume = value & 0x7;
    }

    pub fn write_nr51(&mut self, value: Register) {
        self.nr51 = value;
    }

    pub fn sample_audio(&self) -> (f32, f32) {
        let mut mixed_left = 0.0;
        let mut mixed_right = 0.0;

        let nr51 = self.nr51;

        // Mix in channel 1
        if nr51 & 0x11 != 0 {
            let sample = self.channel_1.sample_analog();
            if nr51 & 0x01 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x10 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 2
        if nr51 & 0x22 != 0 {
            let sample = self.channel_2.sample_analog();

            if nr51 & 0x02 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x20 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 3
        if nr51 & 0x44 != 0 {
            let sample = self.sample_channel_3_analog();

            if nr51 & 0x04 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x40 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 4
        if nr51 & 0x88 != 0 {
            let sample = self.sample_channel_4_analog();
            if nr51 & 0x08 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x80 != 0 {
                mixed_left += sample;
            }
        }

        // Evenly mix channels
        mixed_left /= 4.0;
        mixed_right /= 4.0;

        // Scale by channel volumes, 0 == volume 1, 7 == volume 8
        let final_left = mixed_left * ((self.left_volume as f32 + 1.0) / 8.0);
        let final_right = mixed_right * ((self.right_volume as f32 + 1.0) / 8.0);

        (final_left, final_right)
    }

    fn sample_channel_3_analog(&self) -> f32 {
        0.0
    }

    fn sample_channel_4_analog(&self) -> f32 {
        0.0
    }
}

const DUTY_WAVEFORM_LENGTH: usize = 8;

const DUTY_CYCLE_WAVEFORMS: [[u8; DUTY_WAVEFORM_LENGTH]; 4] = [
    // 12.5% duty cycle
    [1, 1, 1, 1, 1, 1, 1, 0],
    // 25% duty cycle
    [0, 1, 1, 1, 1, 1, 1, 0],
    // 50% duty cycle
    [0, 1, 1, 1, 1, 0, 0, 0],
    // 75% duty cycle
    [1, 0, 0, 0, 0, 0, 0, 1],
];

#[derive(Serialize, Deserialize)]
pub struct PulseChannel {
    /// Duty cycle index [0, 4)
    duty_cycle: u8,

    //// Index into duty sample waveform [0-8)
    duty_sample_index: u8,

    /// A counter down to 0, at which point the period ends and a sample is taken
    period_counter: u16,

    /// Raw value of the full period register (11 bits)
    period_register: u16,

    /// Whether the length timer is enabled
    is_length_timer_enabled: bool,

    /// Value the length timer is reset to
    initial_length_timer: u8,

    /// A counter down to 0 at which point the channel is disabled
    length_timer: u8,

    /// Whether the envelope is incrementing or decrementing
    is_envelope_up: bool,

    /// Value to reset the envelope timer to, 3 bits
    envelope_sweep_pace: u8,

    /// A counter down to 0, at which point the volume is updated due to the envelope
    envelope_timer: u8,

    /// Value of the initial volume register
    initial_volume: u8,

    /// Current digital volume
    volume: u8,

    /// Whether the channel is enabled, disabled channels produce silence
    enabled: bool,
}

impl PulseChannel {
    fn new() -> Self {
        Self {
            duty_cycle: 0,
            duty_sample_index: 0,
            period_counter: 0,
            period_register: 0,
            is_length_timer_enabled: false,
            initial_length_timer: 0,
            length_timer: 0,
            initial_volume: 0,
            is_envelope_up: false,
            envelope_sweep_pace: 0,
            envelope_timer: 0,
            volume: 0,
            enabled: false,
        }
    }

    pub fn write_nrx1(&mut self, value: Register) {
        // Upper two bits of NRX1
        self.duty_cycle = (value & 0xC0) >> 6;

        // Lower six bits of NRX1
        self.initial_length_timer = 64 - (value & 0x3F);
    }

    pub fn write_nrx2(&mut self, value: Register) {
        // Upper four bits of NRX2
        self.initial_volume = (value & 0xF0) >> 4;

        // Bit three of NRX2
        self.is_envelope_up = (value & 0x08) != 0;

        // Lower three bits of NRX2
        self.envelope_sweep_pace = value & 0x07;

        // If the envelope's initial volume is 0 and envelope is decreasing, disable the channel
        if self.initial_volume == 0 && !self.is_envelope_up {
            self.enabled = false;
        }
    }

    pub fn write_nrx3(&mut self, value: Register) {
        // All of NRX3 is lower bits of period register
        self.period_register = (self.period_register & 0x0700) | (value as u16);
    }

    pub fn write_nrx4(&mut self, value: Register) {
        // Lower three bits of NRX4 are upper bits of period register
        self.period_register = (self.period_register & 0x00FF) | (((value as u16) & 0x7) << 8);

        // Bit 6 of NRX4
        self.is_length_timer_enabled = value & 0x40 != 0;

        if self.is_length_timer_enabled {
            self.length_timer = self.initial_length_timer;
        }

        // Bit 7 of NRX4
        let is_triggered = value & 0x80 != 0;
        if is_triggered {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        self.enabled = true;
        self.period_counter = self.initial_period_counter();
        self.volume = self.initial_volume;

        if self.length_timer == 0 {
            self.length_timer = self.initial_length_timer;
        }

        if self.envelope_sweep_pace != 0 {
            self.envelope_timer = self.envelope_sweep_pace;
        }
    }

    fn initial_period_counter(&self) -> u16 {
        2048 - self.period_register
    }

    fn sample_digital(&self) -> u8 {
        let duty_waveform = DUTY_CYCLE_WAVEFORMS[self.duty_cycle as usize];
        let duty_waveform_sample = duty_waveform[self.duty_sample_index as usize];

        duty_waveform_sample * self.volume
    }

    fn sample_analog(&self) -> f32 {
        if !self.enabled {
            return 0.0;
        }

        digital_to_analog(self.sample_digital())
    }

    fn run_m_tick(&mut self) {
        // Subtracting would overflow so period is over
        if self.period_counter == 0 {
            // Advance to next duty sample
            self.duty_sample_index = (self.duty_sample_index + 1) % DUTY_WAVEFORM_LENGTH as u8;

            // Reload period counter
            self.period_counter = self.initial_period_counter();
        }

        self.period_counter -= 1;
    }

    fn advance_length_timer(&mut self) {
        if self.is_length_timer_enabled && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.enabled = false;
            }
        }
    }

    fn advance_envelope_timer(&mut self) {
        if self.envelope_sweep_pace == 0 {
            return;
        }

        if self.envelope_timer > 0 {
            self.envelope_timer -= 1;
        }

        if self.envelope_timer == 0 {
            // Reset envelope timer
            self.envelope_timer = self.envelope_sweep_pace;

            // Update volume due to envelope, clamping to [0x0, 0xF]
            if self.is_envelope_up && self.volume < 0xF {
                self.volume += 1;
            } else if !self.is_envelope_up && self.volume > 0x0 {
                self.volume -= 1;
            }
        }
    }
}
