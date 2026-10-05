#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The acts adopted under Regulation (EU) 2025/327, the European Health Data
# Space Regulation, and the Commission's "Have your say" initiatives on it,
# compared with what this repository has recorded. Regulation (EU) 2025/327
# leaves the content of the harmonised components to acts not yet adopted:
# Art 15(1) (the exchange format), Art 36(1) (the common specifications),
# Art 40(4) (the digital testing environment), Art 39(6) (the declaration
# template), Art 49(4) (the EU database data) and Art 13(4) (the data
# quality requirements). No specification governs this lane: our own design.
#
#   scripts/checks/ehds-acts.sh
#
# EUR-Lex: every act whose legal basis is CELEX 32025R0327, read from the
# Publications Office's Cellar, the repository EUR-Lex serves, through its
# SPARQL endpoint, with the article and paragraph of the legal basis. An act
# is known when scripts/vendor/eu.sh pins it (a `CELEX <n>` in its version)
# or NOT_VENDORED below records why it is not vendored. Any other prints a
# NEW-ACT line, marked `watched` when it rests on one of the articles above.
#
# Have your say: every initiative the register finds for "European Health
# Data Space", and the stage of each one WATCHED_INITIATIVES records. An
# initiative the register lists that KNOWN_INITIATIVES does not prints a
# NEW-INITIATIVE line, and a watched one at another stage a MOVED line.
#
#   scripts/checks/ehds-acts.sh --self-test
#
# Runs the classification over fixtures, offline.
#
# Exit 0 when nothing changed, 1 when an act or an initiative is new or a
# watched initiative moved, 2 when a source could not be read, so a source
# that does not answer never reads as nothing new. Needs awk, curl, grep and
# jq.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly UA=ferrofed-pin-check
readonly BASE_CELEX=32025R0327
readonly SPARQL=https://publications.europa.eu/webapi/rdf/sparql
readonly REGISTER=https://ec.europa.eu/info/law/better-regulation/brpapi
readonly VENDOR_SCRIPT=scripts/vendor/eu.sh

# The articles of Regulation (EU) 2025/327 whose act the components wait for,
# as `Art N(P)` the way basis_text prints a legal basis.
readonly WATCHED_ARTICLES="Art 15(1)
Art 36(1)
Art 40(4)
Art 39(6)
Art 49(4)
Art 13(4)"

# One "CELEX<TAB>reason" record per line: an adopted act read and not
# vendored, because it does not reach an EHR system.
readonly NOT_VENDORED="\
32026R0771	Art 92: the EHDS Board's operation, no obligation on an EHR system
32026R2098	Art 77(4): dataset descriptions for secondary use; FerroFED holds no dataset"

# The initiatives the register listed for the Regulation on 2026-10-05.
readonly KNOWN_INITIATIVES="12663 14992 15353 15594 15673 16155 16712 17192 17512 17513 18579 18853"

# One "id<TAB>stage<TAB>what" record per line: an initiative whose act a
# component waits for, at the stage the register showed on 2026-10-05.
readonly WATCHED_INITIATIVES="\
17512	PLANNING_WORKFLOW	Art 15(1), the exchange format
18579	PLANNING_WORKFLOW	Art 36(1), the common specifications for the harmonised components
17513	PLANNING_WORKFLOW	Art 40(4), the European digital testing environment"

# basis_text CODE: a Cellar legal-basis code (`A23P4`, `A92P11FR1`) as
# `Art 23(4)`, with any further qualifier kept verbatim in brackets.
basis_text() {
  awk -v code="$1" 'BEGIN {
    if (!match(code, /^A[0-9]+/)) { print "[" code "]"; exit }
    text = "Art " substr(code, 2, RLENGTH - 1); rest = substr(code, RLENGTH + 1)
    if (match(rest, /^P[0-9]+/)) { text = text "(" substr(rest, 2, RLENGTH - 1) ")"; rest = substr(rest, RLENGTH + 1) }
    if (rest != "") text = text " [" code "]"
    print text
  }'
}

# known_acts: the CELEX numbers the vendor script pins, then those
# NOT_VENDORED records, one per line.
known_acts() {
  grep -oE 'CELEX 3[0-9]{4}[A-Z][0-9]{4}' "$VENDOR_SCRIPT" | awk '{ print $2 }'
  awk -F'\t' '{ print $1 }' <<< "$NOT_VENDORED"
}

