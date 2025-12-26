use rodio::Source;
use std::sync::Arc;
use std::time::Duration;

use super::circular_buffer::CircularBuffer;
use super::clock::AudioClock;

/// Audio source that pulls from a circular buffer and updates the audio clock.
/// Implements rodio::Source for playback.
pub struct AudioSource {
    buffer: Arc<CircularBuffer<f32>>,
    clock: AudioClock,
    samples_consumed: u64,
}

impl AudioSource {
    pub fn new(buffer: Arc<CircularBuffer<f32>>, clock: AudioClock) -> Self {
        Self {
            buffer,
            clock,
            samples_consumed: 0,
        }
    }
}

impl Iterator for AudioSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we need to clear the buffer (after seek)
        if self.clock.should_clear_buffer() {
            self.buffer.clear();
            self.samples_consumed = 0;
            return Some(0.0); // Return silence
        }

        // Try to get a sample from the circular buffer
        match self.buffer.try_pop() {
            Some(sample) => {
                self.samples_consumed += 1;
                // Update clock every batch of samples for efficiency
                if self.samples_consumed % 256 == 0 {
                    self.clock.advance_samples(256);
                }
                Some(sample)
            }
            None => {
                // Buffer underrun - return silence
                Some(0.0)
            }
        }
    }
}

impl Source for AudioSource {
    fn current_frame_len(&self) -> Option<usize> {
        None // Infinite stream
    }

    fn channels(&self) -> u16 {
        self.clock.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.clock.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        None // Infinite stream
    }
}
