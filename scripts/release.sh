#!/usr/bin/env bash
# Cut a release: bump the version in Cargo.toml, commit, tag vX.Y.Z and push.
# The tag starts .github/workflows/release.yml, which builds the app and
# publishes the GitHub Release.
#
#   scripts/release.sh 0.2.0      # exact version
#   scripts/release.sh patch      # 0.1.0 -> 0.1.1   (also: minor, major)
#   NO_PUSH=1 scripts/release.sh minor   # commit + tag only
set -euo pipefail
cd "$(dirname "$0")/.."

arg="${1:-}"
[[ -n "$arg" ]] || { echo "usage: $0 <X.Y.Z | patch | minor | major>" >&2; exit 2; }

cur="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
IFS=. read -r ma mi pa <<<"${cur%%-*}"
case "$arg" in
  patch) new="$ma.$mi.$((pa + 1))" ;;
  minor) new="$ma.$((mi + 1)).0" ;;
  major) new="$((ma + 1)).0.0" ;;
  *) new="${arg#v}" ;;
esac
[[ "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "not a semver: $new" >&2; exit 2; }

branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$branch" == "main" ]] || { echo "switch to main first (on $branch)" >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo "working tree is not clean" >&2; exit 1; }
git fetch -q origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || { echo "main is not in sync with origin/main" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$new" >/dev/null && { echo "tag v$new already exists" >&2; exit 1; }

echo "Release $cur -> $new"
# Only the [package] version (first match), not dependency versions.
perl -0pi -e "s/^version *= *\"\Q$cur\E\"/version = \"$new\"/m" Cargo.toml
cargo update -p cmdmeow --offline >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null

git add Cargo.toml Cargo.lock
git commit -m "Release v$new"
git tag -a "v$new" -m "Cmd+Meow $new"

if [[ "${NO_PUSH:-0}" == "1" ]]; then
  echo "Tagged v$new. Push with: git push origin main v$new"
else
  git push origin main "v$new"
  echo "Pushed v$new. Release build: https://github.com/romanfurman6/cmdmeow/actions/workflows/release.yml"
fi
