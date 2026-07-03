use crate::ui::left_sidebar::environment_editor::EnvironmentEditorUiState;
use crate::ui::oauth_panel::OAuthPanelState;

/// Transient, non-persisted UI state for the settings-window panels.
///
/// Owned by `ProbeApp` and threaded into the panels each frame, mirroring
/// how `ResponseViewerState` is held. This replaces the previous
/// process-global `OnceLock<Mutex<…>>` singletons in `environment_editor`
/// and `oauth_panel`, so panel state is scoped to the app instance instead
/// of the process.
#[derive(Default)]
pub struct PanelUiState {
    pub environment_editor: EnvironmentEditorUiState,
    pub oauth: OAuthPanelState,
}
