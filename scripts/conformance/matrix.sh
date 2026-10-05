#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
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
#   scripts/conformance/matrix.sh --statement     print the conformance statement block
#   scripts/conformance/matrix.sh --tests-page    print the page of marked tests
#   scripts/conformance/matrix.sh --statement-write  write the statement block and the tests page
#   scripts/conformance/matrix.sh --check-pages   fail when a rendered book page is stale
#
# The conformance statement (#95) is a book page whose block between the
# statement:begin and statement:end markers is rendered here: the claim with
# the specification version, status and commit, the node products the
# harness runs from docs/VERSIONS.md, every point and track with its status,
# and every deferral with its reason. The prose around the block is kept by
# hand. The tests page lists the test each `// conformance:` marker sits
# above, by point and track. `--check-pages` is what the docs build runs, so
# a page that disagrees with the matrix fails it.
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
# (#89): the re-pin change scores it with a test, or records the owner's
# deferral. A re-pin of the specification therefore shows up as a reviewable
# diff of these files.
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
readonly STATEMENT=website/book/src/evaluate/conformance-statement.md
readonly TESTS_PAGE=website/book/src/evaluate/conformance-tests.md
readonly ANTORA=docs/specs/federation-spec/antora.yml
readonly SPEC_REPO=https://github.com/syntaric/openehr-federation-spec
readonly REF_REPO=https://github.com/syntaric/openehr-federation-ref
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
  local kind="$1" file="$2"
  awk -v kind="$kind" '
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
  ' "$file"
}

# Every requirement anchor in requirements.adoc, in specification order.
requirement_ids() {
  grep -oE '\[\[n[0-9]+[a-z]?\]\]' "$PAGES/requirements.adoc" | sed -E 's/\[\[n(.*)\]\]/N\1/'
}

# Write the three derived tables into a directory.
derive_into() {
  local dir="$1"
  [[ -f "$PAGES/conformance.adoc" ]] || die "the vendored specification is missing ($PAGES)"
  table_rows cp "$PAGES/conformance.adoc" > "$dir/cps.raw"
  table_rows track "$PAGES/testing.adoc" > "$dir/tracks.raw"
  requirement_ids > "$dir/requirements.raw"
  [[ -s "$dir/cps.raw" ]] || die "no conformance point parsed from $PAGES/conformance.adoc"
  [[ -s "$dir/tracks.raw" ]] || die "no test track parsed from $PAGES/testing.adoc"
  [[ -s "$dir/requirements.raw" ]] || die "no requirement parsed from $PAGES/requirements.adoc"

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
  if [[ -f "$target" ]]; then existing="$target"; fi
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
      print "<!-- SPDX-FileCopyrightText: Cadasto B.V. -->"
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

# versions_cell KEY: the second cell of the docs/VERSIONS.md table row whose
# first cell is KEY, trimmed.
versions_cell() {
  local key="$1" cell
  cell="$(awk -F'|' -v key="$key" '
    NF >= 3 {
      k = $2; v = $3
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == key) { print v; exit }
    }
  ' docs/VERSIONS.md)"
  [[ -n "$cell" ]] || die "docs/VERSIONS.md has no \"$key\" row"
  printf '%s\n' "$cell"
}

# The Federation Tier version docs/VERSIONS.md pins: the first word of the
# second cell of its specification-pin row.
spec_version() {
  local cell
  cell="$(versions_cell 'Federation Tier with AQL')"
  printf '%s\n' "${cell%% *}"
}

# versions_commit KEY: the 40-hex commit a docs/VERSIONS.md row pins.
versions_commit() {
  local key="$1" commit
  commit="$(versions_cell "$key" | grep -oE '[0-9a-f]{40}' | head -n 1)"
  [[ -n "$commit" ]] || die "the docs/VERSIONS.md \"$key\" row names no commit"
  printf '%s\n' "$commit"
}

# versions_image KEY: the image reference a docs/VERSIONS.md row pins, the
# first code span of its second cell, held to name:tag@sha256:digest.
versions_image() {
  local key="$1" image tick
  tick="$(printf '\x60')"
  image="$(versions_cell "$key" | cut -d "$tick" -f 2)"
  [[ "$image" =~ ^[^@:]+(:[0-9][^@]*)?:[^@]+@sha256:[0-9a-f]{64}$ ]] ||
    die "the docs/VERSIONS.md \"$key\" row pins no image by tag and digest"
  printf '%s\n' "$image"
}

