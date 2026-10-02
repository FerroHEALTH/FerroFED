#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/its-rest.sh
#
# Vendors the openEHR ITS-REST OpenAPI documents into docs/specs/its-rest/:
# the code-generation document of every one of the seven API modules, plus the
# repository's licence file.
#
# The gateway is transparent over the whole ITS-REST surface, read and write,
# and names the parts it does not federate (Federation Tier with AQL §7a), so
# it needs every module: Overview and System for the shared conventions and
# the self-description, EHR, Query and Definition for what it fans out or
# routes, and Demographic and Admin for what it refuses. Demographic and Admin
# are `x-status: DEVELOPMENT` in this release; the provenance records each
# module's status rather than assuming it.
#
# The "openEHR ITS-REST OpenAPI" row of docs/VERSIONS.md pins a tag. A tag is
# mutable and every file in it says `info.version: latest`, so the script
# resolves the tag to a commit and records the commit and the git blob id of
# each file beside it.
#
# Usage:
#   scripts/vendor/its-rest.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/its-rest"
oas="computable/OAS"
modules=(overview system ehr query definition demographic admin)

paths=()
for module in "${modules[@]}"; do
  paths+=("$oas/$module-codegen.openapi.yaml")
done
paths+=("LICENSE")

pin="$(corpus_pin_cell "openEHR ITS-REST OpenAPI")"
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

status_rows=""
for module in "${modules[@]}"; do
  file="$dest/$oas/$module-codegen.openapi.yaml"
  status="$(sed -nE 's/^[[:space:]]+x-status:[[:space:]]*([A-Z]+)[[:space:]]*$/\1/p' "$file" | head -n1)"
  [ -n "$status" ] || die "$module-codegen.openapi.yaml declares no info.x-status"
  title="$(sed -nE 's/^[[:space:]]+title:[[:space:]]*(.+)$/\1/p' "$file" | head -n1)"
  status_rows="$status_rows
| $module | $title | \`$status\` |"
done

rows=""
while IFS= read -r file; do
  path="${file#"$dest"/}"
  rows="$rows
| \`$path\` | \`$(corpus_sha256 "$file")\` | \`$(corpus_blob_id "$file")\` |"
done < <(find "$dest" -type f ! -name PROVENANCE.md | LC_ALL=C sort)

licence="$(sed -nE 's/^[[:space:]]+name:[[:space:]]*(Creative Commons.*)$/\1/p' \
  "$dest/$oas/ehr-codegen.openapi.yaml" | head -n1)"
[ -n "$licence" ] || die "ehr-codegen.openapi.yaml declares no info.license.name"
grep -q 'Apache License' "$dest/LICENSE" || die "the repository LICENSE is not the Apache License"

files="$(corpus_file_count "$dest")"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the openEHR ITS-REST OpenAPI documents

Vendored verbatim by \`scripts/vendor/its-rest.sh\`. Never edit a file here:
change the pin in docs/VERSIONS.md and re-run the script.

- Source: <https://github.com/$repo>
- Pin: tag \`$tag\`, which resolves to commit \`$commit\`
- Fetched: $fetched
- Upstream licence: the specification content declares \`$licence\` in each
  document's \`info.license\`. The repository's own \`LICENSE\` file is the
  Apache License 2.0 and is vendored beside this file, so both statements are
  here and neither is assumed.
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #26 (the façade and the dispatch on the generated ITS-REST contract)

## Why every module

The Federation Tier with AQL specification binds this release by name and
makes the gateway transparent over the whole ITS-REST surface, read and
write, naming what it does not federate (§7a). The gateway therefore reads
every module: the ones it fans out or routes, and the ones it refuses. Each
module's lifecycle status, as its document declares it:

| Module | Title | \`info.x-status\` |
|---|---|---|$status_rows

Only the \`-codegen\` rendering of each module is taken. The \`-html\` and
\`-validation\` renderings of the same release describe the same API, and the
Simplified Formats sources are not here because the gateway passes a commit
body through unmodified and never reads its format.

## Why a blob id per file

Every document at this tag says \`info.version: latest\`, so the file content
carries no release identity of its own. The tag, the commit it resolves to,
and the git blob id of each file are what identify these bytes.

| File | sha256 | git blob id |
|---|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
