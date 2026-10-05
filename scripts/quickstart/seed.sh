#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Seeds the compose quickstart's four FerroEHR nodes with synthetic patients,
# over each node's ITS-REST API and nothing else (no specification governs the
# seed: our own design).
#
# The patients are the [[dev.crossref]] rows of docker/quickstart/ferrofed.toml,
# the gateway's static cross-reference, so the seed and the gateway agree on
# every ehr_id by construction. For each row the script creates the EHR with
# PUT /v1/ehr/{ehr_id}, its EHR_STATUS.subject naming the patient, and commits
# one vendored demo composition to it with POST /v1/ehr/{ehr_id}/composition;
# each node first takes the vendored operational template through
# POST /v1/definition/template/adl1.4. Every patient identifier must lie inside
# the example arc urn:oid:2.999.
#
# Re-running is safe: a node that already holds the template or an EHR answers
# 409, which the script reports and passes, and a composition is committed only
# to an EHR this run created.
#
# Usage, after `docker compose up --wait`:
#   scripts/quickstart/seed.sh
# Environment: FERROFED_BIND_HOST and FERROEHR_A_PORT to FERROEHR_D_PORT as
# compose.yaml reads them; FERROEHR_USER and FERROEHR_PASSWORD, by default the
# quickstart's development user ferroehr / ferroehr.
# Exit 1 naming the step a node refused; 0 once every row is in place.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly CONFIG=docker/quickstart/ferrofed.toml
readonly DEMO=docs/specs/federation-ref/docker/demo-data
readonly TEMPLATE="$DEMO/International Patient Summary.opt"
readonly HOST="${FERROFED_BIND_HOST:-127.0.0.1}"
readonly USER_PASS="${FERROEHR_USER:-ferroehr}:${FERROEHR_PASSWORD:-ferroehr}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

die() {
  echo "seed: $*" >&2
  exit 1
}

# api_root MEMBER: the host URL of the member's ITS-REST API root.
api_root() {
  local member="$1" port
  case "$member" in
    node-a) port="${FERROEHR_A_PORT:-8081}" ;;
    node-b) port="${FERROEHR_B_PORT:-8082}" ;;
    node-c) port="${FERROEHR_C_PORT:-8083}" ;;
    node-d) port="${FERROEHR_D_PORT:-8084}" ;;
    *) die "member '$member' is not a quickstart node" ;;
  esac
  # The quickstart nodes publish plain HTTP on the bind host (loopback by
  # default) and serve no TLS, so this URL cannot be https.
  printf 'http://%s:%s/ferroehr/rest/openehr' "$HOST" "$port"
}

# call METHOD URL CONTENT_TYPE BODY_FILE: sends one request and prints the
# status; the answer body is kept in $work/answer for a refusal message.
call() {
  local method="$1" url="$2" content_type="$3" body_file="$4"
  curl -sS -u "$USER_PASS" -X "$method" "$url" \
    -H "Content-Type: $content_type" -H 'Accept: application/json' \
    -H 'Prefer: return=minimal' --data-binary "@$body_file" \
    -o "$work/answer" -w '%{http_code}'
}

# accept STEP STATUS: 0 when the node created the resource, 1 when it already
# held it (409); any other status ends the run.
accept() {
  local step="$1" status="$2"
  case "$status" in
    200 | 201 | 204) return 0 ;;
    409)
      echo "  $step: already there"
      return 1
      ;;
    *) die "$step was answered $status: $(head -c 400 "$work/answer")" ;;
  esac
}

# rows: one line per [[dev.crossref]] row, "member ehr_id namespace value".
rows() {
  awk '
    function flush() {
      if (open) print row["member"], row["ehr_id"], row["namespace"], row["value"]
      open = 0
      delete row
    }
    /^\[\[dev\.crossref\]\][[:space:]]*$/ { flush(); open = 1; next }
    /^\[/ { flush(); next }
    open && /^[a-z_]+[[:space:]]*=[[:space:]]*"[^"]*"[[:space:]]*$/ {
      key = $0; sub(/[[:space:]]*=.*/, "", key)
      value = $0; sub(/^[^"]*"/, "", value); sub(/".*/, "", value)
      row[key] = value
    }
    END { flush() }
  ' "$CONFIG"
}

[[ -f "$TEMPLATE" ]] || die "the vendored template $TEMPLATE is missing"
command -v curl >/dev/null || die "curl is required"

rows >"$work/rows"
count="$(wc -l <"$work/rows" | tr -d ' ')"
[[ "$count" -gt 0 ]] || die "$CONFIG holds no [[dev.crossref]] row"

seen=" "
while read -r member ehr_id namespace value <&3; do
  [[ "$ehr_id" =~ ^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$ ]] ||
    die "row for $member: '$ehr_id' is not a version 4 UUID"
  [[ "$namespace" =~ ^urn:oid:2\.999(\.[0-9]+)*$ ]] ||
    die "row for $member: the namespace lies outside the example arc urn:oid:2.999"
  [[ "$value" =~ ^[A-Za-z0-9-]+$ ]] ||
    die "row for $member: the identifier value is not a plain synthetic token"
  root="$(api_root "$member")"

  if [[ "$seen" != *" $member "* ]]; then
    echo "$member: template"
    status="$(call POST "$root/v1/definition/template/adl1.4" application/xml "$TEMPLATE")"
    accept "POST /v1/definition/template/adl1.4" "$status" && echo "  created"
    seen="$seen$member "
  fi

  echo "$member: EHR $ehr_id for $value"
  printf '%s' "{\"_type\":\"EHR_STATUS\",\"archetype_node_id\":\"openEHR-EHR-EHR_STATUS.generic.v1\",\
\"name\":{\"_type\":\"DV_TEXT\",\"value\":\"EHR Status\"},\
\"archetype_details\":{\"_type\":\"ARCHETYPED\",\"archetype_id\":{\"_type\":\"ARCHETYPE_ID\",\
\"value\":\"openEHR-EHR-EHR_STATUS.generic.v1\"},\"rm_version\":\"1.1.0\"},\
\"subject\":{\"_type\":\"PARTY_SELF\",\"external_ref\":{\"_type\":\"PARTY_REF\",\
\"id\":{\"_type\":\"GENERIC_ID\",\"value\":\"$value\",\"scheme\":\"ffd-test\"},\
\"namespace\":\"$namespace\",\"type\":\"PERSON\"}},\"is_queryable\":true,\"is_modifiable\":true}" \
    >"$work/ehr_status.json"
  status="$(call PUT "$root/v1/ehr/$ehr_id" application/json "$work/ehr_status.json")"
  accept "PUT /v1/ehr/$ehr_id" "$status" || continue
  echo "  created"

  # The first patient's demo composition for patient 0001, the second's for
  # every other, as a hospital records it at node A and C and a clinic at B
  # and D.
  case "$value" in *-0001) story=12345 ;; *) story=67890 ;; esac
  case "$member" in node-a | node-c) setting=hospital ;; *) setting=clinic ;; esac
  status="$(call POST "$root/v1/ehr/$ehr_id/composition" application/json \
    "$DEMO/composition-$story-$setting.json")"
  accept "POST /v1/ehr/$ehr_id/composition" "$status" && echo "  composition committed"
done 3<"$work/rows"

echo "seed: done, $count EHRs in place"
