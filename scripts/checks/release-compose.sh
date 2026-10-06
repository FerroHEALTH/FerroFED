#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The release compose guard (no specification governs this: our own design).
# Every release attaches deploy/compose/compose.yaml, ferrofed.toml and
# registry.toml under those names. This guard checks, over copies laid out as
# a downloader has them:
#
#   1. no compose file in the repository carries `build:`, because every one
#      runs the published image and only release-image.yml builds it;
#   2. Docker Compose renders the release compose file;
#   3. the stop_grace_period of the release compose file outlasts the drain
#      delay, the drain and the bindings' drain of the example's [server]
#      table, each of which the example must set, and the example
#      sends the access records of its registry to an Audit Record
#      Repository, never to the log target, which names no caller and no
#      patient (Regulation (EU) 2025/327 Annex II 3.2);
#   4. given a static Linux ferrofed binary, `ferrofed config check` accepts
#      the example ferrofed.toml and registry.toml exactly as attached, mounted
#      at the paths the rendered compose file mounts them, in the pinned base
#      image of docker/Dockerfile, with a synthetic file for each credential
#      the example names and a synthetic ES384 key for each signing key; and
#      it refuses the same configuration once a member is left out of the
#      PIX Manager, naming that member, once the PIX Manager's credential
#      would travel over plain http, naming its URL key, and once its access
#      records go to the log target, naming audit.destination.
#
# Usage:
#   scripts/checks/release-compose.sh                  checks 1 to 3
#   scripts/checks/release-compose.sh <ferrofed binary> all four; the binary
#                                                      is a static Linux
#                                                      build for this host's
#                                                      architecture
# Needs `docker compose`, `jq` and `openssl`. Exit 1 naming each failure; 0
# otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly RELEASE=deploy/compose
readonly ASSETS="compose.yaml ferrofed.toml registry.toml"

case "$#" in
  0) binary="" ;;
  1) binary="$1" ;;
  *)
    sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
    exit 2
    ;;
esac
if [[ -n "$binary" ]] && [[ ! -x "$binary" ]]; then
  echo "::error::$binary is not an executable ferrofed binary." >&2
  exit 2
fi

fail=0
bad() {
  echo "::error::$1" >&2
  fail=1
}

echo "== no compose file builds the image"
built=0
while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  if grep -n -E '^[[:space:]]*build:' "$file" >&2; then
    bad "$file carries build:; every compose file runs the published image, which only release-image.yml builds"
    built=1
  fi
done < <(git ls-files -- '*compose*.yaml' '*compose*.yml' ':(exclude)docs/specs/**' ':(glob,exclude)**/vendor/**')
[[ "$built" -eq 0 ]] && echo "OK: no compose file carries build:"

docker compose version

# The downloader's directory: the three assets, byte for byte, and secrets/.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for asset in $ASSETS; do
  cp "$RELEASE/$asset" "$work/$asset"
done
mkdir "$work/secrets"
# A synthetic value for every credential file the example names.
while IFS= read -r secret; do
  printf 'synthetic-%s\n' "$secret" > "$work/secrets/$secret"
