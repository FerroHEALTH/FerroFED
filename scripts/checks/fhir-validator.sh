#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The exchange-format validator lane (#688; no specification governs the
# lane: our own design). The tests that write a patient summary document and
# hold it to the vendored HL7 Europe profiles also write it to the directory
# FERROFED_EMIT_DIR names; this script runs HL7's FHIR Validator
# (validator_cli.jar of hapifhir/org.hl7.fhir.core) over every one of them
# and fails on an error. It is evidence for the technical documentation and
# is never shipped.
#
# The validator runs offline: no terminology server (-tx n/a), no HTTP access
# at all (-no-http-access), and every package it reads comes from a package
# cache this script fills before the run:
#
#   - the vendored packages under docs/specs/: the two that hold every profile
#     the documents claim, hl7.fhir.eu.eps 1.0.0-ballot and hl7.fhir.eu.base
#     2.0.1, and the MyHealth@EU value sets those profiles bind are loaded
#     with -ig, and the vendored packages their dependencies name are put in
#     the cache beside them;
#   - the packages their dependencies name that the repository does not
#     vendor (the FHIR R4 core, HL7 Terminology, the HL7 extension packages,
#     IPS 1.1.0, IPA, SMART App Launch and DICOM), each fetched from the
#     FHIR package registry at the exact version a dependency names and
#     checked against the sha256 in REGISTRY_PACKAGES below. They are the
#     validator's inputs, pinned like the jar, and are never committed.
#
# A run fails on an issue of severity error or fatal, on a document the
# output names no outcome for, and on a claimed profile the validator could
# not find (VALIDATION_VAL_PROFILE_UNKNOWN and its kin, whatever severity it
# reports them at), so a claim is never left unchecked. Warnings and
# information never fail a run: among them are the terminology messages of an
# offline run, a code or value set no loaded package defines, which the
# validator cannot check without a terminology server. Every issue is listed
# in summary.tsv and counted in summary.md, so none is hidden. Example URLs
# are admitted (-allow-example-urls true) because the test documents name
# their entries under gateway.example.org, a name RFC 6761 section 6.5
# reserves for examples.
#
# Usage:
#   FHIR_VALIDATOR_VERSION=x.y.z FHIR_VALIDATOR_SHA256=<hex> \
#     scripts/checks/fhir-validator.sh DOCUMENTS OUT
#   scripts/checks/fhir-validator.sh --judge OUTPUT DOCUMENTS OUT
#   scripts/checks/fhir-validator.sh --self-test
#
# DOCUMENTS is the directory the tests wrote (FERROFED_EMIT_DIR), OUT the
# directory the results go to: validation.json (the validator's
# OperationOutcome output), validation.html, validator.log, summary.tsv and
# summary.md. FHIR_VALIDATOR_JAR names a local copy of the jar in place of a
# download; it is held to the same sha256. --judge reads an existing
# validator output alone. Needs java 17 or later, curl, jq, tar and
# sha256sum or shasum. Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly REGISTRY=https://packages.fhir.org
readonly RELEASES=https://github.com/hapifhir/org.hl7.fhir.core/releases/download

# The vendored packages loaded with -ig: the ones holding the profiles the
# documents claim (bundle-eu-eps, composition-eu-eps, patient-eu-eps and the
# EU core profile a mapped section writes to), and the MyHealth@EU Master
# Value Sets Catalogue, whose value sets those profiles bind (such as
# eHDSIAllergenNoDrug), so the bindings are checked offline.
readonly IGS="\
docs/specs/eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz
docs/specs/eu-hl7-base/hl7.fhir.eu.base-2.0.1.tgz
docs/specs/ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz"

# The vendored packages the dependencies of IGS name, put in the cache.
readonly VENDORED="\
docs/specs/hl7-ips/hl7.fhir.uv.ips-2.0.0.tgz
docs/specs/eu-hl7-extensions/hl7.fhir.eu.extensions.r4-1.3.0.tgz
docs/specs/eu-hl7-extensions/hl7.fhir.eu.extensions.r4-1.3.1.tgz
docs/specs/ihe-pharm-mpd/ihe.pharm.mpd.r4-1.0.0-comment-2.tgz"

