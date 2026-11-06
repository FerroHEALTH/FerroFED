#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The Chapter III conformity record guard (#525), for the obligations
# Articles 25 to 30, 36 to 41 and 49 of Regulation (EU) 2025/327 place on
# the manufacturer of an EHR system and the items of Annex IV. How it
# checks is our own design: no specification governs it.
#
# conformance/chapter-iii.tsv has one or more rows for every provision
# REQUIRED lists. The guard holds:
#
#   the text      each row's requirement is a phrase of its provision's own
#                 text in the vendored Regulation: the article, then the
#                 paragraph, then the point, or the item of Annex IV;
#   the date      each row's applies column is the date Art 105 gives the
#                 provision, and the vendored Art 105 still says so;
#   the tree      each evidence item is a file of the tree, and its anchor,
#                 when it has one, is a heading of a Markdown file or a test
#                 function of a Rust file;
#   the status    met, open, awaits-act, manufacturer or not-applicable; a
#                 met or not-applicable row names no issue, any other names
#                 its issues and says what is missing, and a not-applicable
#                 row says why;
#   no claim      Art 39(1) is not met while an Annex II point is partial or
#                 planned; Art 26(1) and the Chapter III statement of Annex
#                 IV item 4 are not met while another row is open, awaits an
#                 act or waits on the manufacturer; and the manufacturer's
#                 file does not say issue = true while a row is open or
#                 awaits an act;
#   the book      the page block equals a fresh rendering, every
#                 manufacturer row's provision is named in the page's list
#                 of what the manufacturer must do, and the summary lists
#                 the page.
#
# Usage:
#   scripts/checks/chapter-iii.sh
#   scripts/checks/chapter-iii.sh --self-test
# Needs awk, diff, grep, sed and tr. Exit 1 naming each refusal; 2 on a
# usage error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly RECORD=conformance/chapter-iii.tsv
readonly PAGE=website/book/src/evaluate/conformity-record.md
readonly SUMMARY=website/book/src/SUMMARY.md
readonly ANNEX_II=conformance/annex-ii.tsv
readonly DECLARATION=conformance/eu-declaration-of-conformity.toml
readonly REGULATION=docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml
readonly RENDER=scripts/conformance/chapter-iii.sh
readonly OWNER_HEADING='## What the manufacturer must decide or do'
readonly HEADER=$'provision\trequirement\tapplies\tstatus\tevidence\tissue\tnote'
readonly REQUIRED='Art 25(1)|Art 25(2)|Art 26(1)|Art 26(2)|Art 28(a)|Art 28(b)|Art 28(c)|Art 30(1)(a)|Art 30(1)(b)|Art 30(1)(c)|Art 30(1)(d)|Art 30(1)(e)|Art 30(1)(f)|Art 30(1)(g)|Art 30(1)(h)|Art 30(1)(i)|Art 30(1)(j)|Art 30(1)(k)|Art 30(1)(l)|Art 30(1)(m)|Art 30(1)(n)|Art 30(1)(o)|Art 30(2)|Art 30(3)|Art 30(4)|Art 30(5)|Art 36(1)|Art 37(1)|Art 37(2)|Art 37(3)|Art 37(4)|Art 38(1)|Art 38(2)(a)|Art 38(2)(b)|Art 38(2)(c)|Art 38(2)(d)|Art 38(2)(e)|Art 38(3)|Art 39(1)|Art 39(2)|Art 39(3)|Art 39(4)|Art 39(5)|Art 39(6)|Art 40(3)|Art 41(1)|Art 41(2)|Art 41(3)|Art 49(2)|Annex IV 1|Annex IV 2|Annex IV 3|Annex IV 4|Annex IV 5|Annex IV 6|Annex IV 7|Annex IV 8'
# The phrases of Art 105 the applies column rests on.
readonly ART_105='This Regulation shall apply from 26 March 2027.|Articles 25, 26, 27, 47, 48 and 49 shall apply as follows|from 26 March 2029 to priority categories of personal electronic health data referred to in Article 14(1), points (a), (b) and (c)|from 26 March 2031 to priority categories of personal electronic health data referred to in Article 14(1), points (d), (e) and (f)|Chapter III shall apply to EHR systems put into service in the Union referred to in Article 26(2) from 26 March 2031.|and Article 36(1) shall apply from the dates referred to in the third paragraph'
readonly STATEMENT='A statement that the EHR system in question is in conformity'

