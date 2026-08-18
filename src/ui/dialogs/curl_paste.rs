use eframe::egui;

pub enum CurlPasteDialogAction {
    None,
    Close,
    Import,
}

pub fn show(ctx: &egui::Context, curl_input: &mut String) -> CurlPasteDialogAction {
    let mut action = CurlPasteDialogAction::None;

    egui::Window::new("Paste cURL command")
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label("Paste a curl command — method, URL, headers, and body are imported.");
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::multiline(curl_input)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .hint_text(
                        "curl -X POST https://example.com/users -H 'Accept: application/json'",
                    ),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let has_input = !curl_input.trim().is_empty();
                if ui
                    .add_enabled(has_input, egui::Button::new("Import"))
                    .clicked()
                {
                    action = CurlPasteDialogAction::Import;
                }
                if ui.button("Cancel").clicked() {
                    action = CurlPasteDialogAction::Close;
                }
            });
        });

    action
}
