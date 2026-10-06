#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The acts adopted under Regulation (EU) 2025/327, the European Health Data
# Space Regulation, and under Regulation (EU) 2024/2847, the Cyber Resilience
# Act, with the amendments and corrigenda of both, and the Commission's "Have
# your say" initiatives on the first, compared with what this repository has
# recorded. Regulation (EU) 2025/327 leaves the content of the harmonised
# components to acts not yet adopted: Art 15(1) (the exchange format), Art
# 36(1) (the common specifications), Art 40(4) (the digital testing
# environment), Art 39(6) (the declaration template), Art 49(4) (the EU
# database data) and Art 13(4) (the data quality requirements). No
# specification governs this lane: our own design.
#
#   scripts/checks/ehds-acts.sh
#
# EUR-Lex: every act based on or supplementing (an `act`), amending (an
# `amendment`) or correcting (a `corrigendum`) a Regulation of BASES, read
# from the Publications Office's Cellar, the repository EUR-Lex serves,
# through its SPARQL endpoint, with the article and paragraph of the legal
# basis. An act is known when scripts/vendor/eu.sh pins it (a `CELEX <n>` in
# its version) or NOT_VENDORED below records why it is not vendored. Any
# other prints a NEW-ACT line naming its Regulation and its kind, marked
# `watched` when it rests on one of the WATCHED_ARTICLES.
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
readonly SPARQL=https://publications.europa.eu/webapi/rdf/sparql
readonly REGISTER=https://ec.europa.eu/info/law/better-regulation/brpapi
readonly VENDOR_SCRIPT=scripts/vendor/eu.sh

# One "CELEX<TAB>label" record per line: a Regulation whose acts, amendments
# and corrigenda are read, and the label a NEW-ACT line names it by.
readonly BASES="\
32025R0327	EHDS
32024R2847	CRA"

# One "label<TAB>article" record per line: an article whose act a component
# waits for, as `Art N(P)` the way basis_text prints a legal basis.
readonly WATCHED_ARTICLES="\
EHDS	Art 15(1)
EHDS	Art 36(1)
EHDS	Art 40(4)
EHDS	Art 39(6)
EHDS	Art 49(4)
EHDS	Art 13(4)"

# One "CELEX<TAB>reason" record per line: an act read and not vendored,
# because it does not reach an EHR system or has no English version. The
# languages of each corrigendum are those Cellar listed on 2026-10-06.
readonly NOT_VENDORED="\
32026R0771	Art 92: the EHDS Board's operation, no obligation on an EHR system
32026R2098	Art 77(4): dataset descriptions for secondary use; FerroFED holds no dataset
32025R0327R(01)	no English version: it corrects the French, Croatian and Latvian versions only
32025R0327R(02)	no English version: it corrects the Hungarian version only
32025R0327R(03)	no English version: it corrects the Hungarian version only
32024R2847R(03)	no English version: it corrects the French and Hungarian versions only
32024R2847R(05)	no English version: it corrects the Slovak version only
32024R2847R(06)	no English version: it corrects the French version only
32024R2847R(07)	no English version: it corrects the German version only
52026IP0022	a European Parliament resolution on technological sovereignty, not an act adopted under the CRA"

# The initiatives the register listed for the Regulation on 2026-10-05.
readonly KNOWN_INITIATIVES="12663 14992 15353 15594 15673 16155 16712 17192 17512 17513 18579 18853"

# One "id<TAB>stage<TAB>what" record per line: an initiative whose act a
# component waits for, at the stage the register showed on 2026-10-05.
readonly WATCHED_INITIATIVES="\
17512	PLANNING_WORKFLOW	Art 15(1), the exchange format
18579	PLANNING_WORKFLOW	Art 36(1), the common specifications for the harmonised components
17513	PLANNING_WORKFLOW	Art 40(4), the European digital testing environment"

# basis_text CODE: a Cellar legal-basis code (`A23P4`, `A92P11FR1`) as
# `Art 23(4)`, a zero-padded number (`A07P4`) without its zeros, with any
# further qualifier kept verbatim in brackets.
basis_text() {
  awk -v code="$1" 'BEGIN {
    if (!match(code, /^A[0-9]+/)) { print "[" code "]"; exit }
    text = "Art " (substr(code, 2, RLENGTH - 1) + 0); rest = substr(code, RLENGTH + 1)
    if (match(rest, /^P[0-9]+/)) { text = text "(" (substr(rest, 2, RLENGTH - 1) + 0) ")"; rest = substr(rest, RLENGTH + 1) }
    if (rest != "") text = text " [" code "]"
    print text
  }'
}

