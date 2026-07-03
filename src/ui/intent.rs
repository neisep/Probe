//! Panel-to-orchestrator intent channel.
//!
//! CLAUDE.md states: "UI panels are read-only over state — mutations go
//! through `app.rs`." This module defines the intent enum that panels push
//! into a buffer; `app.rs` drains the buffer after each egui frame and
//! applies the intents through `ProbeApp::apply_intent`, which is the
//! single place that centralizes validation, dirty-tracking, persistence
//! triggers, and OAuth-cache invalidation.
//!
//! Only **data** mutations flow through here. Purely transient UI state
//! (current view, selected request, search query, settings_open) is held
//! on `UIState` and may be mutated by panels directly — those edits don't
//! need validation or dirty-tracking.

use std::collections::BTreeMap;

use crate::state::request::RequestAuth;

/// A data mutation requested by a UI panel. Panels never apply these
/// directly; they push into a `Vec<PanelIntent>` that `app.rs` drains.
#[derive(Debug, Clone)]
pub enum PanelIntent {
    // ---- Collection-level request operations -------------------------------
    AddDefaultRequest,
    DuplicateSelectedRequest,
    RemoveSelectedRequest,

    // ---- Single-request edits ----------------------------------------------
    /// Set the HTTP method on the request at `index`.
    SetRequestMethod {
        index: usize,
        method: String,
    },
    /// Set the URL on the request. `commit=false` is a per-keystroke raw
    /// assignment that preserves what the user is typing. `commit=true`
    /// runs the URL normalisers (`set_url` / `adopt_url_query`), splitting
    /// a `?<query>` suffix into params. UI panels typically push
    /// `commit=false` on `changed()` and `commit=true` on `lost_focus()`.
    SetRequestUrl {
        index: usize,
        url: String,
        commit: bool,
    },
    /// Set the request name. `commit=false` raw; `commit=true` normalises.
    SetRequestName {
        index: usize,
        name: String,
        commit: bool,
    },
    /// Set the folder path. `commit=false` raw; `commit=true` normalises.
    SetRequestFolder {
        index: usize,
        folder: String,
        commit: bool,
    },
    /// Replace the auth configuration.
    SetRequestAuth {
        index: usize,
        auth: RequestAuth,
    },
    /// Replace the body — `None` clears it.
    SetRequestBody {
        index: usize,
        body: Option<String>,
    },
    /// Toggle the OAuth-token-attach flag.
    SetAttachOAuth {
        index: usize,
        attach: bool,
    },
    /// Replace the full query-params list (used by the KV editor).
    SetRequestQueryParams {
        index: usize,
        params: Vec<(String, String)>,
    },
    /// Replace the full headers list (used by the KV editor).
    SetRequestHeaders {
        index: usize,
        headers: Vec<(String, String)>,
    },

    // ---- Environment ops ---------------------------------------------------
    /// Add a new environment with an auto-generated name.
    AddAutoNamedEnvironment,
    /// Remove the environment with the given name.
    RemoveEnvironment {
        name: String,
    },
    /// Make the named environment active.
    SelectEnvironment {
        name: String,
    },
    /// Rename the currently active environment.
    RenameActiveEnvironment {
        new_name: String,
    },
    /// Replace the entire variable map for the named environment.
    SetEnvironmentVars {
        name: String,
        vars: BTreeMap<String, String>,
    },

    // ---- Response history --------------------------------------------------
    ClearResponses,
}
