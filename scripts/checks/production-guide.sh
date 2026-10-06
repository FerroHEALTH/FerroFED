#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The production guide guard (no specification governs this: our own design).
# The book's production guide, website/book/src/operate/production.md, shows
# its configuration in TOML blocks, and deploy/nginx/ferrofed.conf is the
# reverse proxy it ships. This guard holds both to what the gateway accepts
# and serves:
#
#   1. every ```toml block of the page opens with `# ferrofed.toml` or
#      `# registry.toml`, and the blocks of each file assemble into it;
#   2. the proxy's upstream is the assembled server.listen, its locations sit
#      under server.base_path, server.public_url ends in that path, [signing]
#      jwks_uri and [pmir] callback_url, as set or as server.public_url gives
#      them, name the routes the proxy passes, and the proxy sets the
#      X-Forwarded-For the gateway reads from the proxy's address alone;
#   3. given a static Linux ferrofed binary, `ferrofed config check` accepts
#      the assembled files, mounted where the page mounts them, in the pinned
#      base image of docker/Dockerfile on a read-only root with no durable
#      volume, with a synthetic file for each secret
#      and a synthetic ES384 key for each signing key; and refuses them
#      without [audit] destination, naming it;
#   4. with the same binary, the gateway serves the assembled files with no
#      network, nginx runs the shipped file in the gateway's network
#      namespace, as a proxy in the same pod would, and each route answers as
#      the page says: the JWK Set, the PMIR feed route, the ITS-REST surface,
#      GET and OPTIONS {base}/ reach the gateway, the health family and every
#      path outside the base stop at the proxy, every answer carries
#      Strict-Transport-Security, and the proxy's access log names the
#      gateway's request id and never a query string.
#
# Usage:
#   scripts/checks/production-guide.sh                  checks 1 and 2
#   scripts/checks/production-guide.sh <ferrofed binary> all four; the binary
#                                                       is a static Linux
#                                                       build for this host's
#                                                       architecture
# Needs awk, sed and grep; with a binary, docker and openssl. Exit 1 naming
# each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly PAGE=website/book/src/operate/production.md
readonly PROXY=deploy/nginx/ferrofed.conf
# The nginx the proxy check runs: its one pin is the docs/VERSIONS.md row.
# shellcheck disable=SC2016 # the backticks are Markdown code marks, matched literally
NGINX="$(sed -nE 's/^\| nginx reverse proxy image \| `([^`]+@sha256:[0-9a-f]{64})` \|.*$/\1/p' docs/VERSIONS.md)"
readonly NGINX

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

