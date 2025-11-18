use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
};

use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use serde::{Deserialize, Serialize};

use crate::{
    address_space::{WAVE_RAM_SIZE, WAVE_RAM_START},
    emulator::{REFRESH_RATE, Register, TICKS_PER_FRAME},
};

/// Rate to sample audio during playback, in Hz
const SAMPLE_RATE: u32 = 44100;

pub const NUM_AUDIO_CHANNELS: u8 = 4;

pub const TICKS_PER_SAMPLE: f64 = (TICKS_PER_FRAME as f64 * REFRESH_RATE) / (SAMPLE_RATE as f64);

const SYSTEM_VOLUME_LEVELS: [f32; 8] = [0.0, 0.0625, 0.125, 0.25, 0.375, 0.5, 0.65, 1.0];
const DEFAULT_SYSTEM_VOLUME_INDEX: usize = 5;

/// Recharge rate for the high pass filter's capacitor when sampling at 44100 Hz
const HPF_RECHARGE_RATE: f32 = 0.996;

/// A generic audio output device which can be attached to an emulator
pub trait AudioOutput {
    fn send_frame(&self, samples: AudioFrame);
    fn set_paused_state(&self, is_paused: bool);
}

enum AudioMessage {
    /// The next audio frame
    FrameSamples(AudioFrame),
    /// Whether audio should be paused
    PausedState(bool),
}

/// A collection of audio samples corresponding to a single (graphical) frame
pub type AudioFrame = Vec<TimedSample>;

fn shared_audio_channel() -> (SharedAudioSender, SharedAudioReceiver) {
    let (send, recv) = mpsc::channel();
    (SharedAudioSender { send }, SharedAudioReceiver { recv })
}

pub struct SharedAudioSender {
    send: Sender<AudioMessage>,
}

impl SharedAudioSender {
    fn send_frame(&self, samples: AudioFrame) {
        self.send.send(AudioMessage::FrameSamples(samples)).unwrap();
    }

    fn set_paused_state(&self, is_paused: bool) {
        self.send
            .send(AudioMessage::PausedState(is_paused))
            .unwrap();
    }
}

pub struct SharedAudioReceiver {
    recv: Receiver<AudioMessage>,
}

impl SharedAudioReceiver {
    fn try_next_message(&self) -> Option<AudioMessage> {
        self.recv.try_recv().ok()
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

/// We target keeping the buffer full with two frames of audio. This introduces a frame of audio
/// latency but reduces the chance of not having audio ready when requested.
const TARGET_BUFFERED_FRAMES: u64 = 2;

struct BufferedSource {
    /// Whether the next sample is for the left channel (true) or right channel (false)
    is_next_sample_left: bool,

    /// The current tick in this frame. Fractional since sample rate does not align perfectly with
    /// ticks.
    current_tick: f64,

    /// The current sample
    current_sample: TimedSample,

    /// Buffer of completed frames ready for playback
    frame_buffer: VecDeque<AudioFrame>,

    /// Queue of pending frames that have been received but not yet processed
    pending_frames: VecDeque<AudioFrame>,

    /// Index of the next sample to read
    next_sample_index: usize,

    /// Receiver for batches of samples for each frame
    receiver: SharedAudioReceiver,

    /// Whether the audio stream is currently paused
    is_paused: bool,

    /// The frame number for the frame currently being played
    frame_number: u64,
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
            frame_buffer: VecDeque::new(),
            pending_frames: VecDeque::new(),
            next_sample_index: 0,
            receiver,
            is_paused: false,
            frame_number: 0,
        }
    }

