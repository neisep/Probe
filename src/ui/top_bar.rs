use crate::state::{AppState, View};
use eframe::egui;

pub fn show_topbar(ui: &mut egui::Ui, state: &mut AppState, active_view: View) {
    egui::Panel::top("top_bar").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Probe");
            ui.separator();
            ui.label("Native Rust + egui REST client");
            ui.separator();
            ui.small(format!("View: {}", active_view.label()));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(req) = state.selected_request() {
                    let method = req.method.as_str();
                    let color = match method {
                        "GET" => egui::Color32::from_rgb(88, 165, 77),
                        "POST" => egui::Color32::from_rgb(66, 133, 244),
                        "PUT" => egui::Color32::from_rgb(244, 180, 0),
                        "DELETE" => egui::Color32::from_rgb(219, 68, 55),
                        _ => egui::Color32::LIGHT_GRAY,
                    };

                    // compact url preview
                    let mut url = req.url.clone();
                    if url.len() > 80 {
                        url.truncate(77);
                        url.push_str("...");
                    }

                    ui.label(url);
                    ui.separator();
                    ui.colored_label(color, method);
                } else {
                    ui.label("No request selected");
                }
            });
        });
    });
}
