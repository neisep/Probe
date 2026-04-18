use crate::state::{AppState, View};
use crate::ui::{request_panel, response_panel};
use eframe::egui;

pub fn show_center(ui: &mut egui::Ui, state: &mut AppState) {
    egui::CentralPanel::default().show_inside(ui, |ui| {
        // Tabs
        ui.horizontal(|ui| {
            let req_selected = state.ui.view == View::Editor;
            let hist_selected = state.ui.view == View::History;
            if ui
                .add_sized(
                    [80.0, 28.0],
                    egui::Button::new("Request").selected(req_selected),
                )
                .clicked()
            {
                state.ui.set_view(View::Editor);
            }
            if ui
                .add_sized(
                    [110.0, 28.0],
                    egui::Button::new("Responses").selected(hist_selected),
                )
                .clicked()
            {
                state.ui.set_view(View::History);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(latest) = state.latest_response() {
                    let status = latest
                        .status
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "pending".into());
                    ui.label(format!("Latest: {}", status));
                }
            });
        });

        ui.add_space(8.0);

        match state.ui.view {
            View::Editor => request_panel::show_request_editor(ui, state),
            View::History => response_panel::show_response_history(ui, state),
        }

        ui.add_space(12.0);
    });
}
