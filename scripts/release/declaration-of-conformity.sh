#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Writes the EU declaration of conformity of a release, Regulation (EU)
# 2025/327 Art 30(1)(e) and Art 39, with every item of Annex IV in its order:
#
#   <out-dir>/ferrofed-v<version>-eu-declaration-of-conformity.md
#
# Each item is read from the repository's own records at the checked-out
# tree: the name and version from CITATION.cff and docs/VERSIONS.md, the
# release date from CHANGELOG.md, the manufacturer from the one place the
# code writes it, the authorised representative and the standards from the
# information sheet, the harmonised components' versions from their
# manifests, and the common specifications from the technical documentation
# index. What only the manufacturer can supply (the place, the signatory,
# the signature, the harmonised standards, the other Union acts and the
# testing environment's result) is read from OWNER, which also says whether
# the manufacturer issues a declaration at all.
#
# A value OWNER leaves empty is written as a marked placeholder, and the
# script refuses, with nothing written, a declaration that still holds one,
# one OWNER does not issue, and a record that is missing or disagrees with
# the version. --draft writes the same document under a -draft name, with
# its placeholders, for the manufacturer to read before issuing it.
#
# Usage:
#   scripts/release/declaration-of-conformity.sh <version> <commit> <out-dir>
#   scripts/release/declaration-of-conformity.sh --draft <version> <commit> <out-dir>
#   scripts/release/declaration-of-conformity.sh status
#   scripts/release/declaration-of-conformity.sh --self-test
#
# <version> is X.Y.Z or X.Y.Z-pre, with or without a leading v; <commit> is
# the 40-character commit the tag names. `status` prints true when OWNER
# issues the declaration and every value is filled, false when it does not
# issue one, and refuses an issued OWNER with an empty value. Exit 1 on a
# refusal, 2 on a usage error. DECLARATION_ROOT names the repository root
# (the self-test points it at a stub tree). No specification governs how a
# release writes or attaches the declaration; it is FerroFED's own design.
set -euo pipefail
export LC_ALL=C

readonly OWNER=conformance/eu-declaration-of-conformity.toml
readonly CITATION=CITATION.cff
readonly PINS=docs/VERSIONS.md
readonly CHANGELOG=CHANGELOG.md
readonly MAKER=app/ferrofed-registry/src/manufacturer.rs
readonly SHEET=website/book/src/evaluate/information-sheet.md
readonly INDEX=conformance/technical-documentation.tsv
# The two harmonised components are published crates, each version the one
# the lock resolves: the logging component from the family's audit library
# (FerroHEALTH/FerroAUDIT, #1176) and the interoperability component from
# FerroHEALTH/FerroBRIDGE (#813).
readonly LOCK=Cargo.lock
readonly LOGGING=ehds-logging
readonly EXCHANGE=eehrxf
readonly REPO_URL=https://github.com/FerroHEALTH/FerroFED
readonly MARK='MANUFACTURER TO SUPPLY'
# The keys of OWNER besides `issue`, in the order the declaration reads them.
readonly FIELDS='other_union_acts testing_environment_result harmonised_standards place_of_issue signatory_name signatory_function on_behalf_of signature'

die() {
  echo "declaration-of-conformity: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 [--draft] <version> <commit> <out-dir> | status | --self-test" >&2
  exit 2
}

root() {
  printf '%s\n' "${DECLARATION_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
}