done < <(sed -nE 's|^[a-z_]+_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
# A signing key is read as one, so each key_file holds a synthetic ES384 key.
while IFS= read -r key; do
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 \
    -out "$work/secrets/$key" 2> /dev/null ||
    bad "a synthetic signing key could not be generated for $key"
done < <(sed -nE 's|^key_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
chmod -R a+rX "$work"

echo "== the release compose file renders"
if rendered="$(docker compose --project-directory "$work" -f "$work/compose.yaml" config --format json)"; then
  echo "OK: $RELEASE/compose.yaml renders"
else
  bad "$RELEASE/compose.yaml does not render"
  rendered=""
fi

echo "== the stop grace period covers the drain delay, the drain and the bindings' drain"
# server_ms FILE KEY: KEY's value in the [server] table of FILE, empty when
# it is unset.
server_ms() {
  local file="$1" key="$2"
  awk -v key="$key" '
    /^\[/ { inside = ($0 == "[server]"); next }
    inside && $1 == key && $2 == "=" { print $3; exit }
  ' "$file"
}
# grace_covers FILE GRACE: whether a grace period of GRACE seconds outlasts
# the drain delay, the drain and the bindings' drain FILE sets, printing the
# arithmetic or the reason it does not. It copies no default from the code,
# so FILE must set all three.
grace_covers() {
  local file="$1" grace="$2" key unset_keys="" delay shutdown bindings
  for key in drain_delay_ms shutdown_timeout_ms bindings_drain_timeout_ms; do
    [[ -n "$(server_ms "$file" "$key")" ]] || unset_keys="$unset_keys $key"
  done
  delay="$(server_ms "$file" drain_delay_ms)"
  shutdown="$(server_ms "$file" shutdown_timeout_ms)"
  bindings="$(server_ms "$file" bindings_drain_timeout_ms)"
  if [[ -n "$unset_keys" ]]; then
    echo "the configuration leaves [server]$unset_keys unset; this guard reads each from the configuration and copies no default from the code"
    return 1
  elif ! [[ "$delay$shutdown$bindings" =~ ^[0-9]+$ ]] || ! [[ "$grace" =~ ^[0-9]+$ ]]; then
    echo "the [server] timeouts are not whole numbers, or the grace period is not in whole seconds"
    return 1
  elif ((grace * 1000 <= delay + shutdown + bindings)); then
    echo "a grace period of $grace s does not outlast drain_delay_ms ($delay) plus shutdown_timeout_ms ($shutdown) plus bindings_drain_timeout_ms ($bindings)"
    return 1
  fi
  echo "a grace period of $grace s outlasts the $delay ms delay, the $shutdown ms drain and the $bindings ms bindings' drain"
}
grace="$(sed -nE 's/^[[:space:]]+stop_grace_period: ([0-9]+)s$/\1/p' "$RELEASE/compose.yaml")"
if out="$(grace_covers "$RELEASE/ferrofed.toml" "$grace")"; then
  echo "OK: stop_grace_period: $out"
else
  bad "$RELEASE/compose.yaml stop_grace_period: $out"
fi
# The check has teeth: a grace period of one second, and the example without
# its bindings' drain budget, each fail it.
grep -v '^bindings_drain_timeout_ms[[:space:]]' "$RELEASE/ferrofed.toml" > "$work/unbudgeted.toml" || true
if grace_covers "$RELEASE/ferrofed.toml" 1 > /dev/null; then
  bad "the grace check accepts a grace period of one second"
elif cmp -s "$RELEASE/ferrofed.toml" "$work/unbudgeted.toml"; then
  bad "$RELEASE/ferrofed.toml sets no [server] bindings_drain_timeout_ms to remove"
elif grace_covers "$work/unbudgeted.toml" "$grace" > /dev/null; then
  bad "the grace check accepts a configuration that leaves bindings_drain_timeout_ms unset"
else
  echo "OK: a short grace period and an unset budget each fail the grace check"
fi

echo "== the access log names the caller and the patient"
# audit_destination FILE: the destination key of FILE's [audit] table.
audit_destination() {
  local file="$1"
  awk '
    /^\[/ { inside = ($0 == "[audit]"); next }
    inside && $1 == "destination" && $2 == "=" { print $3; exit }
  ' "$file"
}
destination="$(audit_destination "$RELEASE/ferrofed.toml")"
if ! grep -qE '^\[registry(\.[a-z]+)?\]$' "$RELEASE/ferrofed.toml"; then
  echo "OK: the example configures no registry, so it records no access"
elif [[ "$destination" != '"repository"' ]]; then
  bad "$RELEASE/ferrofed.toml configures a registry and sends its access records to ${destination:-nowhere}; it needs [audit] destination = \"repository\""
else
  echo "OK: the example sends its access records to an Audit Record Repository"
fi

if [[ -z "$binary" ]]; then
  echo "no ferrofed binary given: config check skipped"
elif [[ -n "$rendered" ]]; then
  echo "== config check over the attached examples"
  # The image the gateway runs in is the Dockerfile's last stage.
  base="$(sed -nE 's|^FROM[[:space:]]+([^[:space:]]+).*|\1|p' docker/Dockerfile | tail -n1)"
  config="$(jq -r '.services.ferrofed.environment.FERROFED_CONFIG // empty' <<< "$rendered")"
  mounts=()
  while IFS= read -r mount; do
    mounts+=(--volume "$mount")
  done < <(jq -r '.services.ferrofed.volumes[] | select(.type == "bind") | "\(.source):\(.target):ro"' <<< "$rendered")
  # check: runs config check as the image does, read-only and unprivileged,
  # with no audit-spool volume, since config check writes nothing.
  check() {
    docker run --rm --read-only --user 65532:65532 --cap-drop ALL \
      --security-opt no-new-privileges:true --network none \
      --env FERROFED_CONFIG="$config" \
      --volume "$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary"):/usr/local/bin/ferrofed:ro" \
      ${mounts[@]+"${mounts[@]}"} --entrypoint /usr/local/bin/ferrofed "$base" config check
  }
  docker pull --quiet "$base" > /dev/null
  if [[ -z "$config" ]] || [[ "${#mounts[@]}" -eq 0 ]]; then
    bad "the rendered compose file names no FERROFED_CONFIG or no bind mount"
  elif out="$(check 2>&1)"; then
    echo "OK: $out"
  else
    bad "ferrofed config check refuses the attached examples: $out"
  fi
  # The check has teeth: a member the PIX Manager does not resolve is refused.
  # Rewritten in place, so the mounted file keeps its inode.
  sed '/^"node-b" = /d' "$work/ferrofed.toml" > "$work/refused.toml"
  cat "$work/refused.toml" > "$work/ferrofed.toml"
  if out="$(check 2>&1)"; then
    bad "ferrofed config check accepts a PIX Manager that resolves only one of two members"
  elif ! grep -q 'node-b' <<< "$out"; then
    bad "ferrofed config check refuses a missing member without naming it: $out"
  else
    echo "OK: a member no PIX Manager resolves is refused by name"
  fi
  # A credential sent over plain http is refused under the production profile.
  # The http URL is the refused input under test; nothing connects to it.
  sed -E 's|^url = "https://(pix\.[^"]*)"$|url = "http://\1"|' "$RELEASE/ferrofed.toml" > "$work/refused.toml"
  cat "$work/refused.toml" > "$work/ferrofed.toml"
  if cmp -s "$RELEASE/ferrofed.toml" "$work/ferrofed.toml"; then
    bad "the example names no https PIX Manager URL to rewrite"
  elif out="$(check 2>&1)"; then
    bad "ferrofed config check accepts a PIX Manager credential sent over plain http"
  elif ! grep -qF 'pixm.manager[0].url' <<< "$out"; then
    bad "ferrofed config check refuses a cleartext credential without naming its key: $out"
  else
    echo "OK: a credential sent over plain http is refused by key"
  fi
  # The access records of a registry sent to the log target are refused.
  awk '
    /^\[audit\.repository\]$/ { skip = 1; next }
    /^\[/ { skip = 0 }
    skip { next }
    $1 == "destination" && $2 == "=" { print "destination = \"log\""; next }
    { print }
  ' "$RELEASE/ferrofed.toml" > "$work/refused.toml"
  cat "$work/refused.toml" > "$work/ferrofed.toml"
  if [[ "$(audit_destination "$work/ferrofed.toml")" != '"log"' ]]; then
    bad "the example names no [audit] destination to rewrite"
  elif out="$(check 2>&1)"; then
    bad "ferrofed config check accepts the access records of a registry sent to the log target"
  elif ! grep -qF 'audit.destination' <<< "$out"; then
    bad "ferrofed config check refuses destination = \"log\" without naming audit.destination: $out"
  else
    echo "OK: the access records sent to the log target are refused by key"
  fi
fi

if [[ "$fail" -ne 0 ]]; then
  echo "release compose: FAILED" >&2
  exit 1
fi
echo "release compose: OK"
