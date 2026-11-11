#!/bin/sh
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Seeds the quickstart's four FerroEHR nodes with the synthetic patients of
# docker/quickstart/patients.tsv, over each node's ITS-REST API and nothing
# else (no specification governs the seed: our own design). compose.yaml runs
# it once, in the `ehr-seed` service, before the gateway starts.
#
# For each row it creates the EHR with PUT /v1/ehr/{ehr_id}, its
# EHR_STATUS.subject naming the patient, and commits one demonstration
# composition to it with POST /v1/ehr/{ehr_id}/composition; each node first
# takes the operational template through POST /v1/definition/template/adl1.4.
# The template and the compositions are the Federation Tier reference
# implementation's synthetic demo data, fetched at the commit docs/VERSIONS.md
# pins and refused unless each file has the SHA-256 recorded below, the bytes
# vendored under docs/specs/federation-ref/docker/demo-data/.
#
# Re-running is safe: a node that already holds the template or an EHR answers
# 409, which the seed reports and passes, and a composition is committed only
# to an EHR this run created.
#
# The seed signs in as the person `quickstart-seeder` through the nodes' own
# client `quickstart-seed`, with a token the quickstart's development issuer
# signs for it under the client credentials grant, and every node creates
# that person's PERSON at its first request (docker/quickstart/ferroehr.toml).
# The issuer names itself as it is asked, so the seed runs in the issuer's
# network namespace, as the nodes do, and asks it on localhost.
#
# Usage: ehr-seed.sh PATIENTS, with the nodes reached as http://localhost:8081
# to http://localhost:8084, or the URL each FERROEHR_<A..D>_URL names, and the
# token asked of the token endpoint QUICKSTART_TOKEN_URL names,
# http://localhost:8190/quickstart/token by default. Exit 1 naming the step a
# node refused or the file that did not verify; 0 once every row is in place.
set -eu

patients="$1"
token_url="${QUICKSTART_TOKEN_URL:-http://localhost:8190/quickstart/token}"
readonly DEMO=https://raw.githubusercontent.com/syntaric/openehr-federation-ref/9cb8606d1fce457b6b7f3a42f703d7069686a413/docker/demo-data
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

die() {
  echo "ehr-seed: $*" >&2
  exit 1
}

# fetch NAME FILE SHA256: downloads the demo file NAME (URL-encoded) to FILE
# and refuses it unless its SHA-256 is SHA256.
fetch() {
  curl -fsS --retry 5 --retry-all-errors -o "$2" "$DEMO/$1" || die "the demo file $1 could not be fetched"
  printf '%s  %s\n' "$3" "$2" | sha256sum -c - > /dev/null || die "the demo file $1 does not have the pinned SHA-256"
}

fetch 'International%20Patient%20Summary.opt' "$work/template.opt" fa7148549a2140a12d8aad32a0c8c2e5935660578b657a85cf8576ddea8cac3c
fetch composition-12345-clinic.json "$work/12345-clinic.json" 17275197e5a1bb1202db8496242fa82db6ff50f90ebc8a4eb4b0f25dc12bc3e9
fetch composition-12345-hospital.json "$work/12345-hospital.json" ba705e3ae796f6180d4d70f79b2166b4d00a2bf255d44d22a4d35287d1429c8e
fetch composition-67890-clinic.json "$work/67890-clinic.json" 5d2f47aad73271a4bb59cbdb644b03be78a3c3a160e6f644548cf364eb07be3c
fetch composition-67890-hospital.json "$work/67890-hospital.json" 508a09bd53c29e5a7538b8512eed169e20973ff9bc6be7281e994bd9eef0d0e0

# api_root MEMBER: the URL of the member's ITS-REST API root.
api_root() {
  case "$1" in
    node-a) printf '%s' "${FERROEHR_A_URL:-http://localhost:8081}" ;;
    node-b) printf '%s' "${FERROEHR_B_URL:-http://localhost:8082}" ;;
    node-c) printf '%s' "${FERROEHR_C_URL:-http://localhost:8083}" ;;
    node-d) printf '%s' "${FERROEHR_D_URL:-http://localhost:8084}" ;;
    *) die "member '$1' is not a quickstart node" ;;
  esac
  printf '/ferroehr/rest/openehr'
}

