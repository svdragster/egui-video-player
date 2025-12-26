use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Audio clock for A/V synchronization.
/// Uses audio playback position as the master clock.
#[derive(Clone)]
pub struct AudioClock {
    /// Current playback position in microseconds
    position_us: Arc<AtomicU64>,
    /// Whether playback is paused
    paused: Arc<AtomicBool>,
    /// Flag to clear audio buffer (set on seek)
    clear_buffer: Arc<AtomicBool>,
    /// Sample rate of audio stream
    sample_rate: u32,
    /// Number of audio channels
    channels: u16,
}

impl AudioClock {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            position_us: Arc::new(AtomicU64::new(0)),
            paused: Arc::new(AtomicBool::new(true)),
            clear_buffer: Arc::new(AtomicBool::new(false)),
            sample_rate,
            channels,
        }
    }

    /// Get current playback position in seconds
    pub fn position(&self) -> f64 {
        self.position_us.load(Ordering::Relaxed) as f64 / 1_000_000.0
    }

    /// Set playback position in seconds (used during seek)
    /// Also sets the clear_buffer flag to discard old audio samples
    pub fn set_position(&self, seconds: f64) {
        let us = (seconds * 1_000_000.0) as u64;
        self.position_us.store(us, Ordering::Relaxed);
        self.clear_buffer.store(true, Ordering::Relaxed);
    }

    /// Check and clear the buffer clear flag (returns true if buffer should be cleared)
    pub fn should_clear_buffer(&self) -> bool {
        self.clear_buffer.swap(false, Ordering::Relaxed)
    }

    /// Advance clock by given number of samples consumed
    pub fn advance_samples(&self, samples: u64) {
        if !self.paused.load(Ordering::Relaxed) {
            let us_per_sample = 1_000_000.0 / (self.sample_rate as f64 * self.channels as f64);
            let delta_us = (samples as f64 * us_per_sample) as u64;
            self.position_us.fetch_add(delta_us, Ordering::Relaxed);
        }
    }

    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}
