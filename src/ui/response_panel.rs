use crate::state::AppState;
use crate::ui::theme;
use eframe::egui;

fn brief_url(url: &str, max: usize) -> String {
    if url.len() <= max {
        url.to_owned()
    } else {
        let start = &url[..max / 2];
        let end = &url[url.len() - (max / 2)..];
        format!("{start}…{end}")
    }
}

fn select_response_row(state: &mut AppState, row_index: usize) {
    state.ui.select_response(row_index);
    if let Some(request_id) = state
        .responses
        .get(row_index)
        .and_then(|response| response.request_id.as_deref())
        && let Some(request_index) = state.find_request_index_by_id(request_id)
    {
        state.ui.select_request(request_index);
    }
}

pub fn show_response_history(ui: &mut egui::Ui, state: &mut AppState) {
    let scoped_indices = state.responses_for_selected_request();

    if state.ui.selected_request.is_none() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(egui::RichText::new("Select a request to see its history").color(theme::TEXT_MUTED));
        });
        return;
    }

    if scoped_indices.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(egui::RichText::new("No responses for this request yet").color(theme::TEXT_MUTED));
        });
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row_index in scoped_indices.iter().rev().take(200).copied() {
                let Some(response) = state.responses.get(row_index).cloned() else {
                    continue;
                };
                let is_selected = state.ui.selected_response == Some(row_index);
                let bg = if is_selected {
                    theme::SELECTION.gamma_multiply(0.4)
                } else {
                    egui::Color32::TRANSPARENT
                };

                let row = egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(theme::method_badge(
                                response.request_method.as_deref().unwrap_or("REQ"),
                            ));
                            let url = response
                                .request_url
                                .as_deref()
                                .unwrap_or("<request unavailable>");
                            ui.label(
                                egui::RichText::new(brief_url(url, 60))
                                    .monospace()
                                    .color(theme::TEXT),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(theme::status_badge(response.status));
                                    ui.add_space(8.0);
                                    let timing = response
                                        .timing_ms
                                        .map(|t| format!("{} ms", t))
                                        .unwrap_or_else(|| "—".into());
                                    ui.label(
                                        egui::RichText::new(timing)
                                            .monospace()
                                            .color(theme::TEXT_MUTED)
                                            .small(),
                                    );
                                    ui.add_space(8.0);
                                    let size = response
                                        .size_bytes
                                        .map(format_bytes)
                                        .unwrap_or_else(|| "—".into());
                                    ui.label(
                                        egui::RichText::new(size)
                                            .monospace()
                                            .color(theme::TEXT_MUTED)
                                            .small(),
                                    );
                                },
                            );
                        });
                    });

                if ui
                    .interact(
                        row.response.rect,
                        ui.id().with(("history_row", row_index)),
                        egui::Sense::click(),
                    )
                    .clicked()
                {
                    select_response_row(state, row_index);
                }
                ui.add_space(2.0);
            }
        });
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
