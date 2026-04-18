use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Draft of a request or message tied to a workspace.
/// Keep this small and serializable — the application stores the full draft content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    /// Stable identifier for the draft (validated by storage layer).
    pub id: String,
    /// Optional workspace this draft belongs to.
    pub workspace_id: Option<String>,
    /// Optional path within the workspace the draft is associated with.
    pub path: Option<String>,
    /// The text payload for the draft.
    pub content: String,
    /// RFC3339 timestamp when created (optional).
    pub created_at: Option<String>,
    /// Optional lightweight tags for quick filtering.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Summary of a response for UI listing and quick restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub id: String,
    pub workspace_id: Option<String>,
    pub request_id: Option<String>,
    pub status_code: Option<u16>,
    pub summary: Option<String>,
    pub duration_ms: Option<u64>,
    pub created_at: Option<String>,
}

/// UI selection/state that should be restored between runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIState {
    pub selected_workspace: Option<String>,
    pub last_opened: Option<String>,
    /// files the user had open in the workspace (optional)
    #[serde(default)]
    pub open_files: Vec<String>,
}

/// Lightweight preview/summary for a Draft used by listing and fast UI previews.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftPreview {
    /// id of the preview entry (can be same as draft id or derived)
    pub id: String,
    /// the draft id this preview corresponds to
    pub draft_id: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub target_url: Option<String>,
    #[serde(default)]
    pub preview_title: Option<String>,
    #[serde(default)]
    pub preview_snippet: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: Option<String>,
}

/// Lightweight preview/summary for a Response for fast UI listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePreview {
    pub id: String,
    pub response_id: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub header_count: Option<usize>,
    #[serde(default)]
    pub size_bytes: Option<usize>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tokens: Option<u32>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: Option<String>,
}

/// Session-level richer state for UI (selected response, active view, open panels)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default)]
    pub selected_response: Option<String>,
    #[serde(default)]
    pub active_view: Option<String>,
    #[serde(default)]
    pub open_panels: Vec<String>,
    pub updated_at: Option<String>,
}

/// Legacy/opaque snapshot used by existing app code: keeps an id and arbitrary JSON payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    /// Opaque JSON payload (legacy field used by many code paths).
    #[serde(default)]
    pub data: Value,
}

/// Rich workspace snapshot capturing metadata and lightweight lists of contents.
/// Stored separately under `workspace_snapshots` to avoid breaking existing code that
/// expects the simple `Snapshot` shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    /// Snapshot id (validated by storage layer)
    pub id: String,
    /// Human friendly name for the snapshot
    pub name: Option<String>,
    /// Root path of the workspace when snapshot was taken
    pub workspace_root: Option<String>,
    pub created_at: Option<String>,
    /// Files that were open when the snapshot was taken
    #[serde(default)]
    pub open_files: Vec<String>,
    /// References to saved draft ids included in the snapshot
    #[serde(default)]
    pub drafts: Vec<String>,
    /// References to response summary ids included in the snapshot
    #[serde(default)]
    pub responses: Vec<String>,
    /// Arbitrary JSON for extension points
    #[serde(default)]
    pub meta: Value,
}
