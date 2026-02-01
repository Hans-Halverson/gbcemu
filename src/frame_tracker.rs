use std::time::Instant;

pub struct FrameTracker {
    /// Timestamp at the start of tracking, seconds are relative to this
    base_time: Instant,

    /// The current second being tracked
    current_second: u64,

    /// The frame count for the current second being tracked
    current_second_frame_count: u32,

    /// The frame rate to report for the emulator. This is the frame rate recorded in the last
    /// completed second.
    current_frame_rate: u32,

    /// Total number of on-time frames since initialization
    on_time_frames: u64,

    /// Total number of missed frames since initialization
    missed_frames: u64,
}

impl FrameTracker {
    pub fn new() -> Self {
        Self {
            base_time: Instant::now(),
            current_second: 0,
            current_second_frame_count: 0,
            current_frame_rate: 0,
            on_time_frames: 0,
            missed_frames: 0,
        }
    }

    pub fn init(&mut self, base_time: Instant) {
        self.base_time = base_time;
    }

    pub fn frame_complete(&mut self) {
        let second = Instant::now().duration_since(self.base_time).as_secs();

        if second == self.current_second {
            self.current_second_frame_count += 1;
        } else {
            // Flush the previous second's count
            self.current_frame_rate = self.current_second_frame_count;

            // Start tracking in the new second
            self.current_second = second;
            self.current_second_frame_count = 1;
        }
    }

    pub fn mark_frame_on_time(&mut self) {
        self.on_time_frames += 1;
    }

    pub fn mark_frame_missed(&mut self) {
        self.missed_frames += 1;
    }

    pub fn total_on_time_percent(&self) -> f64 {
        let total_frames = self.on_time_frames + self.missed_frames;
        (self.on_time_frames as f64 / total_frames as f64) * 100.0
    }

    pub fn current_frame_rate(&self) -> u32 {
        self.current_frame_rate
    }
}

impl Default for FrameTracker {
    fn default() -> Self {
        Self::new()
    }
}
