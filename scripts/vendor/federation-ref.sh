#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/federation-ref.sh
#
# Vendors the Federation Tier reference implementation into
# docs/specs/federation-ref/: the whole
# repository tree at the commit the "Federation Tier reference implementation"
# row of docs/VERSIONS.md pins, minus its Maven manifest, which provenance
# lists with its sha256.
#
# It is evidence, never an oracle and never code this repository builds or
# copies: the specification decides what a gateway does, and this tree shows
# what one working gateway does, including its AQL golden cases, its copies of
# the published schemas and its synthetic demo data.
#
# Run scripts/vendor/federation-spec.sh first: the provenance records whether
# the implementation's schema copies agree with the vendored specification.
#
# Usage:
#   scripts/vendor/federation-ref.sh
#
# Requires: curl, tar, shasum, jq.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl tar shasum jq

dest="docs/specs/federation-ref"
spec="docs/specs/federation-spec"
golden="src/test/resources/aql-golden"
schema_dir="src/test/resources/spec-schemas"
keys="src/test/resources/keys"

pin="$(corpus_pin_cell "Federation Tier reference implementation")"
repo="$(corpus_pin_repo "$pin")"
commit="$(corpus_pin_commit "$pin")"

[ -f "$spec/PROVENANCE.md" ] || die "no $spec yet; run scripts/vendor/federation-spec.sh first"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

say "$repo at $commit"
tree_root="$(corpus_fetch "$repo" "$commit" "$tmp")"
[ -f "$tree_root/LICENSE" ] || die "the archive has no LICENSE"
grep -q 'Apache License' "$tree_root/LICENSE" || die "the LICENSE is not the Apache License"
[ -f "$tree_root/NOTICE" ] || die "the archive has no NOTICE"
[ -d "$tree_root/$golden" ] || die "the archive has no $golden"

# The version the implementation declares tracks the specification it
# implements, not a product release (its pom.xml says so); read it before the
# manifest is dropped.
impl_version="$(awk '/<\/parent>/ { p = 1; next } p && /<version>/ {
  sub(/.*<version>/, ""); sub(/<\/version>.*/, ""); print; exit }' "$tree_root/pom.xml")"
[ -n "$impl_version" ] || die "pom.xml declares no project version"

dropped="$(corpus_drop_manifests "$tree_root")"

rm -rf "$dest"
mkdir -p "$(dirname "$dest")"
mv "$tree_root" "$dest"

schema_rows=""
for file in "$dest/$schema_dir"/*.schema.json; do
  name="$(basename "$file")"
  ours="$(corpus_sha256 "$file")"
  upstream="$spec/modules/ROOT/attachments/$name"
  if [ ! -f "$upstream" ]; then
    verdict="no file of that name in the vendored specification"
  elif [ "$ours" = "$(corpus_sha256 "$upstream")" ]; then
    verdict="identical to the vendored specification"
  else
    verdict="DIFFERS from the vendored specification (\`$(corpus_sha256 "$upstream")\`)"
  fi
  schema_rows="$schema_rows
| \`$schema_dir/$name\` | \`$ours\` | $verdict |"
done

key_rows=""
while IFS= read -r file; do
  key_rows="$key_rows
| \`${file#"$dest"/}\` | \`$(corpus_sha256 "$file")\` |"
done < <(find "$dest/$keys" -type f | LC_ALL=C sort)

files="$(corpus_file_count "$dest")"
cases="$(find "$dest/$golden" -type f -name '*.case' | wc -l | tr -d '[:space:]')"
java="$(find "$dest/src" -type f -name '*.java' | wc -l | tr -d '[:space:]')"
digest="$(corpus_tree_digest "$dest")"
fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Federation Tier reference implementation

The whole repository tree, vendored verbatim by
\`scripts/vendor/federation-ref.sh\`, less the dependency manifest listed
below. Never edit a file here: change the pin in docs/VERSIONS.md and re-run
the script.

- Source: <https://github.com/$repo>
- Pin: commit \`$commit\`
- Declared version: \`$impl_version\` (the \`pom.xml\` project version, which
  tracks the specification version it implements)
- Fetched: $fetched
- Upstream licence: Apache License 2.0, the repository's \`LICENSE\` file,
  vendored beside this file with its \`NOTICE\`
- Layout: the upstream paths, unchanged
- Files: $files, of which $java are Java sources and $cases are AQL golden
  cases under \`$golden/\`
- Tree digest (sha256 over the sorted per-file \`sha256  path\` listing,
  \`PROVENANCE.md\` excluded): \`$digest\`
- Read by: #18 (the golden cases adjudicated against the specification), #35 (the golden cases as a corpus test), #39 (the demo template and compositions as the e2e seed) and #94 (the differential run)

## Evidence, never an oracle

The specification in \`$spec/\` decides what a gateway does. This tree is
one working gateway, a Java and Spring Boot service, read for its golden AQL
rewrite cases, its schema copies, its synthetic demo data and its account of
where the specification needed a decision. It is never compiled here and no
code is copied from it: FerroFED is its own design under its own licence. A
disagreement between this tree and the specification is recorded as an
upstream-report issue, and the specification wins.

## Schema copies

The implementation tests against its own copies of the two published schemas.
Each is compared with the vendored specification at its pin:

| File | sha256 | Against the specification |
|---|---|---|$schema_rows

A difference is a finding about the implementation, recorded on the tracker
and never resolved by editing either copy. The specification's copy is the
one FerroFED validates against.

## Test keys

The RSA key pair below is a test fixture the upstream publishes for its own
token tests. It protects nothing, and no FerroFED test or deployment may use
it. It is recorded here so a secret scanner's finding on this path can be
matched against these hashes and dismissed.

| File | sha256 |
|---|---|$key_rows

## Demo data

\`docker/demo-data/\` holds the upstream's synthetic demo compositions and the
International Patient Summary template its demo seeds. The patient
identifiers in them are invented for the demo; no record of a real person is
here.

## What is left out

The upstream's Maven manifest. A vendored manifest makes this repository's
dependency graph claim a Java toolchain it does not use, and the scanners
would raise advisories against versions nothing here installs.

| File | sha256 |
|---|---|
$dropped

The upstream's \`Dockerfile\`, \`docker-compose.yml\` and \`.github/\` are kept.
They are inert here: GitHub runs only the workflows at the repository root,
and nothing builds this tree.
PROV

say "$files files, $cases golden cases, tree digest $digest"
say "done"
