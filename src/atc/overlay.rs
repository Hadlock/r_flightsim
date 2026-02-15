use super::types::{RadioMessage, Speaker};

pub fn draw_radio_overlay(
    ctx: &egui::Context,
    messages: &[&RadioMessage],
    com1_freq: f32,
) {
    egui::Area::new(egui::Id::new("radio_overlay"))
        .anchor(egui::Align2::RIGHT_TOP, egui::Vec2::new(-10.0, 10.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(egui::Color32::from_rgba_unmultiplied(25, 51, 76, 200))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(380.0);

                    // Frequency header
                    ui.label(
                        egui::RichText::new(format!("COM1: {:.1}", com1_freq))
                            .color(egui::Color32::from_rgb(120, 180, 220))
                            .small()
                            .strong(),
                    );

                    ui.add_space(4.0);

                    // Show last 4 messages
                    let display_msgs: Vec<_> = messages.iter().rev().take(4).rev().collect();

                    if display_msgs.is_empty() {
                        ui.label(
                            egui::RichText::new("  monitoring...")
                                .color(egui::Color32::from_rgb(100, 120, 140))
                                .small(),
                        );
                    } else {
                        for msg in display_msgs {
                            let is_controller = matches!(
                                msg.speaker,
                                Speaker::Controller(_)
                            );
                            let speaker_color = if is_controller {
                                egui::Color32::from_rgb(140, 220, 255) // light cyan
                            } else {
                                egui::Color32::from_rgb(180, 190, 200) // light gray
                            };
                            let text_color = if is_controller {
                                egui::Color32::from_rgb(220, 235, 245)
                            } else {
                                egui::Color32::from_rgb(170, 180, 190)
                            };

                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}:", msg.display_speaker))
                                        .color(speaker_color)
                                        .small()
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(&msg.text)
                                        .color(text_color)
                                        .small(),
                                );
                            });
                        }
                    }
                });
        });
}