# known_acts SCRIPT: the CELEX numbers the vendor script SCRIPT pins, a
# corrigendum's `R(NN)` suffix included, then those NOT_VENDORED records, one
# per line.
known_acts() {
  grep -oE 'CELEX 3[0-9]{4}[A-Z][0-9]{4}(R\([0-9]{2}\))?' "$1" | awk '{ print $2 }'
  awk -F'\t' '{ print $1 }' <<< "$NOT_VENDORED"
}

# classify_acts KNOWN: reads "label<TAB>kind<TAB>celex<TAB>date<TAB>codes<TAB>title"
# records on stdin, `-` for a field Cellar does not record, and prints one
# line per act; returns 1 when one is new.
classify_acts() {
  local known="$1" label kind celex date codes title code who article bases watched new=0
  while IFS=$'\t' read -r label kind celex date codes title; do
    [[ -n "$celex" ]] || continue
    bases=""
    if [[ "$codes" != "-" ]]; then
      for code in ${codes//,/ }; do
        bases="${bases:+$bases, }$(basis_text "$code")"
      done
    fi
    bases="${bases:-no article recorded}"
    if grep -qxF "$celex" <<< "$known"; then
      printf 'known      %s %s %s %s (%s)\n' "$celex" "$label" "$kind" "$date" "$bases"
      continue
    fi
    watched=""
    while IFS=$'\t' read -r who article; do
      [[ "$who" == "$label" ]] || continue
      [[ ", $bases," == *", $article,"* || ", $bases," == *", $article ["* ]] && watched=" watched"
    done <<< "$WATCHED_ARTICLES"
    printf 'NEW-ACT    %s %s %s %s (%s)%s: %s\n' "$celex" "$label" "$kind" "$date" "$bases" "$watched" "$title"
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
  expect "Art 7(4)" "$(basis_text A07P04)" "a zero-padded article and paragraph"
  expect "Art 2(5) [A02P5L2]" "$(basis_text A02P5L2)" "a zero-padded article with a qualifier"
  expect "[ANNEX]" "$(basis_text ANNEX)" "a code of another form"

  status=0
  got="$(printf 'EHDS\tact\t32026R2083\t2026-09-18\tA23P4,A23P8\tOn MyHealth@EU\n' \
    | classify_acts $'32026R2083\n32026R2099')" || status=$?
  expect "0 known      32026R2083 EHDS act 2026-09-18 (Art 23(4), Art 23(8))" "$status $got" "a pinned act"
  status=0
  got="$(printf 'EHDS\tact\t32027R0101\t2027-01-04\tA15P1\tOn the exchange format\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0101 EHDS act 2027-01-04 (Art 15(1)) watched: On the exchange format" "$status $got" "a watched act"
  status=0
  got="$(printf 'EHDS\tact\t32027R0202\t2027-02-01\tA15P10\tOn something else\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0202 EHDS act 2027-02-01 (Art 15(10)): On something else" "$status $got" "an unwatched act"
  status=0
  got="$(printf 'EHDS\tact\t32027R0303\t-\t-\tUnannotated\n' | classify_acts 32026R2083)" || status=$?
  expect "1 NEW-ACT    32027R0303 EHDS act - (no article recorded): Unannotated" "$status $got" "an act with no legal-basis article"
  status=0
  got="$(printf 'CRA\tact\t32027R0404\t2027-03-01\tA15P1\tOn a CRA article of the same number\n' | classify_acts 32025R2392)" || status=$?
  expect "1 NEW-ACT    32027R0404 CRA act 2027-03-01 (Art 15(1)): On a CRA article of the same number" "$status $got" \
    "a CRA act on an article number EHDS watches"
  status=0
  got="$(printf 'CRA\tact\t32025R2392\t2025-11-28\tA7P4\tOn product categories\n' | classify_acts $'32025R2392\n32024R2847R(01)')" || status=$?
  expect "0 known      32025R2392 CRA act 2025-11-28 (Art 7(4))" "$status $got" "a pinned CRA act"
  status=0
  got="$(printf 'CRA\tamendment\t32027R0505\t2027-04-01\t-\tAmending Regulation (EU) 2024/2847\n' | classify_acts 32025R2392)" || status=$?
  expect "1 NEW-ACT    32027R0505 CRA amendment 2027-04-01 (no article recorded): Amending Regulation (EU) 2024/2847" "$status $got" \
    "a new CRA amendment"
  status=0
  got="$(printf 'CRA\tcorrigendum\t32024R2847R(01)\t2026-03-25\t-\tCorrigendum\nCRA\tcorrigendum\t32024R2847R(08)\t2027-05-01\t-\tCorrigendum\n' \
    | classify_acts $'32025R2392\n32024R2847R(01)')" || status=$?
  expect "1 known      32024R2847R(01) CRA corrigendum 2026-03-25 (no article recorded)
NEW-ACT    32024R2847R(08) CRA corrigendum 2027-05-01 (no article recorded): Corrigendum" "$status $got" "a pinned and a new CRA corrigendum"
  expect "32024R2847R(01)" "$(known_acts <(printf '%s\n' 'pin commit x "OJ L (CELEX 32024R2847R(01)), English"') | head -n 1)" \
    "a pinned corrigendum read from the vendor script"

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

# One "Cellar property<TAB>kind" record per line: how an act relates to a
# Regulation of BASES, and the kind a NEW-ACT line names it by.
readonly RELATIONS="\
resource_legal_based_on_resource_legal	act
resource_legal_completes_resource_legal	act
resource_legal_amends_resource_legal	amendment
resource_legal_corrects_resource_legal	corrigendum"

# Every act related to a Regulation of BASES, one row per Regulation and
# act, with the kind that sorts first when an act relates in more than one
# way, the article and paragraph Cellar records on each legal-basis link and
# the English title. The minimum is taken over the string forms: over the
# typed date, grouped by two keys, the endpoint returned another act's date.
pairs=""
while IFS=$'\t' read -r base label; do
  while IFS=$'\t' read -r property kind; do
    pairs="$pairs    (\"$base\"^^xsd:string \"$label\" cdm:$property \"$kind\")"$'\n'
  done <<< "$RELATIONS"
done <<< "$BASES"
readonly QUERY="PREFIX cdm: <http://publications.europa.eu/ontology/cdm#>
PREFIX ann: <http://publications.europa.eu/ontology/annotation#>
PREFIX owl: <http://www.w3.org/2002/07/owl#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
SELECT ?label ?celex (MIN(STR(?k)) AS ?kind) (MIN(STR(?d)) AS ?date) (GROUP_CONCAT(DISTINCT ?code; separator=\",\") AS ?codes) (SAMPLE(?t) AS ?title)
WHERE {
  VALUES (?b ?label ?relation ?k) {
$pairs  }
  ?base cdm:resource_legal_id_celex ?b .
  ?act ?relation ?base ;
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
GROUP BY ?label ?celex
ORDER BY ?label ?celex"

status=0
unreadable=0
if ! acts="$(curl --fail --silent --show-error --max-time 120 --user-agent "$UA" \
  --header 'Accept: application/sparql-results+json' --data-urlencode "query=$QUERY" "$SPARQL" \
  | jq --raw-output --exit-status '.results.bindings
      | if length == 0 then error("no act relates to a watched Regulation") else . end
      | .[] | [.label.value, .kind.value, .celex.value, (.date.value // "-"), (.codes.value // "" | if . == "" then "-" else . end), (.title.value // "-" | gsub("[\t\n]+"; " "))]
      | @tsv' 2>&1)"; then
  printf 'UNREADABLE EUR-Lex: the acts related to %s could not be read from %s (%s)\n' \
    "$(awk -F'\t' '{ printf "%sCELEX %s", (NR > 1 ? " and " : ""), $1 }' <<< "$BASES")" "$SPARQL" "$acts"
  unreadable=1
else
  # A Regulation with no related act at all is a query that no longer reads
  # it, not one known to have nothing new.
  while IFS=$'\t' read -r base label; do
    if ! grep -qE "^$label"$'\t' <<< "$acts"; then
      printf 'UNREADABLE EUR-Lex: Cellar returned no act related to CELEX %s (%s)\n' "$base" "$label"
      unreadable=1
    fi
  done <<< "$BASES"
  classify_acts "$(known_acts "$VENDOR_SCRIPT")" <<< "$acts" || status=1
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
