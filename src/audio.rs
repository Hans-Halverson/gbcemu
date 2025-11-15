use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender},
};

use rodio::{OutputStream, OutputStreamBuilder, Sink, Source};
use serde::{Deserialize, Serialize};

use crate::emulator::{Emulator, REFRESH_RATE, TICKS_PER_FRAME};

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

/// Audio processing unit
#[derive(Serialize, Deserialize)]
pub struct Apu;

impl Apu {
    pub fn new() -> Self {
        Self
    }
}

impl Emulator {
    pub fn sample_audio(&self) -> (f32, f32) {
        let mut mixed_left = 0.0;
        let mut mixed_right = 0.0;

        let nr51 = self.nr51();

        // Mix in channel 1
        if nr51 & 0x11 != 0 {
            let (left, right) = self.sample_channel_1_analog();

            if nr51 & 0x01 != 0 {
                mixed_right += right;
            }

            if nr51 & 0x10 != 0 {
                mixed_left += left;
            }
        }

        // Mix in channel 2
        if nr51 & 0x22 != 0 {
            let (left, right) = self.sample_channel_2_analog();

            if nr51 & 0x02 != 0 {
                mixed_right += right;
            }

            if nr51 & 0x20 != 0 {
                mixed_left += left;
            }
        }

        // Mix in channel 3
        if nr51 & 0x44 != 0 {
            let (left, right) = self.sample_channel_3_analog();

            if nr51 & 0x04 != 0 {
                mixed_right += right;
            }

            if nr51 & 0x40 != 0 {
                mixed_left += left;
            }
        }

        // Mix in channel 4
        if nr51 & 0x88 != 0 {
            let (left, right) = self.sample_channel_4_analog();

            if nr51 & 0x08 != 0 {
                mixed_right += right;
            }

            if nr51 & 0x80 != 0 {
                mixed_left += left;
            }
        }

        // Evenly mix channels
        mixed_left /= 4.0;
        mixed_right /= 4.0;

        let nr50 = self.nr50();
        let left_volume = (nr50 & 0x70) >> 4;
        let right_volume = nr50 & 0x7;

        // Scale by channel volumes, 0 == volume 1, 7 == volume 8
        let final_left = mixed_left * (left_volume as f32 + 1.0 / 8.0);
        let final_right = mixed_right * (right_volume as f32 + 1.0 / 8.0);

        (final_left, final_right)
    }

    fn sample_channel_1_analog(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn sample_channel_2_analog(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn sample_channel_3_analog(&self) -> (f32, f32) {
        (0.0, 0.0)
    }

    fn sample_channel_4_analog(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
}
