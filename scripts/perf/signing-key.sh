#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Writes the ES384 signing key of the gateways the performance gate runs on the
# host, once (no specification governs the gate's key: our own design).
#
# The gateway signs every request to a node with this key: the caller's
# identity travels in an openEHR-federation-client token, and the gateway
# publishes the public half at /.well-known/jwks.json (Federation Tier §13.1,
# N24, N25). perf/ferrofed.toml names the key under
# docker/quickstart/signing/, which git ignores, so no key is ever committed;
# the path stays where a base tree's perf/ferrofed.toml reads it too. A
# development key, NOT for anything real. A key that exists is kept.
#
# Usage, from anywhere, before scripts/perf/gate.sh starts the gateways:
#   scripts/perf/signing-key.sh
# Needs openssl. Exit 1 when the key cannot be made.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly KEY=docker/quickstart/signing/signing-key.pem

die() {
  echo "signing-key: $*" >&2
  exit 1
}

if [[ -s "$KEY" ]]; then
  echo "signing-key: $KEY exists, kept" >&2
  exit 0
fi
command -v openssl >/dev/null || die "openssl is not installed"
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out "$KEY" 2>/dev/null ||
  die "the key could not be generated"
chmod 644 "$KEY"
echo "signing-key: generated the development signing key in $KEY" >&2
