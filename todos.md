# Probe — TODOs

Open work items. Flip `- [ ]` to `- [x]` when an item is finished, then move its
block to [`docs/DONE.md`](docs/DONE.md). Each item carries its current state and a
**Next** line so it can be resumed cold at any time.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done.
Severity tags (`C` = critical, `M` = medium, `Minor`) carry over from the code review.

---

- [~] **Signed releases + CI supply-chain hardening** _(follow-up to the auto-updater)_

  Goal: a compromised GitHub account or CI pipeline must not be able to ship an update
  clients accept. Design: the maintainer signs `SHA256SUMS` with an offline minisign key;
  the public key is compiled in; the trusted comment `probe <tag>` binds the signature to
  the version (stops replaying old signed releases under a newer tag). Not covered, by
  choice: key rotation and freeze protection (TUF territory).

  State (2026-10-25): implemented, uncommitted.
  - App: `src/update/signature.rs` (`minisign-verify`, approved); `release.rs` requires
    `SHA256SUMS.minisig` and accepts only `https://github.com/neisep/Probe/releases/download/<tag>/<asset>`
    URLs; `fetch.rs` verifies the signature before trusting the checksums.
    `src/update/release-key.pub` holds the real release public key (key ID
    `FD103FD4E4ADD88F`, created 2026-10-25); a test asserts it decodes.
  - CI: read-only token except the `release` job; actions pinned to SHAs; no cache;
    `--locked`; tag passed via env; fails if the key is still the placeholder; creates a
    **draft** release; the site trigger moved to the signing script.
  - `.github/scripts/sign-release.sh`: verifies assets against `SHA256SUMS`, caps glibc
    at 2.35, signs, checks the result with the key from the tag (pre-hashed, right
    comment), uploads, publishes, triggers the site.
  - `docs/RELEASING.md`: release flow, key creation and backup, GitHub settings.

  Verified: `cargo test` (signature tests use a throwaway key with minisign-format
  vectors made independently in Python), clippy, workflow YAML, `bash -n` on the script,
  and its checks against test data and the real v0.1.2 assets. **Not verified:** the
  script against a real draft, and a real signed update.

  **Next:** the user backs up the key (`docs/RELEASING.md` § 2), then commits and tags
  `v0.1.3`. The user runs `sign-release.sh v0.1.3`. Then, end to end on
  `:2`: a local `PROBE_VERSION=0.1.2` build updates to the signed v0.1.3. A negative test:
  an unsigned draft must not be offered. Then move this block to `docs/DONE.md`.
