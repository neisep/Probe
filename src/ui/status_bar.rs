use crate::state::AppState;
use eframe::egui;

pub fn show_status(ui: &mut egui::Ui, state: &AppState, status: &str) {
    egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(status);
            ui.separator();
            ui.label(format!("{} requests", state.requests.len()));
            ui.separator();

            if let Some(req) = state.selected_request() {
                let mut url = req.url.clone();
                if url.len() > 60 {
                    url.truncate(57);
                    url.push_str("...");
                }
                ui.label(format!("Active: {} {}", req.method, url));
            } else {
                ui.label("Active: -");
            }

            ui.separator();
            if let Some(resp) = state.latest_response() {
                if let Some(code) = resp.status {
                    let timing = resp
                        .timing_ms
                        .map(|t| format!(" {}ms", t))
                        .unwrap_or_default();
                    ui.label(format!("Last: {}{}", code, timing));
                } else if let Some(err) = &resp.error {
                    ui.colored_label(egui::Color32::from_rgb(220, 180, 0), err);
                } else {
                    ui.label("Last: -");
                }
            } else {
                ui.label("Last: -");
            }

            ui.separator();
            ui.label("Checkpoint: runnable MVP");
        });
    });
}
