# Probe — TODOs

Open work items. Flip `- [ ]` to `- [x]` when an item is finished, then move its
block to [`docs/DONE.md`](docs/DONE.md). Each item carries its current state and a
**Next** line so it can be resumed cold at any time.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done.
Severity tags (`C` = critical, `M` = medium, `Minor`) carry over from the code review.

---

- [~] **M5 — Surface worker failures** _(medium · partly done)_
  - **Goal:** worker/runtime failures are visible to the user, never silent.
  - **State:** startup client-build failure is **already** surfaced — `executor.rs:102-113` sends the error over `ready_tx`, `Runtime::new` (`executor.rs:168-178`) returns `Err`. Per-request errors flow via `Event::Completed { AsyncRequestResult::Err(ErrorInfo) }` (`types.rs:148-164`), polled in `app.rs`.
  - **Done when:** the `Runtime::new` `Err` is rendered in the UI (not swallowed) and no other silent worker-exit paths remain.
  - **Next:** trace where `app.rs` constructs `Runtime` and confirm the `Err` reaches the UI.

- [ ] **M7 — `state_revision` change counter** _(medium)_
  - **Goal:** a monotonically bumped revision on `AppState` mutations for dirty-tracking / cheap change detection, and to make `apply_intent` testable.
  - **State:** `AppState` (`state/app_state.rs:7-14`) has no revision/dirty field; all data mutations are centralized in `apply_intent_to_state` (`app.rs:919+`).
  - **Done when:** the revision bumps once per applied intent, asserted by a unit test.
  - **Next:** add `revision: u64` to `AppState` and bump it at the top of `apply_intent_to_state` (`app.rs:919`).

- [ ] **Migrate `oauth_panel` off its static mutex** _(minor)_
  - **Goal:** hold OAuth panel state on app/UI state like the other panels, not in a process-global static.
  - **State:** `static PANEL_STATE: OnceLock<Mutex<OAuthPanelState>>` (`oauth_panel.rs:70`), struct `:34-44`, accessor `panel_state()` `:72-74`; every other panel reads `UIState` off `AppState`.
  - **Caveat:** `OAuthPanelState` holds non-`Serialize` transients (`mpsc::Receiver`, `Instant`), so relocation needs a non-persisted transient sub-struct.
  - **Done when:** OAuth panel state lives in app/UI state and the static mutex is gone.
  - **Next:** introduce a transient holder on the UI state and move `OAuthPanelState`'s fields onto it.

- [ ] **Add `clippy::dbg_macro` lint** _(minor)_
  - **Goal:** prevent accidental `dbg!` of secret-bearing structs (locks in the Debug-redaction work).
  - **State:** binary crate, root `src/main.rs` (no `lib.rs`); module declarations `main.rs:1-11`; no `dbg!` calls today.
  - **Done when:** `#![warn(clippy::dbg_macro)]` sits at `main.rs:1` and `cargo clippy` is clean.
  - **Next:** add the attribute at the top of `src/main.rs`.
