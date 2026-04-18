use crate::state::AppState;
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

fn request_method_color(method: &str) -> egui::Color32 {
    match method {
        "GET" => egui::Color32::from_rgb(8, 120, 8),
        "POST" => egui::Color32::from_rgb(8, 40, 160),
        "PUT" => egui::Color32::from_rgb(160, 120, 8),
        "DELETE" => egui::Color32::from_rgb(160, 16, 16),
        _ => egui::Color32::from_gray(120),
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
    ui.group(|ui| {
        ui.heading("Response History");
        ui.separator();

        if state.responses.is_empty() {
            ui.label("No responses yet");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for row_index in (0..state.responses.len()).rev().take(200) {
                    let Some(response) = state.responses.get(row_index).cloned() else {
                        continue;
                    };
                    let is_selected = state.ui.selected_response == Some(row_index);
                    let bg_fill = if is_selected {
                        egui::Color32::from_rgb(230, 245, 255)
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    egui::Frame::NONE.fill(bg_fill).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let request_method =
                                response.request_method.as_deref().unwrap_or("REQ");
                            let request_url = response
                                .request_url
                                .as_deref()
                                .unwrap_or("<request unavailable>");

                            ui.monospace(format!("#{:<3}", row_index + 1));

                            if ui
                                .add_sized(
                                    [68.0, 22.0],
                                    egui::Button::new(
                                        egui::RichText::new(request_method)
                                            .monospace()
                                            .color(request_method_color(request_method)),
                                    ),
                                )
                                .clicked()
                            {
                                select_response_row(state, row_index);
                            }

                            ui.label(brief_url(request_url, 60));

                            ui.separator();

                            let status_text = response
                                .status
                                .map(|status| status.to_string())
                                .unwrap_or_else(|| "pending".into());
                            let status_color = match response.status {
                                Some(status) if (200..300).contains(&status) => {
                                    egui::Color32::from_rgb(40, 160, 40)
                                }
                                Some(status) if (300..400).contains(&status) => {
                                    egui::Color32::from_rgb(200, 160, 32)
                                }
                                Some(_) => egui::Color32::from_rgb(190, 60, 60),
                                None => egui::Color32::from_gray(160),
                            };

                            ui.label(
                                egui::RichText::new(status_text)
                                    .color(status_color)
                                    .monospace(),
                            );

                            ui.separator();

                            let timing = response
                                .timing_ms
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "-".into());
                            ui.label(egui::RichText::new(format!("⏱ {} ms", timing)).monospace());
                            let size = response
                                .size_bytes
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "-".into());
                            ui.label(egui::RichText::new(format!("{} bytes", size)).monospace());

                            if let Some(content_type) = &response.content_type {
                                ui.separator();
                                ui.small(content_type);
                            }

                            if let Some(error) = &response.error {
                                ui.add(egui::Label::new(
                                    egui::RichText::new("⚠").color(egui::Color32::YELLOW),
                                ))
                                .on_hover_text(error);
                            }

                            // make the whole row selectable by clicking its area
                            if ui
                                .interact(
                                    ui.max_rect(),
                                    ui.id().with(row_index),
                                    egui::Sense::click(),
                                )
                                .clicked()
                            {
                                select_response_row(state, row_index);
                            }
                        });
                    });

                    ui.add_space(6.0);
                }
            });
        }
    });
}
