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

pub fn show_response_history(ui: &mut egui::Ui, state: &mut AppState) {
    ui.group(|ui| {
        ui.heading("Response History");
        ui.separator();

        if state.responses.is_empty() {
            ui.label("No responses yet");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (index, response) in state.responses.iter().enumerate().rev().take(200) {
                    let row_index = index;
                    let is_selected = state.ui.selected_response == Some(row_index);
                    let bg_fill = if is_selected {
                        egui::Color32::from_rgb(230, 245, 255)
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    egui::Frame::NONE.fill(bg_fill).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.monospace(format!("#{:<3}", row_index + 1));

                            if let Some(request) = state.requests.get(row_index) {
                                let method_color = match request.method.as_str() {
                                    "GET" => egui::Color32::from_rgb(8, 120, 8),
                                    "POST" => egui::Color32::from_rgb(8, 40, 160),
                                    "PUT" => egui::Color32::from_rgb(160, 120, 8),
                                    "DELETE" => egui::Color32::from_rgb(160, 16, 16),
                                    _ => egui::Color32::from_gray(120),
                                };

                                if ui
                                    .add_sized(
                                        [68.0, 22.0],
                                        egui::Button::new(
                                            egui::RichText::new(&request.method)
                                                .monospace()
                                                .color(method_color),
                                        ),
                                    )
                                    .clicked()
                                {
                                    state.ui.select_request(row_index);
                                    state.ui.select_response(row_index);
                                }

                                ui.label(brief_url(&request.url, 60));
                            } else {
                                ui.label("<no request>");
                            }

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
                                state.ui.select_request(row_index);
                                state.ui.select_response(row_index);
                            }
                        });
                    });

                    ui.add_space(6.0);
                }
            });
        }
    });
}
