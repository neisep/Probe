# Codebase Structure

Quick map of the current architecture. For file-by-file detail, use `docs/memory-map.md`.

- `src/main.rs`
  - Native entrypoint.
  - Initializes tracing.
  - Starts `eframe` and constructs `app::ProbeApp`.

- `src/app.rs`
  - Main integration seam.
  - Owns app startup, restore/save, runtime polling, send actions, and projection of runtime results into UI state.
  - Owns workspace import/export dialog actions, staged import confirmation, automatic pre-import backups, bundle serialization, and the staged pre-send request preview flow.
  - Resolves active-environment `{{var}}` placeholders before sending requests.
  - Normalizes request organization metadata before persistence restore/save.
  - Builds final outbound URLs from the base URL plus saved query parameters.
  - Injects auth-generated headers/query params during send preparation.

- `src/state/`
  - In-memory domain and UI state.
  - `app_state.rs`
    - Aggregate state root.
    - Owns requests, responses, environments, active selections, and helper methods.
    - Tracks reusable folder paths for the request workspace.
  - `request.rs`
    - Request draft model.
    - Request name, folder path, method, base URL, query params, auth config, headers, optional body.
  - `response.rs`
    - Response summary model used by the UI and persistence restore path.
    - Carries request metadata, response metadata, preview text, and header lists.
  - `environment.rs`
    - Lightweight environment model.
    - Named key/value store for request substitution.
  - `ui_state.rs`
    - View selection, selected request/response indices, and the ephemeral request search query.
  - `mod.rs`
    - Re-exports state types and shared `StateError`.

- `src/runtime/`
  - Async HTTP execution layer.
  - `types.rs`
    - Request/response DTOs.
    - Variable-resolution helpers and resolution error types.
  - `executor.rs`
    - Dedicated background Tokio runtime.
    - Request submission, event queue, request execution, and explicit request preparation helpers.
  - `mod.rs`
    - Runtime boundary and public re-exports.

- `src/persistence/`
  - File-backed storage rooted at `./data`.
  - `models.rs`
    - Persisted shapes for drafts, previews, responses, environments, session state, and snapshots.
  - `storage.rs`
    - Category-aware JSON storage APIs.
    - Atomic writes, key validation, list/load/save/delete helpers.
  - `mod.rs`
    - Public storage exports.

- `src/ui/`
  - egui composition and panels.
  - `shell.rs`
    - Top-level composition order for panels.
  - `top_bar.rs`
    - App header, active view, active environment, selected folder, selected request summary.
  - `left_sidebar.rs`
    - Nested request folder tree, request actions, quick search/filter, environment section, view switching.
  - `environment_editor.rs`
    - Compact environment selector/editor UI.
    - Included from `left_sidebar.rs` with `#[path = "environment_editor.rs"]`.
  - `request_panel.rs`
    - Request editor for name, folder path, method, base URL, query params, auth config, environment variables, headers, and body.
  - `request_preview_modal.rs`
    - Read-only request preview window shown before a request is submitted.
    - Displays final URL, query params, headers, body preview, and blocking preparation errors.
  - `response_panel.rs`
    - Response history list and response selection.
  - `inspector.rs`
    - Request/response detail inspector.
  - `status_bar.rs`
    - Bottom status summary.
  - `center_panel.rs`
    - Center-area router between request and response content.
  - `mod.rs`
    - UI module exports.

## Current product shape

- Native desktop REST client built with Rust + egui.
- Multi-request workspace.
- Editable request drafts.
- Nested request folders with lightweight collection-style organization.
- Structured query parameter editing with safe URL composition on send.
- Structured auth presets for bearer, basic, and API-key flows.
- Native JSON workspace import/export.
- Staged replace-workspace import confirmation with automatic recovery backups.
- Pre-send request preview with resolved request inspection and validation feedback.
- Request quick search and filter across name, folder, method, and URL.
- Async send flow with response history.
- Response detail inspection with request/response headers.
- Lightweight environments with active-environment selection.
- `{{var}}` substitution for URL, headers, and body before send.
- Local persistence for workspace, responses, session state, and environments.

## Current architectural rules

- `app.rs` is the orchestrator and integration boundary.
- `state`, `runtime`, `persistence`, and `ui` are separate seams.
- Shared seams are integrated centrally after worker changes.
- Persistence is JSON-on-disk, not a database.
- The GUI remains runnable after every slice.

## Expected next major areas

- Final MVP polish.
