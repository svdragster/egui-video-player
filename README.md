# egui Video Player

Video player library for egui using FFmpeg.

![Screenshot_20251226_193011](https://github.com/user-attachments/assets/d7b0fa12-bfd8-46d0-b99b-c13a4941062d)

## Features

- Audio/video sync with audio as master clock
- Seeking support
- Volume control
- Fit-to-window and native size display modes

## Usage

```rust
use egui_video::{VideoPlayer, Volume};
use std::time::Duration;

// Create player
let mut player = VideoPlayer::open(&path, ctx.clone())?;

// Control playback
player.play();
player.pause();
player.seek(Duration::from_secs(30));
player.set_volume(Volume::new(0.5).unwrap());

// Query state
let pos: Duration = player.position();
let dur: Duration = player.duration();
let playing: bool = player.is_playing();

// In your egui update loop
player.update(ctx);
if let Some(tex) = player.texture() {
    ui.image((tex.id(), size));
}
```

## Example

```sh
cargo run --release --example player
```

## Requirements

- Rust 1.70+
- FFmpeg 7+ development libraries

### Installing FFmpeg

**Arch Linux:**
```sh
pacman -S ffmpeg
```

**Ubuntu/Debian:**
```sh
apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev
```

**macOS:**
```sh
brew install ffmpeg
```

**Windows:**
See [ffmpeg-next documentation](https://github.com/zmwangx/rust-ffmpeg/wiki/Notes-on-building#building-on-windows)

## Building

```sh
cargo build --release
```
