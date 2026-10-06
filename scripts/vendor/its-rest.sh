#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/its-rest.sh
#
# Vendors the openEHR ITS-REST OpenAPI documents into docs/specs/its-rest/:
# the code-generation document of every one of the seven API modules, the
# validation document of the Query API, and the repository's licence file.
#
# Federation Tier with AQL §9.1 names `query-validation.openapi.yaml`, schema
# `ResultSet`, as the normative list of the RESULT_SET members, so that one
# validation document is taken beside the code-generation set.
#
# The gateway is transparent over the whole ITS-REST surface, read and write,
# and names the parts it does not federate (Federation Tier with AQL §7a), so
# it needs every module: Overview and System for the shared conventions and
# the self-description, EHR, Query and Definition for what it fans out or
# routes, and Demographic and Admin for what it refuses. Demographic and Admin
# are `x-status: DEVELOPMENT` in this release; the provenance records each
# module's status rather than assuming it.
#
# It also takes the AsciiDoc source of the SMART on openEHR document of the
# same release, docs/smart_app_launch/, whose scope grammar and launch
# context client authentication is held to (#80, #414). That document
# declares its own lifecycle status in manifest_vars.adoc, DEVELOPMENT at
# Release-1.1.0, and the provenance records it rather than assuming it.
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
paths+=("$oas/query-validation.openapi.yaml")
smart="docs/smart_app_launch"
paths+=("$smart")
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
corpus_take "$tree_root" "$dest" ${paths[@]+"${paths[@]}"}

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

# Said only when it holds at the pin, so a re-pin that splits the two
# renderings drops the sentence instead of carrying a stale claim.
query_same=""
if [ "$(corpus_blob_id "$dest/$oas/query-validation.openapi.yaml")" \
  = "$(corpus_blob_id "$dest/$oas/query-codegen.openapi.yaml")" ]; then
  query_same="
At this tag the Query API's validation and code-generation documents are the
same bytes: the table below gives both one git blob id."
fi

smart_status="$(sed -nE 's/^:spec_status:[[:space:]]*([A-Z]+)[[:space:]]*$/\1/p' "$dest/$smart/manifest_vars.adoc")"
[ -n "$smart_status" ] || die "$smart/manifest_vars.adoc declares no spec_status"
smart_title="$(sed -nE 's/^:spec_title:[[:space:]]*(.+)$/\1/p' "$dest/$smart/manifest_vars.adoc")"
[ -n "$smart_title" ] || die "$smart/manifest_vars.adoc declares no spec_title"
grep -q 'boilerplate/full_front_block.adoc' "$dest/$smart/master.adoc" \
  || die "$smart/master.adoc no longer includes the openEHR front block that states its licence"

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
  document's \`info.license\`. The SMART on openEHR source carries no licence
  line of its own: its \`master.adoc\` includes the openEHR front block
  (\`docs/boilerplate/full_front_block.adoc\` of
  <https://github.com/openEHR/specifications-AA_GLOBAL>), whose licence block
  states Creative Commons Attribution-NoDerivs 3.0 Unported
  (<https://creativecommons.org/licenses/by-nd/3.0/>), which permits verbatim
  redistribution with attribution. That front block is cited by URL on the
  default branch of \`specifications-AA_GLOBAL\`; it is neither vendored nor
  pinned here, because it is boilerplate the rendering includes and none of
  the gateway's citations read it. The repository's own \`LICENSE\` file is
  the Apache License 2.0 and is vendored beside this file, so every statement
  is here and none is assumed.
- Layout: the upstream paths, unchanged
- Files: $files
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #26 (the façade and the dispatch on the generated ITS-REST contract),
  #310 (the result-set schema test against \`query-validation.openapi.yaml\`)
  and #414 (client authentication's citations of SMART on openEHR, held to
  the vendored headings and status by the server's citation test)

## SMART on openEHR

\`$smart/\` is the AsciiDoc source of *$smart_title* at this release, whole:
the master document, its chapters (\`master04-service_discovery.adoc\`,
\`master07-authorization.adoc\` and \`master08-scopes.adoc\` among them), its
\`manifest_vars.adoc\` and its diagrams. Its \`manifest_vars.adoc\` declares
\`:spec_status: $smart_status\`: the document is in the $smart_status state
in this release, so what the gateway is held to by it is a draft the
release does not stabilise, and every citation of it says so. The rendered
\`docs/smart_app_launch.html\` is not taken: it is generated from this
source.

## Why every module

The Federation Tier with AQL specification binds this release by name and
makes the gateway transparent over the whole ITS-REST surface, read and
write, naming what it does not federate (§7a). The gateway therefore reads
every module: the ones it fans out or routes, and the ones it refuses. Each
module's lifecycle status, as its document declares it:

| Module | Title | \`info.x-status\` |
|---|---|---|$status_rows

The \`-codegen\` rendering of each module is taken. The \`-html\` and
\`-validation\` renderings of the same release describe the same API, with one
exception taken here: \`query-validation.openapi.yaml\`, because Federation
Tier with AQL §9.1 names it, schema \`ResultSet\`, as the normative list of
the RESULT_SET members that the result-set schema's \`\$defs/itsRest\` subset
restates. The Simplified Formats sources are not here because the gateway
passes a commit body through unmodified and never reads its format.$query_same

## Why a blob id per file

Every document at this tag says \`info.version: latest\`, so the file content
carries no release identity of its own. The tag, the commit it resolves to,
and the git blob id of each file are what identify these bytes.

| File | sha256 | git blob id |
|---|---|---|$rows
PROV

say "$files files, tree digest $digest"
say "done"
