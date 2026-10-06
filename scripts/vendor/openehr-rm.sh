#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-rm.sh
#
# Vendors the openEHR Reference Model specification source into
# docs/specs/openehr-rm/: the AsciiDoc sources of the RM specifications with
# their figures, the class definitions the sources include (`docs/UML/classes`
# and the class index), the component index and manifest, and the
# repository's licence file, from the release the "openEHR Reference Model
# specification source" row of docs/VERSIONS.md pins.
#
# Left out: the rendered `.html` pages (built from the sources here), the UML
# class diagrams under `docs/UML/diagrams` (16 MB of images exported from the
# UML tool), and the UML tool's project files under `computable/`.
#
# Usage:
#   scripts/vendor/openehr-rm.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/openehr-rm"

paths=(
  "docs/common"
  "docs/data_structures"
  "docs/data_types"
  "docs/demographic"
  "docs/ehr"
  "docs/ehr_extract"
  "docs/integration"
  "docs/support"
  "docs/UML/classes"
  "docs/UML/class_index.adoc"
  "docs/index.adoc"
  "manifest.json"
  "README.adoc"
  "LICENSE"
)

pin="$(corpus_pin_cell "openEHR Reference Model specification source")"
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

# The classes the routing and the classification read must be defined here.
for class in locatable archetyped version original_version composition ehr_status dv_text term_mapping; do
  [ -f "$dest/docs/UML/classes/$class.adoc" ] || die "the release has no docs/UML/classes/$class.adoc"
done
grep -q 'Attribution-ShareAlike 3.0' "$dest/LICENSE" ||
  die "the LICENSE is not Creative Commons Attribution-ShareAlike 3.0"

# The component manifest lists every release with its date; the one the tag
# names must be there and dated, so a tag that ships an unreleased line fails
# here.
release="${tag#Release-}"
release_date="$(jq -r --arg id "$release" '.releases[] | select(.id == $id) | .date' "$dest/manifest.json")"
[ -n "$release_date" ] || die "manifest.json lists no dated release $release"
statuses="$(jq -r '[.specifications[] | .id + " " + .spec_status] | join(", ")' "$dest/manifest.json")"

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR Reference Model specification source

Vendored verbatim by \`scripts/vendor/openehr-rm.sh\`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>, published at
  <https://specifications.openehr.org/releases/RM/$tag/>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`; the
  \`manifest.json\` of that commit dates release \`$release\` \`$release_date\`
  and declares the specifications $statuses
- Fetched: $fetched
- Upstream licence: Creative Commons Attribution-ShareAlike 3.0 Unported, the
  repository's \`LICENSE\` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #696 (the RM facts the follow-up routing and the A57
  classification cite: \`LOCATABLE\`, \`archetype_details\`, the change
  control classes, \`DV_TEXT.mappings\` and \`TERM_MAPPING\`), and
  \`scripts/checks/openehr-classes.sh\` (#719), which holds every class and
  attribute the tree names to the definitions under \`docs/UML/classes/\`

## What is here

- \`docs/<specification>/\`: one directory per RM specification (common, data
  structures, data types, demographic, EHR, EHR extract, integration,
  support), the AsciiDoc chapters with their figures.
- \`docs/UML/classes/\`, \`docs/UML/class_index.adoc\`: the class definition
  tables the chapters include, one file per class.
- \`docs/index.adoc\`, \`manifest.json\`, \`README.adoc\`: the component index
  and its release manifest.

The rendered \`.html\` pages, the UML class diagrams under
\`docs/UML/diagrams/\` and the UML tool's project files under
\`computable/\` are left out. The identifier classes (\`OBJECT_VERSION_ID\`,
\`OBJECT_REF\`, \`PARTY_REF\`) are defined in the openEHR BASE component,
which the RM chapters cite; \`docs/specs/openehr-base/\` carries its paired
release (\`scripts/vendor/openehr-base.sh\`).
PROV

say "$files files, tree digest $digest"
say "done"
