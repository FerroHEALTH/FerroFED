#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/checks/conformance-matrix.sh: the conformance-matrix guard, tier 1
# (the §17 conformance points and the §16.3 tracks, #41). Offline and static:
# the CI test run already fails on a failing marked test, so this only checks
# the record.
#
# It fails when:
#   1. a derived column of conformance/matrix.tsv, conformance/tracks.tsv or
#      conformance/requirements.tsv differs from a fresh derivation from the
#      vendored specification (a point removed, added or changed);
#   2. the points a requirement is reached by disagree with the vendored
#      tools/traceability.tsv;
#   3. a requirement is reached by no point and no track and is not listed in
#      tools/traceability-exceptions.txt;
#   4. a status, issue or reason cell breaks the vocabulary: a status outside
#      the set, a status that does not fit the actor, a row other than covered
#      without its issue, or a deferred, node-profile or operator row without
#      its reason;
#   5. a marker names a point or track the matrix does not hold, carries any
#      other word, or does not sit directly above a #[test] or #[tokio::test]
#      attribute;
#   6. a covered row has no marker, a planned or deferred row has one, or a
#      node-profile or operator row is marked outside the harness (tools/);
#   7. the book page differs from `scripts/conformance/matrix.sh --render`;
#   8. conformance/aql-golden/pass-list.txt is unsorted, names a case the
#      vendored corpus does not hold, or records another total than it holds;
#   9. a file under conformance/badges/ or the README conformance block
#      differs from what `scripts/conformance/matrix.sh --badges-write` writes.
#
# The golden test, in the Rust tier, fails when a listed case stops passing or
# an unlisted case passes, so the pass list this reads is what the tests hold.
#
# Run `scripts/conformance/matrix.sh --derive` after a re-pin, and
# `scripts/conformance/matrix.sh --render-write` and `--badges-write` after a
# status change or a pass-list rewrite.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root" || exit 2

readonly MATRIX=conformance/matrix.tsv
readonly TRACKS=conformance/tracks.tsv
readonly REQUIREMENTS=conformance/requirements.tsv
readonly PAGE=website/book/src/evaluate/conformance.md
readonly SPEC_TOOLS=docs/specs/federation-spec/tools
readonly PASS_LIST=conformance/aql-golden/pass-list.txt
readonly GOLDEN=docs/specs/federation-ref/src/test/resources/aql-golden
readonly BADGES=conformance/badges

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail=0

problem() {
  echo "conformance-matrix: $*" >&2
  fail=1
}

for f in "$MATRIX" "$TRACKS" "$REQUIREMENTS" "$PAGE"; do
  [ -f "$f" ] || problem "$f is missing"
done
[ "$fail" -eq 0 ] || exit 1

# The data rows of a table, comments dropped, header kept.
rows() { grep -v '^#' "$1"; }

# 1. The derived columns against a fresh derivation.
bash scripts/conformance/matrix.sh --derived "$work/derived" || exit 1
rows "$MATRIX" | cut -f1-4 > "$work/matrix.derived"
rows "$TRACKS" | cut -f1-4 > "$work/tracks.derived"
rows "$REQUIREMENTS" > "$work/requirements.derived"
for pair in "matrix.derived:cps.tsv:$MATRIX" "tracks.derived:tracks.tsv:$TRACKS" "requirements.derived:requirements.tsv:$REQUIREMENTS"; do
  held="${pair%%:*}"
  rest="${pair#*:}"
  fresh="${rest%%:*}"
  file="${rest#*:}"
  if ! diff -u "$work/derived/$fresh" "$work/$held" > "$work/diff" 2>&1; then
    problem "the derived columns of $file differ from the vendored specification (run scripts/conformance/matrix.sh --derive):"
    sed 's/^/  /' "$work/diff" >&2
  fi
done

# 2. The direct points per requirement against the vendored traceability.tsv.
# Its tracks column is never filled, so only the points compare.
if [ -f "$SPEC_TOOLS/traceability.tsv" ]; then
  # One "requirement <TAB> point" pair per line, so the two sides compare as
  # sorted sets whatever order each file lists its points in.
  awk -F'\t' 'NR > 1 {
      n = split($2, p, ";")
      for (i = 1; i <= n; i++) print $1 "\t" (p[i] == "-" ? "-" : "CP-" substr(p[i], 4))
    }' "$SPEC_TOOLS/traceability.tsv" | sort > "$work/trace.vendored"
  rows "$REQUIREMENTS" | awk -F'\t' 'NR > 1 {
      n = split($2, p, ",")
      for (i = 1; i <= n; i++) print $1 "\t" p[i]
    }' | sort > "$work/trace.ours"
  if ! diff -u "$work/trace.vendored" "$work/trace.ours" > "$work/diff" 2>&1; then
    problem "the points reaching each requirement disagree with $SPEC_TOOLS/traceability.tsv:"
    sed 's/^/  /' "$work/diff" >&2
  fi
