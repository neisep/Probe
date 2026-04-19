use crate::state::AppState;
use crate::ui::left_sidebar::environment_editor;
use eframe::egui;

pub fn show_request_editor(ui: &mut egui::Ui, state: &mut AppState) {
    ui.group(|ui| {
        let selected_method = state
            .selected_request()
            .map(|request| request.method.clone())
            .unwrap_or_default();
        let current_folder = state
            .selected_request()
            .and_then(|request| request.folder_path())
            .unwrap_or_default()
            .to_owned();
        let existing_folders = state.folder_paths();
        let mut pending_folder_selection = None;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Request");
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    if let Some(req) = state.selected_request_mut() {
                        let name_response = ui.add(
                            egui::TextEdit::singleline(&mut req.name)
                                .desired_width(220.0)
                                .hint_text("Request name"),
                        );
                        if name_response.lost_focus() {
                            let name = req.name.clone();
                            req.set_request_name(&name);
                        }
                    } else {
                        let mut request_name = String::new();
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut request_name)
                                .desired_width(220.0)
                                .hint_text("Request name"),
                        );
                    }
                });

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label("Folder");
                    if let Some(req) = state.selected_request_mut() {
                        let folder_response = ui.add(
                            egui::TextEdit::singleline(&mut req.folder)
                                .desired_width(220.0)
                                .hint_text("Collections/API"),
                        );
                        if folder_response.lost_focus() {
                            let folder = req.folder.clone();
                            req.set_folder_path(&folder);
                        }
                    } else {
                        let mut request_folder = String::new();
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut request_folder)
                                .desired_width(220.0)
                                .hint_text("Collections/API"),
                        );
                    }

                    egui::ComboBox::from_id_salt("request_folder_picker")
                        .selected_text(if current_folder.is_empty() {
                            "Move to..."
                        } else {
                            "Pick existing"
                        })
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for folder in &existing_folders {
                                let is_selected = folder == &current_folder;
                                if ui.selectable_label(is_selected, folder).clicked() {
                                    pending_folder_selection = Some(folder.clone());
                                }
                            }
                        });

                    if ui
                        .add_enabled(
                            !current_folder.is_empty(),
                            egui::Button::new("Clear").small(),
                        )
                        .clicked()
                    {
                        pending_folder_selection = Some(String::new());
                    }
                });

                if let Some(folder) = pending_folder_selection {
                    if let Some(req) = state.selected_request_mut() {
                        req.set_folder_path(&folder);
                    }
                }

                if current_folder.is_empty() {
                    ui.small("Stored at the workspace root. Use / to create nested folders.");
                } else {
                    ui.small(format!("Current folder: {current_folder}"));
                }

                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("")
                        .selected_text(selected_method)
                        .show_ui(ui, |ui| {
                            let methods =
                                ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];
                            if let Some(req) = state.selected_request_mut() {
                                for &method in &methods {
                                    ui.selectable_value(&mut req.method, method.to_owned(), method);
                                }
                            }
                        });

                    if let Some(req) = state.selected_request_mut() {
                        ui.add(
                            egui::TextEdit::singleline(&mut req.url)
                                .hint_text("https://example.com/path"),
                        );
                    } else {
                        let mut dummy = String::new();
                        ui.add_enabled(false, egui::TextEdit::singleline(&mut dummy));
                    }
                });
            });
        });

        ui.separator();

        environment_editor::show_request_section(ui, state);
        ui.add_space(6.0);

        if let Some(req) = state.selected_request_mut() {
            ui.collapsing("Headers", |ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.add_space(8.0);
                        ui.label("Value");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("+ Add").clicked() {
                                req.headers.push((String::new(), String::new()));
                            }
                        });
                    });

                    ui.separator();

                    // Editable header rows
                    let mut remove_idx: Option<usize> = None;
                    for i in 0..req.headers.len() {
                        // Use indexing to get mutable refs safely
                        let (k, v) = &mut req.headers[i];
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(k)
                                    .desired_width(120.0)
                                    .hint_text("Header name"),
                            );
                            ui.add(
                                egui::TextEdit::singleline(v)
                                    .desired_width(240.0)
                                    .hint_text("Header value"),
                            );
                            if ui.small_button("✕").clicked() {
                                remove_idx = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove_idx {
                        if i < req.headers.len() {
                            req.headers.remove(i);
                        }
                    }

                    if req.headers.is_empty() {
                        ui.monospace("No headers. Use + Add to create one.");
                    }
                });
            });

            ui.add_space(6.0);

            ui.collapsing("Body", |ui| {
                ui.vertical(|ui| {
                    // Provide a safe editable buffer for Option<String>
                    let mut body_buf = match &req.body {
                        Some(b) => b.clone(),
                        None => String::new(),
                    };

                    let edit = ui.add(
                        egui::TextEdit::multiline(&mut body_buf)
                            .desired_rows(8)
                            .hint_text("Optional request body (JSON, text, etc.)"),
                    );

                    // Body stats and lightweight hints
                    let bytes = body_buf.as_bytes().len();
                    let lines = body_buf.lines().count();
                    let mut hint = "".to_string();
                    if body_buf.trim_start().starts_with('{')
                        || body_buf.trim_start().starts_with('[')
                    {
                        hint = "Looks like JSON".to_string();
                    } else if body_buf.contains("=") && body_buf.contains("&") {
                        hint = "Looks like form data".to_string();
                    }

                    ui.horizontal(|ui| {
                        ui.label(format!("{} bytes", bytes));
                        ui.add_space(8.0);
                        ui.label(format!("{} lines", lines));
                        if !hint.is_empty() {
                            ui.add_space(8.0);
                            ui.monospace(hint);
                        }
                    });

                    if edit.changed() {
                        let trimmed = body_buf.trim();
                        if trimmed.is_empty() {
                            req.body = None;
                        } else {
                            req.body = Some(body_buf);
                        }
                    }
                });
            });
        } else {
            ui.label("No request selected");
        }
    });
}
