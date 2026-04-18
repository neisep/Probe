use crate::state::{AppState, View};
use eframe::egui;

fn short_snippet(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}… (truncated)", &s[..max])
    }
}

pub fn show_inspector(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::right("inspector")
        .resizable(true)
        .default_size(320.0)
        .show_inside(ui, |ui| {
            ui.heading("Inspector");
            ui.separator();

            match state.ui.view {
                View::Editor => {
                    ui.label("Context: Request editor");
                    ui.add_space(6.0);
                    if let Some(req) = state.selected_request() {
                        ui.collapsing("Request Summary", |ui| {
                            ui.horizontal(|ui| {
                                ui.monospace(&req.method);
                                ui.label(&req.url);
                            });
                            ui.separator();
                            ui.label(format!("Headers: {}", req.headers.len()));
                            if !req.headers.is_empty() {
                                for (k, v) in req.headers.iter().take(8) {
                                    ui.horizontal(|ui| {
                                        ui.monospace(k);
                                        ui.label(v);
                                    });
                                }
                            }
                            ui.add_space(6.0);
                            ui.collapsing("Body preview", |ui| {
                                let body = req.body.as_deref().unwrap_or("");
                                let preview = short_snippet(body, 512);
                                let mut preview_owned = preview;
                                ui.add(
                                    egui::TextEdit::multiline(&mut preview_owned)
                                        .desired_rows(6)
                                        .interactive(false),
                                );
                            });
                        });
                    } else {
                        ui.label("No request selected");
                    }

                    ui.add_space(8.0);
                    ui.collapsing("Latest Response", |ui| {
                        if let Some(resp) = state.latest_response() {
                            ui.horizontal(|ui| {
                                ui.label("Status:");
                                ui.monospace(
                                    resp.status
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| "pending".into()),
                                );
                            });
                            ui.label(format!(
                                "Timing: {} ms",
                                resp.timing_ms
                                    .map(|t| t.to_string())
                                    .unwrap_or_else(|| "-".into())
                            ));
                            ui.label(format!("Size: {} bytes", resp.size_bytes.unwrap_or(0)));
                            ui.label(format!(
                                "Headers: {}",
                                resp.header_count
                                    .map(|header_count| header_count.to_string())
                                    .unwrap_or_else(|| "-".into())
                            ));
                            ui.label(format!(
                                "Content-Type: {}",
                                resp.content_type.as_deref().unwrap_or("-")
                            ));

                            if let Some(err) = &resp.error {
                                ui.colored_label(egui::Color32::YELLOW, "Error present");
                                ui.label(err);
                            }

                            ui.collapsing("Response details", |ui| {
                                if let Some(preview_text) = &resp.preview_text {
                                    let preview = short_snippet(preview_text, 512);
                                    let mut preview_owned = preview;
                                    ui.add(
                                        egui::TextEdit::multiline(&mut preview_owned)
                                            .desired_rows(8)
                                            .interactive(false),
                                    );
                                } else {
                                    ui.label("Detailed response preview not available.");
                                }
                            });
                        } else {
                            ui.label("No response available");
                        }
                    });
                }
                View::History => {
                    ui.label("Context: Response history");
                    ui.add_space(6.0);
                    if let Some(selected) = state.ui.selected_response {
                        ui.collapsing("Selected Pair", |ui| {
                            if let Some(req) = state.requests.get(selected) {
                                ui.horizontal(|ui| {
                                    ui.monospace(&req.method);
                                    ui.label(&req.url);
                                });
                            }
                            if let Some(resp) = state.selected_response() {
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label("Status:");
                                    ui.monospace(
                                        resp.status
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| "pending".into()),
                                    );
                                });
                                ui.label(format!(
                                    "Timing: {} ms",
                                    resp.timing_ms
                                        .map(|t| t.to_string())
                                        .unwrap_or_else(|| "-".into())
                                ));
                                ui.label(format!("Size: {} bytes", resp.size_bytes.unwrap_or(0)));
                                ui.label(format!(
                                    "Headers: {}",
                                    resp.header_count
                                        .map(|header_count| header_count.to_string())
                                        .unwrap_or_else(|| "-".into())
                                ));
                                ui.label(format!(
                                    "Content-Type: {}",
                                    resp.content_type.as_deref().unwrap_or("-")
                                ));
                                if let Some(err) = &resp.error {
                                    ui.colored_label(egui::Color32::YELLOW, "Error:");
                                    ui.label(err);
                                }

                                ui.collapsing("Response details", |ui| {
                                    if let Some(preview_text) = &resp.preview_text {
                                        let preview = short_snippet(preview_text, 512);
                                        let mut preview_owned = preview;
                                        ui.add(
                                            egui::TextEdit::multiline(&mut preview_owned)
                                                .desired_rows(8)
                                                .interactive(false),
                                        );
                                    } else {
                                        ui.label("Detailed response preview not available.");
                                    }
                                });
                            } else {
                                ui.label("No response for selected item");
                            }
                        });
                    } else {
                        ui.label("No history item selected. Click a row to inspect.");
                    }
                }
            }
        });
}
