# Probe

A lightweight HTTP client built with Rust and egui.

---

## OAuth2

Probe supports three OAuth2 flows, configured per environment under the **Auth** tab.

### Flows

| Flow | When to use |
|---|---|
| Authorization Code + PKCE | Interactive login; opens browser, handles callback |
| Client Credentials | Service-to-service; no user interaction |
| Device Code | Devices without a browser; shows a user code to enter elsewhere |

### Configuration

Select a flow from the dropdown, fill in the required fields, then click **Get token**.

**Authorization Code + PKCE**
- Authorize URL, Token URL, Client ID — required
- Client secret — only for confidential clients
- Scopes — space-separated (e.g. `openid profile email`)

**Client Credentials**
- Token URL, Client ID, Client secret — required
- Scopes — optional

**Device Code**
- Device auth URL, Token URL, Client ID — required
- Client secret — optional for public clients
- Scopes — optional

All flows share an **Advanced** section for `audience`, `resource`, and arbitrary extra parameters.

### Token lifecycle

- Tokens are stored in `.probe/oauth_tokens/` (one JSON file per environment).
- Before each request, the token is checked: if it expires within 60 seconds, Probe attempts a refresh automatically.
- The **status pill** in the Auth tab shows `expires in Xm`, `expired`, or `no token`.
- Click **Reset token** to clear the stored token for the active environment.

### Header injection

By default, the token is injected as `Authorization: Bearer <token>`.

To customise, expand **Header injection** in the Auth tab:
- **Header name** — defaults to `Authorization`
- **Header prefix** — defaults to `Bearer`; leave blank for a raw token value
- **Inject token into requests** — uncheck to disable injection without deleting the token

If you manually set the same header on a request, the OAuth token will not overwrite it.

### Keyring storage (optional)

Build with `--features keyring-storage` to store tokens in the OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service) instead of plaintext files:

```sh
cargo build --features keyring-storage
```
