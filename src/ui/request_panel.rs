use std::time::{SystemTime, UNIX_EPOCH};

use crate::oauth::config::slugify_env_id;
use crate::oauth::{FlowKind, TokenStore, token_store};
use crate::state::request::{ApiKeyLocation, RequestAuth, RequestAuthKind};
use crate::state::{AppState, RequestDraft, RequestTab};
use crate::ui::theme;
use eframe::egui;

pub fn show_request_editor(ui: &mut egui::Ui, state: &mut AppState) {
    let mut queue_preview_for_selected = false;

    show_header_row(ui, state, &mut queue_preview_for_selected);
    ui.add_space(8.0);

    if state.selected_request_index().is_none() {
        ui.label(
            egui::RichText::new("No request selected")
                .color(theme::TEXT_MUTED)
                .italics(),
        );
        if queue_preview_for_selected
            && let Some(selected_index) = state.selected_request_index()
        {
            state.ui.queue_preview_request(selected_index);
        }
        return;
    }

    show_tab_strip(ui, state);
    ui.add_space(6.0);

    match state.ui.request_tab {
        RequestTab::Params => show_params_tab(ui, state),
        RequestTab::Auth => show_auth_tab(ui, state),
        RequestTab::Headers => show_headers_tab(ui, state),
        RequestTab::Body => show_body_tab(ui, state),
    }

    if queue_preview_for_selected
        && let Some(selected_index) = state.selected_request_index()
    {
        state.ui.queue_preview_request(selected_index);
    }
}

fn show_header_row(ui: &mut egui::Ui, state: &mut AppState, queue_preview: &mut bool) {
    let can_preview = state.selected_request_index().is_some();
    let selected_method = state
        .selected_request()
        .map(|r| r.method.clone())
        .unwrap_or_default();

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("request_method_picker")
            .selected_text(
                egui::RichText::new(&selected_method)
                    .monospace()
                    .strong()
                    .color(theme::method_color(&selected_method)),
            )
            .width(90.0)
            .show_ui(ui, |ui| {
                let methods = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];
                if let Some(req) = state.selected_request_mut() {
                    for &method in &methods {
                        ui.selectable_value(&mut req.method, method.to_owned(), method);
                    }
                }
            });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(
                    can_preview,
                    egui::Button::new(
                        egui::RichText::new("Send").strong().color(theme::TEXT_STRONG),
                    )
                    .fill(theme::ACCENT.gamma_multiply(0.55)),
                )
                .on_hover_text("Review & send (Ctrl+Enter)")
                .clicked()
            {
                *queue_preview = true;
            }

            ui.add_space(6.0);

            if let Some(req) = state.selected_request_mut() {
                let url_response = ui.add(
                    egui::TextEdit::singleline(&mut req.url)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(ui.available_width())
                        .hint_text("https://example.com/path"),
                );
                if url_response.lost_focus() {
                    let url = req.url.clone();
                    if url.contains('?') {
                        req.adopt_url_query(&url);
                    } else {
                        req.set_url(&url);
                    }
                }
            }
        });
    });

    ui.add_space(4.0);

    if let Some(req) = state.selected_request_mut() {
        ui.horizontal(|ui| {
            let name_response = ui.add(
                egui::TextEdit::singleline(&mut req.name)
                    .desired_width(240.0)
                    .hint_text("Request name"),
            );
            if name_response.lost_focus() {
                let name = req.name.clone();
                req.set_request_name(&name);
            }

            ui.label(egui::RichText::new("·").color(theme::TEXT_MUTED));

            let folder_response = ui.add(
                egui::TextEdit::singleline(&mut req.folder)
                    .desired_width(200.0)
                    .hint_text("Folder (optional)"),
            );
            if folder_response.lost_focus() {
                let folder = req.folder.clone();
                req.set_folder_path(&folder);
            }
        });
    }
}

