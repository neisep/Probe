use eframe::egui;

pub enum OpenApiUrlDialogAction {
    None,
    Close,
    Fetch,
}

pub fn show(ctx: &egui::Context, url_input: &mut String) -> OpenApiUrlDialogAction {
    let mut action = OpenApiUrlDialogAction::None;

    egui::Window::new("Import OpenAPI from URL")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Spec URL:");
            let response = ui.text_edit_singleline(url_input);
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                action = OpenApiUrlDialogAction::Fetch;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Fetch").clicked() {
                    action = OpenApiUrlDialogAction::Fetch;
                }
                if ui.button("Cancel").clicked() {
                    action = OpenApiUrlDialogAction::Close;
                }
            });
        });

    action
}
