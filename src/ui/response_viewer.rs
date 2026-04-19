use crate::state::{AppState, ResponseSummary};
use crate::ui::theme::{self, BodyKind};
use eframe::egui;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum BodyTab {
    #[default]
    Body,
    Headers,
    Raw,
}

#[derive(Debug, Default)]
pub struct ResponseViewerState {
    tab: BodyTab,
    pretty: bool,
    wrap: bool,
    search: String,
}

impl ResponseViewerState {
    pub fn new() -> Self {
        Self {
            tab: BodyTab::Body,
            pretty: true,
            wrap: true,
            search: String::new(),
        }
    }
}

pub fn show_response_viewer(
    ui: &mut egui::Ui,
    state: &AppState,
    viewer: &mut ResponseViewerState,
    pending: bool,
) {
    let selected_response_is_for_current_request = state
        .selected_response()
        .and_then(|r| r.request_id.as_deref())
        == Some(
            crate::state::AppState::request_id_for_index(
                state.ui.selected_request.unwrap_or(usize::MAX),
            )
            .as_str(),
        );

    let response = if selected_response_is_for_current_request {
        state.selected_response()
    } else {
        state.latest_response_for_selected_request()
    };

    egui::Frame::NONE
        .fill(theme::BG)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            show_status_strip(ui, response, pending);
            ui.add_space(10.0);

            match response {
                Some(response) if response.error.is_some() => show_error_body(ui, response),
                Some(response) => {
                    show_tabs(ui, viewer);
                    ui.add_space(6.0);
                    match viewer.tab {
                        BodyTab::Body => show_body(ui, response, viewer),
                        BodyTab::Headers => show_headers_panel(ui, response),
                        BodyTab::Raw => show_raw(ui, response),
                    }
                }
                None => show_empty(ui),
            }
        });
}

fn show_status_strip(ui: &mut egui::Ui, response: Option<&ResponseSummary>, pending: bool) {
    ui.horizontal(|ui| {
        if pending {
            ui.spinner();
            ui.label(egui::RichText::new("Sending…").color(theme::TEXT_MUTED));
            return;
        }

        let Some(response) = response else {
            ui.label(egui::RichText::new("No response yet").color(theme::TEXT_MUTED));
            return;
        };

        ui.label(theme::status_badge(response.status));
        if let Some(reason) = status_reason(response.status) {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(reason)
                    .color(theme::TEXT_STRONG)
                    .strong(),
            );
        }
        ui.add_space(14.0);
        stat(ui, format_timing(response.timing_ms));
        ui.add_space(14.0);
        stat(ui, format_size(response.size_bytes));
        if let Some(ct) = &response.content_type {
            ui.add_space(14.0);
            stat(ui, ct.clone());
        }
    });
}

fn stat(ui: &mut egui::Ui, value: impl Into<String>) {
    ui.label(
        egui::RichText::new(value.into())
            .monospace()
            .color(theme::TEXT_MUTED),
    );
}

fn status_reason(status: Option<u16>) -> Option<&'static str> {
    match status? {
        200 => Some("OK"),
        201 => Some("Created"),
        202 => Some("Accepted"),
        204 => Some("No Content"),
        301 => Some("Moved Permanently"),
        302 => Some("Found"),
        304 => Some("Not Modified"),
        400 => Some("Bad Request"),
        401 => Some("Unauthorized"),
        403 => Some("Forbidden"),
        404 => Some("Not Found"),
        409 => Some("Conflict"),
        422 => Some("Unprocessable"),
        429 => Some("Too Many Requests"),
        500 => Some("Internal Error"),
        502 => Some("Bad Gateway"),
        503 => Some("Service Unavailable"),
        504 => Some("Gateway Timeout"),
        _ => None,
    }
}

fn format_timing(timing_ms: Option<u128>) -> String {
    match timing_ms {
        Some(ms) if ms < 1000 => format!("{ms} ms"),
        Some(ms) => format!("{:.2} s", ms as f64 / 1000.0),
        None => "—".to_owned(),
    }
}

fn format_size(size_bytes: Option<usize>) -> String {
    match size_bytes {
        Some(bytes) if bytes < 1024 => format!("{bytes} B"),
        Some(bytes) if bytes < 1024 * 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
        Some(bytes) => format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0)),
        None => "—".to_owned(),
    }
}

fn show_tabs(ui: &mut egui::Ui, viewer: &mut ResponseViewerState) {
    ui.horizontal(|ui| {
        tab_button(ui, viewer, BodyTab::Body, "Body");
        tab_button(ui, viewer, BodyTab::Headers, "Headers");
        tab_button(ui, viewer, BodyTab::Raw, "Raw");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if viewer.tab == BodyTab::Body {
                ui.checkbox(&mut viewer.wrap, "Wrap");
                ui.checkbox(&mut viewer.pretty, "Pretty");
            }
        });
    });
}