failures=0

fail() {
  echo "::error::chapter-iii: $*" >&2
  failures=$((failures + 1))
}

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

# applies_for PROVISION: the Art 105 date code of PROVISION. Articles 25,
# 26, 27 and 49 apply from the category dates, Art 26(2) systems from 26
# March 2031, the Art 36(1) acts from the category dates, the rest from 26
# March 2027.
# It sets APPLIES, so a caller forks no subshell per row.
applies_for() {
  case "$1" in
  'Art 26(2)') APPLIES=2031 ;;
  'Art 25('* | 'Art 26('* | 'Art 27('* | 'Art 49('* | 'Art 36(1)') APPLIES=2029+2031 ;;
  *) APPLIES=2027 ;;
  esac
}

# normalized OUT: the vendored Regulation as one line, each article,
# paragraph and annex opened by a marker (@ART 30@, @PAR 030.001@,
# @ANX IV@), its markup and no-break spaces turned into single spaces.
normalized() {
  local out="$1" nbsp
  nbsp="$(printf '\302\240')"
  tr '\n\r\t' '   ' < "$REGULATION" |
    sed -e 's/<div class="eli-subdivision" id="art_\([0-9]*\)">/ @ART \1@ /g' \
      -e 's/<div id="\([0-9][0-9][0-9]\.[0-9][0-9][0-9]\)">/ @PAR \1@ /g' \
      -e 's/<div class="eli-container" id="anx_\([IVX]*\)">/ @ANX \1@ /g' \
      -e 's/<[^>]*>/ /g' -e "s/$nbsp/ /g" -e 's/&amp;/\&/g' -e 's/  */ /g' > "$out"
}

