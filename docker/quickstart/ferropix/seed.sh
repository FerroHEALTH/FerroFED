#!/bin/sh
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Feeds the quickstart's FerroPIX the synthetic patients of
# docker/quickstart/patients.tsv over ITI-104, the PIXm Patient Identity Feed
# (IHE PIXm 3.1.0 §2:3.104), as each member's Patient Identity Source would:
# one conditional update per EHR, PUT [base]/Patient?identifier=<the member's
# ehr_id domain>|<ehr_id>, of a Patient carrying that ehr_id and the
# patient's identifier, whose authoritative domain links the members'
# records into one patient. compose.yaml runs it once, in the `ferropix-seed`
# service, before the gateway starts (no specification governs the seed: our
# own design).
#
# Re-running is safe: a record FerroPIX holds already is answered 200 and
# replaced with the same content.
#
# Usage: seed.sh PATIENTS BASE, with the Basic credentials in FERROPIX_USER
# and FERROPIX_PASSWORD (development values). Exit 1 naming the line FerroPIX
# refused; 0 once every line is fed.
set -eu

patients="$1"
base="$2"
fed=0

while IFS="$(printf '\t')" read -r member ehr_id namespace value domain; do
  case "$member" in
    '' | '#'*) continue ;;
  esac
  body="{\"resourceType\":\"Patient\",\"identifier\":[\
{\"system\":\"$domain\",\"value\":\"$ehr_id\"},\
{\"system\":\"$namespace\",\"value\":\"$value\"}],\
\"name\":[{\"family\":\"Synthetic\",\"given\":[\"$value\"]}]}"
  status="$(curl -sS -o /dev/null -w '%{http_code}' -X PUT \
    -u "$FERROPIX_USER:$FERROPIX_PASSWORD" \
    -H 'Content-Type: application/fhir+json' -H 'Accept: application/fhir+json' \
    --data-binary "$body" \
    "$base/Patient?identifier=$domain%7C$ehr_id")"
  case "$status" in
    200 | 201) echo "$member: $value fed ($status)" ;;
    *)
      echo "seed: FerroPIX answered $status for $member's record of $value" >&2
      exit 1
      ;;
  esac
  fed=$((fed + 1))
done <"$patients"

[ "$fed" -gt 0 ] || {
  echo "seed: $patients holds no patient" >&2
  exit 1
}
echo "seed: $fed records fed"
