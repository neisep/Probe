# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                              # debug build
cargo build --release                   # release build
cargo build --features keyring-storage  # with OS keychain support
cargo run                               # run the app
cargo check                             # fast type check
cargo fmt                               # format
cargo clippy                            # lint
cargo test                              # all tests
cargo test <test_name>                  # single test
```

## Architecture

**Probe** is a native desktop HTTP client (egui + Tokio). The main layers are:

- **`src/app.rs`** — Orchestrator. Owns the eframe `App` impl, drives the event loop, routes UI actions to runtime/persistence/oauth, resolves environment variables before send, and manages workspace import/export.
- **`src/state/`** — In-memory domain model: `AppState` (aggregate root), `RequestDraft`, `ResponseSummary`, `Environment`, `UIState`.
- **`src/runtime/`** — Async HTTP. A Tokio worker thread accepts requests over mpsc and emits completion events. `executor.rs` owns the thread; `types.rs` defines `AsyncRequest`, `AsyncRequestResult`, and resolution helpers.
- **`src/persistence/`** — JSON file storage under `./data/`. `storage.rs` provides category-aware CRUD; `models.rs` defines persisted shapes separate from in-memory state.
- **`src/oauth/`** — Three OAuth2 flows (auth_code + PKCE, client_credentials, device_code) plus token refresh, browser launch, PKCE generation, and a token store abstraction (`FileTokenStore` or `KeyringTokenStore` behind the `keyring-storage` feature).
- **`src/openapi/`** — Parses OpenAPI 3.x YAML/JSON specs and merges operations into existing requests.
- **`src/http_format/`** — Parses and writes `.http`/`.rest` files.
- **`src/ui/`** — egui panels composed by `shell.rs`. Each panel is one file; they read from `AppState` and emit actions handled by `app.rs`.

Data flow: `ui/` reads state and emits intent → `app.rs` resolves variables, calls `runtime/` → runtime emits events → `app.rs` projects results into `ResponseSummary` and persists via `persistence/`.

## Persistence Layout

All runtime data lives under `./data/` (created automatically):

| Path | Contents |
|------|----------|
| `./data/collections/` | Request drafts (JSON) |
| `./data/.probe/environments/` | Environment definitions |
| `./data/.probe/responses/` | Response history |
| `./data/.probe/oauth_tokens/` | OAuth tokens |
| `./data/.probe/session.json` | UI selections / session state |
| `./data/backups/` | Pre-import workspace snapshots |

## Key Design Rules

- Never `unwrap()` in `src/` — use `Result<T, E>` and propagate errors.
- Validate inputs at module boundaries; trust internal types inside a module.
- Add new behavior in new modules rather than editing existing ones.
- Prefer small, focused traits over large concrete structs.
- UI panels are read-only over state — mutations go through `app.rs`.