# classify_acts KNOWN: reads "celex<TAB>date<TAB>codes<TAB>title" records on
# stdin, `-` for a field Cellar does not record, and prints one line per
# act; returns 1 when one is new.
classify_acts() {
  local known="$1" celex date codes title code bases watched new=0
  while IFS=$'\t' read -r celex date codes title; do
    [[ -n "$celex" ]] || continue
    bases=""
    if [[ "$codes" != "-" ]]; then
      for code in ${codes//,/ }; do
        bases="${bases:+$bases, }$(basis_text "$code")"
      done
    fi
    bases="${bases:-no article recorded}"
    if grep -qxF "$celex" <<< "$known"; then
      printf 'known      %s %s (%s)\n' "$celex" "$date" "$bases"
      continue
    fi
    watched=""
    while IFS= read -r code; do
      [[ ", $bases," == *", $code,"* || ", $bases," == *", $code ["* ]] && watched=" watched"
    done <<< "$WATCHED_ARTICLES"
    printf 'NEW-ACT    %s %s (%s)%s: %s\n' "$celex" "$date" "$bases" "$watched" "$title"
    new=1
  done
  return "$new"
}

# classify_initiatives: reads "id<TAB>stage<TAB>title" records on stdin,
# one per initiative the register lists, the stage `-` for one nobody
# watches (a tab is whitespace to `read`, so an empty field would shift the
# next one into its place), and prints one line per initiative; returns 1 when one is new or
# a watched one moved.
classify_initiatives() {
  local id stage title want what changed=0
  while IFS=$'\t' read -r id stage title; do
    [[ -n "$id" ]] || continue
    if ! grep -qwF "$id" <<< "$KNOWN_INITIATIVES"; then
      printf 'NEW-INITIATIVE %s: %s\n' "$id" "$title"
      changed=1
      continue
    fi
    want="$(awk -F'\t' -v id="$id" '$1 == id { print $2; exit }' <<< "$WATCHED_INITIATIVES")"
    what="$(awk -F'\t' -v id="$id" '$1 == id { print $3; exit }' <<< "$WATCHED_INITIATIVES")"
    if [[ -z "$want" ]]; then
      continue
    fi
    if [[ "$stage" == "$want" ]]; then
      printf 'initiative %s %s (%s)\n' "$id" "$stage" "$what"
    else
      printf 'MOVED      %s: stage %s, recorded %s (%s): %s\n' "$id" "$stage" "$want" "$what" "$title"
      changed=1
    fi
  done
  return "$changed"
}

self_test() {
  local failed=0 got status
  # expect WANT GOT WHAT: one comparison of the self-test.
  expect() {
    if [[ "$2" != "$1" ]]; then
      printf 'ehds-acts: self-test failed: %s gave "%s", wanted "%s".\n' "$3" "$2" "$1" >&2
      failed=1
    fi
  }
  expect "Art 23(4)" "$(basis_text A23P4)" "a paragraph"
  expect "Art 92(11) [A92P11FR1]" "$(basis_text A92P11FR1)" "a qualified paragraph"
  expect "Art 16" "$(basis_text A16)" "an article"
  expect "[ANNEX]" "$(basis_text ANNEX)" "a code of another form"

  status=0
  got="$(printf '32026R2083\t2026-09-18\tA23P4,A23P8\tOn MyHealth@EU\n' \
    | classify_acts $'32026R2083\n32026R2099')" || status=$?
  expect "0 known      32026R2083 2026-09-18 (Art 23(4), Art 23(8))" "$status $got" "a pinned act"
  status=0
  got="$(printf '32027R0101\t2027-01-04\tA15P1\tOn the exchange format\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0101 2027-01-04 (Art 15(1)) watched: On the exchange format" "$status $got" "a watched act"
  status=0
  got="$(printf '32027R0202\t2027-02-01\tA15P10\tOn something else\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0202 2027-02-01 (Art 15(10)): On something else" "$status $got" "an unwatched act"
  status=0
  got="$(printf '32027R0303\t-\t-\tUnannotated\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0303 - (no article recorded): Unannotated" "$status $got" "an act with no legal-basis article"

  status=0
  got="$(printf '17512\tPLANNING_WORKFLOW\tFormat\n18853\t-\tLabel\n' | classify_initiatives)" || status=$?
  expect "0 initiative 17512 PLANNING_WORKFLOW (Art 15(1), the exchange format)" "$status $got" "an unmoved initiative"
  status=0
  got="$(printf '18579\tFEEDBACK_WORKFLOW\tSpecs\n' | classify_initiatives)" || status=$?
  expect "1 MOVED      18579: stage FEEDBACK_WORKFLOW, recorded PLANNING_WORKFLOW (Art 36(1), the common specifications for the harmonised components): Specs" \
    "$status $got" "a moved initiative"
  status=0
  got="$(printf '19999\t-\tA new one\n' | classify_initiatives)" || status=$?
  expect "1 NEW-INITIATIVE 19999: A new one" "$status $got" "a new initiative"

  [[ "$failed" -eq 0 ]] || exit 1
  echo "ehds-acts: self-test OK."
}

case "${1:-}" in
  --self-test)
    self_test
    exit 0
    ;;
  '') ;;
  *)
    echo "usage: scripts/checks/ehds-acts.sh [--self-test]" >&2
    exit 2
    ;;
