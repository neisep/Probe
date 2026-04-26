use eframe::egui;

#[derive(Debug, Clone)]
pub struct RequestPreviewIssue {
    pub summary: String,
    pub target: String,
    pub placeholder: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequestPreviewData {
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub query_params: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub issue: Option<RequestPreviewIssue>,
    pub can_send: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPreviewAction {
    None,
    Close,
    Send,
}

fn show_pairs(ui: &mut egui::Ui, pairs: &[(String, String)], empty_text: &str, id_source: &str) {
    if pairs.is_empty() {
        ui.small(empty_text);
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(id_source)
        .max_height(140.0)
        .show(ui, |ui| {
            egui::Grid::new(format!("{id_source}_grid"))
                .num_columns(2)
                .spacing([12.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    for (name, value) in pairs {
                        ui.monospace(name);
                        ui.label(value);
                        ui.end_row();
                    }
                });
        });
}

pub fn show_request_preview(
    ctx: &egui::Context,
    preview: &RequestPreviewData,
) -> RequestPreviewAction {
    let mut action = RequestPreviewAction::None;

    egui::Window::new("Request preview")
        .collapsible(false)
        .resizable(true)
        .default_width(720.0)
        .default_height(540.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.heading(&preview.request_name);
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.monospace(&preview.method);
                ui.label(&preview.url);
            });

            if let Some(issue) = &preview.issue {
                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 60, 60),
                        egui::RichText::new("Request needs attention").strong(),
                    );
                    ui.label(&issue.summary);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new("Target").strong());
                        ui.monospace(&issue.target);
                    });
                    if let Some(placeholder) = issue.placeholder.as_deref() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(egui::RichText::new("Placeholder").strong());
                            ui.monospace(placeholder);
                        });
                    }
                    if let Some(details) = issue.details.as_deref() {
                        ui.label(details);
                    }
                });
            }

            ui.add_space(8.0);
            ui.collapsing(
                format!("Query params ({})", preview.query_params.len()),
                |ui| {
                    show_pairs(
                        ui,
                        &preview.query_params,
                        "No query parameters.",
                        "preview_query_params",
                    );
                },
            );
            ui.collapsing(format!("Headers ({})", preview.headers.len()), |ui| {
                show_pairs(
                    ui,
                    &preview.headers,
                    "No request headers.",
                    "preview_headers",
                );
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if preview.can_send && ui.button("Send request").clicked() {
                    action = RequestPreviewAction::Send;
                }

                let close_label = if preview.can_send {
                    "Back to editor"
                } else {
                    "Close"
                };
                if ui.button(close_label).clicked() {
                    action = RequestPreviewAction::Close;
                }
            });
        });

    action
}
