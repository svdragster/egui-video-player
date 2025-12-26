mod audio;
mod circular_buffer;
mod clock;
mod decoder;
mod video;

use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, Sender};
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Volume level (0.0 to 1.0)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume(f32);

impl Volume {
    /// Create a new volume level. Returns None if value is outside 0.0..=1.0
    pub fn new(value: f32) -> Option<Self> {
        if (0.0..=1.0).contains(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Get the inner value
    #[must_use]
    pub fn get(self) -> f32 {
        self.0
    }
}

use audio::AudioSource;
use circular_buffer::CircularBuffer;
use clock::AudioClock;
use decoder::{probe_media, start_decoder_thread, DecoderCommand};
use video::VideoFrameQueue;

/// Display mode for video rendering
#[derive(Clone, Copy, PartialEq)]
pub enum DisplayMode {
    FitToWindow,
    NativeSize,
}

/// Player state
#[derive(Clone, Copy, PartialEq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

/// Main video player struct
pub struct VideoPlayer {
    // State
    state: PlayerState,
    display_mode: DisplayMode,
    seeking: bool,
    seek_target: f64,

    // Media info
    width: u32,
    height: u32,
    duration: f64,

    // Threading
    decoder_handle: Option<JoinHandle<()>>,
    command_sender: Sender<DecoderCommand>,
    stop_flag: Arc<AtomicBool>,

    // Audio
    _output_stream: OutputStream, // Keep alive
    _stream_handle: OutputStreamHandle,
    sink: Sink,
    clock: AudioClock,

    // Video
    frame_queue: VideoFrameQueue,
    texture: Option<TextureHandle>,

    // Error reporting
    error_receiver: Receiver<String>,
}

impl VideoPlayer {
    /// Open a video file and prepare for playback
    pub fn open(path: &Path, ctx: Context) -> Result<Self> {
        // Probe media file
        let info = probe_media(path)?;

        // Create audio clock
        let clock = AudioClock::new(info.sample_rate, info.channels);

        // Create audio output
        let (output_stream, stream_handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&stream_handle)?;

        // Create circular buffer for audio (about 1 second of buffer)
        let buffer_size = info.sample_rate as usize * info.channels as usize * 2;
        let audio_buffer = CircularBuffer::new(buffer_size);

        // Create audio source and add to sink
        let audio_source = AudioSource::new(audio_buffer.clone(), clock.clone());
        sink.append(audio_source);
        sink.pause(); // Start paused

        // Create video frame channel
        let (video_sender, video_receiver) = bounded(30);
        let frame_queue = VideoFrameQueue::new(video_receiver, 30);

        // Create command channel
        let (command_sender, command_receiver) = bounded(16);

        // Create error channel
        let (error_sender, error_receiver) = bounded(4);

        // Start decoder thread
        let stop_flag = Arc::new(AtomicBool::new(false));
        let decoder_handle = start_decoder_thread(
            path,
            video_sender,
            audio_buffer,
            command_receiver,
            clock.clone(),
            stop_flag.clone(),
            error_sender,
        )?;

        // Create initial texture
        let texture = ctx.load_texture(
            "video_frame",
            ColorImage::new([info.width as usize, info.height as usize], egui::Color32::BLACK),
            TextureOptions::LINEAR,
        );

        let mut player = Self {
            state: PlayerState::Stopped,
            display_mode: DisplayMode::FitToWindow,
            seeking: false,
            seek_target: 0.0,
            width: info.width,
            height: info.height,
            duration: info.duration,
            decoder_handle: Some(decoder_handle),
            command_sender,
            stop_flag,
            _output_stream: output_stream,
            _stream_handle: stream_handle,
            sink,
            clock,
            frame_queue,
            texture: Some(texture),
            error_receiver,
        };

        // Resume decoder temporarily to get first frame, then seek to show it
        let _ = player.command_sender.send(DecoderCommand::Resume);
        player.seek(Duration::ZERO);

        Ok(player)
    }

    /// Start or resume playback
    pub fn play(&mut self) {
        if self.state != PlayerState::Playing {
            self.state = PlayerState::Playing;
            self.sink.play();
            let _ = self.command_sender.send(DecoderCommand::Resume);
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        if self.state == PlayerState::Playing {
            self.state = PlayerState::Paused;
            self.sink.pause();
            let _ = self.command_sender.send(DecoderCommand::Pause);
        }
    }

    /// Stop playback and seek to beginning
    pub fn stop(&mut self) {
        self.state = PlayerState::Stopped;
        self.sink.pause();
        let _ = self.command_sender.send(DecoderCommand::Pause);
        self.seek(Duration::ZERO);
    }

    /// Seek to position
    pub fn seek(&mut self, position: Duration) {
        let position_secs = position.as_secs_f64().clamp(0.0, self.duration);
        self.seeking = true;
        self.seek_target = position_secs;
        self.sink.pause(); // Pause audio during seek to stop clock advancement
        self.frame_queue.clear();
        self.clock.set_position(position_secs);
        let _ = self.command_sender.send(DecoderCommand::Seek(position_secs));
    }

    /// Check if currently seeking
    #[must_use]
    pub fn is_seeking(&self) -> bool {
        self.seeking
    }

    /// Set volume
    pub fn set_volume(&mut self, volume: Volume) {
        self.sink.set_volume(volume.get());
    }

    /// Get current volume
    #[must_use]
    pub fn volume(&self) -> Volume {
        // Safe: rodio volume is always 0.0..=1.0
        Volume(self.sink.volume())
    }

    /// Toggle display mode
    pub fn toggle_display_mode(&mut self) {
        self.display_mode = match self.display_mode {
            DisplayMode::FitToWindow => DisplayMode::NativeSize,
            DisplayMode::NativeSize => DisplayMode::FitToWindow,
        };
    }

    /// Get current display mode
    #[must_use]
    pub fn display_mode(&self) -> DisplayMode {
        self.display_mode
    }

    /// Update player state and texture (call each frame)
    pub fn update(&mut self, ctx: &Context) {
        // Handle seeking state - check for first frame after seek
        if self.seeking {
            if let Some(frame) = self.frame_queue.get_first_frame_after_seek(self.seek_target) {
                // Frame arrived - seek complete
                if let Some(ref mut texture) = self.texture {
                    // Zero-copy: move pixels directly into ColorImage
                    let image = ColorImage {
                        size: [frame.width as usize, frame.height as usize],
                        pixels: frame.pixels,
                    };
                    texture.set(image, TextureOptions::LINEAR);
                }
                // Update clock to match the actual frame we got
                self.clock.set_position(frame.pts);
                self.seeking = false;
                // Resume audio if we were playing
                if self.state == PlayerState::Playing {
                    self.sink.play();
                }
            }
            ctx.request_repaint();
            return;
        }

        if self.state != PlayerState::Playing {
            return;
        }

        let audio_time = self.clock.position();

        if let Some(frame) = self.frame_queue.get_display_frame(audio_time) {
            // Update texture with new frame (zero-copy)
            if let Some(ref mut texture) = self.texture {
                let image = ColorImage {
                    size: [frame.width as usize, frame.height as usize],
                    pixels: frame.pixels,
                };
                texture.set(image, TextureOptions::LINEAR);
            }
        }

        // Check for end of stream
        if self.frame_queue.is_empty() && audio_time >= self.duration - 0.1 {
            self.state = PlayerState::Stopped;
            self.sink.pause();
        }

        ctx.request_repaint();
    }

    /// Get texture handle for rendering
    #[must_use]
    pub fn texture(&self) -> Option<&TextureHandle> {
        self.texture.as_ref()
    }

    /// Get video dimensions
    #[must_use]
    pub fn video_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get video duration
    #[must_use]
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.duration)
    }

    /// Get current playback position
    #[must_use]
    pub fn position(&self) -> Duration {
        let secs = if self.seeking {
            self.seek_target // Show seek target while seeking
        } else {
            self.clock.position()
        };
        Duration::from_secs_f64(secs)
    }

    /// Check if currently playing
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.state == PlayerState::Playing
    }

    /// Get player state
    #[must_use]
    pub fn state(&self) -> PlayerState {
        self.state
    }

    /// Poll for decoder errors (non-blocking)
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error_receiver.try_recv().ok()
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        // Signal decoder to stop
        self.stop_flag.store(true, Ordering::Relaxed);
        let _ = self.command_sender.send(DecoderCommand::Stop);

        // Wait for decoder thread
        if let Some(handle) = self.decoder_handle.take() {
            let _ = handle.join();
        }
    }
}
