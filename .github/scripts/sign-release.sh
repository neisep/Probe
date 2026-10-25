#!/usr/bin/env bash
# Verifies, signs and publishes a draft Probe release. Run on the maintainer's machine, never in CI.
# Usage: .github/scripts/sign-release.sh vX.Y.Z
# Needs: gh (logged in as the maintainer), minisign, sha256sum, objdump (binutils).
# Signing key: $PROBE_SIGNING_KEY, default ~/.minisign/probe-release.key. See docs/RELEASING.md.
set -euo pipefail

repo="neisep/Probe"
site_repo="neisep/neisep.github.io"
key="${PROBE_SIGNING_KEY:-$HOME/.minisign/probe-release.key}"
# Newest glibc the Linux binary may need (Ubuntu 22.04). Newer breaks Ubuntu 24.04 / Mint 22.
max_glibc="GLIBC_2.35"
tag="${1:?usage: sign-release.sh vX.Y.Z}"

die() { echo "error: $*" >&2; exit 1; }

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "tag must look like vX.Y.Z"
for tool in gh minisign sha256sum objdump; do
  command -v "$tool" >/dev/null || die "$tool is not installed"
done
[[ -f "$key" ]] || die "signing key not found: $key"

is_draft="$(gh release view "$tag" -R "$repo" --json isDraft --jq .isDraft)" ||
  die "no release $tag in $repo"
[[ "$is_draft" == "true" ]] || die "$tag is already published; only drafts are signed"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "Downloading $tag…"
gh release download "$tag" -R "$repo" -D "$work" -p SHA256SUMS -p 'probe-*'
# The public key compiled into this release, which is what the app will verify with.
gh api "repos/$repo/contents/src/update/release-key.pub?ref=$tag" --jq .content |
  base64 -d >"$work/release-key.pub"

# Every binary must be listed with a matching hash, and every listed file must exist.
(cd "$work" && sha256sum --strict -c SHA256SUMS) || die "assets don't match SHA256SUMS"
for file in "$work"/probe-*; do
  name="$(basename "$file")"
  grep -q "  $name\$" "$work/SHA256SUMS" || die "$name is not listed in SHA256SUMS"
done

glibc="$(objdump -T "$work/probe-linux-x86_64" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)"
newest="$(printf '%s\n' "$glibc" "$max_glibc" | sort -V | tail -1)"
[[ "$newest" == "$max_glibc" ]] || die "Linux binary needs $glibc, newer than $max_glibc"

echo
echo "Release $tag"
echo "  Linux binary needs $glibc (limit $max_glibc)"
echo "  Checksums to sign:"
sed 's/^/    /' "$work/SHA256SUMS"
echo
read -r -p "Sign and publish $tag? [y/N] " answer
[[ "$answer" == [yY] ]] || die "aborted, nothing was signed"

# The trusted comment binds the signature to this version; the app requires it.
minisign -S -s "$key" -m "$work/SHA256SUMS" -x "$work/SHA256SUMS.minisig" -t "probe $tag"

# Check the way the app will: with the key in the release, and pre-hashed ("ED").
minisign -V -q -p "$work/release-key.pub" -m "$work/SHA256SUMS" -x "$work/SHA256SUMS.minisig" ||
  die "signature doesn't verify with the release's public key (wrong signing key?)"
algorithm="$(sed -n 2p "$work/SHA256SUMS.minisig" | base64 -d 2>/dev/null | head -c 2)"
[[ "$algorithm" == "ED" ]] || die "signature is not pre-hashed; update minisign"
grep -qx "trusted comment: probe $tag" "$work/SHA256SUMS.minisig" ||
  die "signature lacks the trusted comment 'probe $tag'"

gh release upload "$tag" "$work/SHA256SUMS.minisig" -R "$repo"
gh release edit "$tag" -R "$repo" --draft=false --latest
gh workflow run deploy.yml -R "$site_repo" --ref master

echo "Published $tag. Clients will offer it on their next update check."
