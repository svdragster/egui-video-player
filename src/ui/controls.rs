use crate::player::{DisplayMode, PlayerState, VideoPlayer};
use egui::{Slider, Ui};

pub struct PlayerControls;

impl PlayerControls {
    pub fn show(ui: &mut Ui, player: &mut VideoPlayer) {
        ui.horizontal(|ui| {
            // Play/Pause button
            let play_pause_text = match player.state() {
                PlayerState::Playing => "⏸",
                _ => "▶",
            };

            if ui.button(play_pause_text).clicked() {
                if player.is_playing() {
                    player.pause();
                } else {
                    player.play();
                }
            }

            // Stop button
            if ui.button("⏹").clicked() {
                player.stop();
            }

            ui.separator();

            // Timeline / seek bar
            let duration = player.duration();
            let player_position = player.position();

            ui.label(format_time(player_position));

            // Use memory to persist slider position during drag
            let slider_id = ui.id().with("seek_slider");
            let mut position = ui.memory(|mem| {
                mem.data.get_temp::<f64>(slider_id).unwrap_or(player_position)
            });

            let slider_response = ui.add(
                Slider::new(&mut position, 0.0..=duration)
                    .show_value(false)
                    .trailing_fill(true),
            );

            // Update memory with current position
            if slider_response.dragged() {
                // While dragging, store the dragged position
                ui.memory_mut(|mem| mem.data.insert_temp(slider_id, position));
            } else if !player.is_seeking() {
                // When not dragging and not seeking, sync with player
                ui.memory_mut(|mem| mem.data.insert_temp(slider_id, player_position));
            }

            if slider_response.drag_stopped() || slider_response.clicked() {
                player.seek(position);
            }

            ui.label(format_time(duration));

            ui.separator();

            // Volume control
            ui.label("🔊");
            let mut volume = player.volume();
            if ui
                .add(Slider::new(&mut volume, 0.0..=1.0).show_value(false))
                .changed()
            {
                player.set_volume(volume);
            }

            ui.separator();

            // Display mode toggle
            let mode_text = match player.display_mode() {
                DisplayMode::FitToWindow => "⛶",
                DisplayMode::NativeSize => "⊞",
            };

            if ui
                .button(mode_text)
                .on_hover_text("Toggle display mode (double-click video)")
                .clicked()
            {
                player.toggle_display_mode();
            }
        });
    }
}

fn format_time(seconds: f64) -> String {
    let total_seconds = seconds as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}
