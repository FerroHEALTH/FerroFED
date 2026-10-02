#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/matrix.sh: the conformance matrix's derivation and its
# rendering into the book (the §17 conformance points and the §16.3 tracks,
# #41).
#
#   scripts/conformance/matrix.sh --derived DIR   write the derived tables into DIR
#   scripts/conformance/matrix.sh --derive        refresh conformance/*.tsv in place
#   scripts/conformance/matrix.sh --render        print the book page to stdout
#   scripts/conformance/matrix.sh --render-write  write the book page
#   scripts/conformance/matrix.sh --badges DIR    write the badge files into DIR
#   scripts/conformance/matrix.sh --readme-block  print the README conformance block
#   scripts/conformance/matrix.sh --badges-write  write conformance/badges/ and the README block
#
# The badges (#175) are shields.io endpoint files
# (https://shields.io/badges/endpoint-badge): the Gateway points covered out
# of the Gateway total, the Node and Operator point counts, and the AQL golden
# cases recorded in conformance/aql-golden/pass-list.txt, which the golden test
# rewrites when FERROFED_CONFORMANCE_UPDATE is 1. Each label names the
# specification version docs/VERSIONS.md pins. The README block between the
# conformance:begin and conformance:end markers renders those files. The book
# page links the specification site at that same pin, so a re-pin moves both.
#
# The derived columns come from the vendored specification, never from a hand:
#
#   cps.tsv           cp, actor, requirements, tracks   (conformance.adoc, §17)
#   tracks.tsv        track, title, requirements, cps   (testing.adoc, §16.3)
#   requirements.tsv  requirement, cps, tracks, reachability (requirements.adoc)
#
# `--derive` keeps the hand-held status, issue and reason columns of
# conformance/matrix.tsv and conformance/tracks.tsv, keyed by the row id, and
# the leading comment block of each file; a point or track that is new to the
# specification arrives as `planned` with no issue, which the check refuses
# until a person fills it in. A re-pin of the specification therefore shows up
# as a reviewable diff of these files.
#
# The closure is computed here, not taken from the vendored
# tools/traceability.sh: that script never matches the `| [[track-n]]n` cell
# testing.adoc writes, so its tracks column is empty for every requirement.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly PAGES=docs/specs/federation-spec/modules/ROOT/pages
readonly MATRIX=conformance/matrix.tsv
readonly TRACKS=conformance/tracks.tsv
readonly REQUIREMENTS=conformance/requirements.tsv
readonly PAGE=website/book/src/evaluate/conformance.md
readonly SPEC_SITE=https://syntaric.github.io/openehr-federation-spec/federation-aql
readonly BADGES=conformance/badges
readonly PASS_LIST=conformance/aql-golden/pass-list.txt
readonly README=README.md
readonly BOOK_PAGE=https://ferrofed.eu/docs/evaluate/conformance.html
# The shields.io endpoint prefix every badge file is read through, from main.
readonly ENDPOINT='https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FFerroHEALTH%2FFerroFED%2Fmain%2Fconformance%2Fbadges%2F'

die() {
  echo "conformance-matrix: $*" >&2
  exit 1
}

