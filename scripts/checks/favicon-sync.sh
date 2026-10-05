#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The book theme favicons and the operator console's favicon are copies of the
# brand favicon, not files of their own (no specification governs this: our
# own design). mdBook reads a theme override from
# website/book/theme/favicon.svg and favicon.png, and cargo-leptos copies
# app/ferrofed-viewer/public/favicon.ico into the console's site root, so the
# mark exists more than once and nothing but this check keeps the copies in
# step: change the brand mark, forget a copy, and it serves the old favicon.
#
#   scripts/checks/favicon-sync.sh
#
# Exit 0 when every copy is byte-identical to its source. Exit 1 naming the
# file that differs and the command that regenerates it.
set -euo pipefail
cd "$(dirname "$0")/../.."

# One "copy<TAB>source" record per line. The PNG source is the 32-pixel raster
# of the same SVG, which is what assets/brand/README.md's regeneration block
# writes to both paths.
readonly PAIRS="\
website/book/theme/favicon.svg	assets/brand/favicon.svg
website/book/theme/favicon.png	assets/brand/favicon-32.png
app/ferrofed-viewer/public/favicon.ico	assets/brand/favicon.ico"

fail=0
while IFS=$'\t' read -r copy source; do
  [[ -n "$copy" ]] || continue
  if [[ ! -f "$source" ]]; then
    echo "favicon-sync: $source is missing; the brand directory is the source of both copies." >&2
    fail=1
    continue
  fi
  if [[ ! -f "$copy" ]]; then
    echo "favicon-sync: $copy is missing; regenerate it from $source (assets/brand/README.md)." >&2
    fail=1
    continue
  fi
  if cmp -s "$copy" "$source"; then
    echo "favicon-sync: $copy matches $source"
    continue
  fi
  echo "::error file=$copy::$copy differs from $source. Re-run the regeneration block in assets/brand/README.md so every copy serves the current mark." >&2
  fail=1
done <<< "$PAIRS"

exit "$fail"
