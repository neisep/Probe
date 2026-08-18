//! The single `＋ Import` menu — the one place every import source lives.

use eframe::egui;

use crate::ui::command::UiCommand;

/// Which import sources are temporarily unavailable. Passed in by `app.rs`
/// so this panel stays read-only over app state.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportMenuBusy {
    /// A request is in flight, so replacing the workspace is unsafe.
    pub workspace: bool,
    /// An OpenAPI import is already staged or its URL dialog is open.
    pub openapi: bool,
}

/// One menu entry: label, hover text, the command it emits, and whether it
/// is currently selectable.
struct Entry {
    label: &'static str,
    hover: &'static str,
    command: UiCommand,
    enabled: bool,
}

pub fn show(ui: &mut egui::Ui, busy: ImportMenuBusy, commands: &mut Vec<UiCommand>) {
    let import_entries = [
        Entry {
            label: "Paste cURL…",
            hover: "Paste a curl command and turn it into a request",
            command: UiCommand::OpenCurlPasteDialog,
            enabled: true,
        },
        Entry {
            label: "OpenAPI / Swagger file…",
            hover: "Merge the operations of a local spec file into the collection",
            command: UiCommand::ImportOpenApiFile,
            enabled: !busy.openapi,
        },
        Entry {
            label: "OpenAPI / Swagger URL…",
            hover: "Fetch a spec over HTTP and merge its operations",
            command: UiCommand::OpenOpenApiUrlDialog,
            enabled: !busy.openapi,
        },
        Entry {
            label: ".http / .rest file…",
            hover: "Append the requests of an .http or .rest file",
            command: UiCommand::ImportHttpFile,
            enabled: true,
        },
    ];

    let workspace_entries = [
        Entry {
            label: "Probe workspace…",
            hover: "Replace everything with a previously exported workspace",
            command: UiCommand::ImportWorkspace,
            enabled: !busy.workspace,
        },
        Entry {
            label: "Export workspace…",
            hover: "Write requests, responses, and environments to one file",
            command: UiCommand::ExportWorkspace,
            enabled: true,
        },
    ];

    ui.menu_button("+ Import ⏷", |ui| {
        ui.set_min_width(210.0);
        for entry in &import_entries {
            show_entry(ui, entry, commands);
        }
        ui.separator();
        for entry in &workspace_entries {
            show_entry(ui, entry, commands);
        }
    })
    .response
    .on_hover_text("Import from cURL, OpenAPI, .http files, or a workspace bundle");
}

fn show_entry(ui: &mut egui::Ui, entry: &Entry, commands: &mut Vec<UiCommand>) {
    if ui
        .add_enabled(entry.enabled, egui::Button::new(entry.label))
        .on_hover_text(entry.hover)
        .clicked()
    {
        commands.push(entry.command);
        ui.close();
    }
}