# antora_attribute NAME: a display attribute of the vendored antora.yml.
antora_attribute() {
  local name="$1" value
  value="$(awk -v name="$name:" '
    $1 == name { sub(/^[^:]*:[[:space:]]*/, ""); gsub(/^'\''|'\''[[:space:]]*$/, ""); print; exit }
  ' "$ANTORA")"
  [[ -n "$value" ]] || die "$ANTORA has no $name attribute"
  printf '%s\n' "$value"
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

# One "id <TAB> file::function" line per point or track a `// conformance:`
# marker names, sorted: the test is the first fn the marker sits above. The
# file list is the guard's, tracked and new Rust files outside the vendored
# trees, and the guard holds each marker's placement and vocabulary.
marked_tests() {
  local list
  list="$(mktemp)"
  if ! git ls-files -z --cached --others --exclude-standard -- '*.rs' ':!docs/specs/**' > "$list" 2> /dev/null || [[ ! -s "$list" ]]; then
    rm -f "$list"
    die "the conformance markers are read from a git checkout, and this is none"
  fi
  # shellcheck disable=SC2016 # the single-quoted text is an awk program, run through xargs
  xargs -0 awk '
    FNR == 1 { body = "" }
    /^[[:space:]]*\/\/[[:space:]]*conformance:/ {
      body = $0
      sub(/^[[:space:]]*\/\/[[:space:]]*conformance:[[:space:]]*/, "", body)
      next
    }
    body != "" && /^[[:space:]]*(pub[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]+/ {
      name = $0
      sub(/^[[:space:]]*(pub[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+/, "", name)
      sub(/[^A-Za-z0-9_].*$/, "", name)
      n = split(body, tok, /[[:space:]]+/)
      for (i = 1; i <= n; i++) if (tok[i] != "") print tok[i] "\t" FILENAME "::" name
      body = ""
    }
  ' < "$list" | LC_ALL=C sort -u
  rm -f "$list"
}

# The page of marked tests, by point in section 17 order and then by track.
render_tests_page() {
  local marks
  marks="$(mktemp)"
  marked_tests > "$marks"
  awk -F'\t' '
    FILENAME == ARGV[1] { if ($0 !~ /^#/ && ++m > 1) ids[++nid] = $1; next }
    FILENAME == ARGV[2] { if ($0 !~ /^#/ && ++t > 1) ids[++nid] = "track-" $1; next }
    {
      id = $1
      split($2, part, "::")
      count[id]++
      if (part[1] != last[id]) {
        body[id] = body[id] (body[id] == "" ? "" : "\n") "- `" part[1] "`" (part[1] ~ /\/e2e\// ? " (e2e)" : "") ": `" part[2] "`"
        last[id] = part[1]
      } else {
        body[id] = body[id] ", `" part[2] "`"
      }
    }
    END {
      print "<!-- SPDX-FileCopyrightText: Cadasto B.V. -->"
      print "<!-- SPDX-License-Identifier: BUSL-1.1 -->"
      print "<!-- Generated by scripts/conformance/matrix.sh --statement-write from the conformance markers. Do not edit. -->"
      print ""
      print "# Conformance tests"
      print ""
      print "Every test that carries a `// conformance:` marker, under each point and"
      print "track the marker names, grouped by file. A file marked (e2e) runs in the"
      print "e2e harness, with `FERROFED_E2E=1`, against the node products the"
      print "[conformance statement](conformance-statement.md) names. A point or track"
      print "with no marked test is not listed."
      for (i = 1; i <= nid; i++) {
        id = ids[i]
        if (!(id in count)) continue
        print ""
        print "## " (id ~ /^track-/ ? "Track " substr(id, 7) : id)
        print ""
        printf "%d test%s.\n", count[id], (count[id] == 1 ? "" : "s")
        print ""
        print body[id]
      }
    }
  ' "$MATRIX" "$TRACKS" "$marks"
  rm -f "$marks"
}

# The conformance statement block, its markers included.
render_statement() {
  local site version status date commit reference ferroehr ehrbase marks
  site="$(spec_site)"
  version="$(spec_version)"
  status="$(antora_attribute spec-status)"
  date="$(antora_attribute spec-date)"
  commit="$(versions_commit 'Federation Tier with AQL specification')"
  reference="$(versions_commit 'Federation Tier reference implementation')"
  ferroehr="$(versions_image 'FerroEHR node image')"
  ehrbase="$(versions_image 'EHRbase node image')"
  marks="$(mktemp)"
  marked_tests > "$marks"
  awk -F'\t' -v site="$site" -v version="$version" -v status="$status" -v date="$date" \
    -v commit="$commit" -v reference="$reference" -v spec_repo="$SPEC_REPO" -v ref_repo="$REF_REPO" \
    -v ferroehr="$ferroehr" -v ehrbase="$ehrbase" '
    function issues(s,   out, n, parts, i) {
      if (s == "-") return "-"
      n = split(s, parts, ",")
      out = ""
      for (i = 1; i <= n; i++)
        out = out (out == "" ? "" : ", ") "[" parts[i] "](https://github.com/FerroHEALTH/FerroFED/issues/" substr(parts[i], 2) ")"
      return out
    }
    function list(s,   out) {
      out = s
      gsub(",", ", ", out)
      return out
    }
    function plural(n, word) { return n " " word (n == 1 ? "" : "s") }
    function tests(id, word) { return "[" plural(count[id] + 0, word) "](conformance-tests.md#" tolower(id) ")" }
    function scored(id, s) {
      if (s == "covered") return tests(id, "test")
      if (s == "deferred") return "[deferred](#" tolower(id) ")"
      if (s == "node-profile") return "the member CDR" (count[id] ? ", by " tests(id, "harness check") : "")
      if (s == "operator") return "the federation operator" (count[id] ? ", assisted by " tests(id, "harness check") : "")
      return "nothing yet: " s
    }
    # The image of a pin, its version and its reference.
    function product(name, image,   tag) {
      tag = image
      sub(/@.*/, "", tag)
      sub(/^.*:/, "", tag)
      return name " " tag " | `" image "`"
    }
    # The actor of a deferred track: the actors of the points that score it,
    # else the gateway, when every requirement it names falls in the
    # Federation-Gateway profile of section 16.2 (N1 to N43).
    function track_actor(cps, reqs,   n, p, i, out, num) {
      out = ""
      if (cps != "-") {
        n = split(cps, p, ",")
        for (i = 1; i <= n; i++) if (index(", " out ",", ", " actor[p[i]] ",") == 0) out = out (out == "" ? "" : ", ") actor[p[i]]
        return out
      }
      n = split(reqs, p, ",")
      for (i = 1; i <= n; i++) {
        num = p[i]
        sub(/^N/, "", num)
        sub(/[a-z]$/, "", num)
        if (num + 0 < 1 || num + 0 > 43) { bad = 1; return "" }
      }
      return "Gateway: no conformance point scores the track ([§16.3](" site "/testing.html#_16_3_test_tracks)), and " list(reqs) " " (n == 1 ? "falls" : "fall") " in the Federation-Gateway profile ([§16.2](" site "/testing.html#_16_2_two_conformance_profiles))"
    }
    FILENAME == ARGV[1] {
      if ($0 ~ /^#/ || ++m == 1) next
      row[++nrow] = $0
      actor[$1] = $2
      if ($2 == "Gateway") { gw++; if ($5 == "covered") gwc++; else if ($5 == "deferred") gwd++; else gwp++ }
      else if ($2 == "Node") node++
      else op++
      next
    }
    FILENAME == ARGV[2] {
      if ($0 ~ /^#/ || ++t == 1) next
      trow[++ntrow] = $0
      if ($5 == "covered") trc++; else if ($5 == "deferred") trd++; else trp++
      next
    }
    FILENAME == ARGV[3] {
      if ($0 ~ /^#/ || ++r == 1) next
      nreq++
      reach[$4]++
      next
    }
    {
      count[$1]++
      nmarks++
      if (!($2 in seen)) { seen[$2] = 1; ntests++ }
    }
    END {
      print "<!-- statement:begin -->"
      print "<!-- Generated by scripts/conformance/matrix.sh --statement-write from conformance/*.tsv, docs/VERSIONS.md, the vendored specification and the conformance markers. Do not edit between the statement markers. -->"
      print ""
      print "## The claim"
      print ""
      print "| | |"
      print "|---|---|"
      print "| Profile | Federation-Gateway ([§16.2](" site "/testing.html#_16_2_two_conformance_profiles)) |"
      print "| Specification | Federation Tier with AQL " version ", " tolower(status) " of " date " |"
      print "| Source | [`syntaric/openehr-federation-spec` at `" commit "`](" spec_repo "/tree/" commit ") |"
      print "| Rendered text | <" site "/> |"
      printf "| Gateway points | %d of %d scored by a test, %d deferred, %d planned |\n", gwc, gw, gwd, gwp
      printf "| Node points | %d, scored against the member CDR |\n", node
      printf "| Operator points | %d, scored against the federation operator |\n", op
      printf "| Test tracks | %d of %d scored by a test, %d deferred, %d planned |\n", trc, ntrow, trd, trp
      printf "| Requirements | %d: %d reached by a point, %d by a track only, %d by neither |\n", nreq, reach["direct"], reach["track-only"], reach["orphan"]
      printf "| Marked tests | %d, carrying %d point and track markers |\n", ntests, nmarks
      print ""
      print "## The nodes"
      print ""
      print "| Product | Pinned | Scores |"
      print "|---|---|---|"
      print "| " product("FerroEHR", ferroehr) " | the Connectathon tracks, the differential run and the node profile |"
      print "| " product("EHRbase", ehrbase) " | the node profile |"
      print "| The reference implementation | [`syntaric/openehr-federation-ref` at `" reference "`](" ref_repo "/tree/" reference ") | the differential run, as evidence and never as an oracle |"
      print ""
      print "## The scored matrix"
      print ""
      print "| Point | Actor | Requirements | Tracks | Status | Scored by |"
      print "|---|---|---|---|---|---|"
      for (i = 1; i <= nrow; i++) {
        split(row[i], f, "\t")
        printf "| [%s](%s/conformance.html#%s) | %s | %s | %s | %s | %s |\n", f[1], site, tolower(f[1]), f[2], list(f[3]), list(f[4]), f[5], scored(f[1], f[5])
      }
      print ""
      print "| Track | Title | Points | Status | Scored by |"
      print "|---|---|---|---|---|"
      for (i = 1; i <= ntrow; i++) {
        split(trow[i], f, "\t")
        printf "| [%s](%s/testing.html#track-%s) | %s | %s | %s | %s |\n", f[1], site, f[1], f[2], list(f[4]), f[5], scored("track-" f[1], f[5])
      }
      print ""
      print "## Deferrals"
      for (i = 1; i <= nrow; i++) {
        split(row[i], f, "\t")
        if (f[5] != "deferred") continue
        print ""
        print "### " f[1]
        print ""
        print "- **Actor:** " f[2]
        print "- **Requirements:** " list(f[3]) ", in track " list(f[4])
        print "- **Decision:** " f[7]
        print "- **Recorded on:** " issues(f[6])
      }
      for (i = 1; i <= ntrow; i++) {
        split(trow[i], f, "\t")
        if (f[5] != "deferred") continue
        print ""
        print "### Track " f[1]
        print ""
        print "- **Actor:** " track_actor(f[4], f[3])
        print "- **Requirements:** " list(f[3])
        print "- **Decision:** " f[7]
        print "- **Recorded on:** " issues(f[6])
      }
      print ""
      print "## Points scored against another actor"
      print ""
      for (i = 1; i <= nrow; i++) {
        split(row[i], f, "\t")
        if (f[2] == "Gateway") continue
        printf "- **[%s](%s/conformance.html#%s), %s:** %s (%s).\n", f[1], site, tolower(f[1]), f[2], f[7], issues(f[6])
      }
      print ""
      print "<!-- statement:end -->"
      if (bad) exit 3
    }
  ' "$MATRIX" "$TRACKS" "$REQUIREMENTS" "$marks" || {
    rm -f "$marks"
    die "a deferred track has no point and names a requirement outside the Federation-Gateway profile; name its actor"
  }
  rm -f "$marks"
}

# replace_block FILE NAME RENDERED: replace the block between the NAME:begin
# and NAME:end markers of FILE, markers included, with the RENDERED file.
replace_block() {
  local file="$1" name="$2" rendered="$3" tmp
  if ! grep -qx "<!-- $name:begin -->" "$file" || ! grep -qx "<!-- $name:end -->" "$file"; then
    die "$file has no $name:begin and $name:end markers"
  fi
  tmp="$(mktemp)"
  awk -v block="$rendered" -v begin="<!-- $name:begin -->" -v end="<!-- $name:end -->" '
    $0 == begin { while ((getline line < block) > 0) print line; skipping = 1; next }
    $0 == end { skipping = 0; next }
    !skipping { print }
  ' "$file" > "$tmp"
  cat "$tmp" > "$file"
  rm -f "$tmp"
}

# held_block FILE NAME: the block between the NAME markers of FILE, markers
# included.
held_block() {
  local file="$1" name="$2"
  sed -n "/^<!-- $name:begin -->\$/,/^<!-- $name:end -->\$/p" "$file"
}

# Fail when the matrix page, the statement block or the tests page differs
# from a fresh rendering: what the docs build and the guard run.
check_pages() {
  local work stale=0
  work="$(mktemp -d)"
  render_page > "$work/conformance.md"
  render_statement > "$work/statement.md"
  render_tests_page > "$work/tests.md"
  held_block "$STATEMENT" statement > "$work/statement.held"
  if ! diff -u "$PAGE" "$work/conformance.md" > "$work/diff"; then
    echo "conformance-matrix: $PAGE is stale (run scripts/conformance/matrix.sh --render-write):" >&2
    sed 's/^/  /' "$work/diff" | head -40 >&2
    stale=1
  fi
  if ! diff -u "$work/statement.held" "$work/statement.md" > "$work/diff"; then
    echo "conformance-matrix: the statement block of $STATEMENT is stale (run scripts/conformance/matrix.sh --statement-write):" >&2
    sed 's/^/  /' "$work/diff" | head -40 >&2
    stale=1
  fi
  if ! diff -u "$TESTS_PAGE" "$work/tests.md" > "$work/diff"; then
    echo "conformance-matrix: $TESTS_PAGE is stale (run scripts/conformance/matrix.sh --statement-write):" >&2
    sed 's/^/  /' "$work/diff" | head -40 >&2
    stale=1
  fi
  rm -rf "$work"
  [[ "$stale" -eq 0 ]] || exit 1
}

# The badge colour of k out of n, by the share in quarters.
colour_of() {
  local passed="$1" total="$2" share
  if [[ "$total" -eq 0 ]]; then
    echo lightgrey
    return
  fi
  share=$((passed * 100 / total))
  if [[ "$passed" -eq "$total" ]]; then echo brightgreen
  elif [[ "$share" -ge 75 ]]; then echo green
  elif [[ "$share" -ge 50 ]]; then echo yellow
  elif [[ "$share" -ge 25 ]]; then echo orange
  else echo red
  fi
}

# One endpoint badge file: label, message, colour. The labels and messages
# are written here and carry no character JSON escapes.
badge_json() {
  local label="$1" message="$2" colour="$3"
  printf '{"schemaVersion":1,"label":"%s","message":"%s","color":"%s"}\n' "$label" "$message" "$colour"
}

# matrix_count ACTOR [STATUS]: the number of matrix rows with ACTOR, and with
# ACTOR and STATUS.
matrix_count() {
  local actor="$1" status="${2:-}"
  awk -F'\t' -v actor="$actor" -v status="$status" '
    /^#/ { next }
    ++row == 1 { next }
    $2 == actor && (status == "" || $5 == status) { n++ }
    END { print n + 0 }
  ' "$MATRIX"
}

# The badge labels, by badge name.
label_of() {
  local name="$1" version
  version="$(spec_version)"
  case "$name" in
  federation-gateway) echo "Federation Tier $version gateway points" ;;
  federation-node) echo "Federation Tier $version node points" ;;
  federation-operator) echo "Federation Tier $version operator points" ;;
  aql-golden) echo "AQL golden cases" ;;
  *) die "no badge named $name" ;;
  esac
}

# Write every badge file into a directory.
write_badges() {
  local dir="$1" gateway covered node operator passed total
  [[ -f "$PASS_LIST" ]] || die "$PASS_LIST is missing"
  gateway="$(matrix_count Gateway)"
  covered="$(matrix_count Gateway covered)"
  node="$(matrix_count Node)"
  operator="$(matrix_count Operator)"
  passed="$(grep -cvE '^(#|total |$)' "$PASS_LIST" || true)"
  total="$(sed -n 's/^total \([0-9][0-9]*\)$/\1/p' "$PASS_LIST")"
  [[ -n "$total" ]] || die "$PASS_LIST has no total line"
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
  block="$(mktemp)"
  render_block > "$block"
  replace_block "$README" conformance "$block"
  rm -f "$block"
}

# Write the statement block and the tests page.
write_statement() {
  local block
  block="$(mktemp)"
  render_statement > "$block"
  replace_block "$STATEMENT" statement "$block"
  rm -f "$block"
  render_tests_page > "$TESTS_PAGE"
}

case "${1:-}" in
--badges)
  [[ -n "${2:-}" ]] || die "usage: $0 --badges DIR"
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
  [[ -n "${2:-}" ]] || die "usage: $0 --derived DIR"
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
    if [[ -f "$REQUIREMENTS" ]]; then awk '/^#/ { print; next } { exit }' "$REQUIREMENTS"; fi
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
--statement)
  render_statement
  ;;
--tests-page)
  render_tests_page
  ;;
--statement-write)
  write_statement
  echo "conformance-matrix: wrote the statement block of $STATEMENT and $TESTS_PAGE."
  ;;
--check-pages)
  check_pages
  ;;
*)
  die "usage: $0 --derived DIR | --derive | --render | --render-write | --badges DIR | --readme-block | --badges-write | --statement | --tests-page | --statement-write | --check-pages"
  ;;
esac