else
  problem "$SPEC_TOOLS/traceability.tsv is missing"
fi

# 3. Orphan requirements must be recorded exceptions.
grep -vE '^[[:space:]]*(#|$)' "$SPEC_TOOLS/traceability-exceptions.txt" 2>/dev/null | awk '{ print $1 }' > "$work/exceptions"
rows "$REQUIREMENTS" | awk -F'\t' '$4 == "orphan" { print $1 }' | while read -r req; do
  grep -qx "$req" "$work/exceptions" || echo "$req"
done > "$work/orphans"
while read -r req; do
  [ -n "$req" ] && problem "$req is reached by no conformance point and no track, and is not in $SPEC_TOOLS/traceability-exceptions.txt"
done < "$work/orphans"

# 4. The hand-kept vocabulary.
vocab="$(
  rows "$MATRIX" | awk -F'\t' -v file="$MATRIX" '
    NR == 1 { next }
    {
      if (NF != 7) { print file ": " $1 " has " NF " columns, not 7"; next }
      s = $5; a = $2
      if (s !~ /^(covered|planned|deferred|node-profile|operator)$/) print file ": " $1 " has status \"" s "\", outside the vocabulary"
      else if (a == "Node" && s != "node-profile") print file ": " $1 " is a Node point, so its status is node-profile"
      else if (a == "Operator" && s != "operator") print file ": " $1 " is an Operator point, so its status is operator"
      else if (a == "Gateway" && (s == "node-profile" || s == "operator")) print file ": " $1 " is a Gateway point and cannot be " s
      if ($6 !~ /^(-|#[0-9]+(,#[0-9]+)*)$/) print file ": " $1 " has issue \"" $6 "\"; write #n or #n,#m"
      else if (s != "covered" && $6 == "-") print file ": " $1 " is " s " and names no issue"
      if ((s == "deferred" || s == "node-profile" || s == "operator") && ($7 == "-" || $7 == "")) print file ": " $1 " is " s " and gives no reason"
    }'
  rows "$TRACKS" | awk -F'\t' -v file="$TRACKS" '
    NR == 1 { next }
    {
      if (NF != 7) { print file ": track " $1 " has " NF " columns, not 7"; next }
      if ($5 !~ /^(covered|planned|deferred)$/) print file ": track " $1 " has status \"" $5 "\", outside covered, planned, deferred"
      if ($6 !~ /^(-|#[0-9]+(,#[0-9]+)*)$/) print file ": track " $1 " has issue \"" $6 "\"; write #n or #n,#m"
      else if ($5 != "covered" && $6 == "-") print file ": track " $1 " is " $5 " and names no issue"
      if ($5 == "deferred" && ($7 == "-" || $7 == "")) print file ": track " $1 " is deferred and gives no reason"
    }'
)"
if [ -n "$vocab" ]; then
  while IFS= read -r line; do problem "$line"; done <<< "$vocab"
fi

# 5. Every marker in a tracked or new Rust file outside the vendored trees:
# one "token <TAB> file:line" record per named point or track.
git ls-files --cached --others --exclude-standard -- '*.rs' ':!docs/specs/**' > "$work/sources" 2> /dev/null || true
: > "$work/marks"
while IFS= read -r src; do
  [ -f "$src" ] || continue
  awk -v src="$src" '
    pending != "" {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      if (line !~ /^#\[(test|tokio::test)([](]|$)/) print "PLACEMENT\t" pending
      pending = ""
    }
    /^[[:space:]]*\/\/[[:space:]]*conformance:/ {
      body = $0
      sub(/^[[:space:]]*\/\/[[:space:]]*conformance:[[:space:]]*/, "", body)
      n = split(body, tok, /[[:space:]]+/)
      if (n == 0 || body == "") print "EMPTY\t" src ":" NR
      for (i = 1; i <= n; i++) if (tok[i] != "") print tok[i] "\t" src ":" NR
      pending = src ":" NR
    }
    END { if (pending != "") print "PLACEMENT\t" pending }
  ' "$src" >> "$work/marks"
done < "$work/sources"

rows "$MATRIX" | awk -F'\t' 'NR > 1 { print $1 "\t" $5 }' > "$work/cp-status"
rows "$TRACKS" | awk -F'\t' 'NR > 1 { print "track-" $1 "\t" $5 }' > "$work/track-status"

marker_findings="$(
  awk -F'\t' '
    FILENAME == ARGV[1] { status[$1] = $2; next }
    FILENAME == ARGV[2] { status[$1] = $2; next }
    {
      tok = $1; where = $2
      if (tok == "PLACEMENT") { print where ": the conformance marker must sit directly above #[test] or #[tokio::test]"; next }
      if (tok == "EMPTY") { print where ": the conformance marker names no point or track"; next }
      if (tok !~ /^(CP-[0-9]+[a-z]?|track-[0-9]+)$/) { print where ": \"" tok "\" is not a CP id or a track; the marker carries nothing else"; next }
      if (!(tok in status)) { print where ": " tok " is not in the conformance matrix"; next }
      s = status[tok]
      marked[tok] = 1
      if (s == "planned") print where ": " tok " is marked but planned; set it covered in the matrix"
      else if (s == "deferred") print where ": " tok " is deferred and must carry no marker"
      else if ((s == "node-profile" || s == "operator") && where !~ /^tools\//) print where ": " tok " is a " s " point, scored in the harness under tools/, never on a gateway test"
    }
    END {
      for (tok in status) if (status[tok] == "covered" && !(tok in marked)) print tok " is covered but no test carries its marker"
    }
  ' "$work/cp-status" "$work/track-status" "$work/marks"
)"
if [ -n "$marker_findings" ]; then
  while IFS= read -r line; do problem "$line"; done <<< "$marker_findings"
fi

# 7. The rendered book page.
bash scripts/conformance/matrix.sh --render > "$work/page.md" || exit 1
if ! diff -u "$PAGE" "$work/page.md" > "$work/diff" 2>&1; then
  problem "$PAGE is stale (run scripts/conformance/matrix.sh --render-write):"
  sed 's/^/  /' "$work/diff" | head -40 >&2
fi

# 8. The golden pass list: sorted and unique, every case a vendored file, and
# its total the size of the vendored corpus.
if [ -f "$PASS_LIST" ]; then
  grep -vE '^(#|total |$)' "$PASS_LIST" > "$work/listed"
  if ! LC_ALL=C sort -u -c "$work/listed" 2> /dev/null; then
    problem "$PASS_LIST is not sorted and unique (rerun the golden test with FERROFED_CONFORMANCE_UPDATE=1)"
  fi
  while IFS= read -r case; do
    [ -f "$GOLDEN/$case" ] || problem "$PASS_LIST names $case, which is not in $GOLDEN"
  done < "$work/listed"
  listed_total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$PASS_LIST")"
  corpus_total="$(find "$GOLDEN" -maxdepth 1 -name '*.case' | wc -l | tr -d '[:space:]')"
  if [ "$listed_total" != "$corpus_total" ]; then
    problem "$PASS_LIST records a total of ${listed_total:-none} and $GOLDEN holds $corpus_total cases (rerun the golden test with FERROFED_CONFORMANCE_UPDATE=1)"
  fi
else
  problem "$PASS_LIST is missing"
fi

# 9. The badge files and the README block against a fresh rendering.
if bash scripts/conformance/matrix.sh --badges "$work/badges" > /dev/null; then
  (cd "$work/badges" && ls -- *.json) > "$work/badges.fresh"
  (cd "$BADGES" 2> /dev/null && ls -- *.json 2> /dev/null) > "$work/badges.held" || true
  if ! diff -u "$work/badges.fresh" "$work/badges.held" > "$work/diff" 2>&1; then
    problem "the files under $BADGES differ from the badge set (run scripts/conformance/matrix.sh --badges-write):"
    sed 's/^/  /' "$work/diff" >&2
  fi
  while IFS= read -r badge; do
    [ -f "$BADGES/$badge" ] || continue
    if ! diff -u "$BADGES/$badge" "$work/badges/$badge" > "$work/diff" 2>&1; then
      problem "$BADGES/$badge disagrees with the matrix or the pass list (run scripts/conformance/matrix.sh --badges-write):"
      sed 's/^/  /' "$work/diff" >&2
    fi
  done < "$work/badges.fresh"
else
  problem "the badges could not be rendered"
fi
bash scripts/conformance/matrix.sh --readme-block > "$work/block.md" || exit 1
sed -n '/^<!-- conformance:begin -->$/,/^<!-- conformance:end -->$/p' README.md > "$work/block.held"
if ! diff -u "$work/block.held" "$work/block.md" > "$work/diff" 2>&1; then
  problem "the README.md conformance block is stale (run scripts/conformance/matrix.sh --badges-write):"
  sed 's/^/  /' "$work/diff" >&2
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
gateway="$(rows "$MATRIX" | awk -F'\t' 'NR > 1 && $2 == "Gateway"' | wc -l | tr -d '[:space:]')"
covered="$(rows "$MATRIX" | awk -F'\t' 'NR > 1 && $2 == "Gateway" && $5 == "covered"' | wc -l | tr -d '[:space:]')"
echo "conformance-matrix: OK (gateway points covered: $covered of $gateway; $(wc -l < "$work/marks" | tr -d '[:space:]') marker entries)"