# The seeder's token, one for every node, kept in a header file so it never
# stands on a command line.
curl -fsS --retry 5 --retry-all-errors --retry-connrefused -o "$work/token.json" "$token_url" \
  -d grant_type=client_credentials -d client_id=quickstart-seed -d client_secret=any ||
  die "the development issuer at $token_url did not sign the seeder's token"
token="$(sed -n 's/.*"access_token" *: *"\([^"]*\)".*/\1/p' "$work/token.json")"
[ -n "$token" ] || die "the development issuer's answer carries no access token"
printf 'Authorization: Bearer %s\n' "$token" > "$work/authorization"

# call METHOD URL CONTENT_TYPE BODY_FILE: sends one request and prints the
# status; the answer body is kept in $work/answer for a refusal message.
call() {
  curl -sS -H "@$work/authorization" -X "$1" "$2" \
    -H "Content-Type: $3" -H 'Accept: application/json' \
    -H 'Prefer: return=minimal' --data-binary "@$4" \
    -o "$work/answer" -w '%{http_code}'
}

# accept STEP STATUS: 0 when the node created the resource, 1 when it already
# held it (409); any other status ends the run.
accept() {
  case "$2" in
    200 | 201 | 204) return 0 ;;
    409)
      echo "  $1: already there"
      return 1
      ;;
    *) die "$1 was answered $2: $(head -c 400 "$work/answer")" ;;
  esac
}

awk -F '\t' '!/^#/ && NF >= 4 { print $1, $2, $3, $4 }' "$patients" > "$work/rows"
count="$(wc -l < "$work/rows" | tr -d ' ')"
[ "$count" -gt 0 ] || die "$patients holds no patient"

seen=" "
while read -r member ehr_id namespace value; do
  printf '%s' "$ehr_id" | grep -qE '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' ||
    die "row for $member: '$ehr_id' is not a version 4 UUID"
  printf '%s' "$namespace" | grep -qE '^urn:oid:2\.999(\.[0-9]+)*$' ||
    die "row for $member: the namespace lies outside the example arc urn:oid:2.999"
  printf '%s' "$value" | grep -qE '^[A-Za-z0-9-]+$' ||
    die "row for $member: the identifier value is not a plain synthetic token"
  root="$(api_root "$member")"

  case "$seen" in
    *" $member "*) ;;
    *)
      echo "$member: template"
      status="$(call POST "$root/v1/definition/template/adl1.4" application/xml "$work/template.opt")"
      if accept "POST /v1/definition/template/adl1.4" "$status"; then echo "  created"; fi
      seen="$seen$member "
      ;;
  esac

  echo "$member: EHR $ehr_id for $value"
  printf '%s' "{\"_type\":\"EHR_STATUS\",\"archetype_node_id\":\"openEHR-EHR-EHR_STATUS.generic.v1\",\
\"name\":{\"_type\":\"DV_TEXT\",\"value\":\"EHR Status\"},\
\"archetype_details\":{\"_type\":\"ARCHETYPED\",\"archetype_id\":{\"_type\":\"ARCHETYPE_ID\",\
\"value\":\"openEHR-EHR-EHR_STATUS.generic.v1\"},\"rm_version\":\"1.1.0\"},\
\"subject\":{\"_type\":\"PARTY_SELF\",\"external_ref\":{\"_type\":\"PARTY_REF\",\
\"id\":{\"_type\":\"GENERIC_ID\",\"value\":\"$value\",\"scheme\":\"ffd-test\"},\
\"namespace\":\"$namespace\",\"type\":\"PERSON\"}},\"is_queryable\":true,\"is_modifiable\":true}" \
    > "$work/ehr_status.json"
  status="$(call PUT "$root/v1/ehr/$ehr_id" application/json "$work/ehr_status.json")"
  accept "PUT /v1/ehr/$ehr_id" "$status" || continue
  echo "  created"

  # The first patient's demo composition for patient 0001, the second's for
  # every other, as a hospital records it at node A and C and a clinic at B
  # and D.
  case "$value" in *-0001) story=12345 ;; *) story=67890 ;; esac
  case "$member" in node-a | node-c) setting=hospital ;; *) setting=clinic ;; esac
  status="$(call POST "$root/v1/ehr/$ehr_id/composition" application/json "$work/$story-$setting.json")"
  if accept "POST /v1/ehr/$ehr_id/composition" "$status"; then echo "  composition committed"; fi
done < "$work/rows"

echo "ehr-seed: done, $count EHRs in place"
