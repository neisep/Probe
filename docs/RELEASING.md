# Releasing Probe

Probe updates itself from GitHub Releases, so a release is something every installed copy
will run. A release is only accepted by the app if its `SHA256SUMS` is signed with the
**Probe release key**, which lives on the maintainer's machine and never on GitHub. CI can
build a release, but only the maintainer can make clients accept it.

## How a release flows

1. Push a tag: `git tag v0.1.3 && git push origin v0.1.3`. The tag is the version; nothing
   in `Cargo.toml` needs to change.
2. CI (`.github/workflows/build.yml`) builds Linux and Windows and creates a **draft** release
   with the binaries and `SHA256SUMS`. Drafts are invisible to the updater and the website.
3. On your machine, run:

   ```sh
   .github/scripts/sign-release.sh v0.1.3
   ```

   It downloads the draft and checks that the binaries match `SHA256SUMS` and that the
   Linux binary doesn't need a newer glibc than 2.35. Then it shows the checksums and asks
   before signing. After signing it uploads `SHA256SUMS.minisig`, publishes the release and
   triggers the website rebuild.

The app refuses an update unless:

- the signature verifies with the public key compiled into it (`src/update/release-key.pub`),
- the signature's trusted comment is exactly `probe <tag>`, so an old signed release can't be
  re-published under a newer tag,
- the tag is strictly newer than the running version,
- every file comes from that release's own download folder on `github.com/neisep/Probe`,
- the binary's SHA-256 matches the signed `SHA256SUMS`.

## One-time setup

### 1. Create the release key

Run this from the Probe repository root, since `-p` is a relative path:

```sh
sudo apt install minisign
mkdir -p ~/.minisign && chmod 700 ~/.minisign
minisign -G -s ~/.minisign/probe-release.key -p src/update/release-key.pub
```

If the public key file is missing or wrong, recreate it from the secret key with
`minisign -R -s ~/.minisign/probe-release.key -p src/update/release-key.pub`.

Pick a strong password. `release-key.pub` is the public half and gets committed;
`probe-release.key` is the secret and must never be committed or uploaded anywhere.

Until `release-key.pub` holds a real key, the app refuses every update and the release
workflow fails at *Check the release public key is real*.

### 2. Back the key up

If the key is lost, installed copies can never be updated again: they'd need a manual
reinstall with a new key. Keep a copy of `~/.minisign/probe-release.key` offline, e.g. on an
encrypted USB stick in a drawer, together with a note of the password somewhere safe.

If the key leaks, an attacker still also needs a way to publish a release on GitHub, but
treat it as urgent: create a new key, release a version whose `release-key.pub` is the new
key (signed with the old one, while it still works), and stop using the old key.

### 3. Harden the GitHub account and repository

These are settings, not code, and they protect the step *before* signing:

- **Two-factor login** with a passkey or hardware key on the GitHub account.
- **Immutable releases** (Settings → General → Releases), if available for the repository:
  published release files can't be replaced afterwards.
- **Tag protection**: a ruleset under Settings → Rules that only lets you create and delete
  `v*` tags.
- **Branch protection** on `master`: no force pushes.
- Optional: an environment with *required reviewers* around the `release` job, so even a
  draft needs your click.
- `SITE_DISPATCH_TOKEN` is no longer used by this workflow; the signing script triggers the
  website with your own `gh` login. It can be deleted from the repository's secrets.

## What the workflow does to limit supply-chain risk

- The token is read-only except in the final `release` job, which runs no build code.
- Every action is pinned to a full commit SHA. Update them deliberately, not by tag.
- Release builds run without a cache and with `cargo build --locked`.
- The tag is passed to scripts through environment variables, never pasted into them.
- Linux is built on `ubuntu-22.04` so the binary runs on Ubuntu 22.04 and newer.

## Transition note

Copies at v0.1.2 and older don't check signatures. They'll install the first signed release
like any other, and from then on every update must be signed.
