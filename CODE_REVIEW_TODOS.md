# Code Review — TODO List

From analysis of OAuth2 + OpenAPI import changes (2026-04-25).

---

CREATED A NEW BRANCH WERE WE PUSHED THE THINGS TOO.

## Critical — Fix Before Shipping

- [ DONE ] **`app.rs:449`** — `.unwrap()` on `pending_openapi_import` after an earlier `.is_none()` guard; can panic if value is `None` by the time it's consumed. Use `if let` instead.
- [ DONE ] **`oauth_panel.rs:72-75`** — Mutex poisoning not handled; if any async flow panics while holding `PANEL_STATE`, all subsequent lock attempts will panic. Use `.unwrap_or_else` / poisoned-lock recovery.
- [ DONE ] **`oauth/browser.rs:43-59`** — Loopback HTTP request reader doubles buffer with no upper bound; a large or malicious redirect payload can exhaust memory. Add a max read size.
- [ DONE ] **`oauth/browser.rs:96-97`** — Response write and `stream.shutdown()` both use `let _ =`, silently eating errors. Log or propagate so failed callbacks are visible.
- [ DONE ] **`openapi/source.rs:7-29`** — Thread-panic payload is discarded (`map_err(|_| ...)`); tokio runtime build failures are also swallowed. Capture the panic message and surface it to the user.

---

## Duplicate Code — Extract Shared Helpers

- [ DONE ] **`now_unix()` defined 3×** — `oauth/middleware.rs:97-102`, `oauth/flows/device_code.rs:144-149`, `oauth/flows/client_credentials.rs:74-79`. Extract to `oauth/util.rs` or similar.
- [ DONE ] **Token expiry + scopes computation copy-pasted in all 3 flows** — `device_code.rs:102-111`, `client_credentials.rs:54-62`, `auth_code.rs` (similar block). Extract to a shared `build_cached_token()` helper.
- [ DONE ] **`extra_auth_params` loop duplicated** — `auth_code.rs:46-47`, `client_credentials.rs:45-46`, `device_code.rs:64-68`. Same pattern in all three.

---

## Dead Code — Remove

- [ DONE ] **`app.rs:1290–1372`** — Six unused helper functions: `build_request_preview_from_prepared_request`, `build_request_preview_from_error`, `preview_request_name`, `preview_query_params`, `preview_body_from_bytes`, `preview_body_from_text`. Delete or wire up.
- [ DONE ] **`oauth/browser.rs:28-30`** — `LoopbackListener::port()` is public but never called.
- [ DONE ] **`runtime/types.rs:44-50`** — `Cancelled` and `Failed` are used in `executor.rs`; removed stale `#[allow(dead_code)]`.
- [ DONE ] **`http_format/parser.rs:110,149-154`** — `@tag` directive is parsed but explicitly discarded (`let _ = tags`). Either wire it up or remove the parse branch.

---

## Performance

- [ DONE ] **`app.rs:1420-1429`** — Full deep clone of `AppState` on every workspace export. Added `WorkspaceBundleRef<'a>` with borrowed fields; removed `build_workspace_bundle`.
- [ DONE ] **`oauth/middleware.rs:40`** — Token file loaded from disk on every auth check. Added static `AUTH_CACHE` keyed by `base_dir:env_id` with TTL = `token.expires_at - REFRESH_BUFFER_SECONDS`.
- [ DONE ] **`app.rs:1551-1593`** — `apply_auth_headers` changed from `&[(String, String)]` to owned `Vec`; `extend` now moves instead of cloning.

---

## Low / Quality

- [ DONE ] **`openapi/parser.rs:298+`** — Swagger 2.x never extracts example bodies. Added `Swagger2Schema`, `extract_body_example2`, parity with OpenAPI 3.x.
- [ DONE ] **`openapi/merge.rs:40-41`** — Re-import now uses `merge_query_params`: preserves user-filled values for spec params and appends user-added custom params.
- [ DONE ] **`app.rs:1393`** — Backup path now uses `PathBuf::from(crate::oauth::DATA_DIR).join("backups")`.
- [ DONE ] **Magic numbers** — Added `DEFAULT_TOKEN_LIFETIME_SECONDS` in `flows/mod.rs`, `LOOPBACK_BIND_ADDR` and `LOOPBACK_READ_BUF_SIZE` in `browser.rs`.
- [ DONE ] **`oauth/middleware.rs:110-120`** — Replaced manual `temp_dir()` + `remove_dir_all` with a RAII `TempDir` struct that cleans up on drop.