work="$(mktemp -d)"
gateway="ffd-guide-gateway-$$"
proxy="ffd-guide-proxy-$$"
cleanup() {
  if [[ -n "$binary" ]]; then
    docker rm --force "$proxy" "$gateway" > /dev/null 2>&1 || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

echo "== the page's TOML blocks"
# Each block goes to the file its first line names; a block that names none
# is reported with its line number.
awk -v dir="$work" '
  /^```toml[[:space:]]*$/ { inside = 1; first = 1; start = NR; next }
  inside && /^```[[:space:]]*$/ { inside = 0; if (file != "") print "" >> file; file = ""; next }
  inside && first {
    first = 0
    if ($0 == "# ferrofed.toml") { file = dir "/ferrofed.toml"; blocks++ }
    else if ($0 == "# registry.toml") { file = dir "/registry.toml"; blocks++ }
    else { print start > (dir "/unnamed") }
    next
  }
  inside && file != "" { print >> file }
  END { print blocks + 0 > (dir "/blocks") }
' "$PAGE"
if [[ -s "$work/unnamed" ]]; then
  while IFS= read -r line; do
    bad "$PAGE:$line: a toml block opens with neither # ferrofed.toml nor # registry.toml"
  done < "$work/unnamed"
fi
for file in ferrofed.toml registry.toml; do
  if [[ -s "$work/$file" ]]; then
    echo "OK: the page assembles $file"
  else
    bad "$PAGE assembles no $file"
    touch "$work/$file"
  fi
done
echo "the page carries $(cat "$work/blocks") toml blocks"

echo "== the proxy matches the configuration"
# value TABLE KEY: KEY's string value in TABLE of the assembled configuration.
value() {
  local table="$1" key="$2"
  awk -v table="[$table]" -v key="$key" '
    /^\[/ { inside = ($0 == table); next }
    inside && $1 == key && $2 == "=" { gsub(/"/, "", $3); print $3; exit }
  ' "$work/ferrofed.toml"
}
listen="$(value server listen)"
base="$(value server base_path)"
public="$(value server public_url)"
jwks="$(value signing jwks_uri)"
feed="$(value pmir callback_url)"
# An unset jwks_uri or callback_url is its route under server.public_url.
if [[ -n "$public" ]]; then
  if [[ "$public" != https://*"$base" ]]; then
    bad "the page's server.public_url, $public, does not end in server.base_path $base"
  else
    echo "OK: server.public_url ends in server.base_path"
  fi
  jwks="${jwks:-$public/.well-known/jwks.json}"
  feed="${feed:-$public/pmir/feed}"
fi
upstream="$(sed -nE 's/^[[:space:]]+server ([0-9.:]+);$/\1/p' "$PROXY")"
if [[ -z "$listen" ]] || [[ "$upstream" != "$listen" ]]; then
  bad "$PROXY passes to '$upstream', and the page's server.listen is '$listen'"
else
  echo "OK: the proxy passes to server.listen, $listen"
fi
if [[ -z "$base" ]] || [[ "$base" == "/" ]]; then
  bad "the page sets no server.base_path other than /"
else
  outside=0
  while IFS= read -r path; do
    case "$path" in
      / | "$base" | "$base"/*) ;;
      *)
        bad "$PROXY has the location $path outside the base path $base"
        outside=1
        ;;
    esac
  done < <(sed -nE 's/^[[:space:]]+location (= |\^~ )?([^ ]+) \{$/\2/p' "$PROXY")
  [[ "$outside" -eq 0 ]] && echo "OK: every location of the proxy sits under $base"
  for route in "$base/.well-known/jwks.json" "$base/pmir/feed"; do
    if grep -qE "^[[:space:]]+location = ${route//./\\.} \{$" "$PROXY"; then
      echo "OK: the proxy passes $route"
    else
      bad "$PROXY does not pass $route"
    fi
  done
  if [[ "$jwks" != https://*"$base/.well-known/jwks.json" ]]; then
    bad "the page's [signing] jwks_uri, $jwks, does not name $base/.well-known/jwks.json"
  else
    echo "OK: [signing] jwks_uri names the JWK Set route"
  fi
  if [[ "$feed" != https://*"$base/pmir/feed" ]]; then
    bad "the page's [pmir] callback_url, $feed, does not name $base/pmir/feed"
  else
    echo "OK: [pmir] callback_url names the feed route"
  fi
fi
# The proxy names the client in X-Forwarded-For, set and never appended, and
# the gateway reads that header from the proxy's address alone.
upstream_host="${upstream%:*}"
# shellcheck disable=SC2016 # $remote_addr is nginx's variable, matched literally
if ! grep -qE '^[[:space:]]+proxy_set_header X-Forwarded-For \$remote_addr;$' "$PROXY"; then
  bad "$PROXY does not set X-Forwarded-For to \$remote_addr"
elif [[ "$(value server forwarded_header)" != "x-forwarded-for" ]]; then
  bad "the page's server.forwarded_header is not x-forwarded-for, the header $PROXY sets"
elif ! grep -qE "^trusted_proxies[[:space:]]*=[[:space:]]*\[.*\"${upstream_host//./\\.}\".*\]" "$work/ferrofed.toml"; then
  bad "the page's server.trusted_proxies does not name $upstream_host, the address $PROXY reaches the gateway from"
else
  echo "OK: the gateway takes the client's address from the proxy alone"
fi

if [[ -z "$binary" ]]; then
  echo "no ferrofed binary given: config check and the proxy run skipped"
  if [[ "$fail" -ne 0 ]]; then
    echo "production guide: FAILED" >&2
    exit 1
  fi
  echo "production guide: OK"
  exit 0
fi

if [[ -z "$NGINX" ]]; then
  bad "docs/VERSIONS.md has no digest-pinned 'nginx reverse proxy image' row"
  echo "production guide: FAILED" >&2
  exit 1
fi
mkdir "$work/secrets" "$work/tls"
# A synthetic value for every secret file the configuration names, and a
# synthetic ES384 key for each signing key, which is read as one.
while IFS= read -r secret; do
  printf 'synthetic-%s\n' "$secret" > "$work/secrets/$secret"
done < <(sed -nE 's|^[a-z_]+_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)".*$|\1|p' "$work/ferrofed.toml")
while IFS= read -r key; do
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 \
    -out "$work/secrets/$key" 2> /dev/null ||
    bad "a synthetic signing key could not be generated for $key"
done < <(sed -nE 's|^key_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)".*$|\1|p' "$work/ferrofed.toml")
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 1 \
  -subj /CN=gateway.example.org -addext subjectAltName=DNS:gateway.example.org \
  -keyout "$work/tls/gateway.example.org.key" -out "$work/tls/gateway.example.org.crt" 2> /dev/null ||
  bad "a synthetic certificate for the proxy could not be generated"
chmod -R a+rX "$work"

# The image the gateway runs in is the Dockerfile's last stage.
image="$(sed -nE 's|^FROM[[:space:]]+([^[:space:]]+).*|\1|p' docker/Dockerfile | tail -n1)"
binary_path="$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")"
docker pull --quiet "$image" > /dev/null
docker pull --quiet "$NGINX" > /dev/null

# gateway_run ARGS...: the binary in the gateway's image, as the image runs
# it: read-only, unprivileged, with no network, and the page's mounts. No
# durable volume is mounted: config check writes nothing, and serve is given
# a writable /var/lib/ferrofed in place of the volume.
gateway_run() {
  docker run --read-only --user 65532:65532 --cap-drop ALL \
    --security-opt no-new-privileges:true --network none \
    --volume "$binary_path:/usr/local/bin/ferrofed:ro" \
    --volume "$work/ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro" \
    --volume "$work/registry.toml:/etc/ferrofed/registry.toml:ro" \
    --volume "$work/secrets:/run/secrets/ferrofed:ro" \
    --entrypoint /usr/local/bin/ferrofed "$@"
}

echo "== config check over the page's configuration"
if out="$(gateway_run --rm "$image" config check --config /etc/ferrofed/ferrofed.toml 2>&1)"; then
  echo "OK: $out"
else
  bad "ferrofed config check refuses the page's configuration: $out"
fi
# The check has teeth: without [audit] destination the page's configuration
# is refused by name. Rewritten in place, so the mounted file keeps its inode.
cp "$work/ferrofed.toml" "$work/accepted.toml"
grep -v '^destination = ' "$work/accepted.toml" > "$work/refused.toml"
cat "$work/refused.toml" > "$work/ferrofed.toml"
if cmp -s "$work/accepted.toml" "$work/ferrofed.toml"; then
  bad "the page sets no [audit] destination to remove"
elif out="$(gateway_run --rm "$image" config check --config /etc/ferrofed/ferrofed.toml 2>&1)"; then
  bad "ferrofed config check accepts the page's configuration without [audit] destination"
elif ! grep -qF 'audit.destination' <<< "$out"; then
  bad "ferrofed config check refuses a missing [audit] destination without naming it: $out"
else
  echo "OK: the configuration without [audit] destination is refused by name"
fi
cat "$work/accepted.toml" > "$work/ferrofed.toml"

echo "== the gateway behind the shipped proxy"
gateway_run --detach --name "$gateway" \
  --tmpfs /var/lib/ferrofed:uid=65532,gid=65532,mode=0700 \
  "$image" serve --config /etc/ferrofed/ferrofed.toml > /dev/null
ready=0
for _ in $(seq 1 30); do
  if docker exec "$gateway" /usr/local/bin/ferrofed healthcheck --config /etc/ferrofed/ferrofed.toml > /dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
if [[ "$ready" -ne 1 ]]; then
  bad "the gateway did not turn ready on the page's configuration: $(docker logs "$gateway" 2>&1 | tail -n 5)"
fi
docker run --detach --name "$proxy" --network "container:$gateway" \
  --volume "$(cd "$(dirname "$PROXY")" && pwd)/$(basename "$PROXY"):/etc/nginx/conf.d/default.conf:ro" \
  --volume "$work/tls:/etc/nginx/tls:ro" "$NGINX" > /dev/null
served=0
for _ in $(seq 1 30); do
  if docker exec "$proxy" curl -sk -o /dev/null --resolve gateway.example.org:443:127.0.0.1 \
    https://gateway.example.org/fed/ 2> /dev/null; then
    served=1
    break
  fi
  sleep 1
done
if [[ "$served" -ne 1 ]]; then
  bad "nginx does not serve the shipped file: $(docker logs "$proxy" 2>&1 | tail -n 5)"
fi

# ask EXPECTED REACHES METHOD PATH [CURL ARGS...]: one request through the
# proxy; EXPECTED is the status, REACHES is "gateway" when the answer must
# carry the gateway's x-request-id and "proxy" when it must not.
ask() {
  local expected="$1" reaches="$2" method="$3" path="$4"
  shift 4
  local headers status
  headers="$(docker exec "$proxy" curl -sk -o /dev/null -D - -X "$method" \
    --resolve gateway.example.org:443:127.0.0.1 "$@" \
    "https://gateway.example.org$path" 2> /dev/null | tr -d '\r')" || true
  status="$(sed -nE '1s|^HTTP/[0-9.]+ ([0-9]{3}).*|\1|p' <<< "$headers")"
  if [[ "$status" != "$expected" ]]; then
    bad "$method $path answered '$status' through the proxy, not $expected"
    return
  fi
  if ! grep -qi '^strict-transport-security: ' <<< "$headers"; then
    bad "$method $path answered without Strict-Transport-Security"
    return
  fi
  if [[ "$reaches" == gateway ]] && ! grep -qi '^x-request-id: ' <<< "$headers"; then
    bad "$method $path answered $status without the gateway's x-request-id"
  elif [[ "$reaches" == proxy ]] && grep -qi '^x-request-id: ' <<< "$headers"; then
    bad "$method $path reached the gateway; the proxy should stop it"
  else
    echo "OK: $method $path answers $status from the $reaches"
  fi
}

if [[ "$ready" -eq 1 ]] && [[ "$served" -eq 1 ]]; then
  ask 200 gateway GET "$base/.well-known/jwks.json"
  ask 200 gateway GET "$base/"
  ask 401 gateway OPTIONS "$base/"
  ask 401 gateway POST "$base/v1/query/aql" -H 'Content-Type: application/json' -d '{"q": "SELECT e/ehr_id/value FROM EHR e"}'
  ask 401 gateway POST "$base/pmir/feed" -H 'Content-Type: application/fhir+json' -d '{}'
  ask 403 proxy GET "$base/health/dependencies"
  ask 403 proxy GET "$base/health"
  ask 404 proxy GET /health
  ask 404 proxy GET /.well-known/jwks.json

  # A query string carrying a sentinel in place of a patient identifier
  # never reaches the proxy's access log; the gateway's request id does.
  sentinel="ffd-guide-sentinel-$$"
  id="$(docker exec "$proxy" curl -sk -o /dev/null -D - \
    --resolve gateway.example.org:443:127.0.0.1 \
    "https://gateway.example.org$base/v1/ehr?subject_id=$sentinel&subject_namespace=urn:oid:2.999.1" 2> /dev/null |
    tr -d '\r' | sed -nE 's/^[Xx]-[Rr]equest-[Ii]d: (.*)$/\1/p')" || true
  sleep 1
  log="$(docker exec "$proxy" cat /var/log/nginx/ferrofed-access.log 2> /dev/null)" || log=""
  if grep -qF "$sentinel" <<< "$log"; then
    bad "the proxy's access log records a query string"
  elif [[ -z "$id" ]] || ! grep -qF "request_id=$id" <<< "$log"; then
    bad "the proxy's access log does not name the gateway's request id '$id'"
  else
    echo "OK: the proxy's access log names the gateway's request id and no query string"
  fi
fi

if [[ "$fail" -ne 0 ]]; then
  echo "production guide: FAILED" >&2
  exit 1
fi
echo "production guide: OK"
