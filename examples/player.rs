use egui::{CentralPanel, Color32, ScrollArea, TopBottomPanel, Vec2};
use egui_video::{DisplayMode, PlayerControls, VideoPlayer};
use std::path::PathBuf;

struct VideoPlayerApp {
    player: Option<VideoPlayer>,
    error_message: Option<String>,
}

impl VideoPlayerApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            player: None,
            error_message: None,
        }
    }

    fn open_file(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm", "flv", "wmv"])
            .pick_file()
        {
            self.load_video(path, ctx);
        }
    }

    fn load_video(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.error_message = None;
        match VideoPlayer::open(&path, ctx.clone()) {
            Ok(player) => {
                self.player = Some(player);
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to open video: {}", e));
            }
        }
    }
}

impl eframe::App for VideoPlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Menu bar
        TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        ui.close_menu();
                        self.open_file(ctx);
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });

        // Control bar at bottom
        if let Some(ref mut player) = self.player {
            TopBottomPanel::bottom("controls").show(ctx, |ui| {
                PlayerControls::show(ui, player);
            });
        }

        // Video display area
        CentralPanel::default().show(ctx, |ui| {
            if let Some(ref mut player) = self.player {
                // Update player and get current frame
                player.update(ctx);

                // Get data we need before the closures to avoid borrow conflicts
                let texture_id = player.texture().map(|t| t.id());
                let video_size = player.video_size();
                let display_mode = player.display_mode();

                let mut should_toggle = false;

                if let Some(tex_id) = texture_id {
                    let available_size = ui.available_size();

                    match display_mode {
                        DisplayMode::FitToWindow => {
                            // Scale to fit while maintaining aspect ratio
                            let aspect = video_size.0 as f32 / video_size.1 as f32;
                            let available_aspect = available_size.x / available_size.y;

                            let display_size = if aspect > available_aspect {
                                Vec2::new(available_size.x, available_size.x / aspect)
                            } else {
                                Vec2::new(available_size.y * aspect, available_size.y)
                            };

                            ui.centered_and_justified(|ui| {
                                let response = ui.image((tex_id, display_size));
                                if response.double_clicked() {
                                    should_toggle = true;
                                }
                            });
                        }
                        DisplayMode::NativeSize => {
                            ScrollArea::both().show(ui, |ui| {
                                let response = ui.image((
                                    tex_id,
                                    Vec2::new(video_size.0 as f32, video_size.1 as f32),
                                ));
                                if response.double_clicked() {
                                    should_toggle = true;
                                }
                            });
                        }
                    }
                }

                if should_toggle {
                    player.toggle_display_mode();
                }
            } else {
                // No video loaded - show drop zone / open button
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(ui.available_height() / 3.0);

                        if let Some(ref err) = self.error_message {
                            ui.colored_label(Color32::RED, err);
                            ui.add_space(20.0);
                        }

                        ui.heading("No video loaded");
                        ui.add_space(10.0);

                        if ui.button("Open Video File...").clicked() {
                            self.open_file(ctx);
                        }

                        ui.add_space(10.0);
                        ui.label("Or drag and drop a video file");
                    });
                });
            }
        });

        // Handle file drops
        ctx.input(|i| {
            if !i.raw.dropped_files.is_empty() {
                if let Some(path) = i.raw.dropped_files[0].path.clone() {
                    self.load_video(path, ctx);
                }
            }
        });

        // Request continuous repaint during playback
        if let Some(ref player) = self.player {
            if player.is_playing() {
                ctx.request_repaint();
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");

    let options = eframe::NativeOptions {
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
