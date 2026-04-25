# Code Review — TODO List

From analysis of OAuth2 + OpenAPI import changes (2026-04-25).

---

## Critical — Fix Before Shipping

- [ DONE ] **`app.rs:449`** — `.unwrap()` on `pending_openapi_import` after an earlier `.is_none()` guard; can panic if value is `None` by the time it's consumed. Use `if let` instead.
- [ DONE ] **`oauth_panel.rs:72-75`** — Mutex poisoning not handled; if any async flow panics while holding `PANEL_STATE`, all subsequent lock attempts will panic. Use `.unwrap_or_else` / poisoned-lock recovery.
- [ DONE ] **`oauth/browser.rs:43-59`** — Loopback HTTP request reader doubles buffer with no upper bound; a large or malicious redirect payload can exhaust memory. Add a max read size.
- [ DONE ] **`oauth/browser.rs:96-97`** — Response write and `stream.shutdown()` both use `let _ =`, silently eating errors. Log or propagate so failed callbacks are visible.
- [ DONE ] **`openapi/source.rs:7-29`** — Thread-panic payload is discarded (`map_err(|_| ...)`); tokio runtime build failures are also swallowed. Capture the panic message and surface it to the user.

---

## Duplicate Code — Extract Shared Helpers

- [ DONE ] **`now_unix()` defined 3×** — `oauth/middleware.rs:97-102`, `oauth/flows/device_code.rs:144-149`, `oauth/flows/client_credentials.rs:74-79`. Extract to `oauth/util.rs` or similar.
- [ ] **Token expiry + scopes computation copy-pasted in all 3 flows** — `device_code.rs:102-111`, `client_credentials.rs:54-62`, `auth_code.rs` (similar block). Extract to a shared `build_cached_token()` helper.
- [ ] **`extra_auth_params` loop duplicated** — `auth_code.rs:46-47`, `client_credentials.rs:45-46`, `device_code.rs:64-68`. Same pattern in all three.

---

## Dead Code — Remove

- [ ] **`app.rs:1290–1372`** — Six unused helper functions: `build_request_preview_from_prepared_request`, `build_request_preview_from_error`, `preview_request_name`, `preview_query_params`, `preview_body_from_bytes`, `preview_body_from_text`. Delete or wire up.
- [ ] **`oauth/browser.rs:28-30`** — `LoopbackListener::port()` is public but never called.
- [ ] **`runtime/types.rs:44-50`** — `RequestStatus::Cancelled` and `RequestStatus::Failed` are never constructed or matched. Remove or implement.
- [ ] **`http_format/parser.rs:110,149-154`** — `@tag` directive is parsed but explicitly discarded (`let _ = tags`). Either wire it up or remove the parse branch.

---

## Performance

- [ ] **`app.rs:1420-1429`** — Full deep clone of `AppState` (requests, responses, etc.) on every workspace export. Will lag with large collections.
- [ ] **`oauth/middleware.rs:40`** — Entire token file loaded from disk on every authorization check. Cache the loaded tokens or load only what's needed.
- [ ] **`app.rs:1551-1593`** — Multiple vector copies and `.iter().cloned()` passes during auth header injection into prepared requests.

---

## Low / Quality

- [ ] **`openapi/parser.rs:298+`** — Swagger 2.x never extracts example bodies (`body_example: None` always). OpenAPI 3.x has `extract_body_example3` — add parity for 2.x.
- [ ] **`openapi/merge.rs:40-41`** — Re-import overwrites query params without checking for user-added custom values. Orphaned custom params possible.
- [ ] **`app.rs:1393`** — Backup path hardcoded as `"./data/backups"` (relative to process CWD). Compute relative to storage root instead.
- [ ] **Magic numbers** — `3600` in `device_code.rs:106` and `client_credentials.rs:58`; `4096` in `browser.rs:43`; `"127.0.0.1:0"` in `browser.rs:17` and `source.rs:8`. Replace with named constants.
- [ ] **`oauth/middleware.rs:110-120`** — Test temp directories not cleaned up on test failure. Use `tempfile` crate or add cleanup on failure.
