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
# Usage: seed.sh PATIENTS BASE TOKEN_ENDPOINT, with the Source's client in
# FERROPIX_CLIENT_ID and FERROPIX_CLIENT_SECRET (development values). The seed
# feeds as a machine, under a client credentials token of the development
# issuer (RFC 6749 §4.4), since FerroPIX admits a Patient Identity Source only
# so. Exit 1 naming the line FerroPIX refused, or the token not obtained; 0
# once every line is fed.
set -eu

patients="$1"
base="$2"
token_endpoint="$3"
fed=0

answer="$(curl -sS -u "$FERROPIX_CLIENT_ID:$FERROPIX_CLIENT_SECRET" \
  -d grant_type=client_credentials -d scope=system/Patient.cud "$token_endpoint")"
token="$(printf '%s' "$answer" | sed -n 's/.*"access_token" *: *"\([^"]*\)".*/\1/p')"
[ -n "$token" ] || {
  echo "seed: the issuer at $token_endpoint gave the Source no token" >&2
  exit 1
}

while IFS="$(printf '\t')" read -r member ehr_id namespace value domain; do
  case "$member" in
    '' | '#'*) continue ;;
    *) ;;
  esac
  body="{\"resourceType\":\"Patient\",\"identifier\":[\
{\"system\":\"$domain\",\"value\":\"$ehr_id\"},\
{\"system\":\"$namespace\",\"value\":\"$value\"}],\
\"name\":[{\"family\":\"Synthetic\",\"given\":[\"$value\"]}]}"
  status="$(curl -sS -o /dev/null -w '%{http_code}' -X PUT \
    -H "Authorization: Bearer $token" \
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
