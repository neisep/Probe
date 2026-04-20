# OAuth2 Support — Spec, Architecture & Roadmap

Design doc for adding OAuth2 to Probe (Rust + egui REST client).
Intended as a handoff doc: anyone should be able to pick this up and continue.

---

## 1. OAuth2 Flows & Fields

### Auth Code + PKCE
- **Required:** `auth_url`, `token_url`, `client_id`, `redirect_uri`, `scopes`, `code_verifier`, `code_challenge`
- **Optional:** `client_secret` (confidential clients), `audience`, `resource`, `prompt`, `login_hint`, extra auth/token params

### Client Credentials
- **Required:** `token_url`, `client_id`, `client_secret`, `scopes`
- **Optional:** `audience`, `resource`, `assertion` (JWT bearer), extra token params

### Device Code
- **Required:** `device_auth_url`, `token_url`, `client_id`, `scopes`
- **Optional:** `client_secret`, `audience`, polling interval override

### Refresh
- **Required:** `token_url`, `client_id`, `refresh_token`
- **Optional:** `client_secret`, `scopes` (downscope)

---

## 2. UX (egui)

- Auth tab per environment with flow dropdown
- Fields render dynamically based on selected flow
- `[Get Token]` → launches system browser via `open` crate
- `[Reset Token]` clears cache for that environment
- Token status pill: `expires in 42m` / `expired` / `none`
- Header override: default `Authorization: Bearer`, editable
- Collapsible "Advanced" section for `audience` / `resource` / extra params
- Per-request checkbox: "Attach OAuth2 token"

---

## 3. Rust Architecture

```
src/oauth/
  mod.rs            # Flow enum + TokenRequest trait
  pkce.rs           # 128-char verifier (rand) + sha256→base64url challenge
  flows/
    auth_code.rs
    client_creds.rs
    device.rs
    refresh.rs
  browser.rs        # open::that(url) + loopback listener (tiny_http on 127.0.0.1:<random>)
  store.rs          # TokenStore keyed by (env_id, flow_id)
  refresh.rs        # auto-refresh if exp < now + 60s; fall back to re-auth on failure
```

- Integrates into the existing request pipeline as pre-send middleware.

---

## 4. Per-Environment Token Storage

- **Path:** `data/.probe/tokens/{env_id}.json`
- **Format:**
  ```json
  {
    "flow": "auth_code",
    "access_token": "...",
    "refresh_token": "...",
    "expires_at": "2026-04-20T12:34:56Z",
    "scopes": ["..."],
    "obtained_at": "2026-04-20T11:34:56Z"
  }
  ```
- **Encryption:** optional via `keyring` crate (OS keychain); fallback is plaintext with `0600` perms + `.gitignore`
- **Invalidation:** TTL check on load + manual reset
- **Reset:** deletes file + in-memory cache entry

---

## 5. Roadmap (PR-sized)

| # | PR | Scope |
|---|---|---|
| 1 | `oauth` skeleton | Flow enum, `TokenStore` trait, file backend |
| 2 | Auth Code + PKCE | PKCE helpers, loopback redirect listener, flow impl |
| 3 | egui config panel | Flow dropdown, dynamic fields, per-env binding |
| 4 | Client Credentials + auto-refresh | CC flow, refresh flow, pre-send middleware |
| 5 | Device Code | Flow impl + polling UI |
| 6 | Token UX polish | Status pill, reset button, header override |
| 7 | Keyring encryption | Behind a feature flag |
| 8 | Advanced params + docs | `audience`/`resource`, extra params, user docs |

---

## 6. Open Questions

- Do we support multiple tokens per environment (e.g. different scopes)?
- Should token refresh happen on a background thread or lazily on request send?
- Confidential vs public client toggle — UI or inferred from presence of `client_secret`?
