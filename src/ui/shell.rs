use eframe::egui;

use crate::state::AppState;
use crate::ui::{center_panel, inspector, left_sidebar, status_bar, top_bar};

pub fn show(ui: &mut egui::Ui, state: &mut AppState, status: &str) {
    let active_view = state.ui.view;

    top_bar::show_topbar(ui, state, active_view);
    left_sidebar::show_sidebar(ui, state);
    inspector::show_inspector(ui, state);
    status_bar::show_status(ui, state, status);
    center_panel::show_center(ui, state);
}
