use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{Receiver, Sender, TryRecvError, TrySendError};
use egui::Color32;
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::{Audio as AudioFrame, Video as VideoFrame};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::resampling::Context as ResamplerContext;
use ffmpeg_next::software::scaling::{Context as ScalerContext, Flags};
use ffmpeg_next::util::channel_layout::ChannelLayout;
use ffmpeg_next::util::format::sample::Sample;
use ffmpeg_next::{codec, Packet, Rational};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use super::circular_buffer::CircularBuffer;
use super::clock::AudioClock;

// Compile-time verification that Color32 can be safely transmuted from [u8; 4]
const _: () = assert!(std::mem::size_of::<Color32>() == 4);
const _: () = assert!(std::mem::align_of::<Color32>() == 1);

/// A decoded video frame ready for display
pub struct DecodedVideoFrame {
    pub pixels: Vec<Color32>,
    pub width: u32,
    pub height: u32,
    pub pts: f64, // seconds
}

/// Commands sent to the decoder thread
pub enum DecoderCommand {
    Seek(f64),
    Pause,
    Resume,
    Stop,
}

/// Media info extracted from the file
pub struct MediaInfo {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Open a media file and extract info without starting decoding
pub fn probe_media(path: &Path) -> Result<MediaInfo> {
    let input = ffmpeg_next::format::input(path).context("Failed to open input file")?;

    let video_stream = input
        .streams()
        .best(Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_decoder = codec::Context::from_parameters(video_stream.parameters())?
        .decoder()
        .video()?;

    let audio_stream = input.streams().best(Type::Audio);

    let (sample_rate, channels) = if let Some(audio) = audio_stream {
        let audio_decoder = codec::Context::from_parameters(audio.parameters())?
            .decoder()
            .audio()?;
        (audio_decoder.rate(), audio_decoder.channels() as u16)
    } else {
        (44100, 2) // Default if no audio
    };

    let duration = if input.duration() > 0 {
        input.duration() as f64 / ffmpeg_next::ffi::AV_TIME_BASE as f64
    } else {
        0.0
    };

    Ok(MediaInfo {
        width: video_decoder.width(),
        height: video_decoder.height(),
        duration,
        sample_rate,
        channels,
    })
}

/// Start the decoder thread
pub fn start_decoder_thread(
    path: &Path,
    video_sender: Sender<DecodedVideoFrame>,
    audio_buffer: Arc<CircularBuffer<f32>>,
    command_receiver: Receiver<DecoderCommand>,
    clock: AudioClock,
    stop_flag: Arc<AtomicBool>,
    error_sender: Sender<String>,
) -> Result<JoinHandle<()>> {
    let path = path.to_path_buf();

    let handle = thread::spawn(move || {
        if let Err(e) = decode_loop(
            &path,
            video_sender,
            &audio_buffer,
            command_receiver,
            clock,
            stop_flag,
        ) {
            let _ = error_sender.send(format!("Decoder error: {}", e));
        }
    });

    Ok(handle)
}

fn decode_loop(
    path: &Path,
    video_sender: Sender<DecodedVideoFrame>,
    audio_buffer: &Arc<CircularBuffer<f32>>,
    command_receiver: Receiver<DecoderCommand>,
    clock: AudioClock,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let mut input = ffmpeg_next::format::input(path)?;

    // Find streams
    let video_stream_index = input
        .streams()
        .best(Type::Video)
        .ok_or_else(|| anyhow!("No video stream"))?
        .index();

    let audio_stream_index = input.streams().best(Type::Audio).map(|s| s.index());

    // Get stream info before creating decoders
    let video_stream = input.stream(video_stream_index).unwrap();
    let video_time_base = video_stream.time_base();
    let video_params = video_stream.parameters();

    let (_audio_time_base, audio_params) = if let Some(idx) = audio_stream_index {
        let stream = input.stream(idx).unwrap();
        (stream.time_base(), Some(stream.parameters()))
    } else {
        (Rational::new(1, 1), None)
    };

    // Create decoders
    let mut video_decoder = codec::Context::from_parameters(video_params)?
        .decoder()
        .video()?;

    let mut audio_decoder = if let Some(params) = audio_params {
        Some(codec::Context::from_parameters(params)?.decoder().audio()?)
    } else {
        None
    };

    // Create scaler for video (to RGBA)
    let mut scaler = ScalerContext::get(
        video_decoder.format(),
        video_decoder.width(),
        video_decoder.height(),
        Pixel::RGBA,
        video_decoder.width(),
        video_decoder.height(),
        Flags::BILINEAR,
    )?;

    // Create resampler for audio (to f32 stereo)
    let mut resampler = if let Some(ref decoder) = audio_decoder {
        Some(ResamplerContext::get(
            decoder.format(),
            decoder.channel_layout(),
            decoder.rate(),
            Sample::F32(ffmpeg_next::util::format::sample::Type::Packed),
            ChannelLayout::STEREO,
            clock.sample_rate(),
        )?)
    } else {
        None
    };

    let mut video_frame = VideoFrame::empty();
    let mut audio_frame = AudioFrame::empty();
    let mut rgba_frame = VideoFrame::empty();

    let mut paused = true;
    let mut pending_seek: Option<f64> = None;
    let mut at_eof = false;

    // Main decode loop - use manual packet reading instead of iterator
    loop {
        // Check for stop
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Handle commands (process all pending commands)
        loop {
            match command_receiver.try_recv() {
                Ok(DecoderCommand::Stop) => return Ok(()),
                Ok(DecoderCommand::Pause) => {
                    paused = true;
                    clock.pause();
                }
                Ok(DecoderCommand::Resume) => {
                    paused = false;
                    clock.resume();
                }
                Ok(DecoderCommand::Seek(target)) => {
                    pending_seek = Some(target);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        // Handle pending seek
        if let Some(target) = pending_seek.take() {
            let target_ts = (target * ffmpeg_next::ffi::AV_TIME_BASE as f64) as i64;
            if input.seek(target_ts, ..target_ts).is_ok() {
                // Flush decoders
                video_decoder.flush();
                if let Some(ref mut dec) = audio_decoder {
                    dec.flush();
                }
                clock.set_position(target);
                at_eof = false; // Clear EOF - we can read packets again
            }
        }

        // Skip packet reading if paused or at EOF (wait for seek)
        if paused || at_eof {
            thread::sleep(std::time::Duration::from_millis(10));
            continue;
        }

        // Read next packet
        let mut packet = Packet::empty();
        match packet.read(&mut input) {
            Ok(()) => {
                let stream_index = packet.stream();

                // Decode video
                if stream_index == video_stream_index {
                    video_decoder.send_packet(&packet)?;

                    'frame_loop: while video_decoder.receive_frame(&mut video_frame).is_ok() {
                        // Scale to RGBA
                        scaler.run(&video_frame, &mut rgba_frame)?;

                        // Calculate PTS in seconds
                        let pts = video_frame.pts().unwrap_or(0);
                        let pts_seconds = pts as f64 * f64::from(video_time_base);

                        // Convert RGBA bytes to Color32 via transmute (zero-copy reinterpret)
                        // Safe because: Color32 is repr(C) with same layout as [u8; 4] in RGBA order
                        let pixels: Vec<Color32> = unsafe {
                            let mut rgba = rgba_frame.data(0).to_vec();
                            let len = rgba.len() / 4;
                            let cap = rgba.capacity() / 4;
                            let ptr = rgba.as_mut_ptr() as *mut Color32;
                            std::mem::forget(rgba);
                            Vec::from_raw_parts(ptr, len, cap)
                        };

                        let mut frame = DecodedVideoFrame {
                            pixels,
                            width: rgba_frame.width(),
                            height: rgba_frame.height(),
                            pts: pts_seconds,
                        };

                        // Non-blocking send with command polling
                        loop {
                            // Check for commands first - seek/stop take priority
                            match command_receiver.try_recv() {
                                Ok(DecoderCommand::Stop) => return Ok(()),
                                Ok(DecoderCommand::Pause) => {
                                    paused = true;
                                    clock.pause();
                                }
                                Ok(DecoderCommand::Resume) => {
                                    paused = false;
                                    clock.resume();
                                }
                                Ok(DecoderCommand::Seek(target)) => {
                                    // Seek requested - abandon this frame and process seek
                                    pending_seek = Some(target);
                                    break 'frame_loop;
                                }
                                Err(TryRecvError::Empty) => {}
                                Err(TryRecvError::Disconnected) => return Ok(()),
                            }

                            // Try to send the frame
                            match video_sender.try_send(frame) {
                                Ok(()) => break, // Frame sent successfully
                                Err(TrySendError::Full(f)) => {
                                    frame = f; // Channel full, retry after brief sleep
                                    thread::sleep(std::time::Duration::from_millis(1));
                                }
                                Err(TrySendError::Disconnected(_)) => return Ok(()),
                            }
                        }
                    }
                }

                // Decode audio
                if let Some(audio_idx) = audio_stream_index {
                    if stream_index == audio_idx {
                        if let Some(ref mut decoder) = audio_decoder {
                            decoder.send_packet(&packet)?;

                            while decoder.receive_frame(&mut audio_frame).is_ok() {
                                if let Some(ref mut resampler) = resampler {
                                    // Resample to f32 stereo
                                    let mut resampled = AudioFrame::empty();
                                    if resampler.run(&audio_frame, &mut resampled).is_ok() {
                                        // Get samples as f32
                                        let data = resampled.data(0);
                                        let samples: &[f32] = unsafe {
                                            std::slice::from_raw_parts(
                                                data.as_ptr() as *const f32,
                                                data.len() / 4,
                                            )
                                        };

                                        // Write to circular buffer (never blocks, overwrites oldest if full)
                                        audio_buffer.push_slice(samples);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(ffmpeg_next::Error::Eof) => {
                // End of file - wait for seek or stop command
                at_eof = true;
                continue;
            }
            Err(_) => {
                // Skip corrupted packets
                continue;
            }
        }
    }

    // Flush decoders
    video_decoder.send_eof()?;
    while video_decoder.receive_frame(&mut video_frame).is_ok() {
        scaler.run(&video_frame, &mut rgba_frame)?;
        let pts = video_frame.pts().unwrap_or(0);
        let pts_seconds = pts as f64 * f64::from(video_time_base);

        let pixels: Vec<Color32> = unsafe {
            let mut rgba = rgba_frame.data(0).to_vec();
            let len = rgba.len() / 4;
            let cap = rgba.capacity() / 4;
            let ptr = rgba.as_mut_ptr() as *mut Color32;
            std::mem::forget(rgba);
            Vec::from_raw_parts(ptr, len, cap)
        };

        let frame = DecodedVideoFrame {
            pixels,
            width: rgba_frame.width(),
            height: rgba_frame.height(),
            pts: pts_seconds,
        };

        let _ = video_sender.send(frame);
    }

    if let Some(ref mut decoder) = audio_decoder {
        decoder.send_eof()?;
        while decoder.receive_frame(&mut audio_frame).is_ok() {
            // Process remaining audio...
        }
    }

    Ok(())
}
