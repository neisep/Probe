# Probe — Completed Work (Archive)

Condensed record of finished items, with `file:line` evidence. Active/backlog
work lives in [`../todos.md`](../todos.md).

> **Verification status:** the items below are implemented in the working tree
> but **uncommitted and not yet `cargo test`-verified**. Record the commit hash
> in the `Landed` column once each lands and the suite is green.

Items 1–5 were the "Top-5 Priority Fixes" from the consolidated code review;
items below that are follow-ups completed afterwards.

| # | Item | Landed |
|---|------|--------|
| 1 | Per-request timeouts + response body cap | _(uncommitted)_ |
| 2 | UI read-only invariant via `PanelIntent` | _(uncommitted)_ |
| 3 | OAuth refresh hardening | _(uncommitted)_ |
| 4 | Redact secrets in `Debug` + drop resolved requests | _(uncommitted)_ |
| 5 | `atomic_write` durability + tmp cleanup | _(uncommitted)_ |
| C4 | Token-store concurrent-writer lock | _(uncommitted · tests green)_ |

---

- [x] **1. Per-request timeouts + response body cap** _(C1 + C2)_

  Client built once with `connect_timeout` / `timeout` / `pool_idle_timeout`;
  response body streamed with a 100 MB cap and a `truncated` flag; timeout vs
  connect errors classified.
  Evidence: `src/runtime/executor.rs:20-29` (constants), `:102-105` (client),
  `read_body_capped` `:382-402`, error classify `:404-414`;
  `src/runtime/types.rs:133` (`pub truncated: bool`).

- [x] **2. UI read-only invariant via `PanelIntent`** _(C3)_

  Panels take `&AppState` + `&mut Vec<PanelIntent>` and push intents instead of
  mutating state; `app.rs` drains and applies them through a single surface.
  Evidence: `src/ui/intent.rs` (enum), `src/app.rs:583-598`
  (`apply_pending_intents` / `apply_intent`), call site `:901`; migrated panels
  `request_panel.rs:14`, `environment_editor.rs:119`, `left_sidebar.rs:172`.
  Note: `oauth_panel.rs` not yet migrated — tracked as a Minor backlog item.

- [x] **3. OAuth refresh hardening** _(C5 + C6)_

  `refresh_runtime()` returns `Result` instead of `.expect()`; per-`(base_dir,
  env_id)` single-flight dedupes concurrent refreshes; cache key canonicalized.
  Evidence: `src/oauth/middleware.rs:39-48` (runtime `Result`), `:28-29`
  (`INFLIGHT_REFRESH`), `:174-204` (single-flight), `:54-59` (`cache_key`);
  `src/oauth/mod.rs:98-102` (`OAuthError::Internal`).

- [x] **4. Redact secrets in `Debug` + drop resolved requests** _(M1 + M10)_

  Custom `Debug` impls redact tokens/passwords/api-keys and sensitive headers;
  resolved requests are no longer retained in `SharedState`; pending context
  stores pre-redacted headers.
  Evidence: `src/state/request.rs:62-87`, `src/runtime/types.rs:68-87`,
  `src/runtime/executor.rs:184-195`, `src/app.rs:21-30` + redaction `:685`.

- [x] **5. `atomic_write` durability + tmp cleanup** _(M2)_

  `sync_all()` errors propagated; failed temp files cleaned up; unique tmp paths
  prevent concurrent stomping.
  Evidence: `src/persistence/storage.rs:419-456` (`atomic_write`), `:458-468`
  (`unique_tmp_path`); tests `:694`, `:713`, `:742`.

---

## Follow-ups

- [x] **C4. Token-store concurrent-writer lock** _(critical)_

  `FileTokenStore::put`/`delete` did an unguarded read-modify-write of the whole
  env file, so a refresh thread rotating one flow's `refresh_token` could race a
  writer of another flow and silently drop the rotated token (last writer wins).
  Added a per-env-file in-process write lock (`write_lock` + `env_lock_key`,
  keyed by the canonicalised path) wrapping the load→mutate→save sequence;
  applied to both `FileTokenStore` and the feature-gated `KeyringTokenStore`.
  The refresh flow already preserved a non-rotated token via
  `build_cached_token` (`src/oauth/flows/mod.rs:74-77`), so no change needed
  there.
  Evidence: `src/oauth/store.rs` — `write_lock`/`env_lock_key` helpers,
  guarded `put`/`delete` in `impl TokenStore for FileTokenStore` and
  `KeyringTokenStore`; regression test
  `concurrent_puts_to_same_env_do_not_clobber_a_rotated_refresh_token`.
  Verified: full suite 158 passed; `cargo clippy` clean for the file.
  Note: in-process only — cross-process / multi-instance writers would still
  need OS file locking (out of scope; Probe runs single-instance).

