#!/usr/bin/env bash
# Archive a released tag's docs as a static docs version.
#
# Latest (the unprefixed docs) always represents the NEWEST release, so
# archives are only created for superseded releases: on releasing X.Y.Z the
# release flow calls this with the PREVIOUS stable tag. Also usable directly to
# backfill any historical stable tag.
#
# The docs are taken from the tag itself (git archive), swapped into
# src/content/docs, archived by a starlight-versions build, and the working
# docs are restored afterwards — including on failure.
set -euo pipefail

TAG="${1:?usage: archive-tag.sh vX.Y.Z}"
SLUG="${TAG#v}"
cd "$(dirname "$0")/.."
DOCS=src/content/docs

case "$SLUG" in
  *-*)
    echo "prerelease ${TAG}: not archived"
    exit 0
    ;;
esac
if ! git -C .. rev-parse -q --verify "refs/tags/${TAG}" >/dev/null; then
  echo "tag ${TAG} does not exist" >&2
  exit 1
fi
if [ -d "$DOCS/$SLUG" ]; then
  echo "version ${SLUG} is already archived"
  exit 0
fi

# Top-level docs entries that are not version archives (X.Y.Z dirs).
latest_entries() {
  find "$DOCS" -mindepth 1 -maxdepth 1 ! -name '[0-9]*.[0-9]*.[0-9]*'
}

TMP="$(mktemp -d)"
restore_latest() {
  if [ -d "$TMP/latest" ]; then
    latest_entries | while read -r entry; do rm -rf "$entry"; done
    cp -R "$TMP/latest/." "$DOCS/"
  fi
  rm -rf "$TMP"
}
trap restore_latest EXIT

# 1. Set the working (latest) docs aside and swap in the tag's docs.
mkdir -p "$TMP/latest"
latest_entries | while read -r entry; do cp -R "$entry" "$TMP/latest/"; done
latest_entries | while read -r entry; do rm -rf "$entry"; done
git -C .. archive "$TAG" docs-site/src/content/docs | tar -x -C "$TMP"
find "$TMP/docs-site/src/content/docs" -mindepth 1 -maxdepth 1 \
  ! -name '[0-9]*.[0-9]*.[0-9]*' -exec cp -R {} "$DOCS/" \;

# 2. Legacy tags (pre relative-link migration) carry root-absolute /shki/
#    links; rewrite them to relative so the archive stays inside its version.
if grep -rq '](/shki/' "$DOCS" --include='*.md' --include='*.mdx'; then
  perl -pi -e 's{\]\(/shki/}{](../../}g' "$DOCS"/*/*.md
  perl -pi -e 's{\]\(/shki/}{](../}g' "$DOCS"/*.md
  perl -pi -e 's{\]\(/shki/}{](}g; s{link: /shki/}{link: }g' "$DOCS"/index.mdx
  if grep -rq '](/shki/' "$DOCS" --include='*.md' --include='*.mdx'; then
    echo "unrewritten /shki/ links remain for ${TAG}" >&2
    exit 1
  fi
fi

# 3. Register the version; the build archives the (tag) docs under its slug.
bun scripts/add-version.mjs "$SLUG"
bun install --frozen-lockfile
bun run build
if [ ! -d "$DOCS/$SLUG" ]; then
  echo "archive ${SLUG} was not created by the build" >&2
  exit 1
fi

# 4. Put the latest docs back, order versions newest-first, and rebuild so the
#    switcher and dist reflect the final state.
restore_latest
trap - EXIT
node -e '
  const fs = require("fs");
  const data = JSON.parse(fs.readFileSync("versions.json", "utf8"));
  data.versions.sort((a, b) => b.slug.localeCompare(a.slug, undefined, { numeric: true }));
  fs.writeFileSync("versions.json", JSON.stringify(data, null, 2) + "\n");
'
bun run build
echo "archived ${SLUG} from ${TAG}"
