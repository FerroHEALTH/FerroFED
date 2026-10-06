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
#   2. checks that statefulset.yaml mounts the ConfigMap at /etc/ferrofed and
#      the ferrofed-secrets Secret at /run/secrets/ferrofed, where the
#      configuration names them;
#   3. checks that each replica keeps its audit spool on a volume claim of its
#      own and on no emptyDir, which a reschedule loses;
#   4. checks that terminationGracePeriodSeconds outlasts the drain delay,
#      the drain and the bindings' drain of the configuration's [server]
#      table, each of which the configuration must set;
#   5. checks that each replica serves /metrics on the port [metrics] listen
#      names, annotated for the scraper, and that networkpolicy.yaml opens
#      that port to a named source alone;
#   6. runs `ferrofed config check` over that configuration, with a synthetic
#      file for each `_file` secret it names and a synthetic ES384 key for each
#      `key_file`, the mount paths rewritten to a temporary directory and the
#      audit spool left where the example puts it, since config check writes
#      nothing;
#   7. runs it again without the [signing] table, and again without the
#      [metrics] scrape token while the admin listener is off loopback, each
#      of which it must refuse by name; and
#   8. checks that a configuration with a registry sends its access records
#      to an Audit Record Repository, never to the log target, which names
#      no caller and no patient (Regulation (EU) 2025/327 Annex II 3.2), and
#      that config check refuses the example with destination = "log".
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

echo "== the StatefulSet mounts both where the configuration names them"
for mount in /etc/ferrofed /run/secrets/ferrofed; do
  if grep -qE "^[[:space:]]+mountPath: $mount$" "$EXAMPLE/statefulset.yaml"; then
    echo "OK: statefulset.yaml mounts $mount"
  else
    bad "statefulset.yaml mounts nothing at $mount"
  fi
done
if ! grep -qE '^[[:space:]]+secretName: ferrofed-secrets$' "$EXAMPLE/statefulset.yaml"; then
  bad "statefulset.yaml mounts no ferrofed-secrets Secret"
fi

echo "== the audit spool outlives the pod"
if ! grep -qE '^kind: StatefulSet$' "$EXAMPLE/statefulset.yaml"; then
  bad "statefulset.yaml is no StatefulSet"
fi
if grep -qE '^[[:space:]]+emptyDir:' "$EXAMPLE/statefulset.yaml"; then
  bad "statefulset.yaml keeps a volume on an emptyDir, which a reschedule loses"
fi
if ! grep -qE '^[[:space:]]+mountPath: /var/lib/ferrofed$' "$EXAMPLE/statefulset.yaml"; then
  bad "statefulset.yaml mounts nothing at /var/lib/ferrofed for the audit spool"
fi
if awk '
  /^  volumeClaimTemplates:$/ { claims = 1; next }
  /^  [^ ]/ { claims = 0 }
  claims && /^[[:space:]]+name: audit-spool$/ { found = 1 }
  END { exit !found }
' "$EXAMPLE/statefulset.yaml"; then
  echo "OK: each replica's audit spool is a volume claim of its own"
else
  bad "statefulset.yaml has no audit-spool volumeClaimTemplate"
fi

echo "== the grace period covers the drain delay, the drain and the bindings' drain"
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
    echo "the [server] timeouts or the grace period are not whole numbers"
    return 1
  elif ((grace * 1000 <= delay + shutdown + bindings)); then
    echo "a grace period of $grace s does not outlast drain_delay_ms ($delay) plus shutdown_timeout_ms ($shutdown) plus bindings_drain_timeout_ms ($bindings)"
    return 1
  fi
  echo "a grace period of $grace s outlasts the $delay ms delay, the $shutdown ms drain and the $bindings ms bindings' drain"
}
grace="$(sed -nE 's/^[[:space:]]+terminationGracePeriodSeconds: ([0-9]+)$/\1/p' "$EXAMPLE/statefulset.yaml")"
if out="$(grace_covers "$work/ferrofed.toml" "$grace")"; then
  echo "OK: terminationGracePeriodSeconds: $out"
else
  bad "statefulset.yaml terminationGracePeriodSeconds: $out"
fi
# The check has teeth: a grace period of one second, and the example without
# its bindings' drain budget, each fail it.
grep -v '^bindings_drain_timeout_ms[[:space:]]' "$work/ferrofed.toml" > "$work/unbudgeted.toml" || true
if grace_covers "$work/ferrofed.toml" 1 > /dev/null; then
  bad "the grace check accepts a grace period of one second"
elif cmp -s "$work/ferrofed.toml" "$work/unbudgeted.toml"; then
  bad "configmap.yaml sets no [server] bindings_drain_timeout_ms to remove"
