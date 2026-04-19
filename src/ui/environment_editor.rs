use crate::state::AppState;
use eframe::egui;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Default, PartialEq, Eq)]
struct EnvironmentVariableRow {
    key: String,
    value: String,
}

#[derive(Default)]
struct EnvironmentEditorUiState {
    synced_environment: Option<usize>,
    name_buffer: String,
    variable_rows: Vec<EnvironmentVariableRow>,
}

impl EnvironmentEditorUiState {
    fn sync_from_state(&mut self, state: &AppState) {
        let active_environment = state.active_environment_index();
        if self.synced_environment == active_environment {
            return;
        }

        self.force_sync_from_state(state);
    }

    fn force_sync_from_state(&mut self, state: &AppState) {
        self.synced_environment = state.active_environment_index();
        self.name_buffer = state
            .active_environment_name()
            .unwrap_or_default()
            .to_owned();
        self.variable_rows = state
            .active_variables()
            .map(|variables| {
                variables
                    .iter()
                    .map(|(key, value)| EnvironmentVariableRow {
                        key: key.clone(),
                        value: value.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
}

static ENVIRONMENT_EDITOR_STATE: OnceLock<Mutex<EnvironmentEditorUiState>> = OnceLock::new();

fn environment_editor_state() -> &'static Mutex<EnvironmentEditorUiState> {
    ENVIRONMENT_EDITOR_STATE.get_or_init(|| Mutex::new(EnvironmentEditorUiState::default()))
}

fn with_editor_state<R>(f: impl FnOnce(&mut EnvironmentEditorUiState) -> R) -> Option<R> {
    match environment_editor_state().lock() {
        Ok(mut state) => Some(f(&mut state)),
        Err(_poisoned) => None,
    }
}

fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn next_environment_name(state: &AppState) -> String {
    let mut next_index = state.environments.len().saturating_add(1);

    loop {
        let candidate = format!("Env {next_index}");
        if state.find_environment_index(&candidate).is_none() {
            return candidate;
        }
        next_index += 1;
    }
}

fn apply_variable_rows(
    editor: &EnvironmentEditorUiState,
    state: &mut AppState,
) -> (bool, bool, usize) {
    let mut variables = BTreeMap::new();
    let mut has_pending_key = false;
    let mut has_duplicate_key = false;

    for row in &editor.variable_rows {
        let key = row.key.trim();
        if key.is_empty() {
            if !row.value.trim().is_empty() {
                has_pending_key = true;
            }
            continue;
        }

        if variables
            .insert(key.to_owned(), row.value.clone())
            .is_some()
        {
            has_duplicate_key = true;
        }
    }

    let committed_count = variables.len();

    if let Some(environment) = state.active_environment_mut() {
        environment.vars = variables;
    }

    (has_pending_key, has_duplicate_key, committed_count)
}

pub fn active_environment_label(state: &AppState) -> String {
    state
        .active_environment_name()
        .map(str::to_owned)
        .unwrap_or_else(|| "No environment".to_owned())
}

pub fn show_sidebar_section(ui: &mut egui::Ui, state: &mut AppState) {
    state.ensure_valid_environment_selection();
    ui.heading("Environment");

    let rendered = with_editor_state(|editor| {
        editor.sync_from_state(state);

        let environment_choices: Vec<(String, String)> = state
            .environments
            .iter()
            .map(|environment| {
                let label = format!(
                    "{} ({})",
                    environment.name,
                    pluralize(environment.vars.len(), "var", "vars")
                );
                (environment.name.clone(), label)
            })
            .collect();

        let selected_text = active_environment_label(state);
        let mut selected_environment = None;

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("active_environment_selector")
                .selected_text(selected_text)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (name, label) in &environment_choices {
                        let is_selected = state.active_environment_name() == Some(name.as_str());
                        if ui.selectable_label(is_selected, label).clicked() {
                            selected_environment = Some(name.clone());
                        }
                    }
                });

            if ui.small_button("New").clicked() {
                let name = next_environment_name(state);
                if state.add_environment(&name).is_ok() {
                    let _ = state.select_environment(&name);
                    editor.force_sync_from_state(state);
                }
            }

            if ui
                .add_enabled(
                    state.environments.len() > 1,
                    egui::Button::new("Del").small(),
                )
                .clicked()
            {
                if let Some(name) = state.active_environment_name().map(str::to_owned) {
                    let _removed = state.remove_environment(&name);
                    editor.force_sync_from_state(state);
                }
            }
        });

        if let Some(name) = selected_environment {
            let _ = state.select_environment(&name);
            editor.force_sync_from_state(state);
        }

        let mut rename_error = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name");
            let rename_response = ui.add(
                egui::TextEdit::singleline(&mut editor.name_buffer)
                    .desired_width(180.0)
                    .hint_text("Environment name"),
            );

            let normalized_name = editor.name_buffer.trim().to_owned();
            let active_environment = state.active_environment_index();
            let name_in_use = active_environment.is_some_and(|active_environment| {
                state
                    .environments
                    .iter()
                    .enumerate()
                    .any(|(index, environment)| {
                        index != active_environment && environment.name == normalized_name
                    })
            });

            rename_error = if normalized_name.is_empty() {
                Some("Name cannot be empty")
            } else if name_in_use {
                Some("Name already exists")
            } else {
                None
            };

            if rename_response.changed() && rename_error.is_none() {
                if let Some(environment) = state.active_environment_mut() {
                    environment.name = normalized_name.clone();
                    editor.name_buffer = normalized_name;
                }
            }
        });

