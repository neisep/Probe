use crate::state::{AppState, RequestDraft, ResponseSummary, View};
use eframe::egui;

fn short_snippet(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_owned()
    } else {
        let preview: String = s.chars().take(max_chars).collect();
        format!("{preview}… (truncated)")
    }
}

fn status_text(response: &ResponseSummary) -> String {
    response
        .status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "pending".to_owned())
}

fn status_color(response: &ResponseSummary) -> egui::Color32 {
    match response.status {
        Some(status) if (200..300).contains(&status) => egui::Color32::from_rgb(40, 160, 40),
        Some(status) if (300..400).contains(&status) => egui::Color32::from_rgb(200, 160, 32),
        Some(_) => egui::Color32::from_rgb(190, 60, 60),
        None => egui::Color32::from_gray(160),
    }
}

fn inspected_response<'a>(state: &'a AppState) -> Option<(&'a ResponseSummary, &'static str)> {
    state
        .selected_response()
        .map(|response| (response, "Selected response"))
        .or_else(|| {
            state
                .latest_response()
                .map(|response| (response, "Latest response"))
        })
}

fn request_for_response<'a>(
    state: &'a AppState,
    response: &'a ResponseSummary,
) -> Option<&'a RequestDraft> {
    response
        .request_id
        .as_deref()
        .and_then(|request_id| state.find_request_index_by_id(request_id))
        .and_then(|request_index| state.requests.get(request_index))
}

fn request_method_text(
    response: Option<&ResponseSummary>,
    request: Option<&RequestDraft>,
) -> String {
    response
        .and_then(|response| response.request_method.clone())
        .or_else(|| request.map(|request| request.method.clone()))
        .unwrap_or_else(|| "REQUEST".to_owned())
}

fn request_url_text(response: Option<&ResponseSummary>, request: Option<&RequestDraft>) -> String {
    response
        .and_then(|response| response.request_url.clone())
        .or_else(|| request.map(|request| request.url.clone()))
        .unwrap_or_else(|| "Unavailable".to_owned())
}

fn show_key_value_row(ui: &mut egui::Ui, key: &str, value: impl Into<egui::WidgetText>) {
    ui.label(egui::RichText::new(key).strong());
    ui.label(value);
    ui.end_row();
}

fn show_headers(
    ui: &mut egui::Ui,
    headers: &[(String, String)],
    id_source: &str,
    empty_text: &str,
) {
    if headers.is_empty() {
        ui.small(empty_text);
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(id_source)
        .max_height(140.0)
        .show(ui, |ui| {
            for (key, value) in headers {
                ui.horizontal_wrapped(|ui| {
                    ui.monospace(key);
                    ui.label(value);
                });
            }
        });
}

fn show_read_only_preview(
    ui: &mut egui::Ui,
    text: Option<&str>,
    empty_text: &str,
    rows: usize,
    max_chars: usize,
) {
    if let Some(text) = text {
        let preview = short_snippet(text, max_chars);
        let mut preview_owned = preview;
        ui.add(
            egui::TextEdit::multiline(&mut preview_owned)
                .desired_rows(rows)
                .interactive(false),
        );
    } else {
        ui.small(empty_text);
    }
}

fn show_request_details(
    ui: &mut egui::Ui,
    response: Option<&ResponseSummary>,
    request: Option<&RequestDraft>,
    request_label: &str,
    id_prefix: &str,
) {
    ui.group(|ui| {
        let method_text = request_method_text(response, request);
        let url_text = request_url_text(response, request);

        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(request_label).strong());
            ui.monospace(method_text);
            ui.label(url_text);
        });

        let request_headers = response
            .filter(|response| !response.request_headers.is_empty())
            .map(|response| response.request_headers.as_slice())
            .or_else(|| request.map(|request| request.headers.as_slice()))
            .unwrap_or(&[]);
        let request_header_title = format!("Request headers ({})", request_headers.len());
        ui.collapsing(request_header_title, |ui| {
            show_headers(
                ui,
                request_headers,
                &format!("{id_prefix}_request_headers"),
                "No request headers.",
            );
        });

        if let Some(request) = request {
            let has_body = request.body.as_deref().is_some_and(|body| !body.is_empty());
            let body_title = if has_body {
                "Request body preview"
            } else {
                "Request body"
            };
            ui.collapsing(body_title, |ui| {
                show_read_only_preview(ui, request.body.as_deref(), "No request body.", 6, 700);
            });
        } else {
            ui.collapsing("Request body", |ui| {
                ui.small("Request body is unavailable.");
            });
        }
    });
}

fn show_response_details(
    ui: &mut egui::Ui,
    response: &ResponseSummary,
    response_label: &str,
    id_prefix: &str,
) {
    ui.group(|ui| {
        ui.label(egui::RichText::new(response_label).strong());

        egui::Grid::new(format!("{id_prefix}_response_summary"))
            .num_columns(2)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                show_key_value_row(
                    ui,
                    "Status",
                    egui::RichText::new(status_text(response))
                        .monospace()
                        .color(status_color(response)),
                );
                show_key_value_row(
                    ui,
                    "Timing",
                    egui::RichText::new(format!(
                        "{} ms",
                        response
                            .timing_ms
                            .map(|timing_ms| timing_ms.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ))
                    .monospace(),
                );
                show_key_value_row(
                    ui,
                    "Size",
                    egui::RichText::new(format!(
                        "{} bytes",
                        response
                            .size_bytes
                            .map(|size_bytes| size_bytes.to_string())
                            .unwrap_or_else(|| "-".to_owned())
                    ))
                    .monospace(),
                );
                show_key_value_row(
                    ui,
                    "Headers",
                    egui::RichText::new(
                        response
                            .header_count
                            .map(|header_count| header_count.to_string())
                            .unwrap_or_else(|| "-".to_owned()),
                    )
                    .monospace(),
                );
                show_key_value_row(
                    ui,
                    "Content-Type",
                    response.content_type.as_deref().unwrap_or("-"),
                );
            });

        if let Some(error) = &response.error {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::YELLOW, "Error");
            ui.label(error);
        }

        let header_title = match response.header_count {
            Some(header_count) => format!("Response headers ({header_count})"),
            None => "Response headers".to_owned(),
        };
        ui.collapsing(header_title, |ui| {
            show_headers(
                ui,
                &response.response_headers,
                &format!("{id_prefix}_response_headers"),
                "Detailed response headers are not available.",
            );
        });

        ui.collapsing("Response body preview", |ui| {
            show_read_only_preview(
                ui,
                response.preview_text.as_deref(),
                "Detailed response preview not available.",
                8,
                900,
            );
        });
    });
}

pub fn show_inspector(ui: &mut egui::Ui, state: &mut AppState) {
    egui::Panel::right("inspector")
        .resizable(true)
        .default_size(320.0)
        .show_inside(ui, |ui| {
            ui.heading("Inspector");
            ui.separator();

            let context = match state.ui.view {
                View::Editor => "Request editor",
                View::History => "Response history",
            };
            ui.label(format!("Context: {context}"));
            ui.add_space(6.0);

            let inspected = inspected_response(state);
            let request = inspected
                .and_then(|(response, _)| request_for_response(state, response))
                .or_else(|| state.selected_request());

            show_request_details(
                ui,
                inspected.map(|(response, _)| response),
                request,
                "Request",
                "inspector",
            );
            ui.add_space(8.0);

            if let Some((response, response_label)) = inspected {
                show_response_details(ui, response, response_label, "inspector");
            } else {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Response").strong());
                    ui.small("No response available.");
                });
            }
        });
}