esac

# The acts whose legal basis is the Regulation, with the article and
# paragraph Cellar records on each legal-basis link and the English title.
readonly QUERY="PREFIX cdm: <http://publications.europa.eu/ontology/cdm#>
PREFIX ann: <http://publications.europa.eu/ontology/annotation#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
SELECT ?celex (MIN(?d) AS ?date) (GROUP_CONCAT(DISTINCT ?code; separator=\",\") AS ?codes) (SAMPLE(?t) AS ?title)
WHERE {
  ?base cdm:resource_legal_id_celex \"$BASE_CELEX\"^^xsd:string .
  ?act cdm:resource_legal_based_on_resource_legal ?base ;
       cdm:resource_legal_id_celex ?celex .
  OPTIONAL { ?act cdm:work_date_document ?d }
  OPTIONAL {
    ?axiom owl:annotatedSource ?act ;
           owl:annotatedProperty cdm:resource_legal_based_on_resource_legal ;
           owl:annotatedTarget ?base ;
           ann:comment_on_legal_basis ?code .
  }
  OPTIONAL {
    ?expression cdm:expression_belongs_to_work ?act ;
                cdm:expression_uses_language <http://publications.europa.eu/resource/authority/language/ENG> ;
                cdm:expression_title ?t .
  }
}
GROUP BY ?celex
ORDER BY ?celex"

status=0
unreadable=0
if ! acts="$(curl --fail --silent --show-error --max-time 120 --user-agent "$UA" \
  --header 'Accept: application/sparql-results+json' --data-urlencode "query=$QUERY" "$SPARQL" \
  | jq --raw-output --exit-status '.results.bindings
      | if length == 0 then error("no act is based on the Regulation") else . end
      | .[] | [.celex.value, (.date.value // "-"), (.codes.value // "" | if . == "" then "-" else . end), (.title.value // "-" | gsub("[\t\n]+"; " "))]
      | @tsv' 2>&1)"; then
  printf 'UNREADABLE EUR-Lex: the acts based on CELEX %s could not be read from %s (%s)\n' "$BASE_CELEX" "$SPARQL" "$acts"
  unreadable=1
else
  classify_acts "$(known_acts)" <<< "$acts" || status=1
fi

if ! search="$(curl --fail --silent --show-error --max-time 120 --get --user-agent "$UA" \
  --data-urlencode 'text=European Health Data Space' --data-urlencode size=100 --data-urlencode language=EN \
  "$REGISTER/searchInitiatives" \
  | jq --raw-output --exit-status '.initiativeResultDtoPage
      | if .totalElements > (.content | length) then error("the result spans more than one page") else . end
      | .content[] | [(.id | floor | tostring), (.shortTitle // "-" | gsub("[\t\n]+"; " "))] | @tsv' 2>&1)"; then
  printf 'UNREADABLE Have your say: the initiatives could not be read from %s (%s)\n' "$REGISTER" "$search"
  unreadable=1
else
  records=""
  while IFS=$'\t' read -r id title; do
    [[ -n "$id" ]] || continue
    stage="-"
    if grep -qE "^$id"$'\t' <<< "$WATCHED_INITIATIVES"; then
      if ! stage="$(curl --fail --silent --show-error --max-time 60 --user-agent "$UA" \
        "$REGISTER/groupInitiatives/$id" | jq --raw-output --exit-status '.stage' 2>&1)"; then
        printf 'UNREADABLE Have your say: the stage of initiative %s could not be read (%s)\n' "$id" "$stage"
        unreadable=1
        continue
      fi
    fi
    records="$records$id"$'\t'"$stage"$'\t'"$title"$'\n'
  done <<< "$search"
  # A watched initiative the search no longer returns is not one known to be
  # unmoved.
  while IFS=$'\t' read -r id _; do
    if ! grep -qE "^$id"$'\t' <<< "$records"; then
      printf 'UNREADABLE Have your say: the search no longer returns watched initiative %s\n' "$id"
      unreadable=1
    fi
  done <<< "$WATCHED_INITIATIVES"
  classify_initiatives <<< "$records" || status=1
fi

[[ "$unreadable" -eq 0 ]] || exit 2
exit "$status"