# read_owner ROOT: sets issue and one o_<key> per field from OWNER, refusing
# a line of another form, an unknown or repeated key, and a missing key.
read_owner() {
  local file="$1/$OWNER" line n=0 key value seen=" " re_str re_bool
  re_str='^([a-z_]+) = "([^"\\]*)"$'
  re_bool='^issue = (true|false)$'
  [[ -f "$file" ]] || die "$OWNER is missing"
  issue=""
  for key in $FIELDS; do eval "o_$key=''"; done
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    case "$line" in '' | '#'*) continue ;; esac
    if [[ "$line" =~ $re_bool ]]; then
      key=issue
      value="${BASH_REMATCH[1]}"
    elif [[ "$line" =~ $re_str ]]; then
      key="${BASH_REMATCH[1]}"
      value="${BASH_REMATCH[2]}"
      case " $FIELDS " in *" $key "*) ;; *) die "$OWNER line $n: '$key' is not a key of the declaration" ;; esac
    else
      die "$OWNER line $n is not key = \"value\" (one line, no quote or backslash inside) or issue = true|false"
    fi
    case "$seen" in *" $key "*) die "$OWNER line $n: '$key' is set twice" ;; esac
    seen="$seen$key "
    if [[ "$key" == issue ]]; then issue="$value"; else eval "o_$key=\$value"; fi
  done < "$file"
  for key in issue $FIELDS; do
    case "$seen" in *" $key "*) ;; *) die "$OWNER has no '$key'" ;; esac
  done
}

# empty_fields: the OWNER keys whose value is empty or blank.
empty_fields() {
  local key value out=""
  for key in $FIELDS; do
    eval "value=\$o_$key"
    [[ "$value" =~ [^[:space:]] ]] || out="$out $key"
  done
  printf '%s\n' "${out# }"
}

# owner KEY: the OWNER value, or the marked placeholder that names it.
owner() {
  local value
  eval "value=\$o_$1"
  if [[ "$value" =~ [^[:space:]] ]]; then
    printf '%s\n' "$value"
  else
    # shellcheck disable=SC2016 # the backticks are Markdown the declaration prints, never expanded
    printf '**[%s: `%s` in `%s`]**\n' "$MARK" "$1" "$OWNER"
  fi
}

# status: prints true or false, and refuses an issued OWNER with a blank.
status() {
  local r blank
  r="$(root)"
  read_owner "$r"
  if [[ "$issue" == false ]]; then
    echo false
    return
  fi
  blank="$(empty_fields)"
  [[ -z "$blank" ]] || die "$OWNER issues the declaration (issue = true) but leaves empty: $blank"
  echo true
}

# record WHAT VALUE: refuses an empty record.
record() {
  [[ -n "$2" ]] || die "$1"
  printf '%s\n' "$2"
}

# maker_field ROOT KEY: one field of the MANUFACTURER constant.
maker_field() {
  awk -v key="$2" '
    /^pub const MANUFACTURER/ { inside = 1; next }
    inside && /^};/ { inside = 0 }
    inside {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      if (index(line, key ": \"") == 1) {
        line = substr(line, length(key) + 4)
        sub(/",$/, "", line)
        print line
        exit
      }
    }
  ' "$1/$MAKER"
}

# sheet_row ROOT LABEL: the second cell of the information sheet's row LABEL.
sheet_row() {
  awk -F'|' -v want="$2" '
    /^\|/ {
      c = $2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", c)
      if (c == want) { v = $3; gsub(/^[[:space:]]+|[[:space:]]+$/, "", v); print v; exit }
    }
  ' "$1/$SHEET"
}

# standards ROOT: the rows of the sheet's table (e), as list items.
standards() {
  awk -F'|' '
    /^## \(e\)/ { inside = 1; next }
    inside && /^## / { inside = 0 }
    inside && /^\|/ {
      item = $2; pin = $3
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", item)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", pin)
      if (item == "Item" || item == "Product version" || item ~ /^-+$/ || item == "") next
      print "- " item ": " pin
    }
  ' "$1/$SHEET"
}

# locked_version ROOT NAME: the version Cargo.lock resolves for package NAME.
locked_version() {
  awk -v name="$2" '
    $0 == "name = \"" name "\"" { found = 1; next }
    found && /^version = / { gsub(/^version = "|"$/, ""); print; exit }
    /^\[\[package\]\]/ { found = 0 }
  ' "$1/$LOCK"
}

# common_specifications ROOT: the text of Annex IV point 6, from the index
# rows of Annex III point 3.
common_specifications() {
  local statuses
  statuses="$(awk -F'\t' '$1 == "3" { print $5 }' "$1/$INDEX")"
  [[ -n "$statuses" ]] || die "$INDEX has no row for Annex III point 3, the common specifications"
  case "$statuses" in
  *documented* | *partial*)
    die "$INDEX documents a common specification under Annex III point 3; name it in this script before a declaration is written"
    ;;
  esac
  printf '%s\n' "None. No common specification under Article 36 is adopted, as the technical documentation records for Annex III, point 3."
}

