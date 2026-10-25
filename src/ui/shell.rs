use eframe::egui;

use crate::state::AppState;
use crate::ui::command::UiCommand;
use crate::ui::import_menu::ImportMenuBusy;
use crate::ui::intent::PanelIntent;
use crate::ui::left_sidebar::environment_editor;
use crate::ui::panel_state::PanelUiState;
use crate::ui::response_viewer::ResponseViewerState;
use crate::ui::theme;
use crate::ui::{center_panel, left_sidebar, top_bar};

/// Everything the shell needs from `app.rs` beyond `AppState`: the buffers
/// panels write into, and the flags that gate the import menu and the Save
/// button. Grouped so panels keep taking a single context instead of a
/// growing parameter list.
pub struct ShellContext<'a> {
    pub viewer: &'a mut ResponseViewerState,
    pub panels: &'a mut PanelUiState,
    pub intents: &'a mut Vec<PanelIntent>,
    pub commands: &'a mut Vec<UiCommand>,
    /// A request is in flight.
    pub pending: bool,
    /// Import sources that are temporarily unavailable.
    pub busy: ImportMenuBusy,
    /// There are unsaved changes on disk-bound state.
    pub dirty: bool,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState, ctx: &mut ShellContext<'_>) {
    top_bar::show_topbar(ui, state, ctx.busy, ctx.commands);
    left_sidebar::show_sidebar(ui, state, ctx.intents);
    center_panel::show_center(ui, state, ctx);

    show_settings_window(ui.ctx(), state, ctx.panels, ctx.intents);
}

fn show_settings_window(
    ctx: &egui::Context,
    state: &mut AppState,
    panels: &mut PanelUiState,
    intents: &mut Vec<PanelIntent>,
) {
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
            egui::Frame::window(&ctx.global_style())
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0, theme::BORDER)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    environment_editor::show_sidebar_section(ui, state, panels, intents);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    environment_editor::show_request_section(ui, state, panels, intents);
                });
        });

    state.ui.settings_open = open;
}