fn show_tab_strip(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        for tab in RequestTab::ALL {
            let selected = state.ui.request_tab == tab;
            let text = if selected {
                egui::RichText::new(tab.label())
                    .color(theme::ACCENT_STRONG)
                    .strong()
            } else {
                egui::RichText::new(tab.label()).color(theme::TEXT_MUTED)
            };
            let button = ui.add(egui::Button::new(text).frame(false));
            if button.clicked() {
                state.ui.request_tab = tab;
            }
            if let Some(count) = tab_count_hint(state, tab) {
                ui.label(
                    egui::RichText::new(format!("{count}"))
                        .color(theme::TEXT_MUTED)
                        .small(),
                );
            }
            ui.add_space(10.0);
        }
    });
}

fn tab_count_hint(state: &AppState, tab: RequestTab) -> Option<usize> {
    let req = state.selected_request()?;
    match tab {
        RequestTab::Params => {
            let n = req
                .query_params
                .iter()
                .filter(|(k, _)| !k.trim().is_empty())
                .count();
            (n > 0).then_some(n)
        }
        RequestTab::Headers => {
            let n = req.headers.iter().filter(|(k, _)| !k.trim().is_empty()).count();
            (n > 0).then_some(n)
        }
        RequestTab::Body => req
            .body
            .as_deref()
            .filter(|body| !body.is_empty())
            .map(|body| body.len()),
        RequestTab::Auth => None,
    }
}

fn show_params_tab(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(req) = state.selected_request_mut() else {
        return;
    };
    show_kv_editor(
        ui,
        &mut req.query_params,
        "param_name",
        "param_value",
        "No query parameters",
    );
}

fn show_headers_tab(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(req) = state.selected_request_mut() else {
        return;
    };
    show_kv_editor(
        ui,
        &mut req.headers,
        "header_name",
        "header_value",
        "No headers",
    );
}

fn show_kv_editor(
    ui: &mut egui::Ui,
    rows: &mut Vec<(String, String)>,
    name_hint: &str,
    value_hint: &str,
    empty_text: &str,
) {
    let mut remove_idx: Option<usize> = None;
    for i in 0..rows.len() {
        let (key, value) = &mut rows[i];
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(key)
                    .desired_width(160.0)
                    .hint_text(name_hint),
            );
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(300.0)
                    .hint_text(value_hint),
            );
            if ui.small_button("×").clicked() {
                remove_idx = Some(i);
            }
        });
    }
    if let Some(i) = remove_idx
        && i < rows.len()
    {
        rows.remove(i);
    }

    if rows.is_empty() {
        ui.label(
            egui::RichText::new(empty_text)
                .color(theme::TEXT_MUTED)
                .italics(),
        );
    }

    ui.add_space(4.0);
    if ui.small_button("+ Add row").clicked() {
        rows.push((String::new(), String::new()));
    }
}

fn show_auth_tab(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(req) = state.selected_request_mut() else {
        return;
    };

    let mut auth_kind = req.auth.kind();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Type").color(theme::TEXT_MUTED).small());
        egui::ComboBox::from_id_salt("request_auth_mode")
            .selected_text(auth_kind.label())
            .width(180.0)
            .show_ui(ui, |ui| {
                for kind in RequestAuthKind::ALL {
                    ui.selectable_value(&mut auth_kind, kind, kind.label());
                }
            });
    });

    if auth_kind != req.auth.kind() {
        req.auth = RequestAuth::from_kind(auth_kind);
    }

    ui.add_space(6.0);

    match &mut req.auth {
        RequestAuth::None => {
            ui.label(
                egui::RichText::new("No authentication")
                    .color(theme::TEXT_MUTED)
                    .italics(),
            );
        }
        RequestAuth::Bearer { token } => {
            ui.add(
                egui::TextEdit::singleline(token)
                    .desired_width(360.0)
                    .hint_text("Bearer token or {{API_TOKEN}}"),
            );
        }
        RequestAuth::Basic { username, password } => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(username)
                        .desired_width(160.0)
                        .hint_text("Username"),
                );
                ui.add(
                    egui::TextEdit::singleline(password)
                        .desired_width(200.0)
                        .password(true)
                        .hint_text("Password"),
                );
            });
        }
        RequestAuth::ApiKey {
            location,
            name,
            value,
        } => {
            ui.horizontal(|ui| {
                let mut selected_location = *location;
                egui::ComboBox::from_id_salt("request_api_key_location")
                    .selected_text(selected_location.label())
                    .width(120.0)
                    .show_ui(ui, |ui| {
                        for candidate in ApiKeyLocation::ALL {
                            ui.selectable_value(
                                &mut selected_location,
                                candidate,
                                candidate.label(),
                            );
                        }
                    });
                *location = selected_location;

                ui.add(
                    egui::TextEdit::singleline(name)
                        .desired_width(160.0)
                        .hint_text(match selected_location {
                            ApiKeyLocation::Header => "Header name",
                            ApiKeyLocation::Query => "Param name",
                        }),
                );
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(200.0)
                        .hint_text("Value or {{API_KEY}}"),
                );
            });
        }
    }

    show_oauth_hint(ui, state);
}