# One AsciiDoc table row per output line, for the rows whose first cell holds
# an anchor of the given kind (cp or track). A row is the run of lines between
# blank lines inside a `|===` table; a line opening with `| ` starts a cell and
# any other line continues the cell before it.
#   kind=cp:    CP-n <TAB> actor <TAB> N-ids <TAB> track numbers
#   kind=track: n <TAB> title <TAB> N-ids
table_rows() {
  awk -v kind="$1" '
    function ids(s, pat, prefix, skip,   out) {
      out = ""
      while (match(s, pat)) {
        out = out (out == "" ? "" : ",") prefix substr(s, RSTART + skip, RLENGTH - skip - 1)
        s = substr(s, RSTART + RLENGTH)
      }
      return out == "" ? "-" : out
    }
    function trim(s) {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
      return s
    }
    function flush(   anchor, id, title, i) {
      if (n > 0 && kind == "cp" && match(cell[1], /\[\[cp-[0-9]+[a-z]?\]\]/)) {
        id = "CP-" substr(cell[1], RSTART + 5, RLENGTH - 7)
        print id "\t" trim(cell[2]) "\t" ids(cell[4], "#n[0-9]+[a-z]?\\[", "N", 2) "\t" ids(cell[5], "#track-[0-9]+\\[", "", 7)
      }
      if (n > 0 && kind == "track" && match(cell[1], /\[\[track-[0-9]+\]\]/)) {
        id = substr(cell[1], RSTART + 8, RLENGTH - 10)
        title = trim(cell[2])
        gsub(/\*/, "", title)
        print id "\t" title "\t" ids(cell[4], "#n[0-9]+[a-z]?\\[", "N", 2)
      }
      for (i = 1; i <= n; i++) delete cell[i]
      n = 0
    }
    /^\|===/ { flush(); intable = !intable; next }
    !intable { next }
    /^[[:space:]]*$/ { flush(); next }
    /^\| / { n++; cell[n] = substr($0, 3); next }
    { if (n > 0) cell[n] = cell[n] " " $0 }
    END { flush() }
  ' "$2"
}

# Every requirement anchor in requirements.adoc, in specification order.
requirement_ids() {
  grep -oE '\[\[n[0-9]+[a-z]?\]\]' "$PAGES/requirements.adoc" | sed -E 's/\[\[n(.*)\]\]/N\1/'
}

# Write the three derived tables into a directory.
derive_into() {
  local dir="$1"
  [ -f "$PAGES/conformance.adoc" ] || die "the vendored specification is missing ($PAGES)"
  table_rows cp "$PAGES/conformance.adoc" > "$dir/cps.raw"
  table_rows track "$PAGES/testing.adoc" > "$dir/tracks.raw"
  requirement_ids > "$dir/requirements.raw"
  [ -s "$dir/cps.raw" ] || die "no conformance point parsed from $PAGES/conformance.adoc"
  [ -s "$dir/tracks.raw" ] || die "no test track parsed from $PAGES/testing.adoc"
  [ -s "$dir/requirements.raw" ] || die "no requirement parsed from $PAGES/requirements.adoc"

  {
    printf 'cp\tactor\trequirements\ttracks\n'
    cat "$dir/cps.raw"
  } > "$dir/cps.tsv"

  # A track's points are the points whose tracks cell names it, in §17 order.
  {
    printf 'track\ttitle\trequirements\tcps\n'
    awk -F'\t' '
      NR == FNR { cp[++ncp] = $1; tr[ncp] = "," $4 ","; next }
      {
        out = ""
        for (i = 1; i <= ncp; i++)
          if (index(tr[i], "," $1 ",")) out = out (out == "" ? "" : ",") cp[i]
        print $0 "\t" (out == "" ? "-" : out)
      }
    ' "$dir/cps.raw" "$dir/tracks.raw"
  } > "$dir/tracks.tsv"

  # A requirement is reached directly when a point names it, through a track
  # only when a track names it and no point does, and is an orphan otherwise.
  {
    printf 'requirement\tcps\ttracks\treachability\n'
    awk -F'\t' '
      FILENAME == ARGV[1] { cp[++ncp] = $1; cn[ncp] = "," $3 ","; next }
      FILENAME == ARGV[2] { tk[++ntk] = $1; tn[ntk] = "," $3 ","; next }
      {
        c = ""; t = ""
        for (i = 1; i <= ncp; i++) if (index(cn[i], "," $1 ",")) c = c (c == "" ? "" : ",") cp[i]
        for (i = 1; i <= ntk; i++) if (index(tn[i], "," $1 ",")) t = t (t == "" ? "" : ",") tk[i]
        reach = c != "" ? "direct" : (t != "" ? "track-only" : "orphan")
        print $1 "\t" (c == "" ? "-" : c) "\t" (t == "" ? "-" : t) "\t" reach
      }
    ' "$dir/cps.raw" "$dir/tracks.raw" "$dir/requirements.raw"
  } > "$dir/requirements.tsv"
  rm -f "$dir/cps.raw" "$dir/tracks.raw" "$dir/requirements.raw"
}

