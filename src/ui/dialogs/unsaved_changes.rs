use eframe::egui;

pub enum UnsavedChangesAction {
    None,
    Cancel,
    SaveAndClose,
    CloseWithoutSaving,
}

pub fn show(ctx: &egui::Context, has_pending_import: bool) -> UnsavedChangesAction {
    let mut action = UnsavedChangesAction::None;

    let message = if has_pending_import {
        "You have unsaved changes or a pending import in progress. Would you like to save before closing?"
    } else {
        "You have unsaved changes. Would you like to save before closing?"
    };

    egui::Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(message);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save and close").clicked() {
                    action = UnsavedChangesAction::SaveAndClose;
                }
                if ui.button("Close without saving").clicked() {
                    action = UnsavedChangesAction::CloseWithoutSaving;
                }
                if ui.button("Cancel").clicked() {
                    action = UnsavedChangesAction::Cancel;
                }
            });
        });

    action
}
