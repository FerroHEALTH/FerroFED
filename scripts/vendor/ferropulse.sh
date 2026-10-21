#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferropulse.sh
#
# Vendors FerroPULSE's installation report contract: the report v2 document, the
# envelope v2 JSON Schema and its valid and invalid examples. It reads the
# pin from the "FerroPULSE report envelope v2" row of docs/VERSIONS.md (a
# commit and the sha256 of the schema), copies the files verbatim into
# docs/specs/ferropulse/, checks the
# schema against the pinned digest, and writes a PROVENANCE.md beside them.
# The server's tests validate every report it builds against the schema.
# FerroPULSE owns the contract; never hand-edit the copy, move the pin and
# re-run.
#
# The repository is private, so the fetch goes through the authenticated
# GitHub API (`gh api`, raw media type).
#
# Usage: scripts/vendor/ferropulse.sh
# Requires: gh (authenticated with read access to FerroHEALTH/FerroPULSE),
# shasum, awk.
#
# No specification governs this file: our own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require gh shasum awk

item="FerroPULSE report envelope v2"
dest="docs/specs/ferropulse"
schema="schemas/report-envelope.v2.json"
contract="docs/report-v2.md"

cell="$(corpus_pin_cell "$item")"
repo="$(corpus_pin_repo "$cell")"
sha="$(corpus_pin_commit "$cell")"
want="$(grep -oE '\b[0-9a-f]{64}\b' <<< "$cell" | head -n1 || true)"
[ -n "$want" ] || die "the '$item' row names no sha256 of $schema"

# Copies the file at PATH of the pinned commit to the same path under $dest.
fetch() {
  local path="$1"
  mkdir -p "$(dirname "$dest/$path")"
  gh api -H 'Accept: application/vnd.github.raw' "repos/$repo/contents/$path?ref=$sha" > "$dest/$path"
}

# Prints the names of the files of the directory DIR at the pinned commit.
list() {
  gh api "repos/$repo/contents/$1?ref=$sha" --jq '.[] | select(.type == "file") | .name'
}

say "$repo at $sha"
rm -rf "$dest"
fetch "$schema"
got="$(shasum -a 256 "$dest/$schema" | awk '{ print $1 }')"
[ "$got" = "$want" ] || die "$schema at $sha has sha256 $got, and docs/VERSIONS.md pins $want"
fetch "$contract"
for validity in valid invalid; do
  dir="schemas/examples/report-envelope.v2/$validity"
  while IFS= read -r name; do
    fetch "$dir/$name"
  done < <(list "$dir")
done
count="$(find "$dest" -type f | wc -l | tr -d ' ')"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored material; the bytes beside it keep the
     terms named below, not the licence of this repository. -->

# Provenance: the FerroPULSE report envelope v2

- Source: <https://github.com/$repo> (private; Cadasto B.V., the Licensor
  of this repository too)
- Pin: commit \`$sha\`
- Schema sha256: \`$want\`
- Fetched: $(date -u +%Y-%m-%d) by \`scripts/vendor/ferropulse.sh\`
- Licence: upstream declares \`LicenseRef-Proprietary\` in its \`Cargo.toml\`
  and ships no LICENSE file; Cadasto B.V. holds the copyright of both
  repositories and publishes this copy under this repository's licence
- Files: $count, copied verbatim: \`$contract\` (the contract),
  \`$schema\` (JSON Schema 2020-12) and \`schemas/examples/report-envelope.v2/\`
  (the contract's own valid and invalid examples)

The usage report tests in \`app/ferrofed-server/tests/it/usage_report.rs\`
check the validator against the examples, then validate every report the
server builds against the schema. Never hand-edit these files; change the pin
in \`docs/VERSIONS.md\` and re-run the script.
PROV
say "vendored $count files into $dest"
