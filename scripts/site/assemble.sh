#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# assemble.sh: build the site the way GitHub Pages serves it, from the landing
# page and the book, into one directory.
#
#   scripts/site/assemble.sh OUT
#
# The landing page lands at the site root and the mdBook under /docs/, which is
# what book.toml's site-url declares. The landing page's roadmap block (between
# `<!-- roadmap:begin -->` and `<!-- roadmap:end -->`) is rendered here from the
# tracker's open milestones through the GitHub API when `gh` is authenticated.
# Without a token the block stays empty and the page keeps its link to the
# milestones page, so an assembly on a laptop never fails on the network.
# SITE_ROADMAP=off leaves the block empty without asking the API, which is how
# the offline link guard (scripts/checks/site-links.sh) assembles the site.
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
readonly REPO=FerroHEALTH/FerroFED

# The open milestones as list items, oldest due date first. Every field that
# reaches the page is HTML-escaped, because a milestone title is text somebody
# typed.
roadmap() {
  gh api "repos/$REPO/milestones?state=open&sort=due_on&direction=asc" --jq '
    def esc: gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;");
    ["<ul class=\"roadmap\">"]
    + map("  <li><span class=\"tag\">" + (.title | esc) + "</span>"
          + "<a href=\"" + .html_url + "\" rel=\"noopener\">" + (.open_issues | tostring) + " open, " + (.closed_issues | tostring) + " closed</a>"
          + (if .due_on == null then "" else "<span class=\"due\">due " + .due_on[0:10] + "</span>" end)
          + "</li>")
    + ["</ul>"] | .[]'
}

# One temporary tree for the rendered book and one file for the roadmap block,
# both removed on every exit path. `mdbook build --dest-dir` resolves a
# relative path against the book root, so the absolute path mktemp returns is
# what keeps the output out of website/book/.
book="$(mktemp -d)"
block="$(mktemp)"
source="$(mktemp -d)"
component=""
trap 'rm -rf "$book" "$source" ${component:+"$component"}; rm -f "$block"' EXIT

# The conformance pages are rendered from the matrix and the test markers, so
# a page that disagrees with them fails the build before mdBook runs (#95).
bash scripts/conformance/matrix.sh --check-pages

# The statement block and the tests page read the markers of every test file,
# so the repository holds placeholders and the site renders both into its own
# copy of the book, which keeps a rendering from going stale on main (#912).
cp -R website/book/src website/book/book.toml "$source/"
for extra in website/book/vendor website/book/js website/book/theme; do
  if [[ -e "$extra" ]]; then cp -R "$extra" "$source/"; fi
done
bash scripts/conformance/matrix.sh --statement-write-into "$source/src" > /dev/null

mdbook build "$source" --dest-dir "$book"

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
# The interoperability component, eehrxf, is published from
# FerroHEALTH/FerroBRIDGE (#813), and its remaining-dispensations extension
# keeps its ferrofed.eu canonical URL: its definitions are read from the crate
# archive crates.io serves at the version Cargo.lock resolves, held to the
# checksum the lock records.
component="$(mktemp -d)"
component_version="$(awk '$0 == "name = \"eehrxf\"" { f = 1; next } f && /^version = / { gsub(/^version = "|"$/, ""); print; exit }' Cargo.lock)"
component_sum="$(awk '$0 == "name = \"eehrxf\"" { f = 1; next } f && /^checksum = / { gsub(/^checksum = "|"$/, ""); print; exit } /^\[\[package\]\]/ { f = 0 }' Cargo.lock)"
[[ -n "$component_version" && -n "$component_sum" ]] || {
  echo "assemble: Cargo.lock resolves no published eehrxf" >&2
  exit 1
}
curl --proto '=https' --tlsv1.2 -fsSL --retry 3 \
  -o "$component/eehrxf.crate" "https://static.crates.io/crates/eehrxf/eehrxf-$component_version.crate"
if [[ "$(shasum -a 256 "$component/eehrxf.crate" | cut -d' ' -f1)" != "$component_sum" ]]; then
  echo "assemble: eehrxf $component_version does not match the checksum Cargo.lock records" >&2
  exit 1
fi
tar -xzf "$component/eehrxf.crate" -C "$component"

readonly DEFINITIONS=(
  "$component/eehrxf-$component_version/src/medication"
  app/ferrofed-identity/src/ihe/mcsd/definition
  app/ferrofed-registry/src
  app/ferrofed-server/src/access
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

if [[ "${SITE_ROADMAP:-on}" == off ]]; then
  echo "assemble: SITE_ROADMAP=off, so the roadmap block stays empty."
elif roadmap > "$block" 2>/dev/null && [[ -s "$block" ]]; then
  awk -v blockfile="$block" '
    /<!-- roadmap:begin -->/ { print; while ((getline line < blockfile) > 0) print line; skip = 1; next }
    /<!-- roadmap:end -->/ { skip = 0 }
    !skip { print }
  ' "$LANDING/index.html" > "$OUT/index.html"
  echo "assemble: roadmap rendered from the open milestones."
else
  echo "assemble: no GitHub API access, so the roadmap block stays empty." >&2
fi

echo "assemble: site at $OUT"
