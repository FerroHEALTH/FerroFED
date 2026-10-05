#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The Kubernetes example guard (no specification governs this: our own
# design). kubeconform holds deploy/kubernetes/ to the Kubernetes schemas;
# this guard holds it to what the gateway accepts. Given a ferrofed binary
# for this host, it:
#
#   1. reads ferrofed.toml and registry.toml out of configmap.yaml, as the
#      ConfigMap mounts them;
#   2. checks that deployment.yaml mounts the ConfigMap at /etc/ferrofed and
#      the ferrofed-secrets Secret at /run/secrets/ferrofed, where the
#      configuration names them;
#   3. runs `ferrofed config check` over that configuration, with a synthetic
#      file for each `_file` secret it names and a synthetic ES384 key for each
#      `key_file`, the mount paths rewritten to a temporary directory; and
#   4. runs it again without the [signing] table, which it must refuse.
#
# Usage:
#   scripts/checks/kubernetes-example.sh <ferrofed binary>
# Needs awk, sed and openssl. Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly EXAMPLE=deploy/kubernetes

if [[ "$#" -ne 1 ]] || [[ ! -x "$1" ]]; then
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
fi
binary="$1"

fail=0
bad() {
  local message="$1"
  echo "::error::$message" >&2
  fail=1
}

# data_key NAME: the block scalar NAME holds under data: in configmap.yaml,
# its four-space indent removed.
data_key() {
  local name="$1"
  awk -v key="  $name: |" '
    $0 == key { inside = 1; next }
    inside && /^    / { print substr($0, 5); next }
    inside && /^[[:space:]]*$/ { print ""; next }
    inside { exit }
  ' "$EXAMPLE/configmap.yaml"
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir "$work/secrets"

echo "== the ConfigMap's files"
data_key ferrofed.toml > "$work/ferrofed.toml"
data_key registry.toml > "$work/registry.toml"
for file in ferrofed.toml registry.toml; do
  if [[ -s "$work/$file" ]]; then
    echo "OK: configmap.yaml carries $file"
  else
    bad "configmap.yaml carries no $file under data"
  fi
done

echo "== the Deployment mounts both where the configuration names them"
for mount in /etc/ferrofed /run/secrets/ferrofed; do
  if grep -qE "^[[:space:]]+mountPath: $mount$" "$EXAMPLE/deployment.yaml"; then
    echo "OK: deployment.yaml mounts $mount"
  else
    bad "deployment.yaml mounts nothing at $mount"
  fi
done
if ! grep -qE '^[[:space:]]+secretName: ferrofed-secrets$' "$EXAMPLE/deployment.yaml"; then
  bad "deployment.yaml mounts no ferrofed-secrets Secret"
fi

# A synthetic value for every secret file the configuration names, and a
# synthetic ES384 key for each signing key, which is read as one.
while IFS= read -r secret; do
  printf 'synthetic-%s\n' "$secret" > "$work/secrets/$secret"
done < <(sed -nE 's|^[a-z_]+_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
while IFS= read -r key; do
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 \
    -out "$work/secrets/$key" 2> /dev/null ||
    bad "a synthetic signing key could not be generated for $key"
done < <(sed -nE 's|^key_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
sed -i.orig -e "s|/run/secrets/ferrofed/|$work/secrets/|g" \
  -e "s|/etc/ferrofed/|$work/|g" "$work/ferrofed.toml"

# check FILE: config check over FILE, as the image runs it.
check() {
  local file="$1"
  env -u FERROFED_CONFIG "$binary" config check --config "$file"
}

echo "== config check over the example"
if out="$(check "$work/ferrofed.toml" 2>&1)"; then
  echo "OK: $out"
else
  bad "ferrofed config check refuses the Kubernetes example: $out"
fi

# The check has teeth: the example without its signing key is refused.
awk '
  /^\[signing\]$/ { skip = 1; next }
  /^\[/ { skip = 0 }
  !skip
' "$work/ferrofed.toml" > "$work/unsigned.toml"
if out="$(check "$work/unsigned.toml" 2>&1)"; then
  bad "ferrofed config check accepts the Kubernetes example without [signing]"
elif ! grep -q 'signing' <<< "$out"; then
  bad "ferrofed config check refuses the example without [signing] without naming it: $out"
else
  echo "OK: the example without [signing] is refused by name"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "kubernetes example: FAILED" >&2
  exit 1
fi
echo "kubernetes example: OK"
