# Probe — TODOs

Open work items. Flip `- [ ]` to `- [x]` when an item is finished, then move its
block to [`docs/DONE.md`](docs/DONE.md). Each item carries its current state and a
**Next** line so it can be resumed cold at any time.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done.
Severity tags (`C` = critical, `M` = medium, `Minor`) carry over from the code review.

---

- [~] **Auto-updater (GitHub Releases, in-place self-update)** _(wishlist #61, Tier 6)_

  State (2026-10-25): steps 1–4 implemented on `feature/workspace`, uncommitted. Verified:
  `cargo test` 250 passed, `--features keyring-storage` 237 passed, clippy clean for the new
  code, release build compiles. A live check against GitHub returned "up to date" (404, no
  releases yet). GUI checked on `DISPLAY=:2` (2026-10-25): badge, manual check → "up to
  date" + `state.json` written, every dialog state (forced via a temporary diagnostic, since
  removed), *Close* after a failure → idle, disabled *Install* tooltip in debug. **Not
  verified:** a real end-to-end update (needs two published releases), the Windows swap. CI files were copied as-is from
  `ci/release-changelog` and then extended with `SHA256SUMS` + the tag/version gate. The user
  runs all git actions themselves.

  **Decisions (2026-07-03):** unauthenticated `GET
  https://api.github.com/repos/neisep/Probe/releases/latest` (repo is public); check at startup at
  most once per 24 h, plus a manual check; full in-place update (download → verify sha256 →
  replace binary → restart). A build only ever picks the asset for its own OS/arch, and if none
  matches it offers nothing.

  **CI base:** `.github/workflows/build.yml` + `.github/scripts/changelog.sh`, copied
  byte-identical from `ci/release-changelog` (`6289e1e`). It already publishes
  `probe-linux-x86_64` and `probe-windows-x86_64.exe` on a `v*` tag.

  **1. CI (`.github/workflows/build.yml`, release job)**
  - Generate `SHA256SUMS` (`sha256sum probe-linux-x86_64 probe-windows-x86_64.exe`) and upload
    it as a release asset.
  - Fail the release if the tag `vX.Y.Z` doesn't equal `Cargo.toml`'s `version`. Otherwise the
    app would always see itself as outdated, or never see an update.

  **2. New module `src/update/`** (pure logic separate from IO, unit-tested)
  - `release.rs` (pure): serde types for the release JSON (`tag_name`, `body`, `html_url`,
    `assets[].name/browser_download_url/size`); `parse_version("vX.Y.Z")` → `(u64,u64,u64)`
    (strict; anything else → no update, so no `semver` crate); `is_newer` against
    `env!("CARGO_PKG_VERSION")`; `asset_name()` built from `std::env::consts::{OS, ARCH}`
    (`linux`/`windows` × `x86_64`, else `None`); `select_asset` by exact name;
    `expected_sha256(sums_text, asset_name)` parses `SHA256SUMS`; `check_due(last, now)` for the
    24 h throttle.
  - `fetch.rs` (IO): a background thread with its own current-thread tokio runtime + reqwest
    returning an `mpsc::Receiver`, mirroring `openapi::source::fetch_url`
    (`src/openapi/source.rs:16`). Sends `User-Agent: probe/<version>` and
    `Accept: application/vnd.github+json`, HTTPS only, timeouts. A 404 (no releases yet) means
    "up to date", not an error. The download streams with `Response::chunk()` under a 256 MB cap.
  - `install.rs` (IO): create the temp file **next to the exe first**, so a non-writable install
    dir fails before downloading. Write the bytes, verify the sha256 (`sha2`), then on unix
    `chmod 0o755` and `rename` over the exe (atomic; the running process keeps the old inode).
    On Windows, rename the running exe to `probe.exe.old`, move the new one into place, and
    remove `*.old` on the next startup. This uses std only, so no `self-replace` crate.
    Capture `current_exe()` **before** replacing, because on Linux it reads `… (deleted)`
    afterwards.
  - `mod.rs`: an `Updater` with `UpdatePhase { Idle, Checking, UpToDate, Available(Release),
    Installing, Installed, Failed(String) }` and a `poll()` called from the frame loop (like
    `poll_pending_openapi_fetch`, `src/app.rs:783`). This keeps `app.rs` to one field plus a few
    calls.

  **3. Persistence:** `save_update_state`/`load_update_state` in `src/persistence/storage.rs`
  via `write_internal_json`, like `save_session_state` (`:304`) →
  `data/.probe/update/state.json` `{ last_checked_at }`. Written only after a successful check.

  **4. UI / app wiring**
  - Bottom status bar: a small `v0.1.0` label on the left. Clicking it runs the manual check
    (`UiCommand::CheckForUpdates`). When an update is found it becomes the accent-coloured
    "Update available: vX.Y.Z", and clicking opens the dialog. Text only, because `⬆` isn't in
    the verified-glyph list.
  - `src/ui/dialogs/update.rs`: current → new version, release notes (plain scrolling text),
    buttons *Install*, *Later*, *Release page* (via the existing `webbrowser` dep). Progress
    while installing; afterwards *Restart now*. Returns an action enum like the other dialogs.
  - Restart: set `restart_exe` and send `ViewportCommand::Close`, so the existing
    unsaved-changes prompt (`src/app.rs:1047`) still applies; cancelling it clears the flag.
    Implement `eframe::App::on_exit(&mut self)` (wgpu build, no glow) to spawn `restart_exe`,
    which inherits cwd, so `./data` stays the same.
  - Startup auto-check only in release builds (`!cfg!(debug_assertions)`). Its failures are only
    logged (`tracing::warn`); manual-check failures go to `self.status`. In debug builds the
    manual check works but *Install* is disabled ("dev build"), so `cargo run` never overwrites
    `target/debug/probe`.

  **New dependency:** `sha2 = "0.10"` only (needs approval). `semver` and `self-replace` from
  the July plan are replaced by std code.

  **Verification**
  - Unit tests: version parse/compare (incl. `v0.10.0 > v0.9.9`, junk tags), asset selection
    per OS/arch and no match, `SHA256SUMS` parse + mismatch rejection, throttle boundary,
    release JSON fixture, 404 → up to date, install into a temp dir (good hash replaces; bad hash
    leaves the original untouched).
  - `cargo test`, `cargo test --features keyring-storage`, `cargo clippy`.
  - GUI (`DISPLAY=:2`): the version label, the manual check against the real repo (no release
    yet → "up to date"), and the dialog layout.
  - End to end, after the first real `v0.1.x` release: run an older release build from a
    scratch dir, then update → restart → new version shown. Windows path untested locally.

  **Open concern:** a startup call to GitHub sits a little against wishlist #1 ("no telemetry
  by default"). It sends no identifiers, but a settings toggle may be wanted later.

  **Next:** commit, tag `v0.1.0`
  (must equal `Cargo.toml`), bump to `0.1.1` and tag again, and update an installed `v0.1.0`
  end to end. Then move this block to `docs/DONE.md`.
