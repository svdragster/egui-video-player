# egui-video-player

Cross-platform video player using FFmpeg and egui.

## Features

- Audio/video sync with audio as master clock
- Seeking support
- Volume control
- Fit-to-window and native size display modes

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

## Usage

```sh
cargo run --release
```

Use "Open File" to select a video file.
