#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# One database per member node on one FerroEHR PostgreSQL server (no
# specification governs this: our own design).
#
# Mounted into /docker-entrypoint-initdb.d of the FerroEHR PostgreSQL image,
# whose entrypoint runs the init scripts in sorted order once, on an empty data
# directory. The image's own 10-ferroehr-init.sh has by then created the first
# node's role and database from PG_INIT_USER, PG_INIT_PASSWORD and PG_INIT_DB.
# This script runs that same script once more for every name in
# FERROFED_NODE_DATABASES, with the three PG_INIT_ variables set to the name,
# so each further node gets a login role of that name owning a database of that
# name. The image's script creates the cluster-wide group roles only where they
# are missing, so the nodes share them.
#
# Development credentials only: each role's password is its name.
#
# Usage (as the image's entrypoint runs it):
#   FERROFED_NODE_DATABASES="ferroehr_b ferroehr_c" 20-ferrofed-node-databases.sh
set -Eeuo pipefail

readonly IMAGE_INIT=/docker-entrypoint-initdb.d/10-ferroehr-init.sh

if [ ! -x "$IMAGE_INIT" ]; then
  echo "ferrofed init: $IMAGE_INIT is missing or not executable" >&2
  exit 1
fi

read -r -a names <<<"${FERROFED_NODE_DATABASES:-}"
if [ "${#names[@]}" -eq 0 ]; then
  echo "ferrofed init: FERROFED_NODE_DATABASES names no database, nothing to add"
  exit 0
fi

for name in "${names[@]}"; do
  # The name is spliced into SQL identifiers and a literal by the image's
  # script, so only a plain lower-case identifier is admitted.
  if ! [[ "$name" =~ ^[a-z_][a-z0-9_]{0,62}$ ]]; then
    echo "ferrofed init: '$name' is not a lower-case PostgreSQL identifier" >&2
    exit 1
  fi
  echo "ferrofed init: node database '$name'"
  PG_INIT_USER="$name" PG_INIT_PASSWORD="$name" PG_INIT_DB="$name" "$IMAGE_INIT"
done
