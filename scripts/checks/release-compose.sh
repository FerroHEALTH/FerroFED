#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# The release compose guard: deploy/compose/compose.yaml, the compose.yaml
# every release carries, renders with the example values of
# deploy/compose/example/, refuses to render without each required variable,
# and the gateway configuration it embeds is one `ferrofed config check`
# accepts (no specification governs this: our own design).
#
# Docker Compose does the rendering, so the variables are filled in exactly as
# they are for an operator. The rendered configuration names container paths
# (/etc/ferrofed/registry and /run/secrets/ferrofed/); the guard points them at
# a temporary copy of the example registry and at synthetic secret files, then
# runs the binary it is given over the result.
#
# Usage:
#   scripts/checks/release-compose.sh                   render and refuse only
#   scripts/checks/release-compose.sh <ferrofed binary> also run config check
# Needs `docker compose` and `jq`. Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly COMPOSE=deploy/compose/compose.yaml
readonly EXAMPLE=deploy/compose/example
readonly REQUIRED="FERROFED_FEDERATION_ID FERROFED_PIXM_URL FERROFED_PIXM_MEMBERS"

case "$#" in
  0) binary="" ;;
  1) binary="$1" ;;
  *)
    sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
    exit 2
    ;;
esac
if [ -n "$binary" ] && [ ! -x "$binary" ]; then
  echo "::error::$binary is not an executable ferrofed binary." >&2
  exit 2
fi

docker compose version

fail=0
bad() {
  echo "::error file=$COMPOSE::$1" >&2
  fail=1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir "$work/secrets"
# Synthetic credentials for the two files example.env names.
printf 'synthetic-token-a\n' > "$work/secrets/node-a"
printf 'synthetic-password-b\n' > "$work/secrets/node-b"
cp "$EXAMPLE/registry.toml" "$work/registry.toml"

# compose ENV_FILE ARGS...: docker compose over the release file with ENV_FILE
# as its variables, the example registry and the synthetic secrets.
compose() {
  local env_file="$1"
  shift
  FERROFED_REGISTRY="$work/registry.toml" FERROFED_SECRETS_DIR="$work/secrets" \
    docker compose --project-directory "$(dirname "$COMPOSE")" -f "$COMPOSE" \
    --env-file "$env_file" "$@"
}

echo "== the release compose file renders with the example values"
if compose "$EXAMPLE/example.env" config --quiet; then
  echo "OK: $COMPOSE renders"
else
  bad "$COMPOSE does not render with $EXAMPLE/example.env"
fi

echo "== each required variable is refused when missing"
for name in $REQUIRED; do
  grep -v -E "^${name}=" "$EXAMPLE/example.env" > "$work/without.env"
  if out="$(compose "$work/without.env" config --quiet 2>&1)"; then
    bad "$COMPOSE renders without $name"
  elif ! grep -q "set $name" <<< "$out"; then
    bad "without $name, docker compose says something other than its message: $out"
  else
    echo "OK: $name is required"
  fi
done

echo "== the embedded configuration"
rendered="$(compose "$EXAMPLE/example.env" config --format json)"
content="$(jq -r '.configs["ferrofed-toml"].content // empty' <<< "$rendered")"
label="$(jq -r '.services.ferrofed.labels["eu.ferrofed.compose.configuration"] // empty' <<< "$rendered")"
if [ -z "$content" ]; then
  bad "$COMPOSE embeds no ferrofed-toml configuration"
elif [ "$content" != "$label" ]; then
  bad "the gateway's configuration label is not the embedded configuration, so a changed value would not recreate it"
else
  echo "OK: the configuration and the gateway's label agree"
fi

if [ -z "$binary" ]; then
  echo "no ferrofed binary given: config check skipped"
else
  # The container paths, pointed at the files this run wrote.
  sed -e "s|\"/etc/ferrofed/registry\"|\"$work/registry.toml\"|" \
    -e "s|\"/run/secrets/ferrofed/|\"$work/secrets/|g" \
    <<< "$content" > "$work/ferrofed.toml"
  if grep -q -E '/etc/ferrofed/|/run/secrets/' "$work/ferrofed.toml"; then
    bad "the embedded configuration names a container path this guard does not know"
  fi
  # A clean environment, so no FERROFED__ override of the caller's applies.
  if out="$(env -i PATH="$PATH" "$binary" config check --config "$work/ferrofed.toml" 2>&1)"; then
    echo "OK: $out"
  else
    bad "ferrofed config check refuses the embedded configuration: $out"
  fi
  # The check has teeth: a member the PIX Manager does not resolve is refused.
  sed -e 's|, "node-b" = "urn:oid:2.999.20"||' "$work/ferrofed.toml" > "$work/refused.toml"
  if env -i PATH="$PATH" "$binary" config check --config "$work/refused.toml" > /dev/null 2>&1; then
    bad "ferrofed config check accepts a configuration whose PIX Manager resolves only one of two members"
  else
    echo "OK: config check refuses a member no PIX Manager resolves"
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "release compose: FAILED" >&2
  exit 1
fi
echo "release compose: OK"
