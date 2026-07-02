#!/usr/bin/env bash
# fetch-dng-fixtures — download real-camera DNG fixtures for RawlerDecoder tests
#
# Real camera raws run tens of MB, too large to commit directly, so they are
# hosted as GitHub Release assets in this repository instead and fetched here
# rather than checked into git history. Same integrity pattern as the macOS
# SDK fetch in .github/workflows/ci.yml: a pinned SHA256 is verified before
# the file is trusted. See docs/DECISIONS.md D10 and docs/ROADMAP.md "Real DNG
# fixture" for the background.
#
# Invoked via `just fixtures`. Idempotent: skips a fixture already present
# with a matching checksum, so re-runs on a dev box are free.

set -euo pipefail

DEST_DIR="tests/fixtures/dng"
RELEASE_TAG="fixtures-dng-v1"
RELEASE_BASE="https://github.com/attila/lenslab/releases/download/$RELEASE_TAG"

# One "name:sha256" entry per fixture, uploaded as an asset on the
# $RELEASE_TAG release. See docs/ROADMAP.md "Real DNG fixture" for what each
# fixture covers.
FIXTURES=(
    "xtrans_xt3.dng:06221d01ba5d40be34b780c1abedcf94f93c200138369dbc10b217d1e346a034"
    "bayer_k1.dng:5fb695961202601a0704829a8e977e36f99ff96a823081379a4d3dd94e17ad95"
)

if [ "${#FIXTURES[@]}" -eq 0 ]; then
    echo "fetch-dng-fixtures: no fixtures configured yet in $0 — nothing to fetch." >&2
    exit 0
fi

mkdir -p "$DEST_DIR"

for entry in "${FIXTURES[@]}"; do
    name="${entry%%:*}"
    sha256="${entry##*:}"
    dest="$DEST_DIR/$name"

    if [ -f "$dest" ] && echo "$sha256  $dest" | sha256sum -c - >/dev/null 2>&1; then
        echo "$name: already present, checksum OK"
        continue
    fi

    echo "$name: fetching from $RELEASE_BASE/$name"
    curl -fsSL -o "$dest" "$RELEASE_BASE/$name"
    echo "$sha256  $dest" | sha256sum -c -
done
