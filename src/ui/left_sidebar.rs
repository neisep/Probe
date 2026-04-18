use crate::state::{AppState, View};
use eframe::egui;

fn compact_text(text: &str, max: usize) -> String {
    let mut compact = text.trim().to_owned();
    if compact.len() > max {
        compact.truncate(max.saturating_sub(3));
        compact.push_str("...");
    }
    compact
}

fn method_color(method: &str) -> egui::Color32 {
    match method {
        "GET" => egui::Color32::from_rgb(88, 165, 77),
        "POST" => egui::Color32::from_rgb(66, 133, 244),
        "PUT" => egui::Color32::from_rgb(244, 180, 0),
        "DELETE" => egui::Color32::from_rgb(219, 68, 55),
        _ => egui::Color32::LIGHT_GRAY,
    }
}

pub fn show_sidebar(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::left("sidebar")
        .resizable(true)
        .default_size(260.0)
        .show_inside(ui, |ui| {
            let has_selected_request = state.selected_request_index().is_some();

            ui.horizontal(|ui| {
                ui.heading("Requests");
                ui.add_space(8.0);

                if ui
                    .small_button("New")
                    .on_hover_text("Create a fresh request draft")
                    .clicked()
                {
                    let new_index = state.add_default_request();
                    state.ui.select_request(new_index);
                    state.ui.set_view(View::Editor);
                }

                if ui
                    .add_enabled(has_selected_request, egui::Button::new("Dup").small())
                    .on_hover_text("Duplicate the selected request draft")
                    .clicked()
                {
                    if let Some(new_index) = state.duplicate_selected_request() {
                        state.ui.select_request(new_index);
                        state.ui.set_view(View::Editor);
                    }
                }

                if ui
                    .add_enabled(has_selected_request, egui::Button::new("Del").small())
                    .on_hover_text("Delete the selected request draft")
                    .clicked()
                {
                    let _removed = state.remove_selected_request();
                    state.ui.set_view(View::Editor);
                }
            });
            ui.separator();

            let selected_index = state.selected_request_index();

            if state.requests.is_empty() {
                ui.label("No requests yet");
            } else {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, req) in state.requests.iter().enumerate() {
                        let is_selected = selected_index == Some(i);
                        let display_name = compact_text(&req.display_name(), 40);
                        let method = req.method.as_str();

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!(" {method} "))
                                    .monospace()
                                    .strong()
                                    .color(method_color(method))
                                    .background_color(egui::Color32::from_black_alpha(12)),
                            );

                            if ui.selectable_label(is_selected, display_name).clicked() {
                                state.ui.select_request(i);
                                state.ui.set_view(View::Editor);
                            }
                        });
                    }
                });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Summary");
            ui.label(format!("Requests: {}", state.requests.len()));
            ui.label(format!("Responses: {}", state.responses.len()));
            if let Some(index) = selected_index {
                ui.label(format!("Selected index: {}", index));
            } else {
                ui.label("Selected index: -");
            }
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
            ui.label("• New/Dup/Del: sidebar buttons");
            ui.label("• Send: Use bottom 'Send selected request' button");
        });
}
