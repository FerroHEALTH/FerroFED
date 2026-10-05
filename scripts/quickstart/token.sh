#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Mints an access token the compose quickstart's gateway accepts, and prints
# it (no specification governs the quickstart issuer: our own design).
#
# The quickstart gateway trusts one development issuer, whose key set it reads
# from docker/quickstart/issuer/jwks.json (docker/quickstart/ferrofed.toml,
# [auth]). The first run generates that issuer's RSA key pair with openssl and
# writes the key set; the private key stays in docker/quickstart/issuer/,
# which git ignores, so no key is ever committed. A development key, NOT for
# anything real.
#
# The token is an RFC 9068 access token signed RS256: issuer, audience and
# subject as the quickstart configuration names them, every SMART on openEHR
# scope in the user/ compartment, the purpose of use TREAT in the IHE IUA
# extension, valid for one hour.
#
# Usage, after `docker compose up --wait`:
#   curl -H "Authorization: Bearer $(scripts/quickstart/token.sh)" ...
# Needs openssl and od. Exit 1 when a key cannot be made or a token signed.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly DIR=docker/quickstart/issuer
readonly KEY="$DIR/key.pem"
readonly JWKS="$DIR/jwks.json"
readonly ISSUER=ferrofed-quickstart-issuer
readonly AUDIENCE=ferrofed-quickstart
# The HL7 code system's canonical URI, an identifier nothing fetches: http is its spelling.
readonly ACT_REASON=http://terminology.hl7.org/CodeSystem/v3-ActReason

die() {
  echo "token: $*" >&2
  exit 1
}

command -v openssl >/dev/null || die "openssl is not installed"

# b64url: standard input as unpadded base64url (RFC 7515 section 2).
b64url() {
  openssl base64 -A | tr '+/' '-_' | tr -d '='
}

# modulus: the hexadecimal modulus of the private key.
modulus() {
  openssl rsa -in "$KEY" -noout -modulus 2>/dev/null | sed 's/^Modulus=//'
}

# unhex: the hexadecimal text on standard input as bytes.
unhex() {
  local hex
  hex="$(tr -d '\n')"
  local i
  for ((i = 0; i < ${#hex}; i += 2)); do
    printf '%b' "\\x${hex:i:2}"
  done
}

if [[ ! -s "$KEY" ]] || [[ ! -s "$JWKS" ]]; then
  umask 077
  openssl genrsa -out "$KEY" 2048 2>/dev/null || die "the key pair could not be generated"
  umask 022
  n="$(modulus | unhex | b64url)"
  kid="$(modulus | openssl dgst -sha256 | sed 's/^.*= *//' | cut -c1-16)"
  printf '{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"%s","n":"%s","e":"AQAB"}]}\n' \
    "$kid" "$n" >"$JWKS"
  chmod 644 "$JWKS"
  echo "token: generated the development issuer's key set in $JWKS" >&2
fi

kid="$(modulus | openssl dgst -sha256 | sed 's/^.*= *//' | cut -c1-16)"
now="$(date +%s)"
jti="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
header="$(printf '{"alg":"RS256","typ":"at+jwt","kid":"%s"}' "$kid" | b64url)"
payload="$(printf '{"iss":"%s","sub":"quickstart-user","aud":"%s","client_id":"quickstart-client","iat":%s,"exp":%s,"jti":"%s","scope":"user/aql-*.s user/composition-*.cruds user/template-*.cruds","extensions":{"ihe_iua":{"subject_organization_id":"urn:oid:2.999.7","purpose_of_use":[{"system":"%s","code":"TREAT"}]}}}' \
  "$ISSUER" "$AUDIENCE" "$now" "$((now + 3600))" "$jti" "$ACT_REASON" | b64url)"
signature="$(printf '%s.%s' "$header" "$payload" |
  openssl dgst -sha256 -sign "$KEY" -binary | b64url)" || die "the token could not be signed"
printf '%s.%s.%s\n' "$header" "$payload" "$signature"
