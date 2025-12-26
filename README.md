# egui_video

Video player library for egui using FFmpeg.

## Features

- Audio/video sync with audio as master clock
- Seeking support
- Volume control
- Fit-to-window and native size display modes

## Usage

```rust
use egui_video::{VideoPlayer, PlayerControls};

// Open a video
let player = VideoPlayer::open(&path, ctx.clone())?;

// In your update loop
player.update(ctx);

// Show controls
PlayerControls::show(ui, &mut player);

// Render the video texture
if let Some(texture) = player.texture() {
    ui.image((texture.id(), size));
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
