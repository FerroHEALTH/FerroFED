#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# assemble.sh: build the site the way GitHub Pages serves it, from the landing
# page and the book, into one directory.
#
#   scripts/site/assemble.sh OUT
#
# The landing page lands at the site root and the mdBook under /docs/, which is
# what book.toml's site-url declares. Two markers of the landing pages are
# rendered on the way, and a marker nothing renders fails the assembly:
#
#   * {{version:ferrofed}} becomes the [workspace.package] version of the
#     root Cargo.toml, so the hero badge always names the release main
#     carries.
#   * {{hash:style.css}} becomes the first twelve hex digits of the
#     stylesheet's sha256, so a browser that cached an older stylesheet
#     fetches the new one with the new page.
#
# No specification governs the assembly: our own design.
#
# The brand assets (assets/brand) and llms.txt live outside the landing
# directory and are copied in here, because both are addressed from the site
# root.
#
# The script leaves the source tree untouched. The book is rendered into a
# temporary directory through `mdbook build --dest-dir` and copied from there,
# so assembling the site on a tree you are mid-edit on never rebuilds
# website/book/book/ behind your back.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly OUT="${1:?usage: $0 OUT}"
readonly LANDING=website/landing

# product_version: the [workspace.package] version of the root Cargo.toml.
product_version() {
  awk '/^\[workspace\.package\]/ { p = 1; next } /^\[/ { p = 0 }
    p && $1 == "version" { if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit } }' Cargo.toml
}

# One temporary tree for the rendered book, removed on every exit path.
# `mdbook build --dest-dir` resolves a relative path against the book root, so
# the absolute path mktemp returns is what keeps the output out of
# website/book/.
book="$(mktemp -d)"
source="$(mktemp -d)"
component=""
trap 'rm -rf "$book" "$source" ${component:+"$component"}' EXIT

# The conformance pages are rendered from the matrix and the test markers, so
# a page that disagrees with them fails the build before mdBook runs (#95).
bash scripts/conformance/matrix.sh --check-pages

# The statement block and the tests page read the markers of every test file,
# so the repository holds placeholders and the site renders both into its own
# copy of the book, which keeps a rendering from going stale on main (#912).
# The copy keeps the repository's layout, so the Changelog page's include of
# ../../../CHANGELOG.md resolves as it does in the checkout.
staged="$source/website/book"
mkdir -p "$staged"
cp CHANGELOG.md "$source/"
cp -R website/book/src website/book/book.toml "$staged/"
for extra in website/book/vendor website/book/js website/book/theme; do
  if [[ -e "$extra" ]]; then cp -R "$extra" "$staged/"; fi
done
bash scripts/conformance/matrix.sh --statement-write-into "$staged/src" > /dev/null

mdbook build "$staged" --dest-dir "$book"

rm -rf "${OUT:?}"
mkdir -p "$OUT/docs"
cp -R "$LANDING"/. "$OUT/"
cp -R "$book"/. "$OUT/docs/"

# The brand directory holds the favicons and the mark the landing page links,
# so it has to reach the site root for those URLs to resolve.
mkdir -p "$OUT/assets"
cp -R assets/brand "$OUT/assets/"

# The llms.txt convention puts the file at the site root, which is the one URL
# that makes it useful.
cp llms.txt "$OUT/llms.txt"

# Each FHIR definition FerroFED publishes, a <Type>-<id>.json beside the code
# that reads it, is served at its ferrofed.eu URL, fhir/<Type>/<id> from the
# site root, and beside it as <id>.json, which GitHub Pages serves as
# application/json (FHIR R4 2.5.1: the definition SHALL be available to every
# party that exchanges the extension). A NamingSystem has no canonical URL in
# R4, so it is served at the identifier system it names, fhir/sid/<id>.
# The interoperability component's remaining-dispensations extension is
# FerroBRIDGE's, defined and served under ferrobridge.eu since eehrxf moved
# there (#1432), so the site serves no definition of it.

readonly DEFINITIONS=(
  app/ferrofed-identity/src/ihe/mcsd/definition
  app/ferrofed-registry/src
  app/ferrofed-server/src/access
  app/ferrofed-server/src/fhir/definition
)
for dir in "${DEFINITIONS[@]}"; do
  for definition in "$dir"/StructureDefinition-*.json "$dir"/CodeSystem-*.json \
    "$dir"/NamingSystem-*.json; do
    [[ -e "$definition" ]] || continue
    name="$(basename "$definition" .json)"
    type="${name%%-*}"
    id="${name#*-}"
    case "$type" in
      NamingSystem) path="fhir/sid" ;;
      *) path="fhir/$type" ;;
    esac
    if [[ -e "$OUT/$path/$id" ]]; then
      echo "assemble: two definitions are served at $path/$id" >&2
      exit 1
    fi
    mkdir -p "$OUT/$path"
    cp "$definition" "$OUT/$path/$id"
    cp "$definition" "$OUT/$path/$id.json"
  done
done

version="$(product_version)"
if [[ -z "$version" ]]; then
  echo "assemble: Cargo.toml has no [workspace.package] version" >&2
  exit 1
fi
style_hash="$(shasum -a 256 "$OUT/style.css" | cut -c1-12)"
while IFS= read -r page; do
  VERSION="$version" STYLE_HASH="$style_hash" perl -pi -e \
    's/\{\{version:ferrofed\}\}/$ENV{VERSION}/g; s/\{\{hash:style\.css\}\}/$ENV{STYLE_HASH}/g' "$page"
  if grep -q '{{' "$page"; then
    echo "assemble: $page keeps a marker nothing renders" >&2
    exit 1
  fi
done < <(find "$OUT" -path "$OUT/docs" -prune -o -name '*.html' -print)

echo "assemble: site at $OUT"
