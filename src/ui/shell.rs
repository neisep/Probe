use eframe::egui;

use crate::state::AppState;
use crate::ui::left_sidebar::environment_editor;
use crate::ui::response_viewer::ResponseViewerState;
use crate::ui::theme;
use crate::ui::{center_panel, left_sidebar, top_bar};

pub fn show(
    ui: &mut egui::Ui,
    state: &mut AppState,
    viewer: &mut ResponseViewerState,
    pending: bool,
) {
    let active_view = state.ui.view;

    top_bar::show_topbar(ui, state, active_view);
    left_sidebar::show_sidebar(ui, state);
    center_panel::show_center(ui, state, viewer, pending);

    show_settings_window(ui.ctx(), state);
}

fn show_settings_window(ctx: &egui::Context, state: &mut AppState) {
    let mut open = state.ui.settings_open;
    if !open {
        return;
    }

    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .default_height(460.0)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0, theme::BORDER)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    environment_editor::show_sidebar_section(ui, state);
                });
        });

    state.ui.settings_open = open;
}