elif grace_covers "$work/unbudgeted.toml" "$grace" > /dev/null; then
  bad "the grace check accepts a configuration that leaves bindings_drain_timeout_ms unset"
else
  echo "OK: a short grace period and an unset budget each fail the grace check"
fi

echo "== the admin listener is scraped, and open to the scraper alone"
metrics_port="$(awk '
  /^\[/ { inside = ($0 == "[metrics]"); next }
  inside && $1 == "listen" && $2 == "=" { print $3; exit }
' "$work/ferrofed.toml" | sed -E 's/^".*:([0-9]+)"$/\1/')"
container_port="$(awk '
  /^[[:space:]]+- name: metrics$/ { named = 1; next }
  named && /containerPort:/ { print $2; exit }
' "$EXAMPLE/statefulset.yaml")"
annotated="$(sed -nE 's/^[[:space:]]+prometheus\.io\/port: "([0-9]+)"$/\1/p' "$EXAMPLE/statefulset.yaml")"
if ! [[ "$metrics_port" =~ ^[0-9]+$ ]]; then
  bad "configmap.yaml sets no [metrics] listen port"
elif [[ "$container_port" != "$metrics_port" ]]; then
  bad "statefulset.yaml's metrics port ($container_port) is not [metrics] listen's ($metrics_port)"
elif [[ "$annotated" != "$metrics_port" ]]; then
  bad "statefulset.yaml's prometheus.io/port annotation ($annotated) is not [metrics] listen's ($metrics_port)"
else
  echo "OK: each replica serves /metrics on $metrics_port, annotated for the scraper"
fi
if ! grep -qE '^kind: NetworkPolicy$' "$EXAMPLE/networkpolicy.yaml" 2> /dev/null; then
  bad "networkpolicy.yaml is no NetworkPolicy"
elif ! awk '
  /^    - from:$/ { from = 1; next }
  /^    - / { from = 0 }
  from && /^[[:space:]]+- port: metrics$/ { found = 1 }
  END { exit !found }
' "$EXAMPLE/networkpolicy.yaml"; then
  bad "networkpolicy.yaml opens the metrics port to no named source"
else
  echo "OK: networkpolicy.yaml opens the metrics port to the scraper alone"
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
# The audit spools keep their paths: config check writes nothing and reads
# each spool directory where it stands.
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

# The admin listener is off loopback, so the example without its scrape
# token is refused, naming the key that authenticates the scrape.
awk '
  /^\[/ { inside = ($0 == "[metrics]") }
  inside && $1 == "scrape_token_file" { next }
  { print }
' "$work/ferrofed.toml" > "$work/unscraped.toml"
if cmp -s "$work/ferrofed.toml" "$work/unscraped.toml"; then
  bad "configmap.yaml sets no [metrics] scrape_token_file for its admin listener off loopback"
elif out="$(check "$work/unscraped.toml" 2>&1)"; then
  bad "ferrofed config check accepts the Kubernetes example without metrics.scrape_token_file"
elif ! grep -q 'metrics.scrape_token_file' <<< "$out"; then
  bad "ferrofed config check refuses the example without its scrape token without naming it: $out"
else
  echo "OK: the example without its scrape token is refused by name"
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
destination="$(audit_destination "$work/ferrofed.toml")"
if ! grep -qE '^\[registry(\.[a-z]+)?\]$' "$work/ferrofed.toml"; then
  echo "OK: the example configures no registry, so it records no access"
elif [[ "$destination" != '"repository"' ]]; then
  bad "configmap.yaml configures a registry and sends its access records to ${destination:-nowhere}; it needs [audit] destination = \"repository\""
else
  echo "OK: the example sends its access records to an Audit Record Repository"
fi
# The check has teeth: the example with its access records on the log target
# is refused, naming the key.
awk '
  /^\[audit\.repository\]$/ { skip = 1; next }
  /^\[/ { skip = 0 }
  skip { next }
  $1 == "destination" && $2 == "=" { print "destination = \"log\""; next }
  { print }
' "$work/ferrofed.toml" > "$work/logged.toml"
if [[ "$(audit_destination "$work/logged.toml")" != '"log"' ]]; then
  bad "the example names no [audit] destination to rewrite"
elif out="$(check "$work/logged.toml" 2>&1)"; then
  bad "ferrofed config check accepts the Kubernetes example with destination = \"log\""
elif ! grep -q 'audit.destination' <<< "$out"; then
  bad "ferrofed config check refuses destination = \"log\" without naming audit.destination: $out"
else
  echo "OK: the example with destination = \"log\" is refused by name"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "kubernetes example: FAILED" >&2
  exit 1
fi
echo "kubernetes example: OK"