---

## Backlog follow-ups (post-review)

- [x] **M5. Surface worker failures** _(medium)_

  Startup `Runtime::new` errors already landed in `self.status`, but rendered
  in the same muted grey as normal messages and were lost once `status`
  changed. Added a **persistent** worker-health badge: when `runtime` is
  `None`, the status bar shows a red "⚠ Runtime offline" label (with a
  hover explaining the cause) that stays until the app is restarted.
  Per-request submit failures already surface via `"Submit error: …"`, and no
  other worker path exits silently.
  Evidence: `src/ui/theme.rs` (`DANGER` const), `src/app.rs` status bar
  (runtime-offline badge next to `self.status`).
  Verified: full suite green.

- [x] **M7. `state_revision` change counter** _(medium)_

  `AppState` now carries a monotonic `revision: u64`, bumped once at the top
  of the single mutation funnel (`apply_intent_to_state`) so every applied
  intent counts exactly once — including no-op-looking intents, since the bump
  precedes dispatch. Exposes `revision()`; field is `pub(crate)` only so
  in-crate constructors can zero-init it.
  Evidence: `src/state/app_state.rs` (`revision` field, `revision()`,
  `bump_revision()`), `src/app.rs:apply_intent_to_state` (bump), test
  `each_applied_intent_bumps_revision_exactly_once`.
  Verified: 159 passed.

- [x] **Migrate panels off static mutexes** _(minor)_

  Both `oauth_panel` (`PANEL_STATE`) and its sibling `environment_editor`
  (`ENVIRONMENT_EDITOR_STATE`) held transient UI state in process-global
  `OnceLock<Mutex<…>>` singletons. Introduced `ui::panel_state::PanelUiState`,
  owned by `ProbeApp` (mirroring `ResponseViewerState`) and threaded through
  `shell::show` → the environment-editor sections → `oauth_panel::show`. Both
  statics and the poison-recovery `with_editor_state` helper are gone; the
  Auth tab mutates the OAuth panel via a disjoint borrow of the holder.
  Evidence: `src/ui/panel_state.rs`, `src/ui/oauth_panel.rs` (`show` now takes
  `&mut OAuthPanelState`), `src/ui/environment_editor.rs`, `src/ui/shell.rs`,
  `src/app.rs` (`panels` field).
  Verified: full suite green; `cargo clippy` clean.

- [x] **Add `clippy::dbg_macro` lint** _(minor)_

  `#![warn(clippy::dbg_macro)]` at the crate root guards against committing a
  `dbg!(…)` that would dump secret-bearing structs to stderr, bypassing the
  `Debug`-redaction work.
  Evidence: `src/main.rs:1`. Verified: `cargo clippy` clean (no `dbg!` calls).

---

## Feature work

- [x] **cURL paste import (URL-bar auto-detect → new request)** _(wishlist #21, Tier 2)_

  Paste a `curl …` command into the request URL field and get a fully-populated
  new request. New self-contained `src/curl_format/` module mirroring the
  `.http` importer: `tokenizer.rs` (shell-style tokenizer — single/double
  quotes, backslash + `\`-newline / `^`-newline continuations, run
  concatenation) and `parser.rs` (`parse_curl` maps `-X`/`--request`, `-H`,
  `-d`/`--data*`/`--json`, `-u`/`--user`, bearer + `--oauth2-bearer`, `-F`/`@file`
  best-effort, query-string splitting via `RequestDraft::adopt_url_query`, and
  curl's method defaulting). Surfaced through a new
  `PanelIntent::ImportCurlAsRequest`, handled in `ProbeApp::apply_intent` (parse
  → `AppState::add_imported_request` → select + `View::Editor`; errors reported
  in `self.status`, no state change). URL-bar routing gated on
  `curl_format::looks_like_curl`, so pasting a curl command creates a new
  request non-destructively while a plain URL behaves as before.
  Scope this pass: URL-bar auto-detect only (dedicated dialog, global clipboard
  paste, and reverse "copy as curl" deferred; the parser is structured so those
  are thin add-ons).
  Evidence: `src/curl_format/{mod,tokenizer,parser}.rs`, `src/main.rs`
  (`mod curl_format;`), `src/ui/intent.rs` (`ImportCurlAsRequest`),
  `src/app.rs` (`apply_intent` handler + funnel no-op arm),
  `src/state/app_state.rs` (`add_imported_request`), `src/ui/request_panel.rs`
  (URL-bar detection).
  Verified: full suite 187 passed (incl. tokenizer + parser unit tests);
  `cargo clippy` clean for `curl_format`; `cargo fmt` applied.
  Note: GUI paste path not exercised headlessly — the parse → `RequestDraft`
  mapping (including the realistic dev-tools command) is covered by unit tests.
