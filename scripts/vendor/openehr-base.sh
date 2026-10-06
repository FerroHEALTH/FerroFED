#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-base.sh
#
# Vendors the openEHR BASE component specification source into
# docs/specs/openehr-base/: the AsciiDoc sources of the Architecture Overview,
# Foundation Types, Base Types and Resource specifications with their
# figures, the class definitions the sources include (`docs/UML/classes` and
# the class index), the component index, and the repository's licence file,
# from the release the "openEHR BASE specification source" row of
# docs/VERSIONS.md pins. That release is the one paired with the pinned RM
# release, whose chapters cite the identifier classes BASE defines.
#
# Left out: the rendered `.html` pages (built from the sources here), the UML
# class diagrams under `docs/UML/diagrams` (5 MB of images exported from the
# UML tool), the UML tool's project files under `computable/`, the UML export
# scripts and the BMM example.
#
# Usage:
#   scripts/vendor/openehr-base.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/openehr-base"

specifications=(architecture_overview foundation_types base_types resource)
paths=(
  "docs/UML/classes"
  "docs/UML/class_index.adoc"
  "docs/index.adoc"
  "README.adoc"
  "LICENSE"
)
for spec in "${specifications[@]}"; do
  paths+=("docs/$spec")
done

pin="$(corpus_pin_cell "openEHR BASE specification source")"
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
dropped="$(corpus_drop_manifests "$dest")"
[ -z "$dropped" ] || die "the release carries a dependency manifest: $dropped"

# The identifier classes the follow-up routing and the access record cite
# must be defined here.
for class in object_version_id object_ref party_ref hier_object_id object_id uid_based_id archetype_id; do
  [ -f "$dest/docs/UML/classes/$class.adoc" ] || die "the release has no docs/UML/classes/$class.adoc"
done
grep -q 'Attribution-ShareAlike 3.0' "$dest/LICENSE" ||
  die "the LICENSE is not Creative Commons Attribution-ShareAlike 3.0"

# BASE 1.1.0 ships no manifest.json; the amendment record of the Base Types
# specification heads its entries with the release, so a tag that ships
# another line fails here.
release="${tag#Release-}"
grep -qF "*BASE Release $release*" "$dest/docs/base_types/master00-amendment_record.adoc" ||
  die "the Base Types amendment record names no BASE Release $release"
rm_tag="$(corpus_pin_tag "$(corpus_pin_cell "openEHR Reference Model specification source")")"
statuses=""
for spec in "${specifications[@]}"; do
  status="$(sed -nE 's/^:spec_status:[[:space:]]*([A-Z_]+).*/\1/p' "$dest/docs/$spec/master.adoc" | head -n1)"
  [ -n "$status" ] || die "docs/$spec/master.adoc declares no spec_status"
  statuses="${statuses:+$statuses, }$spec $status"
done

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR BASE component specification source

Vendored verbatim by \`scripts/vendor/openehr-base.sh\`. Never edit a file
here: change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, published at
  <https://specifications.openehr.org/releases/BASE/$tag/>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`; the amendment
  record of the Base Types specification heads its entries \`BASE Release
  $release\`, and the specifications declare the status $statuses
- Paired with: the openEHR Reference Model \`$rm_tag\` vendored under
  \`docs/specs/openehr-rm/\`, whose chapters cite the classes defined here
- Fetched: $fetched
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's \`LICENSE\` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #702 (the identifier classes \`OBJECT_VERSION_ID\`,
  \`OBJECT_REF\` and \`PARTY_REF\` the follow-up routing and the access
  record cite)

## What is here

- \`docs/<specification>/\`: one directory per BASE specification
  (architecture overview, foundation types, base types, resource), the
  AsciiDoc chapters with their figures.
- \`docs/UML/classes/\`, \`docs/UML/class_index.adoc\`: the class definition
  tables the chapters include, one file per class.
- \`docs/index.adoc\`, \`README.adoc\`: the component index and the
  repository's description.

The rendered \`.html\` pages, the UML class diagrams under
\`docs/UML/diagrams/\`, the UML tool's project files under \`computable/\`,
the UML export scripts and \`example/example.bmm\` are left out.
PROV

say "$files files, tree digest $digest"
say "done"
