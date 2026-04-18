use crate::state::{AppState, View};
use eframe::egui;

fn short_url(url: &str, max: usize) -> String {
    let mut s = url.to_string();
    if s.len() > max {
        s.truncate(max.saturating_sub(3));
        s.push_str("...");
    }
    s
}

pub fn show_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(260.0)
        .show_inside(ui, |ui| {
            ui.heading("Requests");
            ui.separator();

            if state.requests.is_empty() {
                ui.label("No requests yet");
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, req) in state.requests.iter().enumerate() {
                        let is_selected = state.ui.selected_request == Some(i);
                        ui.horizontal(|ui| {
                            let method = req.method.as_str();
                            let color = match method {
                                "GET" => egui::Color32::from_rgb(88, 165, 77),
                                "POST" => egui::Color32::from_rgb(66, 133, 244),
                                "PUT" => egui::Color32::from_rgb(244, 180, 0),
                                "DELETE" => egui::Color32::from_rgb(219, 68, 55),
                                _ => egui::Color32::LIGHT_GRAY,
                            };

                            ui.colored_label(color, method);
                            if ui
                                .selectable_label(is_selected, short_url(&req.url, 48))
                                .clicked()
                            {
                                state.ui.select_request(i);
                            }

                            // show a best-effort response status if available
                            let resp_status = state
                                .responses
                                .get(i)
                                .and_then(|r| r.status)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "-".to_string());
                            ui.add_space(4.0);
                            ui.label(resp_status);
                        });
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Summary");
            ui.label(format!("Requests: {}", state.requests.len()));
            ui.label(format!("Responses: {}", state.responses.len()));
            ui.add_space(8.0);

            ui.heading("Views");
            ui.separator();
            for v in [View::Editor, View::History] {
                let is_selected = state.ui.view == v;
                if ui.selectable_label(is_selected, v.label()).clicked() {
                    state.ui.set_view(v);
                }
            }

            ui.add_space(10.0);
            ui.heading("Shortcuts");
            ui.separator();
            ui.label("• New request: (n/a in MVP)");
            ui.label("• Send: Use bottom 'Send selected request' button");
        });
}