# The rest of the dependency closure of IGS, one "id<TAB>version<TAB>sha256"
# record per line, each the registry tarball at the version a dependency
# names. Below them are the packages the validator loads by itself:
# hl7.fhir.xver-extensions, and hl7.terminology and hl7.fhir.uv.extensions,
# which it asks for at their newest version and, with no HTTP access, takes
# at the newest the cache holds; they are pinned at the versions the vendored
# manifests name for the R4 forms, with their own dependencies. A newer
# release of one of these never applies; a re-pin of a vendored package or
# of the validator re-derives the list.
readonly REGISTRY_PACKAGES="\
hl7.fhir.r4.core	4.0.1	ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44
hl7.fhir.eu.base	2.0.0	b2858677ae11245bbc768ced2e205c5354ebf71ae8d7a7a0f2915c3eb6124999
hl7.fhir.uv.extensions.r4	5.2.0	b406e75575f05676559d0759770c5939d023ee72fb2ef38e0b3259328487720a
hl7.fhir.uv.extensions.r4	5.3.0	dfbc3ac95df91ed845cc6b60920d2875f679361fa0e16f227cee93c4a9ab2104
hl7.fhir.uv.xver-r5.r4	0.1.0	7ee6f04d78ced803dd567559a0d178bafbd2d3b71db61bbbf6b15c796a1a664d
hl7.terminology.r4	6.2.0	79404c9cc95491fc0155627cd039c401a6eb4748175328131e91b709a41300e2
hl7.terminology.r4	6.3.0	5e080e5343406f964022b086da713df374e410245c518166c5b1b93a800cdd71
hl7.terminology.r4	6.5.0	a28b638483a11df696ed92198276236d759e41c7a3a8960c9e7e7d0a1185bd06
hl7.terminology.r4	7.1.0	1cb0cd5601972925fcd04f2c175d9cc63a9ecd9346a91a6e735f1a865dc5fba1
hl7.terminology.r4	7.3.0	1a1ef2aa22ecc820341267f2bdba3b2d1f4adafb9bdddfc9e51c611cd64f3b54
hl7.fhir.uv.ipa	1.1.0	013b9f91683c496fc09eed93a8798b47dcbfb775587890fa3e11253a2409feef
hl7.fhir.uv.smart-app-launch	2.0.0	627e7a64e7abe130201fbae2ee7d2cb85d7d94972458d302c4d06d59e2c7d084
hl7.fhir.uv.ips	1.1.0	403c4141101810e924f2928287985084819d8a5cc3a62e2b3840a557129840ef
hl7.terminology.r4	5.0.0	c863bef14eddb01d2884d8f14826b9b152f9fcef6b99e107025136af69ce2c18
fhir.dicom	2022.4.20221006	117a6137c7f08220a92aadaec23f10e603730fc13b583d46011a777a1f23e7ed
hl7.fhir.xver-extensions	0.1.0	f3bb9fa2083402e88a02b41f433655274e8a1cca563211c8f7ba6fd0badf537a
hl7.terminology	7.3.0	8318f87cdcb44a6dfc5c197ca569c9b0499b06f45522324bfc4b4b810d51d4b9
hl7.fhir.uv.extensions	5.3.0	6e3a3f9929e05d2b813a3f98fa1ad2f5122fb6b2819fa73e6c5520002fb2c5b0
hl7.fhir.r5.core	5.0.0	74b27cd1bfce9e80eaceac431edf230b0945a443564fbf5512f82e5fa50a80d4
hl7.fhir.uv.extensions.r5	5.2.0	e02fc6eff0f37c1611a35aa93ef9aaa3b55ab7371a5c96f4d2a5834106a13170
hl7.fhir.uv.extensions.r5	5.3.0	f1039cac888d79ebd29878d7debe5e647ffe7ed962a9033da095258a03a06105
hl7.terminology.r5	7.1.0	473303108b5607aad7b910581739e8bbf3e61a625ed9e739c66e0ca597d4aefe"

fail=0
bad() {
  local message="$1"
  echo "::error::$message" >&2
  fail=1
}

# sha256 FILE: the hex SHA-256 of FILE.
sha256() {
  local file="$1"
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$file" | awk '{ print $1 }'
  else
    shasum -a 256 "$file" | awk '{ print $1 }'
  fi
}

# fetch URL FILE SHA256: downloads URL to FILE unless it is there, and holds
# it to SHA256.
fetch() {
  local url="$1" file="$2" want="$3" got
  if [[ ! -f "$file" ]]; then
    curl -sSfL --retry 3 -o "$file.part" "$url"
    mv "$file.part" "$file"
  fi
  got="$(sha256 "$file")"
  if [[ "$got" != "$want" ]]; then
    echo "::error::$url has sha256 $got, pinned $want" >&2
    rm -f "$file"
    return 1
  fi
}