# Refresh a hand-kept table: its leading comment block, the derived columns
# from the derivation, and the hand columns of the existing row with that id.
refresh() {
  local derived="$1" target="$2" header="$3"
  local tmp existing=/dev/null
  tmp="$(mktemp)"
  if [ -f "$target" ]; then existing="$target"; fi
  {
    awk '/^#/ { print; next } { exit }' "$existing"
    printf '%s\n' "$header"
    awk -F'\t' -v OFS='\t' '
      FILENAME == ARGV[1] { if ($0 !~ /^#/ && ++seen > 1) hand[$1] = $5 "\t" $6 "\t" $7; next }
      FNR == 1 { next }
      { print $0, ($1 in hand ? hand[$1] : "planned\t-\t-") }
    ' "$existing" "$derived"
  } > "$tmp"
  mv "$tmp" "$target"
}

# Markdown for one table cell list: issue references become tracker links.
render_page() {
  local site
  site="$(spec_site)"
  awk -F'\t' -v site="$site" '
    function issues(s,   out, n, parts, i, num) {
      if (s == "-") return "-"
      n = split(s, parts, ",")
      out = ""
      for (i = 1; i <= n; i++) {
        num = substr(parts[i], 2)
        out = out (out == "" ? "" : ", ") "[" parts[i] "](https://github.com/FerroHEALTH/FerroFED/issues/" num ")"
      }
      return out
    }
    function list(s,   out) {
      out = s
      gsub(",", ", ", out)
      return out
    }
    function cplink(id,   anchor) {
      anchor = tolower(id)
      return "[" id "](" site "/conformance.html#" anchor ")"
    }
    FILENAME == ARGV[1] {
      if ($0 ~ /^#/ || ++m == 1) next
      row[++nrow] = $0
      if ($2 == "Gateway") { gw++; if ($5 == "covered") gwc++; if ($5 == "planned") gwp++; if ($5 == "deferred") gwd++ }
      else other++
      next
    }
    FILENAME == ARGV[2] {
      if ($0 ~ /^#/ || ++t == 1) next
      trow[++ntrow] = $0
      next
    }
    {
      if ($0 ~ /^#/ || ++r == 1) next
      reach[$4]++
      req[++nreq] = $0
    }
    END {
      print "<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->"
      print "<!-- SPDX-License-Identifier: BUSL-1.1 -->"
      print "<!-- Generated by scripts/conformance/matrix.sh --render-write from conformance/*.tsv. Do not edit. -->"
      print ""
      print "# Conformance matrix"
      print ""
      print "Every conformance point of the Federation Tier with AQL specification"
      print "(section 17), the test tracks of section 16.3 and every requirement, with"
      print "the status FerroFED holds for each. The actor, requirement and track"
      print "columns are derived from the vendored specification; the status, issue and"
      print "reason columns are kept by FerroFED. A point is covered only when a test"
      print "carries its marker and CI runs it."
      print ""
      printf "**Gateway points:** %d of %d covered, %d planned, %d deferred.\n", gwc, gw, gwp, gwd
      print ""
      printf "The other %d points belong to a member node or to the federation operator,\n", other
      print "and a gateway is never marked down for them (section 17)."
      print ""
      print "## Conformance points"
      print ""
      print "| Point | Actor | Requirements | Tracks | Status | Issues | Reason |"
      print "|---|---|---|---|---|---|---|"
      for (i = 1; i <= nrow; i++) {
        split(row[i], f, "\t")
        printf "| %s | %s | %s | %s | %s | %s | %s |\n", cplink(f[1]), f[2], list(f[3]), list(f[4]), f[5], issues(f[6]), f[7]
      }
      print ""
      print "## Test tracks"
      print ""
      print "| Track | Title | Requirements | Points | Status | Issues | Reason |"
      print "|---|---|---|---|---|---|---|"
      for (i = 1; i <= ntrow; i++) {
        split(trow[i], f, "\t")
        printf "| [%s](%s/testing.html#track-%s) | %s | %s | %s | %s | %s | %s |\n", f[1], site, f[1], f[2], list(f[3]), list(f[4]), f[5], issues(f[6]), f[7]
      }
      print ""
      print "## Requirements"
      print ""
      printf "%d requirements: %d reached by a conformance point, %d by a test track only, %d by neither.\n", nreq, reach["direct"], reach["track-only"], reach["orphan"]
      print ""
      print "| Requirement | Points | Tracks | Reachability |"
      print "|---|---|---|---|"
      for (i = 1; i <= nreq; i++) {
        split(req[i], f, "\t")
        printf "| [%s](%s/requirements.html#%s) | %s | %s | %s |\n", f[1], site, tolower(f[1]), list(f[2]), list(f[3]), f[4]
      }
    }
  ' "$MATRIX" "$TRACKS" "$REQUIREMENTS"
}

# The Federation Tier version docs/VERSIONS.md pins: the first word of the
# second cell of its specification-pin row.
spec_version() {
  local version
  version="$(awk -F'|' '
    NF >= 3 {
      k = $2; v = $3
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == "Federation Tier with AQL") { split(v, w, /[[:space:]]/); print w[1]; exit }
    }
  ' docs/VERSIONS.md)"
  [ -n "$version" ] || die "docs/VERSIONS.md has no Federation Tier with AQL pin row"
  printf '%s\n' "$version"
}

# The specification site at the pinned version. The site keeps one version
# per minor release (the `version` key of the vendored antora.yml), so the
# path carries the major and minor of the pin.
spec_site() {
  local version
  version="$(spec_version)"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "docs/VERSIONS.md pins the Federation Tier as \"$version\", not MAJOR.MINOR.PATCH"
  printf '%s/%s\n' "$SPEC_SITE" "${version%.*}"
}

# The badge colour of k out of n, by the share in quarters.
colour_of() {
  local passed="$1" total="$2" share
  if [ "$total" -eq 0 ]; then
    echo lightgrey
    return
  fi
  share=$((passed * 100 / total))
  if [ "$passed" -eq "$total" ]; then echo brightgreen
  elif [ "$share" -ge 75 ]; then echo green
  elif [ "$share" -ge 50 ]; then echo yellow
  elif [ "$share" -ge 25 ]; then echo orange
  else echo red
  fi
}

# One endpoint badge file: label, message, colour. The labels and messages
# are written here and carry no character JSON escapes.
badge_json() {
  printf '{"schemaVersion":1,"label":"%s","message":"%s","color":"%s"}\n' "$1" "$2" "$3"
}

# The number of matrix rows with actor $1, and with actor $1 and status $2.
matrix_count() {
  awk -F'\t' -v actor="$1" -v status="${2:-}" '
    /^#/ { next }
    ++row == 1 { next }
    $2 == actor && (status == "" || $5 == status) { n++ }
    END { print n + 0 }
  ' "$MATRIX"
}

# The badge labels, by badge name.
label_of() {
  local version
  version="$(spec_version)"
  case "$1" in
  federation-gateway) echo "Federation Tier $version gateway points" ;;
  federation-node) echo "Federation Tier $version node points" ;;
  federation-operator) echo "Federation Tier $version operator points" ;;
  aql-golden) echo "AQL golden cases" ;;
  *) die "no badge named $1" ;;
  esac
}

