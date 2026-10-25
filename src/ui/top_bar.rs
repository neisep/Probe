use crate::state::AppState;
use crate::ui::command::UiCommand;
use crate::ui::import_menu::{self, ImportMenuBusy};
use crate::ui::left_sidebar::environment_editor;
use crate::ui::theme;
use eframe::egui;

pub fn show_topbar(
    ui: &mut egui::Ui,
    state: &mut AppState,
    busy: ImportMenuBusy,
    commands: &mut Vec<UiCommand>,
) {
    egui::Panel::top("top_bar").show_inside(ui, |ui| {
        ui.set_min_height(40.0);
        ui.set_max_height(40.0);
        egui::Frame::NONE
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading(
                        egui::RichText::new("Probe")
                            .color(theme::ACCENT_STRONG)
                            .strong(),
                    );

                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "env · {}",
                            environment_editor::active_environment_label(state)
                        ))
                        .color(theme::TEXT_MUTED)
                        .small(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .small_button("⚙ Settings")
                            .on_hover_text("Environments and settings")
                            .clicked()
                        {
                            state.ui.settings_open = !state.ui.settings_open;
                        }

                        ui.add_space(10.0);

                        // Every import source lives behind this one menu; the
                        // method/URL echo that used to sit here duplicated the
                        // request editor rendered directly below.
                        import_menu::show(ui, busy, commands);
                    });
                });
            });
    });
}