# run DRAFT VERSION COMMIT OUT: checks every record and writes the declaration.
run() {
  local draft="$1" version="${2#v}" commit="$3" out="$4" r name pinned citation date
  local maker_name maker_address maker_email maker_website representative
  local logging exchange specs list blank file tmp
  if ! printf '%s\n' "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
    echo "declaration-of-conformity: '$2' is not an X.Y.Z version" >&2
    usage
  fi
  if ! printf '%s\n' "$commit" | grep -qE '^[0-9a-f]{40}$'; then
    echo "declaration-of-conformity: '$commit' is not a 40-character commit" >&2
    usage
  fi
  r="$(root)"
  read_owner "$r"
  if [[ "$draft" == false ]]; then
    [[ "$issue" == true ]] || die "$OWNER does not issue the declaration (issue = false), so v$version has none"
    blank="$(empty_fields)"
    [[ -z "$blank" ]] || die "$OWNER issues the declaration but leaves empty: $blank"
  fi
  for f in "$CITATION" "$PINS" "$CHANGELOG" "$MAKER" "$SHEET" "$INDEX" "$LOCK"; do
    [[ -f "$r/$f" ]] || die "$f is missing"
  done

  name="$(record "$CITATION has no title" "$(sed -n 's/^title: "\(.*\)"$/\1/p' "$r/$CITATION")")"
  citation="$(sed -n 's/^version: //p' "$r/$CITATION")"
  [[ "$citation" == "$version" ]] || die "$CITATION names version '$citation', not $version"
  pinned="$(awk -F'|' '{ c = $2; gsub(/^ +| +$/, "", c); if (c == "Product version") { v = $3; gsub(/ /, "", v); print v; exit } }' "$r/$PINS")"
  [[ "$pinned" == "$version" ]] || die "$PINS pins product version '$pinned', not $version"
  date="$(awk -v h="## [$version] - " 'index($0, h) == 1 { print substr($0, length(h) + 1); exit }' "$r/$CHANGELOG")"
  printf '%s\n' "$date" | grep -qE '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' ||
    die "$CHANGELOG has no dated '## [$version]' section, so v$version has no date of issue"
  maker_name="$(record "$MAKER names no manufacturer" "$(maker_field "$r" name)")"
  maker_address="$(record "$MAKER names no postal address" "$(maker_field "$r" postal_address)")"
  maker_email="$(record "$MAKER names no email address" "$(maker_field "$r" email)")"
  maker_website="$(record "$MAKER names no website" "$(maker_field "$r" website)")"
  representative="$(record "$SHEET has no 'Authorised representative' row" "$(sheet_row "$r" 'Authorised representative')")"
  logging="$(record "$LOCK resolves no $LOGGING" "$(locked_version "$r" "$LOGGING")")"
  exchange="$(record "$LOCK resolves no $EXCHANGE" "$(locked_version "$r" "$EXCHANGE")")"
  list="$(record "$SHEET has no standards table under (e)" "$(standards "$r")")"
  specs="$(common_specifications "$r")"

  file="ferrofed-v${version}-eu-declaration-of-conformity"
  [[ "$draft" == false ]] || file="$file-draft"
  tmp="$(mktemp)"
  {
    if [[ "$draft" == true ]]; then
      printf '> **DRAFT.** Not issued, and no claim of conformity. No conformity assessment of this\n'
      printf '> version against Chapter III of Regulation (EU) 2025/327 has been carried out, and the\n'
      printf '> Article 40 testing environment does not exist yet. Point 4 is the statement Annex IV has\n'
      printf '> an issued declaration make; this draft makes no such statement. Every item marked %s\n' "$MARK"
      printf '> waits on the manufacturer.\n\n'
    fi
    cat << EOF
# EU declaration of conformity

Regulation (EU) 2025/327 on the European Health Data Space, Article 39 and
Annex IV. This declaration is for the harmonised software components of the
EHR system named in point 1.

## 1. The EHR system

- **Name:** $name
- **Version:** $version, released on $date
- **Reference:** the release \`v$version\` of <$REPO_URL>, tag \`v$version\`
  at commit \`$commit\`; the gateway image \`ghcr.io/ferrohealth/ferrofed:$version\`
  and the gateway binary \`ferrofed\` of that release
- **Harmonised software components (Article 25(1)):** the European logging
  software component, crate \`ehds-logging\` $logging; the European
  interoperability software component, crate \`eehrxf\` $exchange, published
  from <https://github.com/FerroHEALTH/FerroBRIDGE>

## 2. The manufacturer

- **Name:** $maker_name
- **Postal address:** $maker_address
- **Contact:** $maker_email, $maker_website
- **Authorised representative:** $representative

## 3. Responsibility

This EU declaration of conformity is issued under the sole responsibility of
the manufacturer.

## 4. Conformity

The EHR system named in point 1 is in conformity with the provisions laid
down in Chapter III of Regulation (EU) 2025/327.

- **Other Union law providing for an EU declaration of conformity
  (Article 39(2)):** $(owner other_union_acts)
- **Result from the testing environment of Article 40:** $(owner testing_environment_result)

## 5. Harmonised standards

$(owner harmonised_standards)

## 6. Common specifications

$specs

## 7. Issue and signature

- **Place of issue:** $(owner place_of_issue)
- **Date of issue:** $date
- **Signed by:** $(owner signatory_name), $(owner signatory_function)
- **On behalf of:** $(owner on_behalf_of)
- **Signature:** $(owner signature)

## 8. Additional information

The technical documentation of this release is the index
<$REPO_URL/blob/v$version/website/book/src/evaluate/technical-documentation.md>
at the tag. The release carries the information sheet and the instructions
for use as \`ferrofed-v$version-information-sheet.md\` and
\`ferrofed-v$version-instructions-for-use.md\`.

The standards, formats and specifications the EHR system supports, with
their versions, as the table of point (e) of its information sheet lists them
(point (e) names further ones beside the table):

$list
EOF
  } > "$tmp"
  if [[ "$draft" == false ]] && grep -qF "$MARK" "$tmp"; then
    rm "$tmp"
    die "the declaration of v$version still holds a placeholder, so it is not written"
  fi
  mkdir -p "$out"
  mv "$tmp" "$out/$file.md"
  echo "wrote $out/$file.md"
}

self_test() {
  local work failed=0 n=0 sha=0123456789abcdef0123456789abcdef01234567 doc
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -r '$work'" EXIT
  doc="$work/out/ferrofed-v1.2.3-eu-declaration-of-conformity.md"
  # owner_file ISSUE VALUE: an OWNER that sets every field to VALUE.
  owner_file() {
    local key
    {
      echo '# the manufacturer'
      echo "issue = $1"
      for key in $FIELDS; do printf '%s = "%s"\n' "$key" "$2"; done
    } > "$work/repo/$OWNER"
  }
  # fixture VERSION: a stub tree whose records describe VERSION.
  fixture() {
    local repo="$work/repo"
    if [[ -d "$repo" ]]; then rm -r "$repo"; fi
    if [[ -d "$work/out" ]]; then rm -r "$work/out"; fi
    mkdir -p "$repo/conformance" "$repo/docs" "$repo/app/ferrofed-registry/src" \
      "$repo/website/book/src/evaluate"
    printf 'title: "FerroFED"\nversion: %s\n' "$1" > "$repo/$CITATION"
    printf '| Item | Pin |\n|---|---|\n| Product version | %s | x |\n' "$1" > "$repo/$PINS"
    printf '# Changelog\n\n## [%s] - 2026-01-02\n\n- a change\n' "$1" > "$repo/$CHANGELOG"
    printf 'pub const MANUFACTURER: Manufacturer = Manufacturer {\n    name: "Maker B.V.",\n    postal_address: "Street 1, Town",\n    email: "info@maker.example",\n    website: "https://maker.example/",\n};\n' \
      > "$repo/$MAKER"
    printf '## (a) The manufacturer\n\n| | |\n|---|---|\n| Authorised representative | none |\n\n## (e) Standards\n\n| Item | Pin | What |\n|---|---|---|\n| Product version | 1.2.3 | this |\n| Federation Tier with AQL | 1.0.0 | the federation |\n| openEHR AQL | 1.1.0 | the queries |\n\n## Support\n\n| Release | Date |\n|---|---|\n| 1.2.3 | 2026-01-02 |\n' \
      > "$repo/$SHEET"
    printf 'item\telement\ttarget\tsection\tstatus\tissue\n3\tcommon\tx.md\t-\tplanned\t#1\n' > "$repo/$INDEX"
    printf '[[package]]\nname = "eehrxf"\nversion = "0.5.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n\n[[package]]\nname = "ehds-logging"\nversion = "0.4.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n' > "$repo/$LOCK"
    owner_file true 'Town of signing'
  }
  # attempt WANT ARGS...: runs the script on the fixture and checks the exit.
  attempt() {
    local want="$1" status=0
    shift
    n=$((n + 1))
    DECLARATION_ROOT="$work/repo" "$BASH" "$0" "$@" > "$work/log" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "declaration-of-conformity: self-test case $n ($*) exited $status, wanted $want:" >&2
      cat "$work/log" >&2
      failed=1
    fi
  }
  # check WHAT COMMAND...: records a failure when COMMAND fails.
  check() {
    local what="$1"
    shift
    if ! "$@"; then
      echo "declaration-of-conformity: self-test: $what" >&2
      failed=1
    fi
  }

  fixture 1.2.3
  attempt 0 1.2.3 "$sha" "$work/out"
  check 'the declaration is written under its versioned name' test -f "$doc"
  for heading in '## 1. The EHR system' '## 2. The manufacturer' '## 3. Responsibility' \
    '## 4. Conformity' '## 5. Harmonised standards' '## 6. Common specifications' \
    '## 7. Issue and signature' '## 8. Additional information'; do
    check "the declaration has '$heading'" grep -qxF "$heading" "$doc"
  done
  check 'the version and the date are read' grep -qF -- '- **Version:** 1.2.3, released on 2026-01-02' "$doc"
  check 'the commit is named' grep -qF "$sha" "$doc"
  check 'the manufacturer is read from the code' grep -qF -- '- **Postal address:** Street 1, Town' "$doc"
  check 'the representative is read from the sheet' grep -qF -- '- **Authorised representative:** none' "$doc"
  # shellcheck disable=SC2016 # the backticks are Markdown the declaration holds, never expanded
  check 'the components carry their versions' grep -qF '`eehrxf` 0.5.0' "$doc"
  check 'the standards are read from the sheet' grep -qxF -- '- openEHR AQL: 1.1.0' "$doc"
  check 'a table after (e) is not a standard' sh -c "! grep -qF -- '- 1.2.3' '$doc'"
  check 'the product version is not a standard' sh -c "! grep -qF -- '- Product version' '$doc'"
  check 'the owner values are written' grep -qF -- '- **Place of issue:** Town of signing' "$doc"
  check 'no placeholder is left' sh -c "! grep -qF '$MARK' '$doc'"

  attempt 0 status
  check 'a complete issued file reads true' grep -qx true "$work/log"

  fixture 1.2.3-rc.1
  attempt 0 v1.2.3-rc.1 "$sha" "$work/out"
  check 'a pre-release tag is read without its v' \
    test -f "$work/out/ferrofed-v1.2.3-rc.1-eu-declaration-of-conformity.md"

  fixture 1.2.3
  owner_file false ''
  attempt 0 status
  check 'an unissued file reads false' grep -qx false "$work/log"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'an unissued declaration is refused' grep -qF 'does not issue the declaration' "$work/log"
  check 'a refusal writes nothing' test ! -e "$work/out"
  attempt 0 --draft 1.2.3 "$sha" "$work/out"
  # shellcheck disable=SC2016 # the backticks are Markdown the draft holds, never expanded
  check 'a draft is written under its own name' \
    grep -qF "$MARK"': `place_of_issue`' "$work/out/ferrofed-v1.2.3-eu-declaration-of-conformity-draft.md"
  check 'a draft is marked as one' grep -qF '**DRAFT.**' "$work/out/ferrofed-v1.2.3-eu-declaration-of-conformity-draft.md"
  check 'a draft claims no conformity' grep -qF 'no claim of conformity' "$work/out/ferrofed-v1.2.3-eu-declaration-of-conformity-draft.md"

  fixture 1.2.3
  sed 's/^signatory_function = .*/signatory_function = "  "/' "$work/repo/$OWNER" > "$work/o" && mv "$work/o" "$work/repo/$OWNER"
  attempt 1 status
  check 'status names a blank value' grep -qF 'leaves empty: signatory_function' "$work/log"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'an issued declaration with a blank is refused' grep -qF 'leaves empty: signatory_function' "$work/log"
  check 'a blank writes nothing' test ! -e "$work/out"

  fixture 1.2.3
  echo 'signed_by = "someone"' >> "$work/repo/$OWNER"
  attempt 1 status
  check 'an unknown key is refused' grep -qF "'signed_by' is not a key" "$work/log"

  fixture 1.2.3
  echo 'signature = "again"' >> "$work/repo/$OWNER"
  attempt 1 status
  check 'a repeated key is refused' grep -qF "'signature' is set twice" "$work/log"

  fixture 1.2.3
  echo 'place_of_issue = "a \"quoted\" town"' >> "$work/repo/$OWNER"
  attempt 1 status
  check 'a quote inside a value is refused' grep -qF 'is not key = "value"' "$work/log"

  fixture 1.2.3
  grep -v '^signature' "$work/repo/$OWNER" > "$work/o" && mv "$work/o" "$work/repo/$OWNER"
  attempt 1 status
  check 'a missing key is refused' grep -qF "has no 'signature'" "$work/log"

  fixture 1.2.3
  sed 's/^issue = .*/issue = yes/' "$work/repo/$OWNER" > "$work/o" && mv "$work/o" "$work/repo/$OWNER"
  attempt 1 status

  fixture 1.2.2
  printf 'title: "FerroFED"\nversion: 1.2.3\n' > "$work/repo/$CITATION"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'a pin matrix of another version is refused' grep -qF "pins product version '1.2.2'" "$work/log"

  fixture 1.2.30
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'a longer version is not this version' grep -qF "names version '1.2.30'" "$work/log"

  fixture 1.2.3
  printf '# Changelog\n\n## [Unreleased]\n' > "$work/repo/$CHANGELOG"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'an undated release is refused' grep -qF 'no date of issue' "$work/log"

  fixture 1.2.3
  printf 'item\telement\ttarget\tsection\tstatus\tissue\n3\tcommon\tx.md\t-\tdocumented\t-\n' > "$work/repo/$INDEX"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'a documented common specification is refused' grep -qF 'documents a common specification' "$work/log"

  fixture 1.2.3
  rm "$work/repo/$MAKER"
  attempt 1 1.2.3 "$sha" "$work/out"
  check 'a missing manufacturer is refused' grep -qF 'manufacturer.rs is missing' "$work/log"

  fixture 1.2.3
  attempt 2 1.2 "$sha" "$work/out"
  attempt 2 1.2.3 abc "$work/out"
  attempt 2 1.2.3 "$sha"
  attempt 2 status extra
  check 'a usage error writes nothing' test ! -e "$work/out"

  if [[ "$failed" -ne 0 ]]; then
    echo "declaration-of-conformity: self-test failed." >&2
    exit 1
  fi
  echo "declaration-of-conformity: self-test passed ($n cases)."
}

case "${1:-}" in
--self-test)
  [[ "$#" -eq 1 ]] || usage
  self_test
  ;;
status)
  [[ "$#" -eq 1 ]] || usage
  status
  ;;
--draft)
  [[ "$#" -eq 4 ]] || usage
  run true "$2" "$3" "$4"
  ;;
*)
  [[ "$#" -eq 3 ]] || usage
  run false "$1" "$2" "$3"
  ;;
esac