# Write every badge file into a directory.
write_badges() {
  local dir="$1" gateway covered node operator passed total
  [ -f "$PASS_LIST" ] || die "$PASS_LIST is missing"
  gateway="$(matrix_count Gateway)"
  covered="$(matrix_count Gateway covered)"
  node="$(matrix_count Node)"
  operator="$(matrix_count Operator)"
  passed="$(grep -cvE '^(#|total |$)' "$PASS_LIST" || true)"
  total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$PASS_LIST")"
  [ -n "$total" ] || die "$PASS_LIST has no total line"
  mkdir -p "$dir"
  badge_json "$(label_of federation-gateway)" "$covered / $gateway covered" "$(colour_of "$covered" "$gateway")" > "$dir/federation-gateway.json"
  badge_json "$(label_of federation-node)" "$node, a member node's to meet" blue > "$dir/federation-node.json"
  badge_json "$(label_of federation-operator)" "$operator, the operator's to meet" blue > "$dir/federation-operator.json"
  badge_json "$(label_of aql-golden)" "$passed / $total" "$(colour_of "$passed" "$total")" > "$dir/aql-golden.json"
}

# The README block with its markers: the three matrix badges, linked to the
# book's conformance page, then the golden badge, linked to its pass list.
render_block() {
  local name
  echo '<!-- conformance:begin -->'
  for name in federation-gateway federation-node federation-operator; do
    echo "[![$(label_of "$name")](${ENDPOINT}${name}.json)]($BOOK_PAGE)"
  done
  echo "[![$(label_of aql-golden)](${ENDPOINT}aql-golden.json)]($PASS_LIST)"
  echo '<!-- conformance:end -->'
}

