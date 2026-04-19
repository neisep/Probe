use crate::state::{AppState, View};
use crate::ui::response_viewer::{self, ResponseViewerState};
use crate::ui::{request_panel, response_panel, theme};
use eframe::egui;

pub fn show_center(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewer: &mut ResponseViewerState,
    pending: bool,
) {
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::BG))
        .show_inside(ui, |ui| {
            if state.ui.view == View::History {
                show_history(ui, state);
                return;
            }

            let available = ui.available_height();
            let request_height = (available * 0.42).clamp(220.0, 360.0);

            egui::Frame::NONE
                .fill(theme::PANEL)
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.set_min_height(request_height);
                    ui.set_max_height(request_height);
                    egui::ScrollArea::vertical()
                        .id_salt("request_editor_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            request_panel::show_request_editor(ui, state);
                        });
                });

            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("Response")
                        .color(theme::TEXT_MUTED)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    let scoped_count = state.responses_for_selected_request().len();
                    if ui
                        .small_button(format!("History ({scoped_count})"))
                        .clicked()
                    {
                        state.ui.set_view(View::History);
                    }
                });
            });

            response_viewer::show_response_viewer(ui, state, viewer, pending);
        });
}

fn show_history(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            let scoped_count = state.responses_for_selected_request().len();
            let request_label = state
                .selected_request()
                .map(|req| {
                    let name = req.name.trim();
                    if name.is_empty() {
                        req.url.clone()
                    } else {
                        name.to_owned()
                    }
                })
                .unwrap_or_else(|| "No request selected".to_owned());
            ui.horizontal(|ui| {
                ui.heading(format!("History · {request_label}"));
                ui.label(
                    egui::RichText::new(format!("· {scoped_count} responses"))
                        .color(theme::TEXT_MUTED)
                        .small(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Back").clicked() {
                        state.ui.set_view(View::Editor);
                    }
                });
            });
            ui.add_space(6.0);
            response_panel::show_response_history(ui, state);
        });
}
