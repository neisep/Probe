# Memory Map

This document is the navigation index for the current architecture

- Architecture status: **Slice 12 complete**
- Current app shape:
  - Native Rust + egui desktop REST client
  - Multi-request workspace
  - Structured query parameter editing
  - Structured request auth presets
  - Native workspace import/export bundle with staged replacement
  - Pre-send request preview with resolved request inspection
  - Async send + response history
  - Response/request inspection with persisted headers
  - Lightweight environments + `{{var}}` resolution
  - Nested request folders with persisted path organization

## Top-level flow

1. `src/main.rs`
   - Initializes tracing
   - Starts egui/eframe
   - Builds `ProbeApp`

2. `src/app.rs`
   - Creates runtime and storage
   - Restores persisted workspace/session/environment state
   - Owns the send button action
   - Resolves active-environment variables before request submission
   - Polls runtime events and converts them into `ResponseSummary`
   - Persists the updated app snapshot

3. `src/ui/*`
   - Renders the shell and editors/viewers
   - Mutates `AppState`

4. `src/runtime/*`
   - Executes HTTP requests on a background Tokio runtime
   - Emits status/completion events

5. `src/persistence/*`
  - Saves and restores drafts, responses, environments, and session/workspace state under `./data`
  - Stores automatic pre-import backups under `./data/backups`

## Module memory map

### `src/app.rs`

- Purpose:
  - Orchestrator layer
  - Only place where state, runtime, persistence, and UI are wired together
- Key responsibilities:
  - startup/restore
  - save snapshot
  - export/import a versioned workspace bundle
  - stage destructive imports behind a confirmation window
  - create automatic backups before replacing the current workspace
  - stage a request preview before submission
  - send the previewed request
  - apply environment resolution
  - compose encoded request URLs from base URL + saved query rows
  - inject request auth into headers/query params during send preparation
  - project runtime results into `ResponseSummary`
  - restore workspace/session/environment state

### `src/state/app_state.rs`

- Purpose:
  - Aggregate application state
- Owns:
  - `ui`
  - `requests`
  - `responses`
  - `environments`
  - `active_environment`
- Important helpers:
  - request selection/add/duplicate/remove
  - normalized folder listing/grouping
  - environment add/select/remove
  - active variable access
  - selection validation

### `src/state/request.rs`

- Purpose:
  - Editable request draft model
- Fields:
  - name
  - folder path
  - method
  - base url
  - query parameter rows
  - auth mode + auth inputs
  - headers
  - optional body

### `src/state/response.rs`

- Purpose:
  - UI-friendly response summary
- Fields include:
  - originating request identity
  - originating request headers
  - response headers
  - status/timing/size/content type
  - preview text
  - error text

### `src/state/environment.rs`

- Purpose:
  - Lightweight named environment
- Shape:
  - `name`
  - `vars: BTreeMap<String, String>`

### `src/state/ui_state.rs`

- Purpose:
  - Pure UI selection state
- Owns:
  - selected request index
  - selected response index
  - current view

### `src/runtime/types.rs`

- Purpose:
  - Shared async/runtime DTOs and resolution helpers
- Key types:
  - `AsyncRequest`
  - `ResponseInfo`
  - `ErrorInfo`
  - `ResolutionValues`
  - `ResolutionError`
  - `UnresolvedBehavior`
- Key behavior:
  - resolve `{{var}}` placeholders in URL, headers, and body text

### `src/runtime/executor.rs`

- Purpose:
  - Background HTTP engine
- Key behavior:
  - validates methods
  - applies headers/body
  - submits requests to reqwest
  - captures headers/body/timing
  - queues runtime events for the UI thread

### `src/persistence/models.rs`

- Purpose:
  - On-disk JSON shapes
- Important groups:
  - drafts + previews
  - response summaries + preview detail
  - environment records + environment snapshot
  - session state
  - workspace snapshot

### `src/persistence/storage.rs`

- Purpose:
  - File-backed persistence API
- Storage categories currently used:
  - `drafts`
  - `draft_previews`
  - `responses`
  - `response_previews`
  - `workspace_snapshots`
  - `environments`
  - `session`
  - `snapshots`
- Important characteristics:
  - validates categories and keys
  - atomic temp-file writes + rename
  - backward-compatible serde defaults where possible

### `src/ui/shell.rs`

- Purpose:
  - Central egui composition order
- Panels arranged through:
  - top bar
  - sidebar
  - inspector
  - center content
  - bottom/status panels

### `src/ui/left_sidebar.rs`

- Purpose:
  - Request workspace navigation
  - Environment selector entrypoint
- Key behavior:
  - request create/duplicate/delete/select
  - nested request folder tree
  - environment section
  - view switching

### `src/ui/environment_editor.rs`

- Purpose:
  - Small environment management UI state and rendering
- Key behavior:
  - active environment selection
  - environment add/delete
  - inline rename
  - key/value variable editing

### `src/ui/request_panel.rs`

- Purpose:
  - Request authoring
- Key behavior:
  - request name + folder editing
  - existing-folder picker
  - method/url editing
  - query parameter editing
  - auth mode editing
  - environment variable editing section
  - header editing
  - body editing

### `src/ui/request_preview_modal.rs`

- Purpose:
  - Read-only pre-send request inspection
- Key behavior:
  - shows resolved method + URL
  - shows final query params and headers
  - previews request body text
  - surfaces blocking preparation errors before send

### Workspace bundle flow

- Export serializes the current requests, responses, environments, and UI selection state to a versioned JSON bundle.
- Import validates the bundle version, replaces the in-memory workspace, rehydrates response/request links, and persists the imported workspace through the normal snapshot path.

### `src/ui/response_panel.rs`

- Purpose:
  - Response history list
- Key behavior:
  - response selection
  - request-response pairing
  - compact metadata display

### `src/ui/inspector.rs`

- Purpose:
  - Detailed request/response inspection
- Key behavior:
  - selected or latest response detail
  - request headers/body preview
  - response headers/body preview

## Persistence map under `./data`

- `drafts/`
  - full request draft payloads
- `draft_previews/`
  - lightweight draft list entries
- `responses/`
  - stored response summary rows
- `response_previews/`
  - response preview + detail wrapper
- `workspace_snapshots/`
  - current workspace snapshot
- `environments/`
  - persisted environment records
- `session/state.json`
  - selected request/response + active environment + active view metadata
- `snapshots/last.json`
  - legacy compatibility snapshot

## Current slices completed

1. Bootstrap native app
2. Core state models
3. First integrated request/send/persist slice
4. Multi-request workspace
5. HTTP fidelity + richer response detail
6. Environments + variable resolution

## Likely next slices

1. Collections/folders
2. Request ergonomics
3. Import/export + polish