# Replace the README block, markers included, with a fresh rendering.
write_block() {
  local block
  if ! grep -qx '<!-- conformance:begin -->' "$README" || ! grep -qx '<!-- conformance:end -->' "$README"; then
    die "$README has no conformance:begin and conformance:end markers"
  fi
  block="$(mktemp)"
  render_block > "$block"
  awk -v block="$block" '
    $0 == "<!-- conformance:begin -->" { while ((getline line < block) > 0) print line; skipping = 1; next }
    $0 == "<!-- conformance:end -->" { skipping = 0; next }
    !skipping { print }
  ' "$README" > "$block.readme"
  cat "$block.readme" > "$README"
  rm -f "$block" "$block.readme"
}

case "${1:-}" in
--badges)
  [ -n "${2:-}" ] || die "usage: $0 --badges DIR"
  write_badges "$2"
  ;;
--readme-block)
  render_block
  ;;
--badges-write)
  rm -f "$BADGES"/*.json
  write_badges "$BADGES"
  write_block
  echo "conformance-matrix: wrote $BADGES/ and the $README conformance block."
  ;;
--derived)
  [ -n "${2:-}" ] || die "usage: $0 --derived DIR"
  mkdir -p "$2"
  derive_into "$2"
  ;;
--derive)
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  derive_into "$work"
  refresh "$work/cps.tsv" "$MATRIX" "$(printf 'cp\tactor\trequirements\ttracks\tstatus\tissue\treason')"
  refresh "$work/tracks.tsv" "$TRACKS" "$(printf 'track\ttitle\trequirements\tcps\tstatus\tissue\treason')"
  {
    if [ -f "$REQUIREMENTS" ]; then awk '/^#/ { print; next } { exit }' "$REQUIREMENTS"; fi
    cat "$work/requirements.tsv"
  } > "$work/requirements.out"
  mv "$work/requirements.out" "$REQUIREMENTS"
  echo "conformance-matrix: refreshed $MATRIX, $TRACKS and $REQUIREMENTS from the vendored specification."
  ;;
--render)
  render_page
  ;;
--render-write)
  render_page > "$PAGE"
  echo "conformance-matrix: wrote $PAGE."
  ;;
*)
  die "usage: $0 --derived DIR | --derive | --render | --render-write | --badges DIR | --readme-block | --badges-write"
  ;;
esac
