use crate::state::AppState;
use crate::ui::intent::PanelIntent;
use crate::ui::panel_state::PanelUiState;
use eframe::egui;
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq, Eq)]
struct EnvironmentVariableRow {
    key: String,
    value: String,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum EnvTab {
    #[default]
    Variables,
    Auth,
}

/// Transient state for the environment editor panel. Held on `PanelUiState`
/// (owned by `ProbeApp`) and passed in by reference each frame.
#[derive(Default)]
pub struct EnvironmentEditorUiState {
    synced_environment: Option<usize>,
    name_buffer: String,
    variable_rows: Vec<EnvironmentVariableRow>,
    active_tab: EnvTab,
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

fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// Build the variable map from the editor rows. Returns the committed map
/// plus diagnostic flags for the UI to surface.
fn collect_variable_rows(
    editor: &EnvironmentEditorUiState,
) -> (BTreeMap<String, String>, bool, bool) {
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

    (variables, has_pending_key, has_duplicate_key)
}

pub fn active_environment_label(state: &AppState) -> String {
    state
        .active_environment_name()
        .map(str::to_owned)
        .unwrap_or_else(|| "No environment".to_owned())
}

pub fn show_sidebar_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    panels: &mut PanelUiState,
    intents: &mut Vec<PanelIntent>,
) {
    state.ensure_valid_environment_selection();
    ui.heading("Environment");

    {
        let editor = &mut panels.environment_editor;
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
                intents.push(PanelIntent::AddAutoNamedEnvironment);
            }

            if ui
                .add_enabled(
                    state.environments.len() > 1,
                    egui::Button::new("Del").small(),
                )
                .clicked()
                && let Some(name) = state.active_environment_name().map(str::to_owned)
            {
                intents.push(PanelIntent::RemoveEnvironment { name });
            }
        });

        if let Some(name) = selected_environment {
            intents.push(PanelIntent::SelectEnvironment { name });
        }

        let mut rename_error = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name");
            let original_name = editor.name_buffer.clone();
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

            if rename_response.changed()
                && rename_error.is_none()
                && editor.name_buffer != original_name
            {
                intents.push(PanelIntent::RenameActiveEnvironment {
                    new_name: normalized_name,
                });
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
    }
}

pub fn show_request_section(
    ui: &mut egui::Ui,
    state: &mut AppState,
    panels: &mut PanelUiState,
    intents: &mut Vec<PanelIntent>,
) {
    state.ensure_valid_environment_selection();

    {
        // Disjoint borrows of the two transient panel states so the Auth tab
        // can mutate the OAuth panel while the editor is also borrowed.
        let PanelUiState {
            environment_editor: editor,
            oauth,
        } = panels;
        editor.sync_from_state(state);

        egui::CollapsingHeader::new("Environment")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(active_environment_label(state));
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(editor.active_tab == EnvTab::Variables, "Variables")
                        .clicked()
                    {
                        editor.active_tab = EnvTab::Variables;
                    }
                    if ui
                        .selectable_label(editor.active_tab == EnvTab::Auth, "Auth")
                        .clicked()
                    {
                        editor.active_tab = EnvTab::Auth;
                    }
                });
                ui.separator();

                match editor.active_tab {
                    EnvTab::Variables => render_variables_tab(ui, editor, state, intents),
                    EnvTab::Auth => {
                        let env_name = state.active_environment_name();
                        crate::ui::oauth_panel::show(ui, oauth, env_name);
                    }
                }
            });
    }
}

fn render_variables_tab(
    ui: &mut egui::Ui,
    editor: &mut EnvironmentEditorUiState,
    state: &AppState,
    intents: &mut Vec<PanelIntent>,
) {
    ui.horizontal(|ui| {
        ui.small("Variables are edited per active environment.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+ Add").clicked() {
                editor.variable_rows.push(EnvironmentVariableRow::default());
            }
        });
    });

    let mut remove_index = None;
    let rows_before: Vec<EnvironmentVariableRow> = editor.variable_rows.clone();
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

    if let Some(index) = remove_index
        && index < editor.variable_rows.len()
    {
        editor.variable_rows.remove(index);
    }

    if editor.variable_rows.is_empty() {
        ui.monospace("No variables. Use + Add to create one.");
    }

    let rows_changed = editor.variable_rows != rows_before || remove_index.is_some();
    let (variables, has_pending_key, has_duplicate_key) = collect_variable_rows(editor);

    if rows_changed && let Some(name) = state.active_environment_name().map(str::to_owned) {
        intents.push(PanelIntent::SetEnvironmentVars {
            name,
            vars: variables.clone(),
        });
    }
    let committed_count = variables.len();

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
}
