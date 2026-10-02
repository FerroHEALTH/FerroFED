#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# scripts/fuzz/seeds.sh: writes the generated seed corpora of the fuzz targets
# (fuzz/README.md) from the vendored corpora, never from hand-copied text:
#
#   aql_rewrite      the facade query of every reference golden case
#   adhoc_query      the same queries as ITS-REST AdhocQueryExecute bodies,
#                    bare, with paging and a query parameter, and with an
#                    offset member past zero
#   result_set_meta  the federated RESULT_SET example of the specification
#                    (result-set.adoc, section 9.4)
#   options_root     the OPTIONS {base}/ example of the specification
#                    (rest-facade.adoc, section 7a.2)
#
# The script owns only the files named gen-*. A regression seed committed after
# a finding (.claude/rules/testing.md) carries any other name and is never
# touched. With --check it regenerates into a scratch directory and fails when
# the committed gen-* files differ, so a re-vendored corpus cannot leave stale
# seeds behind.
#
# Usage:
#   scripts/fuzz/seeds.sh           # rewrite fuzz/seeds/*/gen-*
#   scripts/fuzz/seeds.sh --check   # exit 1 when they are out of date

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
golden="$root/docs/specs/federation-ref/src/test/resources/aql-golden"
pages="$root/docs/specs/federation-spec/modules/ROOT/pages"
targets=(aql_rewrite adhoc_query result_set_meta options_root)

die() {
  echo "fuzz-seeds: $*" >&2
  exit 1
}

command -v jq >/dev/null 2>&1 || die "jq is required"
[ -d "$golden" ] || die "no golden cases at $golden (scripts/vendor/federation-ref.sh)"
[ -d "$pages" ] || die "no specification pages at $pages (scripts/vendor/federation-spec.sh)"

# The lines of the "== facade" section of a golden case, joined by spaces.
facade_of() {
  awk '/^== /{inside = ($0 == "== facade"); next} inside && NF {print}' "$1" | paste -sd ' ' -
}

# The body of the first [source,json] block of an AsciiDoc page.
json_block_of() {
  awk '
    /^\[source,json\]/ {armed = 1; next}
    armed && /^----$/ {if (open) exit; open = 1; next}
    open {print}
  ' "$1"
}

generate() {
  local out="$1" case name query
  for target in "${targets[@]}"; do
    mkdir -p "$out/$target"
  done
  for case in "$golden"/*.case; do
    name="$(basename "$case" .case)"
    query="$(facade_of "$case")"
    [ -n "$query" ] || die "$name has no facade section"
    printf '%s' "$query" >"$out/aql_rewrite/gen-$name.aql"
    jq -cn --arg q "$query" '{q: $q}' >"$out/adhoc_query/gen-$name.json"
    jq -cn --arg q "$query" \
      '{q: $q, offset: 0, fetch: 10, query_parameters: {pid: "sentinel-4711"}}' \
      >"$out/adhoc_query/gen-$name-paged.json"
    jq -cn --arg q "$query" '{q: $q, offset: 5, fetch: 10}' \
      >"$out/adhoc_query/gen-$name-offset.json"
  done
  json_block_of "$pages/result-set.adoc" >"$out/result_set_meta/gen-section-9-4.json"
  json_block_of "$pages/rest-facade.adoc" >"$out/options_root/gen-section-7a-2.json"
  for file in "$out/result_set_meta/gen-section-9-4.json" "$out/options_root/gen-section-7a-2.json"; do
    jq -e . "$file" >/dev/null || die "$(basename "$file") is not JSON: the page layout moved"
  done
}

case "${1:-}" in
"")
  for target in "${targets[@]}"; do
    rm -f "$root/fuzz/seeds/$target"/gen-*
  done
  generate "$root/fuzz/seeds"
  echo "fuzz-seeds: wrote fuzz/seeds/*/gen-*"
  ;;
--check)
  scratch="$(mktemp -d)"
  trap 'rm -rf -- "$scratch"' EXIT
  generate "$scratch/fresh"
  for target in "${targets[@]}"; do
    mkdir -p "$scratch/committed/$target"
    for seed in "$root/fuzz/seeds/$target"/gen-*; do
      [ -f "$seed" ] && cp "$seed" "$scratch/committed/$target/"
    done
  done
  diff -r "$scratch/fresh" "$scratch/committed" >&2 ||
    die "fuzz/seeds/*/gen-* is out of date: run scripts/fuzz/seeds.sh and commit the result"
  echo "fuzz-seeds: OK, every gen-* seed matches its source"
  ;;
*)
  die "usage: scripts/fuzz/seeds.sh [--check]"
  ;;
esac
