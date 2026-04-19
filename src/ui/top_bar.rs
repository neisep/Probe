use crate::state::{AppState, View};
use crate::ui::left_sidebar::environment_editor;
use crate::ui::theme;
use eframe::egui;

pub fn show_topbar(ui: &mut egui::Ui, state: &mut AppState, _active_view: View) {
    egui::Panel::top("top_bar").show_inside(ui, |ui| {
        ui.set_min_height(40.0);
        ui.set_max_height(40.0);
        egui::Frame::NONE
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading(
                        egui::RichText::new("Probe")
                            .color(theme::ACCENT_STRONG)
                            .strong(),
                    );

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "env · {}",
                            environment_editor::active_environment_label(state)
                        ))
                        .color(theme::TEXT_MUTED)
                        .small(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("⚙ Settings")
                            .on_hover_text("Environments and settings")
                            .clicked()
                        {
                            state.ui.settings_open = !state.ui.settings_open;
                        }

                        ui.add_space(14.0);

                        if let Some(req) = state.selected_request() {
                            let mut url = req.url.clone();
                            if url.len() > 60 {
                                url.truncate(57);
                                url.push_str("…");
                            }
                            ui.label(
                                egui::RichText::new(url).monospace().color(theme::TEXT),
                            );
                            ui.add_space(8.0);
                            ui.label(theme::method_badge(&req.method));
                        }
                    });
                });
            });
    });
}
