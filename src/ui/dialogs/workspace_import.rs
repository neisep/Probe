use eframe::egui;

use crate::workspace::PendingWorkspaceImport;

pub enum WorkspaceImportDialogAction {
    None,
    Cancel,
    Confirm,
}

pub fn show(ctx: &egui::Context, pending: &PendingWorkspaceImport) -> WorkspaceImportDialogAction {
    let mut action = WorkspaceImportDialogAction::None;

    egui::Window::new("Confirm workspace import")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "Replace the current workspace with {}?",
                pending.path.display()
            ));
            ui.add_space(6.0);
            ui.label(format!("Requests: {}", pending.preview.request_count));
            ui.label(format!("Responses: {}", pending.preview.response_count));
            ui.label(format!("Environments: {}", pending.preview.environment_count));
            if let Some(label) = pending.preview.selected_request_label.as_deref() {
                ui.label(format!("Selected request: {label}"));
            }
            ui.add_space(6.0);
            ui.small(
                "Probe will create an automatic backup of the current workspace before applying the import.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Import and replace").clicked() {
                    action = WorkspaceImportDialogAction::Confirm;
                }
                if ui.button("Cancel").clicked() {
                    action = WorkspaceImportDialogAction::Cancel;
                }
            });
        });

    action
}
