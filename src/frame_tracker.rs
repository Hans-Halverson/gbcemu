use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Instant,
};

pub struct FrameTracker {
    /// Timestamp at the start of tracking, seconds are relative to this
    base_time: Instant,

    /// The current second being tracked
    current_second: u64,

    /// The frame count for the current second being tracked
    current_second_frame_count: u32,

    /// The frame rate to report for the emulator. This is the frame rate recorded in the last
    /// completed second.
    output_frame_rate: Option<Arc<AtomicU32>>,
}

impl FrameTracker {
    pub fn new() -> Self {
        Self {
            base_time: Instant::now(),
            current_second: 0,
            current_second_frame_count: 0,
            output_frame_rate: None,
        }
    }

    pub fn init(&mut self, base_time: Instant, output_frame_rate: Option<Arc<AtomicU32>>) {
        self.base_time = base_time;
        self.output_frame_rate = output_frame_rate;
    }

    pub fn frame_complete(&mut self) {
        let second = Instant::now().duration_since(self.base_time).as_secs();

        if second == self.current_second {
            self.current_second_frame_count += 1;
        } else {
            // Flush the previous second's count
            if let Some(output_frame_rate) = &self.output_frame_rate {
                output_frame_rate.store(self.current_second_frame_count, Ordering::Relaxed);
            }

            self.current_second = second;
            self.current_second_frame_count = 1;
        }
    }
}

impl Default for FrameTracker {
    fn default() -> Self {
        Self::new()
    }
}
