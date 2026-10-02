#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/aql.sh
#
# Vendors the openEHR AQL specification source into docs/specs/aql/: the
# AsciiDoc sources of the AQL and AQL examples documents, their figures, the
# ANTLR grammar the AQL document publishes (AqlLexer.g4, AqlParser.g4), the
# component index and manifest, and the repository's licence file, from the
# release the "openEHR AQL specification source" row of docs/VERSIONS.md pins.
#
# The rendered `.html` pages of the same release are left out: they are built
# from the sources here and carry no content of their own.
#
# Usage:
#   scripts/vendor/aql.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/aql"
grammar="docs/AQL/grammar"

paths=(
  "docs/AQL"
  "docs/AQL_examples"
  "docs/index.adoc"
  "manifest.json"
  "README.adoc"
  "LICENSE"
)

pin="$(corpus_pin_cell "openEHR AQL specification source")"
repo="$(corpus_pin_repo "$pin")"
tag="$(corpus_pin_tag "$pin")"
commit="$(corpus_resolve_tag "$repo" "$tag")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo tag $tag resolves to $commit"
tree_root="$(corpus_fetch "$repo" "$commit" "$tmp")"

rm -rf "$dest"
mkdir -p "$dest"
corpus_take "$tree_root" "$dest" "${paths[@]}"

for file in AqlLexer.g4 AqlParser.g4; do
  [ -f "$dest/$grammar/$file" ] || die "the release has no $grammar/$file"
done
grep -q 'Attribution-ShareAlike 3.0' "$dest/LICENSE" ||
  die "the LICENSE is not Creative Commons Attribution-ShareAlike 3.0"

# The component manifest lists every release with its date; the one the tag
# names must be there and dated, so a tag that ships an unreleased line fails
# here. The AQL document's own lifecycle status is recorded beside it.
release="${tag#Release-}"
release_date="$(jq -r --arg id "$release" '.releases[] | select(.id == $id) | .date' "$dest/manifest.json")"
[ -n "$release_date" ] || die "manifest.json lists no dated release $release"
aql_status="$(jq -r '.specifications[] | select(.id == "AQL") | .spec_status' "$dest/manifest.json")"
[ -n "$aql_status" ] || die "manifest.json declares no spec_status for AQL"

rows=""
while IFS= read -r file; do
  path="${file#"$dest"/}"
  rows="$rows
| \`$path\` | \`$(corpus_sha256 "$file")\` |"
done < <(find "$dest/$grammar" -type f | LC_ALL=C sort)

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR AQL specification source

Vendored verbatim by \`scripts/vendor/aql.sh\`
(.claude/rules/vendored-inputs.md). Never edit a file here: change the pin in
docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`; the
  \`manifest.json\` of that commit dates release \`$release\` \`$release_date\`
  and declares the AQL document \`$aql_status\`
- Fetched: $fetched
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's \`LICENSE\` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #18 (the rewrite on the AQL grammar and examples)

## What is here

- \`docs/AQL/\`: the AQL specification, one AsciiDoc file per chapter, with
  its figures.
- \`docs/AQL/grammar/\`: the ANTLR 4 grammar the specification publishes as
  its syntax reference.
- \`docs/AQL_examples/\`: the AQL examples document.
- \`docs/index.adoc\`, \`manifest.json\`, \`README.adoc\`: the component index
  and its release manifest.

The rendered \`.html\` pages of the release are left out; they are built from
these sources.

| Grammar file | sha256 |
|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