# phrase_refusals TEXT: one line per row of the record whose requirement is
# not a phrase of its provision's own text in TEXT, the normalized
# Regulation: the article, then the paragraph, then the point, or the item
# of Annex IV. One awk for every row, since a process per row is slow.
phrase_refusals() {
  awk -F'\t' -v required="|$REQUIRED|" '
    function upto(s, a, b,   i, j, k) {
      i = index(s, a); j = index(s, b)
      k = (i > 0 && (j == 0 || i < j)) ? i : j
      return k > 0 ? substr(s, 1, k - 1) : s
    }
    function point(s, x,   letters, at, start, rest, next_x, stop) {
      start = index(s, "(" x ") ")
      if (start == 0) return ""
      rest = substr(s, start + 4)
      letters = "abcdefghijklmnopqrstuvwxyz"
      next_x = substr(letters, index(letters, x) + 1, 1)
      stop = index(rest, "(" next_x ") ")
      return stop > 0 ? substr(rest, 1, stop - 1) : rest
    }
    function own(prov,   n, at, s, stop, rest, art, para, pt, marker) {
      if (prov ~ /^Annex IV [1-8]$/) {
        n = substr(prov, 10) + 0
        at = index(text, "@ANX IV@")
        if (at == 0) return ""
        s = substr(text, at + 8)
        at = index(s, " " n ". ")
        if (at == 0) return ""
        s = substr(s, at + length(" " n ". "))
        stop = index(s, " " (n + 1) ". ")
        return stop > 0 ? substr(s, 1, stop - 1) : s
      }
      rest = substr(prov, 5)
      art = rest; sub(/\(.*$/, "", art)
      rest = substr(rest, length(art) + 1)
      para = ""; pt = ""
      if (match(rest, /^\([0-9]+\)/)) { para = substr(rest, 2, RLENGTH - 2); rest = substr(rest, RLENGTH + 1) }
      if (match(rest, /^\([a-z]\)$/)) { pt = substr(rest, 2, 1); rest = "" }
      if (rest != "") return ""
      at = index(text, "@ART " art "@")
      if (at == 0) return ""
      s = substr(text, at + length("@ART " art "@"))
      s = upto(s, "@ART ", "@ANX ")
      if (para != "") {
        marker = sprintf("@PAR %03d.%03d@", art, para)
        at = index(s, marker)
        if (at == 0) return ""
        s = substr(s, at + length(marker))
        stop = index(s, "@PAR ")
        if (stop > 0) s = substr(s, 1, stop - 1)
      }
      return pt != "" ? point(s, pt) : s
    }
    FNR == NR { text = text $0; next }
    /^#/ || /^$/ || $1 == "provision" || NF != 7 { next }
    index(required, "|" $1 "|") == 0 { next }
    {
      s = own($1)
      if (s == "") printf "line %d: the Regulation has no %s.\n", FNR, $1
      else if (index(s, $2) == 0) printf "line %d: \"%s\" is not a phrase of %s of the Regulation.\n", FNR, $2, $1
    }
  ' "$1" "$RECORD"
}

# has_heading FILE TEXT: FILE has a Markdown heading reading TEXT, outside a
# fenced code block.
has_heading() {
  local file="$1" text="$2"
  awk -v want="$text" '
    /^```/ { fence = !fence; next }
    !fence && /^#+ / { h = $0; sub(/^#+ /, "", h); if (h == want) found = 1 }
    END { exit !found }
  ' "$file"
}

# has_test FILE NAME: FILE defines the function NAME under a test attribute,
# among the attributes, their continuation lines and the comments directly
# above it, up to a blank line or a closing brace.
has_test() {
  local file="$1" name="$2"
  awk -v want="$name" '
    { line[NR] = $0 }
    $0 ~ ("fn " want "\\(") {
      for (k = NR - 1; k > 0 && line[k] !~ /^[[:space:]]*(\}.*)?$/; k--)
        if (line[k] ~ /^[[:space:]]*#\[.*test(\]|\()/) found = 1
    }
    END { exit !found }
  ' "$file"
}

# check_evidence LINE EVIDENCE: every item of EVIDENCE, split on "; ".
check_evidence() {
  local line="$1" rest="$2" item path anchor
  [[ "$rest" != - && -n "$rest" ]] || {
    fail "$RECORD line $line names no evidence."
    return
  }
  while [[ -n "$rest" ]]; do
    item="${rest%%; *}"
    if [[ "$item" == "$rest" ]]; then rest=""; else rest="${rest#*; }"; fi
    path="${item%%::*}"
    anchor=""
    [[ "$path" == "$item" ]] || anchor="${item#*::}"
    case "$path" in
    /* | *..* | '')
      fail "$RECORD line $line: \"$path\" is not a path inside the tree."
      continue
      ;;
    *) ;;
    esac
    if [[ ! -f "$path" ]]; then
      fail "$RECORD line $line: $path does not exist."
      continue
    fi
    [[ -n "$anchor" ]] || continue
    case "$path" in
    *.md) has_heading "$path" "$anchor" ||
      fail "$RECORD line $line: $path has no heading \"$anchor\"." ;;
    *.rs) has_test "$path" "$anchor" ||
      fail "$RECORD line $line: $path has no test \"$anchor\"." ;;
    *) fail "$RECORD line $line: $path takes no anchor; only a Markdown heading or a Rust test does." ;;
    esac
  done
}

# check_text TEXT: the Art 105 phrases are in the vendored Regulation.
check_text() {
  local text="$1" rest="$ART_105" phrase
  while [[ -n "$rest" ]]; do
    phrase="${rest%%|*}"
    if [[ "$phrase" == "$rest" ]]; then rest=""; else rest="${rest#*|}"; fi
    grep -Fq -- "$phrase" "$text" || fail "$REGULATION no longer says \"$phrase\" (Art 105)."
  done
}

# owner_section: the page's list of what the manufacturer must do.
owner_section() {
  awk -v want="$OWNER_HEADING" '
    $0 == want { on = 1; next }
    on && /^## / { exit }
    on { print }
  ' "$PAGE"
}

run() {
  local text row line=0 seen=" " provision requirement applies status evidence issue note
  local unmet="" owners statement_met="" fresh tabs refusal
  for f in "$RECORD" "$PAGE" "$SUMMARY" "$ANNEX_II" "$DECLARATION" "$REGULATION" "$RENDER"; do
    [[ -f "$f" ]] || {
      fail "$f is missing."
      finish
      return
    }
  done
  text="$(mktemp)"
  normalized "$text"
  check_text "$text"
  while IFS= read -r refusal; do
    fail "$RECORD $refusal"
  done < <(phrase_refusals "$text")
  owners="$(owner_section)"
  while IFS= read -r row; do
    line=$((line + 1))
    case "$row" in '#'* | '') continue ;; *) ;; esac
    tabs="${row//[!$'\t']/}"
    if [[ "${#tabs}" -ne 6 ]]; then
      fail "$RECORD line $line has not seven tab-separated columns."
      continue
    fi
    IFS=$'\t' read -r provision requirement applies status evidence issue note <<< "$row"
    if [[ "$provision" == provision ]]; then
      [[ "$row" == "$HEADER" ]] ||
        fail "$RECORD header is not provision, requirement, applies, status, evidence, issue, note."
      continue
    fi
    case "|$REQUIRED|" in
    *"|$provision|"*) ;;
    *)
      fail "$RECORD line $line: $provision is not a provision this record holds."
      continue
      ;;
    esac
    case "$seen" in
    *"|$provision|$requirement|"*) fail "$RECORD line $line: $provision has a second row for the same phrase." ;;
    *) seen="$seen|$provision|$requirement| " ;;
    esac
    applies_for "$provision"
    [[ "$applies" == "$APPLIES" ]] ||
      fail "$RECORD line $line: $provision applies from $APPLIES under Art 105, not $applies."
    check_evidence "$line" "$evidence"
    case "$status" in
    met | not-applicable)
      [[ "$issue" == - ]] || fail "$RECORD line $line: a $status row names no issue."
      [[ "$status" == met || "$note" != - ]] ||
        fail "$RECORD line $line: a not-applicable row says why in its note."
      ;;
    open | awaits-act | manufacturer)
      unmet="$unmet; $provision"
      [[ "$issue" =~ ^([A-Za-z]+/[A-Za-z]+)?#[0-9]+( ([A-Za-z]+/[A-Za-z]+)?#[0-9]+)*$ ]] ||
        fail "$RECORD line $line: a $status row names its issues as #N or Owner/Repo#N, separated by one space."
      [[ "$note" != - ]] || fail "$RECORD line $line: a $status row says what is missing in its note."
      if [[ "$status" == manufacturer ]]; then
        [[ "$owners" == *"$provision"* ]] ||
          fail "$PAGE names $provision nowhere under \"$OWNER_HEADING\"."
      fi
      ;;
    *) fail "$RECORD line $line: status \"$status\" is not met, open, awaits-act, manufacturer or not-applicable." ;;
    esac
    if [[ "$status" == met ]]; then
      case "$provision|$requirement" in
      'Art 26(1)|'* | "Annex IV 4|$STATEMENT"*) statement_met="$statement_met $provision" ;;
      'Art 39(1)|'*)
        awk -F'\t' '!/^#/ && ($3 == "partial" || $3 == "planned") { found = 1 } END { exit !found }' "$ANNEX_II" &&
          fail "$RECORD line $line: Art 39(1) is met while $ANNEX_II has a point partial or planned."
        ;;
      *) ;;
      esac
    fi
  done < "$RECORD"
  rm -f -- "$text"
  local rest="$REQUIRED" want
  while [[ -n "$rest" ]]; do
    want="${rest%%|*}"
    if [[ "$want" == "$rest" ]]; then rest=""; else rest="${rest#*|}"; fi
    case "$seen" in
    *"|$want|"*) ;;
    *) fail "$RECORD has no row for $want." ;;
    esac
  done
  if [[ -n "$statement_met" && -n "$unmet" ]]; then
    fail "$RECORD marks$statement_met met while rows are not:${unmet#;}."
  fi
  if grep -Eq '^issue[[:space:]]*=[[:space:]]*true' "$DECLARATION" &&
    awk -F'\t' '!/^#/ && ($4 == "open" || $4 == "awaits-act") { found = 1 } END { exit !found }' "$RECORD"; then
    fail "$DECLARATION says issue = true while $RECORD has rows open or awaiting an act."
  fi
  fresh="$(mktemp)"
  if bash "$RENDER" --page-block > "$fresh" 2> /dev/null; then
    sed -n '/^<!-- chapter-iii:begin -->$/,/^<!-- chapter-iii:end -->$/p' "$PAGE" | diff -q "$fresh" - > /dev/null ||
      fail "the $PAGE block is stale (run $RENDER --write)."
  else
    fail "$RENDER could not render the page block."
  fi
  rm -f -- "$fresh"
  grep -Fq '(evaluate/conformity-record.md)' "$SUMMARY" || fail "$SUMMARY does not list $PAGE."
  finish
}

finish() {
  if [[ "$failures" -gt 0 ]]; then
    echo "chapter-iii: $failures refusals." >&2
    exit 1
  fi
  echo "chapter-iii: every provision has its row, its phrase, its date and its evidence."
}

self_test() {
  local script render status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  render="$(cd "$(dirname "${BASH_SOURCE[0]}")/../conformance" && pwd)/chapter-iii.sh"
  repo="$work/repo"
  mkdir -p "$repo/scripts/checks" "$repo/scripts/conformance" "$repo/conformance" \
    "$repo/docs/specs/eu-ehds" "$repo/website/book/src/evaluate" "$repo/tests"
  cp "$script" "$repo/scripts/checks/chapter-iii.sh"
  cp "$render" "$repo/scripts/conformance/chapter-iii.sh"
  flunk() {
    echo "chapter-iii: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  expect() {
    local name="$1" want="$2" pattern="${3:-}"
    status=0
    (cd "$repo" && bash scripts/checks/chapter-iii.sh) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
    [[ -z "$pattern" ]] || grep -Fq -- "$pattern" "$work/out" || flunk "$name did not say \"$pattern\"."
  }
  # leaf PROVISION: the fixture's text of PROVISION, free of parentheses.
  leaf() {
    printf 'rule %s' "$1" | tr '(' '_' | tr -d ')'
  }
  # The fixture: a Regulation whose every provision has the text "rule
  # <provision>", split by markup and a no-break space; Art 105; one met row
  # per provision; the page with its owner list; a heading and a test.
  fixture() {
    local prov art para pt last_art="" last_para="" rest rest_p
    {
      printf '<p>rule Art 25_1 before the articles</p>\n'
      rest="$REQUIRED"
      while [[ -n "$rest" ]]; do
        prov="${rest%%|*}"
        if [[ "$prov" == "$rest" ]]; then rest=""; else rest="${rest#*|}"; fi
        case "$prov" in
        'Annex IV '*)
          [[ "$last_art" == anx ]] || printf '<div class="eli-container" id="anx_IV"><p>ANNEX IV</p>\n'
          last_art=anx
          printf '<p>%s.</p><p>rule\302\240<span>%s</span></p>\n' "${prov#Annex IV }" "$(leaf "$prov")"
          continue
          ;;
        *) ;;
        esac
        art="${prov#Art }"
        art="${art%%(*}"
        para=""
        pt=""
        rest_p="${prov#Art "$art"}"
        if [[ "$rest_p" =~ ^\(([0-9]+)\)(.*)$ ]]; then
          para="${BASH_REMATCH[1]}"
          rest_p="${BASH_REMATCH[2]}"
        fi
        [[ "$rest_p" =~ ^\(([a-z])\)$ ]] && pt="${BASH_REMATCH[1]}"
        if [[ "$art" != "$last_art" ]]; then
          printf '<div class="eli-subdivision" id="art_%s"><p>Article %s</p>\n' "$art" "$art"
          last_art="$art"
          last_para=""
        fi
        if [[ -n "$para" && "$para" != "$last_para" ]]; then
          printf '<div id="%03d.%03d"><p>%s. </p>\n' "$art" "$para" "$para"
          last_para="$para"
        fi
        if [[ -n "$pt" ]]; then
          printf '<p>(%s)</p><p>rule\302\240%s</p>\n' "$pt" "$(leaf "$prov" | sed 's/^rule //')"
        else
          printf '<p>rule\302\240%s</p>\n' "$(leaf "$prov" | sed 's/^rule //')"
        fi
      done
      printf '<div class="eli-subdivision" id="art_105"><p>%s</p></div>\n' "$(printf '%s' "$ART_105" | tr '|' ' ')"
    } > "$repo/$REGULATION"
    {
      printf '# a comment\n%s\n' "$HEADER"
      rest="$REQUIRED"
      while [[ -n "$rest" ]]; do
        prov="${rest%%|*}"
        if [[ "$prov" == "$rest" ]]; then rest=""; else rest="${rest#*|}"; fi
        applies_for "$prov"
        printf '%s\t%s\t%s\tmet\t%s::The evidence; tests/it.rs::a_test\t-\t-\n' \
          "$prov" "$(leaf "$prov")" "$APPLIES" "$PAGE"
      done
    } > "$repo/$RECORD"
    {
      printf '# Conformity record\n\n## The evidence\n\n<!-- chapter-iii:begin -->\n<!-- chapter-iii:end -->\n\n'
      printf '%s\n\n- Art 30(1)(e)\n\n## After\n' "$OWNER_HEADING"
    } > "$repo/$PAGE"
    printf -- '- [Conformity record](evaluate/conformity-record.md)\n' > "$repo/$SUMMARY"
    printf 'point\trequirement\tstatus\tevidence\tissue\tnote\n1.1\trule\tmet\tx\t-\t-\n' > "$repo/$ANNEX_II"
    printf 'issue = false\n' > "$repo/$DECLARATION"
    printf '#[test]\nfn a_test() {}\n\nfn not_a_test() {}\n' > "$repo/tests/it.rs"
    (cd "$repo" && bash scripts/conformance/chapter-iii.sh --write)
  }
  # row PROVISION FIELD VALUE: set one field of PROVISION's row and render
  # the page again; a FIELD of 0 drops the row.
  row() {
    local prov="$1" field="$2" value="${3:-}"
    awk -F'\t' -v OFS='\t' -v prov="$prov" -v field="$field" -v value="$value" '
      $1 == prov && field == 0 { next }
      $1 == prov { $field = value }
      { print }
    ' "$repo/$RECORD" > "$work/record" && mv "$work/record" "$repo/$RECORD"
    (cd "$repo" && bash scripts/conformance/chapter-iii.sh --write)
  }

  fixture
  expect "a complete record" 0 "every provision has its row"

  row 'Art 30(1)(c)' 0
  expect "a provision with no row" 1 "has no row for Art 30(1)(c)"
  fixture
  printf 'Art 31(1)\trule\t2027\tmet\t%s\t-\t-\n' "$PAGE" >> "$repo/$RECORD"
  (cd "$repo" && bash scripts/conformance/chapter-iii.sh --write)
  expect "a provision the record does not hold" 1 "Art 31(1) is not a provision this record holds"
  fixture
  grep -F 'Art 30(3)' "$repo/$RECORD" > "$work/dup"
  cat "$work/dup" >> "$repo/$RECORD"
  (cd "$repo" && bash scripts/conformance/chapter-iii.sh --write)
  expect "a second row for the same phrase" 1 "Art 30(3) has a second row for the same phrase"
  fixture
  row 'Art 30(1)(b)' 2 'rule Art 30_1_a'
  expect "a phrase of the point before" 1 "is not a phrase of Art 30(1)(b)"
  fixture
  row 'Art 38(2)(a)' 2 'rule Art 38_1'
  expect "a phrase of another paragraph" 1 "is not a phrase of Art 38(2)(a)"
  fixture
  row 'Annex IV 5' 2 'rule Annex IV 6'
  expect "a phrase of the next item" 1 "is not a phrase of Annex IV 5"
  fixture
  row 'Art 39(1)' 3 2029+2031
  expect "a date Art 105 does not give" 1 "Art 39(1) applies from 2027 under Art 105"
  fixture
  sed 's/from 26 March 2031 to priority/from 26 March 2033 to priority/' "$repo/$REGULATION" > "$work/reg" &&
    mv "$work/reg" "$repo/$REGULATION"
  expect "an Art 105 that moved" 1 "no longer says"
  fixture
  row 'Art 41(1)' 5 'missing.md'
  expect "evidence that does not exist" 1 "missing.md does not exist"
  fixture
  row 'Art 41(1)' 5 "$PAGE::No such heading"
  expect "a heading that does not exist" 1 "has no heading \"No such heading\""
  fixture
  row 'Art 41(1)' 5 'tests/it.rs::not_a_test'
  expect "a function that is no test" 1 "has no test \"not_a_test\""
  fixture
  row 'Art 37(1)' 4 'done'
  expect "an unknown status" 1 "status \"done\""
  fixture
  row 'Art 30(4)' 4 not-applicable
  expect "a not-applicable row with no reason" 1 "says why in its note"
  fixture
  row 'Art 40(3)' 4 awaits-act
  expect "an awaits-act row with no issue" 1 "names its issues as #N"
  row 'Art 40(3)' 6 '#675,#799'
  expect "issues not separated by one space" 1 "names its issues as #N"
  row 'Art 40(3)' 6 '#675 FerroHEALTH/FerroBRIDGE#1327'
  expect "an awaits-act row with no note" 1 "says what is missing"
  row 'Art 40(3)' 7 'no testing environment'
  row 'Art 26(1)' 4 open
  row 'Art 26(1)' 6 '#525'
  row 'Art 26(1)' 7 'the rows below'
  expect "an awaits-act row with its issue and note" 0
  printf 'issue = true\n' > "$repo/$DECLARATION"
  expect "a declaration issued while a row awaits an act" 1 "says issue = true while"
  printf 'issue = false\n' > "$repo/$DECLARATION"
  row 'Art 26(1)' 4 met
  row 'Art 26(1)' 6 -
  expect "Art 26(1) met while a row awaits an act" 1 "marks Art 26(1) met while rows are not: Art 40(3)"
  fixture
  row 'Art 39(2)' 4 manufacturer
  row 'Art 39(2)' 6 '#940'
  row 'Art 39(2)' 7 'other acts'
  expect "a manufacturer row the owner list does not name" 1 "names Art 39(2) nowhere"
  fixture
  printf 'point\trequirement\tstatus\tevidence\tissue\tnote\n1.1\trule\tpartial\tx\t#1\tmissing\n' > "$repo/$ANNEX_II"
  expect "Art 39(1) met while an Annex II point is partial" 1 "Art 39(1) is met while"
  fixture
  sed 's/^| met | [0-9]* |$/| met | 0 |/' "$repo/$PAGE" > "$work/page" && mv "$work/page" "$repo/$PAGE"
  expect "a page block that differs from the record" 1 "is stale"
  fixture
  printf '\n' > "$repo/$SUMMARY"
  expect "a page the summary does not list" 1 "does not list"
  echo "chapter-iii: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
"")
  cd "$(dirname "$0")/../.."
  run
  ;;
*) usage ;;
esac
