# Code Analysis — OAuth2 + OpenAPI changes

Date: 2026-04-25
Branch: `feature/oauth2`
Scope: 12 uncommitted modified files (OAuth2 flows, OpenAPI import/merge, middleware, app glue).

Read all 12 modified files plus their close neighbours (`oauth/mod.rs`, `oauth/store.rs`, `oauth/config.rs`, `oauth/flows/refresh.rs`, `openapi/source.rs`, `ui/oauth_panel.rs`). Findings ranked by what would actually want fixing before merging.

---

## Real correctness gaps (smaller blast radius)

**8. `openapi/merge.rs:33-61` re-orders the user's collection on re-import**
- Existing requests that match an `import_key` are reordered to follow the *spec's* iteration order, not the user's prior position.
- Hand-crafted requests are always pushed to the bottom of the list.

`compute_merge` is correct on a per-request basis but disruptive at the collection level. If reordering is intentional, fine — if not, preserve original positions and append truly-new ops at the end.

**9. `flows/mod.rs:18-22 collect_extra_params` never trims `v` and won't dedupe against `audience`/`resource`**
If the user adds `("audience", "...")` as an extra param while also setting the dedicated `audience` field, both get sent. Most servers tolerate this; some (Auth0) don't. Trim values, and at least dedupe keys.

---

## Duplication that wasn't removed (the prior TODO checklist marked these done — they aren't)

**10. `now_unix()` is still defined three times.** `crate::oauth::now_unix` exists at `oauth/mod.rs:24`, but `oauth/flows/refresh.rs:68` and `ui/oauth_panel.rs:567` each have their own copy. The TODO list claims this was extracted; the middleware and `flows/mod.rs` were updated, the other two were not.

**11. `refresh.rs:45-65` still hand-builds the `Token` instead of calling `build_cached_token`.** `build_cached_token` was added in `flows/mod.rs:35` exactly for this case (it has a `fallback_refresh_token` parameter so refresh can supply the prior refresh_token), but `refresh::run` still has its own `expires_at` / `scopes` / `refresh_token` block. The two implementations are equivalent today; they will drift.

**12. `normalized_optional` exists in two places.** `oauth/config.rs:118 normalize_optional` and `ui/oauth_panel.rs:574 normalized_optional` are byte-identical. Re-export one.

**13. `spawn_auth_code_flow`, `spawn_client_credentials_flow`, `spawn_device_code_flow` (`oauth_panel.rs:594/617/640`)** are three near-clones of the same `spawn thread → build current_thread runtime → block_on → send result` pattern. Worth a generic helper that takes a future and a result mapper.

**14. `flows/{auth_code,client_credentials,device_code}.rs` — the placeholder `AuthUrl::new("http://localhost/")` trick repeats in three places.** The `oauth2` crate forces you to pass an `AuthUrl` even for non-interactive flows; collapse the boilerplate into a helper (`build_basic_client_with_token_only(...)`).

---

## Performance (after the items above)

**15. `middleware.rs:130 block_on_refresh` builds a fresh tokio runtime every call.** Once the cache is busted, every refresh pays runtime construction. Not huge (~ms), but trivially avoidable with a `OnceLock<tokio::runtime::Runtime>` if the synchronous shape is kept.

**16. `merge::compute_merge` does an O(n) `existing.iter().find(...)` inside `merge_query_params` per spec param.** Fine for tens of params, quadratic on large specs (Stripe-class). Build a `HashMap<&str, &str>` of existing once.

**17. `app.rs:75 PendingOpenApiImport.merged`** holds the entire merged `Vec<RequestDraft>` between preview and confirm. For a thousand-operation spec this is real memory not needed — the merge is cheap, recompute on confirm.

---

## Lower priority / cosmetic

- `oauth/browser.rs:42-63` — the read loop is correct but has an awkward dance with `MAX_REQUEST_SIZE`. Once the buffer hits 64K and the marker isn't seen, the error is raised — fine, but the `if buf.len() >= MAX_REQUEST_SIZE` check inside the resize branch is redundant if the resize is also bounded by `min(MAX)`. Tighten or just unify.
- `oauth/browser.rs:20` — `LOOPBACK_BIND_ADDR.parse().expect(...)` is a hard panic on a constant; harmless because the constant is fixed, but a `const` `SocketAddr` (or the `Ipv4Addr::LOCALHOST` form) would remove the runtime parse altogether.
- `openapi/source.rs:7` — no scheme allowlist on the URL. `reqwest::get` will refuse `file://`, but a malicious paste of a spec URL that redirects to an internal address still works. Consider an allowlist of `http`/`https` and surfacing redirects.
- `runtime/types.rs:22 #[allow(dead_code)]` is still on `UnresolvedBehavior` even though both variants are constructed in `app.rs`. Drop the attribute.
- `app.rs:1056-1059` — close-without-saving doesn't account for an open `pending_openapi_import` or `pending_workspace_import`; user could lose their staged import. The close prompt only mentions "unsaved changes" generically.
- `oauth_panel.rs:436/441` — `let _ = webbrowser::open(...)` silently eats failures (a TODO already flagged the same pattern in `browser.rs`); keep behaviour consistent.

---

## What was checked and is fine

- `LoopbackListener` shutdown / read-bound: the 64KB cap and explicit error logging are now in place (prior TODO items #3, #4 are real).
- `WorkspaceBundleRef<'a>` borrowed export path is wired up (prior TODO #15 done).
- `MergePreview` re-import correctness: `merge_query_params` preserves user-filled values for spec params and keeps user-added custom params (test `user_query_values_and_custom_params_preserved_on_update` covers it).
- `validate_key` on token-store paths blocks traversal (`..`, `/`) and the `atomic_write` path uses `path.with_extension("tmp")` correctly.
- `parse_request` correctly round-trips `# @probe-import-key` (test `import_key_directive_round_trips`).
- `Swagger2Schema::example` extraction is in place (prior TODO #11 done).

---

## TL;DR

Two items to treat as ship-blockers:
1. **Stale `AUTH_CACHE` after Reset Token / config change** — silent bug, user thinks the token is cleared, requests still go out.
2. **OAuth refresh and OpenAPI URL fetch run synchronously on the egui thread** — UI freezes during real-world use.

Everything else is improvable but not blocking. The duplication items 10-12 are worth fixing now because the prior TODO list claimed they were done — you'll trip over them later if you trust the checklist.
