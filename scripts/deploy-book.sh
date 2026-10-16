#!/usr/bin/env bash
# Put a built mdBook on the `gh-pages` branch, one directory per release line, so older versions stay readable.
#
#   scripts/deploy-book.sh BOOK_DIR TAG [REMOTE]
#
#   BOOK_DIR  the output of `mdbook build docs --dest-dir BOOK_DIR`
#   TAG       the release tag, `v0.3.0`; the directory is `v0.3` (major.minor), so 0.3.1 replaces 0.3.0's pages
#   REMOTE    the git remote that holds the branch (default `origin`)
#
# Layout written on the branch: `v<major>.<minor>/`, `latest/` (a copy of the newest release's book), `index.html`
# (a redirect to `latest/`), `.nojekyll`, and `.latest-version`. A pre-release (`v0.4.0-rc.1`) gets its own
# directory only. A release older than `.latest-version` does not touch `latest/`. The branch is created when it
# does not exist. The branch is pushed; nothing here enables GitHub Pages (that is a repository setting).
#
# Run from inside a clone of the repository (the workflow's checkout). Authentication is whatever the clone
# already has for REMOTE.
set -euo pipefail

BOOK=${1:?usage: deploy-book.sh BOOK_DIR TAG [REMOTE]}
TAG=${2:?usage: deploy-book.sh BOOK_DIR TAG [REMOTE]}
REMOTE=${3:-origin}
BRANCH=gh-pages

if [[ ! -f "$BOOK/index.html" ]]; then
  echo "deploy-book: $BOOK/index.html does not exist; build the book first" >&2
  exit 1
fi
if [[ ! "$TAG" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "deploy-book: '$TAG' is not a release tag (vMAJOR.MINOR.PATCH)" >&2
  exit 1
fi
VERSION=${TAG#v}
LINE="v${BASH_REMATCH[1]}.${BASH_REMATCH[2]}"
PRE=${BASH_REMATCH[4]}

WORK=$(mktemp -d)
trap 'git worktree remove --force "$WORK" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT

if git ls-remote --exit-code --heads "$REMOTE" "$BRANCH" >/dev/null 2>&1; then
  git fetch --quiet "$REMOTE" "$BRANCH"
  git worktree add --quiet --detach "$WORK" FETCH_HEAD
else
  git worktree add --quiet --detach "$WORK"
  git -C "$WORK" checkout --quiet --orphan "$BRANCH"
  git -C "$WORK" rm -rf --quiet . >/dev/null 2>&1 || true
fi

rm -rf "${WORK:?}/${LINE}"
mkdir -p "$WORK/$LINE"
cp -R "$BOOK"/. "$WORK/$LINE/"

CURRENT=""
[[ -f "$WORK/.latest-version" ]] && CURRENT=$(cat "$WORK/.latest-version")
if [[ -z "$PRE" ]] && { [[ -z "$CURRENT" ]] || [[ "$(printf '%s\n%s\n' "$CURRENT" "$VERSION" | sort -V | tail -n 1)" == "$VERSION" ]]; }; then
  rm -rf "${WORK:?}/latest"
  mkdir -p "$WORK/latest"
  cp -R "$BOOK"/. "$WORK/latest/"
  printf '%s\n' "$VERSION" > "$WORK/.latest-version"
fi

cat > "$WORK/index.html" <<'HTML'
<!doctype html>
<meta charset="utf-8">
<title>oaxaca_blinder</title>
<meta http-equiv="refresh" content="0; url=latest/">
<link rel="canonical" href="latest/">
<a href="latest/">oaxaca_blinder documentation</a>
HTML
: > "$WORK/.nojekyll"

git -C "$WORK" add --all .
if git -C "$WORK" diff --cached --quiet; then
  echo "deploy-book: nothing changed on $BRANCH"
  exit 0
fi
git -C "$WORK" \
  -c user.name="github-actions[bot]" \
  -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
  commit --quiet -m "docs: book for $TAG"
git -C "$WORK" push --quiet "$REMOTE" "HEAD:refs/heads/$BRANCH"
echo "deploy-book: $TAG published to $BRANCH under $LINE/$([[ -f "$WORK/latest/index.html" && "$(cat "$WORK/.latest-version")" == "$VERSION" ]] && echo ' and latest/')"
