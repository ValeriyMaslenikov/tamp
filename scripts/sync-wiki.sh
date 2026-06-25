#!/usr/bin/env bash
#
# Manual fallback for publishing the wiki — the CI counterpart is
# .github/workflows/publish-wiki.yml. Mirrors docs/wiki/ (the source of truth)
# into the GitHub Wiki repo (<repo>.wiki.git) using your own git credentials.
#
# GitHub exposes wikis only over git (there is no Wiki REST API), so this is a
# plain clone → copy → commit → push.
#
# PREREQUISITE: the wiki must be initialized once via the GitHub UI (the repo's
# Wiki tab → "Create the first page") before .wiki.git exists.
#
# Usage:
#   scripts/sync-wiki.sh                 # uses the default repo below
#   WIKI_REPO=git@github.com:you/repo.wiki.git scripts/sync-wiki.sh
#   DRY_RUN=1 scripts/sync-wiki.sh       # show what would change, don't push

set -euo pipefail

WIKI_REPO="${WIKI_REPO:-https://github.com/ValeriyMaslenikov/tamp.wiki.git}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/docs/wiki"

if [ ! -d "$SRC" ]; then
  echo "error: $SRC not found" >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Cloning $WIKI_REPO …"
git clone --quiet "$WIKI_REPO" "$TMP/wiki"

# Replace the page set: drop old top-level *.md (keep .git history), copy ours.
find "$TMP/wiki" -maxdepth 1 -name '*.md' -delete
cp "$SRC"/*.md "$TMP/wiki/"

cd "$TMP/wiki"
git add -A

if git diff --cached --quiet; then
  echo "Wiki already up to date — nothing to sync."
  exit 0
fi

echo "Changes to publish:"
git --no-pager diff --cached --stat

if [ "${DRY_RUN:-0}" = "1" ]; then
  echo "DRY_RUN=1 — not committing or pushing."
  exit 0
fi

git commit --quiet -m "docs(wiki): sync from docs/wiki/"
git push --quiet
echo "Wiki updated."
