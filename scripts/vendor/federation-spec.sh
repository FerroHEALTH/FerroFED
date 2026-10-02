#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/federation-spec.sh
#
# Vendors the source of the Federation Tier with AQL specification into
# docs/specs/federation-spec/: the whole
# repository tree at the commit the "Federation Tier with AQL specification"
# row of docs/VERSIONS.md pins, minus the upstream's npm manifests for its
# Antora build, which provenance lists with their sha256.
#
# The tree carries the normative AsciiDoc pages (the N# requirements and the
# CP# conformance points), the two published JSON schemas
# (options-root.schema.json, federated-result-set.schema.json), the PlantUML
# sources and rendered diagrams, and the upstream's own check scripts.
#
# Usage:
#   scripts/vendor/federation-spec.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/federation-spec"
attachments="modules/ROOT/attachments"
schemas=(
  "$attachments/options-root.schema.json"
  "$attachments/federated-result-set.schema.json"
)

pin="$(corpus_pin_cell "Federation Tier with AQL specification")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"
matrix_version="$(corpus_pin_cell "Federation Tier with AQL")"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo at $commit"
tree_root="$(corpus_fetch "$repo" "$commit" "$tmp")"
[ -f "$tree_root/LICENSE" ] || die "the archive has no LICENSE"
grep -q 'CC0 1.0 Universal' "$tree_root/LICENSE" || die "the LICENSE is not CC0 1.0 Universal"
for path in "${schemas[@]}"; do
  [ -f "$tree_root/$path" ] || die "the archive has no $path"
  jq -e . "$tree_root/$path" > /dev/null || die "$path is not JSON"
done

# The version the document declares, which is what a release moves.
spec_version="$(sed -nE "s/^[[:space:]]*spec-version:[[:space:]]*'([^']+)'.*/\1/p" "$tree_root/antora.yml")"
spec_status="$(sed -nE "s/^[[:space:]]*spec-status:[[:space:]]*'([^']+)'.*/\1/p" "$tree_root/antora.yml")"
spec_date="$(sed -nE "s/^[[:space:]]*spec-date:[[:space:]]*'([^']+)'.*/\1/p" "$tree_root/antora.yml")"
[ -n "$spec_version" ] || die "antora.yml declares no spec-version"
[ "$spec_version" = "$matrix_version" ] ||
  die "antora.yml declares spec-version $spec_version, docs/VERSIONS.md pins $matrix_version"

dropped="$(corpus_drop_manifests "$tree_root")"

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$tree_root" "$dest"

schema_rows=""
for path in "${schemas[@]}"; do
  schema_rows="$schema_rows
| \`$path\` | \`$(corpus_sha256 "$dest/$path")\` |"
done

files="$(corpus_file_count "$dest")"
pages="$(find "$dest/modules/ROOT/pages" -type f -name '*.adoc' | wc -l | tr -d '[:space:]')"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier with AQL specification source

The whole repository tree, vendored verbatim by
\`scripts/vendor/federation-spec.sh\`, less the dependency manifests listed
below. Never edit a file here: change the pin in docs/VERSIONS.md and re-run
the script.

- Source: <https://github.com/$repo>
- Pin: commit \`$commit\`
- Declared version: \`spec-version: '$spec_version'\`, status
  \`$spec_status\`, dated \`$spec_date\` (\`antora.yml\`)
- Fetched: $fetched
- Upstream licence: Creative Commons Zero v1.0 Universal (CC0 1.0), the
  repository's \`LICENSE\` file, vendored beside this file
- Layout: the upstream paths, unchanged
- Files: $files, of which $pages are \`modules/ROOT/pages/*.adoc\`
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #25 (the CP and N traceability of the conformance instrument), #33 (the wire types validated against both schemas) and #17 (the re-pin to 1.0)

## What is here

- \`modules/ROOT/pages/\`: the normative body (§1 to §18), the two annexes
  (A, the IHE binding; B, the Dutch Generic Functions) and the change record.
  The N# requirements are in \`requirements.adoc\` and the CP# conformance
  points in \`conformance.adoc\`.
- \`modules/ROOT/attachments/\`: the two published JSON schemas.
- \`diagrams/\` and \`modules/ROOT/images/\`: the PlantUML sources and the
  rendered figures.
- \`tools/\`: the upstream's own reference, anchor, schema and traceability
  checks, with the requirement-to-conformance-point table
  \`traceability.tsv\`.
- \`.github/workflows/\`: the upstream's site build. Inert here, since GitHub
  runs only the workflows at the repository root.

| Schema | sha256 |
|---|---|$schema_rows

## What is left out

The upstream's npm manifests for its Antora site build. They are not
specification content, and a vendored manifest makes this repository's
dependency graph claim an npm toolchain it does not use.

| File | sha256 |
|---|---|
$dropped

## The version

The pinned commit is past the \`0.9.0\` git tag. It carries the SEC review
amendments of 2026-09-28 (the \`meta.federation\` nesting, all-or-nothing as
the default completion strategy, the \`node-error\` endpoint status), which
change the wire contract while the document still declares
\`spec-version: '$spec_version'\`. The 1.0 release replaces this pin.
PROV

say "$files files, $pages pages, tree digest $digest"
say "done"