fn show_oauth_hint(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(4.0);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let active_token = state.active_environment_name().and_then(|env_name| {
        let env_id = slugify_env_id(env_name);
        let store = token_store();
        for flow in [
            FlowKind::AuthCodePkce,
            FlowKind::ClientCredentials,
            FlowKind::DeviceCode,
        ] {
            if let Ok(Some(token)) = store.get(&env_id, flow.as_str()) {
                if !token.is_expired(now) {
                    return Some(token);
                }
            }
        }
        None
    });

    let Some(req) = state.selected_request_mut() else {
        return;
    };

    ui.horizontal(|ui| {
        ui.checkbox(&mut req.attach_oauth, "Attach OAuth2 token");
        if req.attach_oauth {
            if let Some(token) = &active_token {
                let seconds = token.expires_at.saturating_sub(now);
                let label = if seconds < 60 {
                    format!("(expires in {seconds}s)")
                } else if seconds < 3600 {
                    format!("(expires in {}m)", seconds / 60)
                } else {
                    format!("(expires in {}h {}m)", seconds / 3600, (seconds % 3600) / 60)
                };
                ui.small(egui::RichText::new("●").color(egui::Color32::from_rgb(52, 168, 83)));
                ui.small(egui::RichText::new(label).color(egui::Color32::from_rgb(52, 168, 83)));
            } else {
                ui.small(
                    egui::RichText::new("no token — configure in Settings → Auth")
                        .color(theme::TEXT_MUTED),
                );
            }
        }
    });
}

fn show_body_tab(ui: &mut egui::Ui, state: &mut AppState) {
    let Some(req) = state.selected_request_mut() else {
        return;
    };

    let mut body_buf = req.body.clone().unwrap_or_default();
    let edit = ui.add(
        egui::TextEdit::multiline(&mut body_buf)
            .font(egui::TextStyle::Monospace)
            .desired_rows(10)
            .desired_width(f32::INFINITY)
            .hint_text("Request body (JSON, text, …)"),
    );

    if edit.changed() {
        let trimmed = body_buf.trim();
        req.body = if trimmed.is_empty() {
            None
        } else {
            Some(body_buf.clone())
        };
    }

    let hint = if body_buf.trim_start().starts_with('{') || body_buf.trim_start().starts_with('[') {
        Some("JSON")
    } else if body_buf.contains('=') && body_buf.contains('&') {
        Some("form")
    } else {
        None
    };

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} bytes · {} lines", body_buf.len(), body_buf.lines().count()))
                .color(theme::TEXT_MUTED)
                .small(),
        );
        if let Some(h) = hint {
            ui.label(
                egui::RichText::new(h)
                    .color(theme::TEXT_MUTED)
                    .monospace()
                    .small(),
            );
        }
    });

    // Keep unused import happy (RequestDraft used via mut ref).
    let _: &RequestDraft = &*req;
}