        if let Some(message) = rename_error {
            ui.small(egui::RichText::new(message).color(egui::Color32::from_rgb(219, 68, 55)));
        } else {
            ui.small(pluralize(
                state.active_variables().map_or(0, BTreeMap::len),
                "variable",
                "variables",
            ));
        }
    });

    if rendered.is_none() {
        ui.small("Environment editor unavailable");
    }
}

pub fn show_request_section(ui: &mut egui::Ui, state: &mut AppState) {
    state.ensure_valid_environment_selection();

    let rendered = with_editor_state(|editor| {
        editor.sync_from_state(state);

        egui::CollapsingHeader::new("Environment")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(active_environment_label(state));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("+ Add").clicked() {
                            editor.variable_rows.push(EnvironmentVariableRow::default());
                        }
                    });
                });
                ui.small("Variables are edited per active environment.");
                ui.separator();

                let mut remove_index = None;
                for (index, variable) in editor.variable_rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut variable.key)
                                .desired_width(140.0)
                                .hint_text("KEY"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut variable.value)
                                .desired_width(260.0)
                                .hint_text("Value"),
                        );

                        if ui.small_button("✕").clicked() {
                            remove_index = Some(index);
                        }
                    });
                }

                if let Some(index) = remove_index {
                    if index < editor.variable_rows.len() {
                        editor.variable_rows.remove(index);
                    }
                }

                if editor.variable_rows.is_empty() {
                    ui.monospace("No variables. Use + Add to create one.");
                }

                let (has_pending_key, has_duplicate_key, committed_count) =
                    apply_variable_rows(editor, state);

                if has_pending_key {
                    ui.small(
                        egui::RichText::new("Rows with values need a key before they apply.")
                            .color(egui::Color32::from_rgb(244, 180, 0)),
                    );
                }

                if has_duplicate_key {
                    ui.small(
                        egui::RichText::new("Duplicate keys collapse to the last value.")
                            .color(egui::Color32::from_rgb(244, 180, 0)),
                    );
                }

                if !has_pending_key && !has_duplicate_key && committed_count > 0 {
                    ui.small(format!(
                        "{} applied to {}",
                        pluralize(committed_count, "variable", "variables"),
                        active_environment_label(state)
                    ));
                }
            });
    });

    if rendered.is_none() {
        egui::CollapsingHeader::new("Environment")
            .default_open(true)
            .show(ui, |ui| {
                ui.small("Environment editor unavailable");
            });
    }
}