    fn handle_messages(&mut self) {
        while let Some(message) = self.receiver.try_next_message() {
            match message {
                AudioMessage::FrameSamples(frame_samples) => {
                    self.pending_frames.push_back(frame_samples)
                }
                AudioMessage::PausedState(is_paused) => self.is_paused = is_paused,
            }
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
        self.handle_messages();

        // Silence when paused
        if self.is_paused {
            return Some(0.0);
        }

        // Check if frame is complete. Make sure not to skip if between left and right samples.
        if self.is_next_sample_left && self.current_tick >= (TICKS_PER_FRAME as f64 - 0.1) {
            // Frame is done, start a new frame
            self.frame_number += 1;
            self.current_tick = 0.0;
            self.next_sample_index = 0;

            // Move on to the next frame of samples if one exists. Otherwise loop current frame.
            if self.frame_buffer.len() > 1 || self.pending_frames.len() > 0 {
                self.frame_buffer.pop_front();
            }

            if self.frame_number >= TARGET_BUFFERED_FRAMES {
                // Fill buffer up to target number of frames
                for _ in self.frame_buffer.len()..(TARGET_BUFFERED_FRAMES as usize) {
                    if let Some(frame) = self.pending_frames.pop_front() {
                        self.frame_buffer.push_back(frame);
                    }
                }

                // Combine all pending frames into a single frame if there are multiple.
                if self.pending_frames.len() > 1 {
                    let merged_frame = merge_into_single_frame(&self.pending_frames);

                    self.pending_frames.clear();
                    self.pending_frames.push_back(merged_frame);
                }
            }
        }

        // Find the next sample for the current tick. Remain at the last sample if we reach the end
        // end of the sample buffer.
        while let Some(sample) = self
            .frame_buffer
            .get(0)
            .and_then(|f| f.get(self.next_sample_index))
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

fn merge_into_single_frame(frames: &VecDeque<AudioFrame>) -> AudioFrame {
    // Round up integer division to ensure we fill the entire new frame
    let frame_length = frames[0].len();
    let samples_per_frame = (frame_length + frames.len() - 1) / frames.len();

    let mut new_frame = Vec::with_capacity(frame_length);

    // Choose samples evenly from each frame to fill the new frame
    for i in 0..frame_length {
        let frame_index = i / samples_per_frame;
        let frame = &frames[frame_index];
        let sample_index = ((i % samples_per_frame) * frames.len()).min(frame.len() - 1);
        new_frame.push(frame[sample_index]);
    }

    new_frame
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
    fn send_frame(&self, samples: AudioFrame) {
        self.sender.send_frame(samples);
    }

    fn set_paused_state(&self, is_paused: bool) {
        self.sender.set_paused_state(is_paused);
    }
}

/// Map digital 0x0-0xF to analog 1.0 to -1.0
fn digital_to_analog(digit: u8) -> f32 {
    1.0 - ((digit as f32) / 7.5)
}

#[derive(Serialize, Deserialize)]
struct HighPassFilter {
    /// Charge of the capacitor
    charge: f32,
}

impl HighPassFilter {
    fn new() -> Self {
        Self { charge: 0.0 }
    }

    fn apply(&mut self, input_sample: f32) -> f32 {
        let output_sample = input_sample - self.charge;
        self.charge = input_sample - output_sample * HPF_RECHARGE_RATE;

        output_sample
    }
}

/// Audio processing unit
#[derive(Serialize, Deserialize)]
pub struct Apu {
    /// Channel 1: Pulse wave with sweep
    channel_1: PulseChannel,

    /// Channel 2: Pulse wave without sweep
    channel_2: PulseChannel,

    /// Channel 3: Custom wave channel
    channel_3: WaveChannel,

    /// Channel 4: Noise channel
    channel_4: NoiseChannel,

    /// The internal DIV-APU counter, incremented at 512 Hz
    div_apu: u8,

    /// Volume for left output channel. Part of NR51 register.
    left_volume: u8,

    /// Volume for right output channel. Part of NR51 register.
    right_volume: u8,

    /// Full NR51 register value. Each bit controls whether left/right output uses each channel.
    nr51: Register,

    /// Capacitor used in the left channel's high-pass filter
    hpf_left: HighPassFilter,

    /// Capacitor used in the right channel's high-pass filter
    hpf_right: HighPassFilter,

    /// Whether the APU is currently powered on
    is_on: bool,

    /// Volume for the entire system output. Is an index into the fixed SYSTEM_VOLUME_LEVELS array.
    ///
    /// Corresponds to volume knob on original hardware.
    system_volume_index: usize,

    /// Whether the APU is currently muted
    is_muted: bool,

    /// Debug flags to disable each channel's output
    debug_disable_channel_1: bool,
    debug_disable_channel_2: bool,
    debug_disable_channel_3: bool,
    debug_disable_channel_4: bool,

    /// Debug flag to disable the high-pass filter
    debug_disable_hpf: bool,
}

impl Apu {
    pub fn new() -> Self {
        Self {
            channel_1: PulseChannel::new(/* has_sweep */ true),
            channel_2: PulseChannel::new(/* has_sweep */ false),
            channel_3: WaveChannel::new(),
            channel_4: NoiseChannel::new(),
            div_apu: 0,
            left_volume: 0,
            right_volume: 0,
            nr51: 0,
            hpf_left: HighPassFilter::new(),
            hpf_right: HighPassFilter::new(),
            is_on: true,
            system_volume_index: DEFAULT_SYSTEM_VOLUME_INDEX,
            is_muted: false,
            debug_disable_channel_1: false,
            debug_disable_channel_2: false,
            debug_disable_channel_3: false,
            debug_disable_channel_4: false,
            debug_disable_hpf: false,
        }
    }

    pub fn channel_1_mut(&mut self) -> &mut PulseChannel {
        &mut self.channel_1
    }

    pub fn channel_2_mut(&mut self) -> &mut PulseChannel {
        &mut self.channel_2
    }

    pub fn channel_3_mut(&mut self) -> &mut WaveChannel {
        &mut self.channel_3
    }

    pub fn channel_4_mut(&mut self) -> &mut NoiseChannel {
        &mut self.channel_4
    }

    pub fn is_on(&self) -> bool {
        self.is_on
    }

    pub fn increase_system_volume(&mut self) {
        self.system_volume_index =
            (self.system_volume_index + 1).min(SYSTEM_VOLUME_LEVELS.len() - 1);
    }

    pub fn decrease_system_volume(&mut self) {
        self.system_volume_index = self.system_volume_index.saturating_sub(1);
    }

    pub fn toggle_muted(&mut self) {
        self.is_muted = !self.is_muted;
    }

    pub fn toggle_channel(&mut self, channel: usize) {
        match channel {
            1 => self.debug_disable_channel_1 = !self.debug_disable_channel_1,
            2 => self.debug_disable_channel_2 = !self.debug_disable_channel_2,
            3 => self.debug_disable_channel_3 = !self.debug_disable_channel_3,
            4 => self.debug_disable_channel_4 = !self.debug_disable_channel_4,
            _ => {}
        }
    }

    pub fn toggle_hpf(&mut self) {
        self.debug_disable_hpf = !self.debug_disable_hpf;
    }

    pub fn advance_div_apu(&mut self) {
        let old_div_apu = self.div_apu;
        self.div_apu = self.div_apu.wrapping_add(1);

        let falling_edges = old_div_apu & !self.div_apu;

        if (falling_edges & 0x1) != 0 {
            self.channel_1.advance_length_timer();
            self.channel_2.advance_length_timer();
            self.channel_3.advance_length_timer();
            self.channel_4.advance_length_timer();
        }

        if (falling_edges & 0x2) != 0 {
            self.channel_1.advance_sweep_timer();
        }

        if (falling_edges & 0x4) != 0 {
            self.channel_1.advance_envelope_timer();
            self.channel_2.advance_envelope_timer();
            self.channel_4.advance_envelope_timer();
        }
    }

    pub fn advance_period_timers(&mut self, tick_number: u32) {
        // Pulse channels advance period every 4 ticks
        if tick_number % 4 == 0 {
            self.channel_1.advance_period_timer();
            self.channel_2.advance_period_timer();
        }

        // Wave channel advance period every 2 ticks
        if tick_number % 2 == 0 {
            self.channel_3.advance_period_timer();
        }

        // Noise channel advance period every 8 ticks, as this is the smallest possible period
        // (when clock divider and shift are both 0).
        if tick_number % 8 == 0 {
            self.channel_4.advance_period_timer();
        }
    }

    pub fn write_nr50(&mut self, value: Register) {
        self.left_volume = (value & 0x70) >> 4;
        self.right_volume = value & 0x7;
    }

    pub fn write_nr51(&mut self, value: Register) {
        self.nr51 = value;
    }

    pub fn read_nr52(&self) -> Register {
        let mut value = 0x70; // Unused bits always read as 1

        if self.channel_1.is_enabled {
            value |= 0x01;
        }

        if self.channel_2.is_enabled {
            value |= 0x02;
        }

        if self.channel_3.is_enabled {
            value |= 0x04;
        }

        if self.channel_4.is_enabled {
            value |= 0x08;
        }

        if self.is_on {
            value |= 0x80;
        }

        value
    }

    pub fn write_nr52(&mut self, value: Register) {
        let was_on = self.is_on;

        // Top bit controls whether APU is on
        self.is_on = (value & 0x80) != 0;

        // If APU was disabled then each individual channel should be disabled
        if was_on && !self.is_on {
            self.channel_1.is_enabled = false;
            self.channel_2.is_enabled = false;
            self.channel_3.is_enabled = false;
            self.channel_4.is_enabled = false;
        }
    }

    pub fn sample_audio(&self) -> (f32, f32) {
        // When muted immediately return silence without sampling
        if self.is_muted {
            return (0.0, 0.0);
        }

        let mut mixed_left = 0.0;
        let mut mixed_right = 0.0;

        let nr51 = self.nr51;

        // Mix in channel 1
        if nr51 & 0x11 != 0 && !self.debug_disable_channel_1 {
            let sample = self.channel_1.sample_analog();
            if nr51 & 0x01 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x10 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 2
        if nr51 & 0x22 != 0 && !self.debug_disable_channel_2 {
            let sample = self.channel_2.sample_analog();

            if nr51 & 0x02 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x20 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 3
        if nr51 & 0x44 != 0 && !self.debug_disable_channel_3 {
            let sample = self.channel_3.sample_analog();

            if nr51 & 0x04 != 0 {
                mixed_right += sample;
            }

            if nr51 & 0x40 != 0 {
                mixed_left += sample;
            }
        }

        // Mix in channel 4
        if nr51 & 0x88 != 0 && !self.debug_disable_channel_4 {
            let sample = self.channel_4.sample_analog();
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

        let system_volume = SYSTEM_VOLUME_LEVELS[self.system_volume_index];

        let final_left = mixed_left * system_volume * Self::channel_volume_analog(self.left_volume);
        let final_right =
            mixed_right * system_volume * Self::channel_volume_analog(self.right_volume);

        (final_left, final_right)
    }

    pub fn apply_hpf(&mut self, left_sample: f32, right_sample: f32) -> (f32, f32) {
        if self.debug_disable_hpf {
            return (left_sample, right_sample);
        }

        let filtered_left = self.hpf_left.apply(left_sample);
        let filtered_right = self.hpf_right.apply(right_sample);

        (filtered_left, filtered_right)
    }

    /// Channel volume 0 maps to volume 1, 7 maps to volume 8
    fn channel_volume_analog(channel_volume: u8) -> f32 {
        (channel_volume as f32 + 1.0) / 8.0
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
    period_timer: u16,

    /// Raw value of the full period register (11 bits)
    period_register: u16,

    /// Whether the length timer is enabled
    is_length_timer_enabled: bool,

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
    is_enabled: bool,

    /// Whether the DAC is enabled
    is_dac_enabled: bool,

    /// Whether this channel has sweep functionality
    has_sweep: bool,

    /// Whether the sweep is currently enabled
    is_sweep_enabled: bool,

    /// Whether the sweep is increasing or decreasing the period
    is_sweep_up: bool,

    /// Sweep step used when adjusting the period, 3 bits
    sweep_step: u8,

    /// Pace of the sweep, 3 bits
    sweep_pace: u8,

    /// A counter down to 0, at which point the period is adjusted by the sweep
    sweep_timer: u8,

    /// Shadow copy of the period register used for sweep calculations
    shadow_period_register: u16,
}

impl PulseChannel {
    fn new(has_sweep: bool) -> Self {
        Self {
            duty_cycle: 0,
            duty_sample_index: 0,
            period_timer: 0,
            period_register: 0,
            is_length_timer_enabled: false,
            length_timer: 0,
            initial_volume: 0,
            is_envelope_up: false,
            envelope_sweep_pace: 0,
            envelope_timer: 0,
            volume: 0,
            is_enabled: false,
            is_dac_enabled: false,
            has_sweep,
            is_sweep_enabled: false,
            is_sweep_up: false,
            sweep_step: 0,
            sweep_pace: 0,
            sweep_timer: 0,
            shadow_period_register: 0,
        }
    }

    const MAX_LENGTH_TIMER: u8 = 64;

    pub fn write_nrx0(&mut self, value: Register) {
        // Lower three bits are the sweep step
        self.sweep_step = value & 0x07;

        // Third bit marks the sweep the direction
        self.is_sweep_up = (value & 0x08) == 0;

        // Bits 4-6 are the sweep pace
        self.sweep_pace = (value & 0x70) >> 4;
    }

    pub fn write_nrx1(&mut self, value: Register) {
        // Upper two bits of NRX1
        self.duty_cycle = (value & 0xC0) >> 6;

        // Lower six bits of NRX1
        self.length_timer = Self::MAX_LENGTH_TIMER - (value & 0x3F);
    }

    pub fn write_nrx2(&mut self, value: Register) {
        // Upper four bits of NRX2
        self.initial_volume = (value & 0xF0) >> 4;

        // Bit three of NRX2
        self.is_envelope_up = (value & 0x08) != 0;

        // Lower three bits of NRX2
        self.envelope_sweep_pace = value & 0x07;

        // If the envelope's initial volume is 0 and envelope is decreasing, disable the channel
        self.is_dac_enabled = (self.initial_volume != 0) || self.is_envelope_up;
        if !self.is_dac_enabled {
            self.is_enabled = false;
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

        // Bit 7 of NRX4
        let is_triggered = value & 0x80 != 0;
        if is_triggered {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        // Channel can only be enabled if DAC is enabled
        if self.is_dac_enabled {
            self.is_enabled = true;
        }

        self.period_timer = self.initial_period_timer();
        self.volume = self.initial_volume;

        if self.length_timer == 0 {
            self.length_timer = Self::MAX_LENGTH_TIMER;
        }

        if self.envelope_sweep_pace != 0 {
            self.envelope_timer = self.envelope_sweep_pace;
        }

        if self.has_sweep {
            self.trigger_sweep_timer();
        }
    }

    fn initial_period_timer(&self) -> u16 {
        2048 - self.period_register
    }

    fn sample_digital(&self) -> u8 {
        if !self.is_enabled {
            return 0;
        }

        let duty_waveform = DUTY_CYCLE_WAVEFORMS[self.duty_cycle as usize];
        let duty_waveform_sample = duty_waveform[self.duty_sample_index as usize];

        duty_waveform_sample * self.volume
    }

    fn sample_analog(&self) -> f32 {
        if !self.is_dac_enabled {
            return 0.0;
        }

        digital_to_analog(self.sample_digital())
    }

    fn advance_period_timer(&mut self) {
        // Subtracting would overflow so period is over
        if self.period_timer == 0 {
            // Advance to next duty sample
            self.duty_sample_index = (self.duty_sample_index + 1) % DUTY_WAVEFORM_LENGTH as u8;

            // Reload period timer
            self.period_timer = self.initial_period_timer();
        }

        self.period_timer -= 1;
    }

    fn advance_length_timer(&mut self) {
        if self.is_length_timer_enabled && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.is_enabled = false;
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

    fn initial_sweep_timer(&self) -> u8 {
        // Pace of 0 is treated as 8
        if self.sweep_pace == 0 {
            8
        } else {
            self.sweep_pace
        }
    }

    fn trigger_sweep_timer(&mut self) {
        self.sweep_timer = self.initial_sweep_timer();
        self.shadow_period_register = self.period_register;
        self.is_sweep_enabled = self.sweep_pace != 0 || self.sweep_step != 0;

        // Overflow check is performed but only when sweep step is non-zero
        if self.sweep_step != 0 {
            let next_sweep_period = self.calculate_next_sweep_period();
            if Self::is_sweep_period_out_of_bounds(next_sweep_period) {
                self.is_enabled = false;
            }
        }
    }

    fn advance_sweep_timer(&mut self) {
        if !self.has_sweep || !self.is_sweep_enabled || self.sweep_pace == 0 {
            return;
        }

        if self.sweep_timer > 0 {
            self.sweep_timer -= 1;
        }

        if self.sweep_timer == 0 {
            // Reset sweep timer
            self.sweep_timer = self.initial_sweep_timer();

            // Calculate new period and perform overflow check
            let new_sweep_period = self.calculate_next_sweep_period();

            // Perform overflow check
            if Self::is_sweep_period_out_of_bounds(new_sweep_period) {
                self.is_enabled = false;
                return;
            }

            // Update period registers if no overflow
            self.shadow_period_register = new_sweep_period;
            self.period_register = new_sweep_period;

            // Must perform a second overflow check after the adjustment
            let second_sweep_period = self.calculate_next_sweep_period();
            if Self::is_sweep_period_out_of_bounds(second_sweep_period) {
                self.is_enabled = false;
            }
        }
    }

    fn calculate_next_sweep_period(&self) -> u16 {
        let sweep_adjustment = self.shadow_period_register >> self.sweep_step;
        if self.is_sweep_up {
            self.shadow_period_register + sweep_adjustment
        } else {
            self.shadow_period_register - sweep_adjustment
        }
    }

    fn is_sweep_period_out_of_bounds(period: u16) -> bool {
        period > 0x7FF
    }
}

const NUM_CUSTOM_WAVE_SAMPLES: u8 = 32;

#[derive(Serialize, Deserialize)]
pub struct WaveChannel {
    /// Whether this channel is enabled, disabled channels produce silence
    is_enabled: bool,

    /// Whether the DAC is enabled
    is_dac_enabled: bool,

    /// Volume level (2 bits)
    volume: u8,

    /// Index into custom waveform [0-32)
    wave_sample_index: u8,

    /// Custom waveform. 32 4-bit samples stored as 16 bytes, read left to right upper nibble first.
    wave_ram: [u8; WAVE_RAM_SIZE],

    /// A counter down to 0, at which point the period ends and a sample is taken
    period_timer: u16,

    /// Raw value of the full period register (11 bits)
    period_register: u16,

    /// Whether the length timer is enabled
    is_length_timer_enabled: bool,

    /// A counter down to 0 at which point the channel is disabled
    length_timer: u16,
}

impl WaveChannel {
    fn new() -> Self {
        Self {
            is_enabled: false,
            is_dac_enabled: false,
            volume: 0,
            wave_sample_index: 0,
            wave_ram: [0; WAVE_RAM_SIZE],
            period_timer: 0,
            period_register: 0,
            is_length_timer_enabled: false,
            length_timer: 0,
        }
    }

    const MAX_LENGTH_TIMER: u16 = 256;

    pub fn write_nr30(&mut self, value: Register) {
        // Highest bit is the channel enable flag
        self.is_dac_enabled = (value & 0x80) != 0;
        if !self.is_dac_enabled {
            self.is_enabled = false;
        }
    }

    pub fn write_nr31(&mut self, value: Register) {
        // All of NR31 is the initial length timer
        self.length_timer = Self::MAX_LENGTH_TIMER - value as u16;
    }

    pub fn write_nr32(&mut self, value: Register) {
        // Bits 5-6 of NR32 are the volume
        self.volume = (value & 0x60) >> 5;
    }

    pub fn write_nr33(&mut self, value: Register) {
        // All of NR33 is lower bits of period register
        self.period_register = (self.period_register & 0x0700) | (value as u16);
    }

    pub fn write_nr34(&mut self, value: Register) {
        // Lower three bits of NR34 are upper bits of period register
        self.period_register = (self.period_register & 0x00FF) | (((value as u16) & 0x7) << 8);

        // Bit 6 of NR34
        self.is_length_timer_enabled = value & 0x40 != 0;

        // Bit 7 of NR34
        let is_triggered = value & 0x80 != 0;
        if is_triggered {
            self.trigger();
        }
    }

    pub fn write_wave_ram(&mut self, address: u16, value: u8) {
        self.wave_ram[(address - WAVE_RAM_START) as usize] = value;
    }

    fn sample_digital(&self) -> u8 {
        if !self.is_enabled {
            return 0;
        }

        let wave_ram_byte = self.wave_ram[(self.wave_sample_index as usize) / 2];

        // High nibble contains sample before low nibble
        let wave_sample = if self.wave_sample_index % 2 == 0 {
            (wave_ram_byte & 0xF0) >> 4
        } else {
            wave_ram_byte & 0x0F
        };

        // Apply volume adjustment by shifting digital sample
        if self.volume == 0 {
            0
        } else {
            wave_sample >> (self.volume - 1)
        }
    }

    fn sample_analog(&self) -> f32 {
        if !self.is_dac_enabled {
            return 0.0;
        }

        digital_to_analog(self.sample_digital())
    }

    fn trigger(&mut self) {
        // Channel can only be enabled if DAC is enabled
        if self.is_dac_enabled {
            self.is_enabled = true;
        }

        self.period_timer = self.initial_period_timer();
        self.wave_sample_index = 0;

        if self.length_timer == 0 {
            self.length_timer = Self::MAX_LENGTH_TIMER;
        }
    }

    fn initial_period_timer(&self) -> u16 {
        2048 - self.period_register
    }

    fn advance_period_timer(&mut self) {
        // Subtracting would overflow so period is over
        if self.period_timer == 0 {
            // Advance to next sample within wave
            self.wave_sample_index = (self.wave_sample_index + 1) % NUM_CUSTOM_WAVE_SAMPLES;

            // Reload period timer
            self.period_timer = self.initial_period_timer();
        }

        self.period_timer -= 1;
    }

    fn advance_length_timer(&mut self) {
        if self.is_length_timer_enabled && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.is_enabled = false;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct NoiseChannel {
    /// Whether the channel is enabled, disabled channels produce silence
    is_enabled: bool,

    /// Whether the DAC is enabled
    is_dac_enabled: bool,

    /// If true the LFSR is 15 bits wide, otherwise 7 bits wide
    is_lfsr_wide: bool,

    /// The LFSR register itself
    lfsr: u16,

    /// Whether the current sample bit is set (i.e. the last bit shifted out of the LFSR)
    current_sample_bit: bool,

    /// Part of NR43 used to calculate clock timer
    clock_shift: u8,

    /// Part of NR43 used to calculate clock timer
    clock_divider: u8,

    /// A counter down to 0, at which point a new noise sample is generated from the LFSR
    clock_timer: u16,

    /// Whether the length timer is enabled
    is_length_timer_enabled: bool,

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
}

impl NoiseChannel {
    fn new() -> Self {
        Self {
            is_enabled: false,
            is_dac_enabled: false,
            is_lfsr_wide: false,
            lfsr: 0,
            current_sample_bit: false,
            clock_shift: 0,
            clock_divider: 0,
            clock_timer: 0,
            is_length_timer_enabled: false,
            length_timer: 0,
            is_envelope_up: false,
            envelope_sweep_pace: 0,
            envelope_timer: 0,
            initial_volume: 0,
            volume: 0,
        }
    }

    const MAX_LENGTH_TIMER: u8 = 64;

    pub fn write_nr41(&mut self, value: Register) {
        // Lower six bits of NR41
        self.length_timer = Self::MAX_LENGTH_TIMER - (value & 0x3F);
    }

    pub fn write_nr42(&mut self, value: Register) {
        // Upper four bits of NRX2
        self.initial_volume = (value & 0xF0) >> 4;

        // Bit three of NRX2
        self.is_envelope_up = (value & 0x08) != 0;

        // Lower three bits of NRX2
        self.envelope_sweep_pace = value & 0x07;

        // If the envelope's initial volume is 0 and envelope is decreasing, disable the channel
        self.is_dac_enabled = (self.initial_volume != 0) || self.is_envelope_up;
        if !self.is_dac_enabled {
            self.is_enabled = false;
        }
    }

    pub fn write_nr43(&mut self, value: Register) {
        // Clock divider is lower three bits
        self.clock_divider = value & 0x7;

        // LFSR width is bit three
        self.is_lfsr_wide = (value & 0x8) == 0;

        // Top four bits are the shift clock frequency
        self.clock_shift = (value & 0xF0) >> 4;
    }

    pub fn write_nr44(&mut self, value: Register) {
        // Bit 6 of NR44
        self.is_length_timer_enabled = value & 0x40 != 0;

        // Bit 7 of NR44
        let is_triggered = value & 0x80 != 0;
        if is_triggered {
            self.trigger();
        }
    }

    fn trigger(&mut self) {
        // Channel can only be enabled if DAC is enabled
        if self.is_dac_enabled {
            self.is_enabled = true;
        }

        self.clock_timer = self.initial_clock_timer();
        self.volume = self.initial_volume;
        self.lfsr = 0;

        if self.length_timer == 0 {
            self.length_timer = Self::MAX_LENGTH_TIMER;
        }

        if self.envelope_sweep_pace != 0 {
            self.envelope_timer = self.envelope_sweep_pace;
        }
    }

    fn initial_clock_timer(&self) -> u16 {
        // A clock divider of 0 maps to 0.5. Entire noise channel is clocked every 8 ticks instead
        // of every 16 ticks
        if self.clock_divider == 0 {
            1u16 << self.clock_shift
        } else {
            ((self.clock_divider as u16) << 1) << self.clock_shift
        }
    }

    fn sample_digital(&self) -> u8 {
        if !self.is_enabled {
            return 0;
        }

        if self.current_sample_bit {
            self.volume
        } else {
            0
        }
    }

    fn sample_analog(&self) -> f32 {
        if !self.is_dac_enabled {
            return 0.0;
        }

        digital_to_analog(self.sample_digital())
    }

    fn advance_period_timer(&mut self) {
        // Clock shift of 14 or 15 actually stops the channel from being clocked entirely
        if self.clock_shift >= 14 {
            return;
        }

        // Subtracting would overflow so period is over
        if self.clock_timer == 0 {
            // Advance the LFSR. Last bit becomes the current sample bit.
            let new_bit = !(self.lfsr ^ (self.lfsr >> 1)) & 0x1;

            if self.is_lfsr_wide {
                // Wide LFSR copies new bit to bit 15
                self.lfsr = (self.lfsr & 0x7FFF) | ((new_bit << 15) & 0x8000);
            } else {
                // Narrow LFSR copies new bit to bit 7
                self.lfsr = (self.lfsr & 0x7F) | ((new_bit << 7) & 0x80);
            }

            // LFSR is shifted right one bit
            self.lfsr >>= 1;
            self.current_sample_bit = self.lfsr & 0x1 != 0;

            // Reload period timer
            self.clock_timer = self.initial_clock_timer();
        }

        self.clock_timer -= 1;
    }

    fn advance_length_timer(&mut self) {
        if self.is_length_timer_enabled && self.length_timer > 0 {
            self.length_timer -= 1;

            if self.length_timer == 0 {
                self.is_enabled = false;
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
