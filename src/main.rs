mod app;
mod player;
mod ui;

use app::VideoPlayerApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Video Player",
        options,
        Box::new(|cc| Ok(Box::new(VideoPlayerApp::new(cc)))),
    )
}
