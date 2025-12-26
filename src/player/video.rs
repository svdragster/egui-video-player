use crossbeam_channel::Receiver;
use std::collections::VecDeque;

use super::decoder::DecodedVideoFrame;

/// Threshold for frame dropping (seconds behind audio)
const DROP_THRESHOLD: f64 = 0.02;
/// Threshold for holding frames (seconds ahead of audio)
const HOLD_THRESHOLD: f64 = 0.02;

/// Queue that manages video frames and sync to audio clock
pub struct VideoFrameQueue {
    receiver: Receiver<DecodedVideoFrame>,
    buffer: VecDeque<DecodedVideoFrame>,
    current_frame: Option<DecodedVideoFrame>,
    max_buffer_size: usize,
}

impl VideoFrameQueue {
    pub fn new(receiver: Receiver<DecodedVideoFrame>, max_buffer_size: usize) -> Self {
        Self {
            receiver,
            buffer: VecDeque::with_capacity(max_buffer_size),
            current_frame: None,
            max_buffer_size,
        }
    }

    /// Update the queue by receiving new frames from the decoder
    pub fn receive_frames(&mut self) {
        // Receive frames up to buffer capacity
        while self.buffer.len() < self.max_buffer_size {
            match self.receiver.try_recv() {
                Ok(frame) => {
                    self.buffer.push_back(frame);
                }
                Err(_) => break,
            }
        }
    }

    /// Get the frame that should be displayed for the given audio time.
    /// Returns the frame data if a new frame should be shown.
    pub fn get_display_frame(&mut self, audio_time: f64) -> Option<&DecodedVideoFrame> {
        self.receive_frames();

        // Drop frames that are too late
        while let Some(frame) = self.buffer.front() {
            if frame.pts < audio_time - DROP_THRESHOLD {
                self.buffer.pop_front();
            } else {
                break;
            }
        }

        // Check if next frame should be shown
        if let Some(frame) = self.buffer.front() {
            if frame.pts <= audio_time + HOLD_THRESHOLD {
                // Time to show this frame
                self.current_frame = self.buffer.pop_front();
            }
        }

        self.current_frame.as_ref()
    }

    /// Get the current frame without advancing
    pub fn current_frame(&self) -> Option<&DecodedVideoFrame> {
        self.current_frame.as_ref()
    }

    /// Get the first available frame after a seek (more lenient than sync logic)
    /// Accepts any frame at or after the seek target
    pub fn get_first_frame_after_seek(&mut self, seek_target: f64) -> Option<&DecodedVideoFrame> {
        self.receive_frames();

        // Drop frames that are before the seek target (with some tolerance)
        while let Some(frame) = self.buffer.front() {
            if frame.pts < seek_target - 0.5 {
                self.buffer.pop_front();
            } else {
                break;
            }
        }

        // Take the first available frame
        if self.buffer.front().is_some() {
            self.current_frame = self.buffer.pop_front();
        }

        self.current_frame.as_ref()
    }

    /// Clear all buffered frames (used during seek)
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_frame = None;
        // Drain the receiver
        while self.receiver.try_recv().is_ok() {}
    }

    /// Check if queue is empty (end of stream reached)
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty() && self.current_frame.is_none() && self.receiver.is_empty()
    }
}
