//! Panel-to-orchestrator command channel.
//!
//! `intent::PanelIntent` carries **data mutations** only. File-level
//! operations — save, export, and the various imports — are *commands*
//! with side effects (native file dialogs, background fetches, status
//! messages), so they travel on their own channel instead of being bolted
//! onto the state funnel.
//!
//! Panels push into a `Vec<UiCommand>`; `app.rs` drains the buffer after
//! each egui frame and dispatches each command to the handler that owns
//! the corresponding IO.

/// A file-level command requested by a UI panel. Panels never perform IO
/// themselves; `app.rs::apply_pending_commands` is the single dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiCommand {
    /// Persist the whole workspace to `./data/`.
    SaveWorkspace,
    /// Write a workspace bundle to a user-chosen file.
    ExportWorkspace,
    /// Pick a workspace bundle and stage it for confirmation.
    ImportWorkspace,
    /// Pick an OpenAPI/Swagger spec file and stage the merge preview.
    ImportOpenApiFile,
    /// Open the "fetch OpenAPI spec from URL" dialog.
    OpenOpenApiUrlDialog,
    /// Pick a `.http`/`.rest` file and append its requests.
    ImportHttpFile,
    /// Open the "paste a cURL command" dialog.
    OpenCurlPasteDialog,
}