# cache_package CACHE TARBALL: unpacks TARBALL into the package cache CACHE
# under the id#version its own manifest names, the layout the FHIR package
# cache uses.
cache_package() {
  local cache="$1" tarball="$2" name
  name="$(tar -xzOf "$tarball" package/package.json | jq -r '"\(.name)#\(.version)"')"
  case "$name" in
  *'#'*) ;;
  *)
    echo "::error::$tarball has no package/package.json naming its id and version" >&2
    return 1
    ;;
  esac
  rm -rf "${cache:?}/$name"
  mkdir -p "$cache/$name"
  tar -xzf "$tarball" -C "$cache/$name"
  [[ -f "$cache/$name/package/package.json" ]] || {
    echo "::error::$tarball did not unpack to package/" >&2
    return 1
  }
  echo "cached $name"
}

# issues OUTPUT: one "file<TAB>severity<TAB>message-id<TAB>where<TAB>text"
# line per issue of the validator output OUTPUT, a single OperationOutcome or
# a Bundle of them, each naming its file in the operationoutcome-file
# extension. The file is its base name.
issues() {
  local output="$1"
  jq -r '
    def ext($name): (.extension // []) | map(select((.url // "") | endswith("/" + $name))) | .[0];
    (if .resourceType == "Bundle" then [.entry[]?.resource] else [.] end)[]
    | (ext("operationoutcome-file") | .valueString // "") as $file
    | ($file | split("/") | last) as $base
    | .issue[]?
    | [ $base,
        (.severity // ""),
        (ext("operationoutcome-message-id") | (.valueCode // .valueString // "")),
        (((.expression // .location // []) | .[0]) // ""),
        ((.details.text // .diagnostics // "") | gsub("[\t\r\n]"; " "))
      ]
    | @tsv' "$output"
}

# outcome_files OUTPUT: the base name of every file the validator output
# OUTPUT holds an outcome for, one per line.
outcome_files() {
  local output="$1"
  jq -r '
    (if .resourceType == "Bundle" then [.entry[]?.resource] else [.] end)[]
    | (.extension // []) | map(select((.url // "") | endswith("/operationoutcome-file"))) | .[0].valueString // empty
    | split("/") | last' "$output"
}

# judge OUTPUT DOCUMENTS OUT: holds the validator output OUTPUT for the
# documents in DOCUMENTS to the rules in the header, writes summary.tsv and
# summary.md to OUT, and returns 1 when a run fails.
judge() {
  local output="$1" documents="$2" out="$3" status=0 doc base n=0 outcomes failing
  mkdir -p "$out"
  if ! jq -e '.resourceType == "Bundle" or .resourceType == "OperationOutcome"' "$output" > /dev/null 2>&1; then
    echo "::error::$output is no OperationOutcome and no Bundle of them" >&2
    return 1
  fi
  {
    printf 'file\tseverity\tmessage-id\twhere\ttext\n'
    issues "$output"
  } > "$out/summary.tsv"
  outcomes="$(outcome_files "$output" | sort -u)"
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] || continue
    n=$((n + 1))
    base="${doc##*/}"
    # A single OperationOutcome may carry no file extension: it is the one
    # document's outcome.
    if [[ -z "$outcomes" ]] && jq -e '.resourceType == "OperationOutcome"' "$output" > /dev/null; then
      continue
    fi
    if ! grep -qxF "$base" <<< "$outcomes"; then
      echo "::error::the validator output names no outcome for $base" >&2
      status=1
    fi
  done
  if [[ "$n" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with FERROFED_EMIT_DIR set?" >&2
    status=1
  fi
  failing="$(awk -F '\t' '
    NR > 1 {
      unknown = ($3 ~ /^(VALIDATION_VAL_PROFILE_UNKNOWN|Validation_VAL_Profile_Unknown|Validation_VAL_Unknown_Profile|VALIDATION_VAL_GLOBAL_PROFILE_UNKNOWN)/) \
        || ($5 ~ /has not been checked because it could not be found/)
      if ($2 == "error" || $2 == "fatal" || unknown) print
    }' "$out/summary.tsv")"
  if [[ -n "$failing" ]]; then
    while IFS=$'\t' read -r file severity id where text; do
      echo "::error::$file: $severity ${id:+[$id] }at ${where:-?}: $text" >&2
    done <<< "$failing"
    status=1
  fi
  {
    echo "## HL7 FHIR Validator over the exchange-format documents"
    echo
    echo "$n document(s); a run fails on an error, a fatal issue or an unchecked profile claim."
    echo
    echo "| Document | fatal | error | warning | information |"
    echo "|---|---|---|---|---|"
    awk -F '\t' '
      NR > 1 { c[$1 SUBSEP $2]++; f[$1] = 1 }
      END {
        for (file in f) {
          printf "| %s | %d | %d | %d | %d |\n", file, c[file SUBSEP "fatal"], c[file SUBSEP "error"], c[file SUBSEP "warning"], c[file SUBSEP "information"]
        }
      }' "$out/summary.tsv" | sort
    echo
    echo "### Warnings by message id"
    echo
    awk -F '\t' 'NR > 1 && $2 == "warning" { c[($3 == "" ? "(none)" : $3)]++ } END { for (id in c) printf "- %s: %d\n", id, c[id] }' "$out/summary.tsv" | sort
    if [[ -n "$failing" ]]; then
      echo
      echo "### Failing issues"
      echo
      while IFS=$'\t' read -r file severity id where text; do
        echo "- \`$file\` $severity ${id:+\`$id\` }at \`${where:-?}\`: $text"
      done <<< "$failing"
    fi
  } > "$out/summary.md"
  cat "$out/summary.md"
  return "$status"
}

# run DOCUMENTS OUT: fills the package cache, fetches the jar, runs the
# validator offline over every document in DOCUMENTS and judges its output.
run() {
  local documents="$1" out="$2"
  : "${FHIR_VALIDATOR_VERSION:?the validator version to run}"
  : "${FHIR_VALIDATOR_SHA256:?the sha256 of validator_cli.jar at that version}"
  command -v java > /dev/null 2>&1 || {
    echo "::error::java is not on PATH; the validator needs Java 17 or later" >&2
    return 1
  }
  local work="${RUNNER_TEMP:-${TMPDIR:-/tmp}}/fhir-validator"
  local home="$work/home" downloads="$work/downloads"
  local cache="$home/.fhir/packages"
  mkdir -p "$downloads" "$out"
  rm -rf "$home"
  mkdir -p "$home"

  local jar="${FHIR_VALIDATOR_JAR:-$downloads/validator_cli-$FHIR_VALIDATOR_VERSION.jar}"
  fetch "$RELEASES/$FHIR_VALIDATOR_VERSION/validator_cli.jar" "$jar" "$FHIR_VALIDATOR_SHA256"

  # The validator writes the packages.ini of its own cache version into an
  # empty cache and clears a cache whose packages.ini it does not accept, so
  # it creates the cache first; each package then goes in an id#version
  # folder holding the unpacked tarball.
  java -Duser.home="$home" -jar "$jar" -help > "$work/init.log" 2>&1 || true
  if [[ ! -f "$cache/packages.ini" ]]; then
    cat "$work/init.log" >&2
    echo "::error::the validator created no package cache at $cache" >&2
    return 1
  fi

  local id version digest tarball
  while IFS=$'\t' read -r id version digest; do
    [[ -n "$id" ]] || continue
    tarball="$downloads/$id-$version.tgz"
    fetch "$REGISTRY/$id/$version" "$tarball" "$digest"
    cache_package "$cache" "$tarball"
  done <<< "$REGISTRY_PACKAGES"
  while IFS= read -r tarball; do
    [[ -n "$tarball" ]] || continue
    cache_package "$cache" "$tarball"
  done <<< "$IGS
$VENDORED"

  local docs=() doc
  for doc in "$documents"/*.json; do
    [[ -f "$doc" ]] && docs+=("$doc")
  done
  if [[ "${#docs[@]}" -eq 0 ]]; then
    echo "::error::$documents holds no document: did the tests run with FERROFED_EMIT_DIR set?" >&2
    return 1
  fi
  local igs=()
  while IFS= read -r tarball; do
    [[ -n "$tarball" ]] && igs+=(-ig "$tarball")
  done <<< "$IGS"

  local code=0
  java -Xmx6g -Duser.home="$home" -jar "$jar" \
    ${docs[@]+"${docs[@]}"} \
    -version 4.0.1 \
    -tx n/a \
    -no-http-access \
    -allow-example-urls true \
    ${igs[@]+"${igs[@]}"} \
    -output "$out/validation.json" \
    -html-output "$out/validation.html" \
    > "$out/validator.log" 2>&1 || code=$?
  tail -n 40 "$out/validator.log"
  if [[ ! -f "$out/validation.json" ]]; then
    echo "::error::the validator wrote no output (exit $code); see validator.log" >&2
    return 1
  fi
  cp ${docs[@]+"${docs[@]}"} "$out/"
  judge "$out/validation.json" "$documents" "$out"
}

# self_test: holds judge to its rules over synthetic validator outputs.
self_test() {
  local scratch
  scratch="$(mktemp -d)"
  # shellcheck disable=SC2064 # the path is fixed now, on purpose
  trap "rm -rf '$scratch'" EXIT
  mkdir -p "$scratch/docs"
  echo '{}' > "$scratch/docs/a.json"
  echo '{}' > "$scratch/docs/b.json"
  local file='http://hl7.org/fhir/StructureDefinition/operationoutcome-file'
  local mid='http://hl7.org/fhir/StructureDefinition/operationoutcome-message-id'
  outcome() {
    local name="$1" severity="$2" id="$3"
    printf '{"resourceType":"OperationOutcome","extension":[{"url":"%s","valueString":"/x/%s"}],"issue":[{"extension":[{"url":"%s","valueString":"%s"}],"severity":"%s","code":"processing","details":{"text":"t"},"expression":["Bundle"]}]}' \
      "$file" "$name" "$mid" "$id" "$severity"
  }
  bundle() {
    printf '{"resourceType":"Bundle","type":"collection","entry":[{"resource":%s},{"resource":%s}]}' "$1" "$2"
  }
  local cases=0 wrong=0
  check() {
    local label="$1" want="$2" json="$3" got=0
    printf '%s' "$json" > "$scratch/out.json"
    judge "$scratch/out.json" "$scratch/docs" "$scratch/result" > /dev/null 2>&1 || got=1
    cases=$((cases + 1))
    if [[ "$got" != "$want" ]]; then
      echo "::error::self-test: $label: judged $got, expected $want" >&2
      wrong=1
    fi
  }
  check "information alone passes" 0 \
    "$(bundle "$(outcome a.json information All_OK)" "$(outcome b.json information All_OK)")"
  check "a warning passes" 0 \
    "$(bundle "$(outcome a.json warning UNKNOWN_CODESYSTEM)" "$(outcome b.json information All_OK)")"
  check "an error fails" 1 \
    "$(bundle "$(outcome a.json error Validation_VAL_Profile_Minimum)" "$(outcome b.json information All_OK)")"
  check "a fatal issue fails" 1 \
    "$(bundle "$(outcome a.json fatal Error_parsing_JSON)" "$(outcome b.json information All_OK)")"
  check "an unknown profile fails at any severity" 1 \
    "$(bundle "$(outcome a.json warning VALIDATION_VAL_PROFILE_UNKNOWN_NOT_POLICY)" "$(outcome b.json information All_OK)")"
  check "a document with no outcome fails" 1 \
    "$(bundle "$(outcome a.json information All_OK)" "$(outcome c.json information All_OK)")"
  check "no validator output fails" 1 '{"resourceType":"Parameters"}'
  rm -f "$scratch/docs/"*.json
  check "no document fails" 1 '{"resourceType":"OperationOutcome","issue":[]}'
  if [[ "$wrong" -ne 0 ]]; then
    return 1
  fi
  echo "self-test: $cases cases judged as expected"
}

case "${1:-}" in
--self-test)
  self_test
  ;;
--judge)
  [[ "$#" -eq 4 ]] || {
    echo "usage: $0 --judge OUTPUT DOCUMENTS OUT" >&2
    exit 2
  }
  judge "$2" "$3" "$4"
  ;;
-* | '')
  echo "usage: $0 DOCUMENTS OUT | --judge OUTPUT DOCUMENTS OUT | --self-test" >&2
  exit 2
  ;;
*)
  [[ "$#" -eq 2 ]] || {
    echo "usage: $0 DOCUMENTS OUT" >&2
    exit 2
  }
  run "$1" "$2"
  ;;
esac
[[ "$fail" -eq 0 ]] || exit 1
