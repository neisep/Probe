#!/usr/bin/env bash
# Prints markdown release notes for <tag>, built from the conventional commits since the previous tag.
# Usage: changelog.sh <tag> [owner/repo]   (repo defaults to $GITHUB_REPOSITORY)
set -euo pipefail

tag="${1:?usage: changelog.sh <tag> [owner/repo]}"
repo="${2:-${GITHUB_REPOSITORY:-}}"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null || { echo "unknown tag: $tag" >&2; exit 1; }

prev="$(git describe --tags --abbrev=0 "$tag^" 2>/dev/null || true)"
subjects="$(git log --no-merges --format='%s' "${prev:+$prev..}$tag")"
conventional='^[a-z]+(\([^)]*\))?!?: '

# Prints "### <heading>" plus one bullet per commit whose type matches <types>; "fix(ui): x" -> "- **ui:** x".
section() {
  local lines
  lines="$(grep -E "^($2)(\([^)]*\))?!?: " <<<"$subjects" \
    | sed -E 's/^[a-z]+\(([^)]+)\)!?: /- **\1:** /; s/^[a-z]+(\(\))?!?: /- /' || true)"
  if [[ -n "$lines" ]]; then printf '### %s\n\n%s\n\n' "$1" "$lines"; fi
}

notes="$(section Added feat; section Fixed fix; section Improved 'perf|refactor')"
other="$(grep -vE "$conventional" <<<"$subjects" | grep -v '^$' | sed 's/^/- /' || true)"
if [[ -n "$other" ]]; then notes+=$'\n\n'"### Other"$'\n\n'"$other"; fi

printf '%s\n' "${notes:-Maintenance release.}"
if [[ -n "$prev" && -n "$repo" ]]; then
  printf '\n**Full changelog:** https://github.com/%s/compare/%s...%s\n' "$repo" "$prev" "$tag"
fi
