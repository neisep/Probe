use eframe::egui;

use crate::openapi_import::PendingOpenApiImport;

pub enum OpenApiImportDialogAction {
    None,
    Cancel,
    Confirm,
}

pub fn show(ctx: &egui::Context, pending: &PendingOpenApiImport) -> OpenApiImportDialogAction {
    let mut action = OpenApiImportDialogAction::None;

    let (source, new_count, updated_count, unchanged_count) = (
        pending.source.clone(),
        pending.preview.new_count,
        pending.preview.updated_count,
        pending.preview.unchanged_count,
    );

    egui::Window::new("Confirm OpenAPI import")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!("Source: {source}"));
            ui.add_space(6.0);
            ui.label(format!("New requests:       {new_count}"));
            ui.label(format!("Updated requests:   {updated_count}"));
            ui.label(format!("Unchanged requests: {unchanged_count}"));
            ui.add_space(4.0);
            ui.small(
                "Auth, headers, and body you have set on existing requests will be preserved.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Import").clicked() {
                    action = OpenApiImportDialogAction::Confirm;
                }
                if ui.button("Cancel").clicked() {
                    action = OpenApiImportDialogAction::Cancel;
                }
            });
        });

    action
}