fn tab_button(ui: &mut egui::Ui, viewer: &mut ResponseViewerState, tab: BodyTab, label: &str) {
    let selected = viewer.tab == tab;
    let text = if selected {
        egui::RichText::new(label).color(theme::ACCENT_STRONG).strong()
    } else {
        egui::RichText::new(label).color(theme::TEXT_MUTED)
    };
    if ui.add(egui::Button::new(text).frame(false)).clicked() {
        viewer.tab = tab;
    }
}

fn show_body(ui: &mut egui::Ui, response: &ResponseSummary, viewer: &mut ResponseViewerState) {
    let Some(raw_body) = response.body_text.as_deref().or(response.preview_text.as_deref())
    else {
        ui.label(egui::RichText::new("No body").color(theme::TEXT_MUTED).italics());
        return;
    };

    let kind = theme::classify_content_type(response.content_type.as_deref());
    let display_text = if viewer.pretty && kind == BodyKind::Json {
        theme::pretty_print_json(raw_body).unwrap_or_else(|| raw_body.to_owned())
    } else {
        raw_body.to_owned()
    };

    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut viewer.search)
                .desired_width(220.0)
                .hint_text("Find…"),
        );
        if !viewer.search.is_empty() && ui.small_button("×").clicked() {
            viewer.search.clear();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(display_text.clone());
            }
        });
    });
    ui.add_space(6.0);

    let filtered = if viewer.search.trim().is_empty() {
        display_text.clone()
    } else {
        let needle = viewer.search.to_ascii_lowercase();
        display_text
            .lines()
            .filter(|line| line.to_ascii_lowercase().contains(&needle))
            .collect::<Vec<_>>()
            .join("\n")
    };

    egui::Frame::NONE
        .fill(theme::PANEL)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            let available = ui.available_size();
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .max_height(available.y.max(240.0))
                .show(ui, |ui| {
                    if kind == BodyKind::Json && viewer.pretty && viewer.search.trim().is_empty() {
                        let wrap_width = if viewer.wrap {
                            ui.available_width()
                        } else {
                            f32::INFINITY
                        };
                        let job = theme::json_layout_job(&filtered, 13.0, wrap_width);
                        ui.label(job);
                    } else {
                        let mut content = filtered;
                        ui.add(
                            egui::TextEdit::multiline(&mut content)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(20)
                                .code_editor(),
                        );
                    }
                });
        });
}

fn show_headers_panel(ui: &mut egui::Ui, response: &ResponseSummary) {
    egui::Frame::NONE
        .fill(theme::PANEL)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            if response.response_headers.is_empty() {
                ui.label(egui::RichText::new("No headers").color(theme::TEXT_MUTED).italics());
                return;
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("response_headers_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (key, value) in &response.response_headers {
                                ui.label(
                                    egui::RichText::new(key)
                                        .monospace()
                                        .color(theme::ACCENT)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new(value).monospace().color(theme::TEXT));
                                ui.end_row();
                            }
                        });
                });
        });
}

fn show_raw(ui: &mut egui::Ui, response: &ResponseSummary) {
    let mut raw = String::new();
    if let Some(status) = response.status {
        raw.push_str(&format!("HTTP {status}"));
        if let Some(reason) = status_reason(Some(status)) {
            raw.push(' ');
            raw.push_str(reason);
        }
        raw.push('\n');
    }
    for (key, value) in &response.response_headers {
        raw.push_str(&format!("{key}: {value}\n"));
    }
    raw.push('\n');
    if let Some(body) = response.body_text.as_deref().or(response.preview_text.as_deref()) {
        raw.push_str(body);
    }

    egui::Frame::NONE
        .fill(theme::PANEL)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut raw = raw;
                    ui.add(
                        egui::TextEdit::multiline(&mut raw)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .code_editor(),
                    );
                });
        });
}

fn show_error_body(ui: &mut egui::Ui, response: &ResponseSummary) {
    egui::Frame::NONE
        .fill(theme::STATUS_5XX.gamma_multiply(0.12))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Request failed")
                    .strong()
                    .color(theme::STATUS_5XX),
            );
            ui.add_space(4.0);
            if let Some(error) = &response.error {
                ui.label(egui::RichText::new(error).color(theme::TEXT));
            }
            if let Some(details) = &response.body_text {
                ui.add_space(8.0);
                let mut details = details.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut details)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(10)
                        .interactive(false),
                );
            }
        });
}

fn show_empty(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        ui.label(egui::RichText::new("Send a request to see the response").color(theme::TEXT_MUTED));
    });
}
