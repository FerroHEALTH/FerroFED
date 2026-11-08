#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The performance gate of the federation gateway (#1687, #1731):
# ferrohealth-perf (FerroHEALTH/FerroSYS#249) in its A/B mode, the head of a
# change judged against its base measured beside it on the same runner, each
# the release build of ferrofed over the quickstart's four FerroEHR nodes and
# FerroPIX (compose.yaml, at their pins), with a database of its own in one
# PostgreSQL server for FerroFED's store, at the "FerroPIX database image"
# pin of docs/VERSIONS.md.
#
#   scripts/perf/gate.sh build [BASE]
#       Builds the release gateway of the checkout, the head, and of its
#       base: the merge base of HEAD with BASE, a commit or ref, or with no
#       BASE the newest v* tag before HEAD, else HEAD's first parent. The
#       base is built from `git archive` of that commit under target/, with
#       its own toolchain pin, into the same target directory, so the
#       dependencies build once.
#   scripts/perf/gate.sh run
#       Starts the quickstart's nodes and FerroPIX, seeds them, starts the
#       store's PostgreSQL over TLS and both gateways, the head on 8080 with
#       perf/ferrofed.toml and the base on 8070 with the base's own
#       perf/ferrofed.toml, renders perf/plan.toml with the quickstart's
#       development token, and runs `ferrohealth-perf ab`: the head's idle
#       phase, then pairs of load phases in turn. Then it plants a poller of
#       the head's store that never waits and runs the idle phase of
#       perf/busy-loop.toml beside it, which must fail on idle transactions.
#       Exit 0 when both hold, 1 when the gate fails or the planted poller
#       passes, 2 when the run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the pin reader, the plan rendering, the registry rewrite, the
#       base resolution and the agreement of the two plans' idle limits
#       offline.
#
# Needs Docker with Compose, curl, git, openssl and ferrohealth-perf on PATH,
# on Linux, where the tool reads the servers' CPU and memory from /proc. No
# specification governs this: our own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly PLAN=perf/plan.toml
readonly BUSY_PLAN=perf/busy-loop.toml
readonly CONFIG=perf/ferrofed.toml
readonly OVERRIDE=perf/compose.yaml
readonly WORK=target/perf-run
readonly BASE_TREE=target/perf-base
readonly HEAD_URL=http://127.0.0.1:8080
readonly BASE_URL=http://127.0.0.1:8070
readonly STORE_NAME=ferrofed-perf-store
readonly STORE_PORT=5433
readonly STORE_PASSWORD=perf
readonly SERVICES=(ferroehr-a ferroehr-b ferroehr-c ferroehr-d ferropix)
# admin_url DB: the superuser on DB, which the gate counts transactions as.
admin_url() { printf 'postgresql://postgres:%s@127.0.0.1:%s/%s?sslmode=disable' "$STORE_PASSWORD" "$STORE_PORT" "$1"; }

# A poll of the emergency count of the store (app/ferrofed-server/src/store/
# emergency.rs) with no wait between two reads.
readonly POLL='SELECT count(*) FROM ferrofed.emergency_access;'

die() {
  echo "perf-gate: $*" >&2
  exit 2
}

# pin ITEM [MATRIX]: the pin of the docs/VERSIONS.md row whose first cell is
# ITEM, backticks off.
pin() {
  local item="$1" matrix="${2:-$root/docs/VERSIONS.md}"
  awk -F'|' -v item="$item" '{ gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2) }
    $2 == item { v = $3; gsub(/[[:space:]`]/, "", v); print v; exit }' "$matrix"
}

# render PLAN OUT TOKEN: PLAN with its token placeholder replaced, refused
# when a placeholder is left.
render() {
  local text
  text="$(< "$1")"
  text="${text//\{token\}/$3}"
  if grep -qE '\{[a-z_]*token\}' <<< "$text"; then
    echo "perf-gate: $1 holds a placeholder the gate does not fill" >&2
    return 1
  fi
  printf '%s\n' "$text" > "$2"
}

# registry DOCUMENT: the quickstart's registry DOCUMENT with each node's
# compose host and port replaced by the loopback port compose publishes it on.
registry() {
  sed -E \
    -e 's|http://ferroehr-a:8080/|http://127.0.0.1:8081/|' \
    -e 's|http://ferroehr-b:8080/|http://127.0.0.1:8082/|' \
    -e 's|http://ferroehr-c:8080/|http://127.0.0.1:8083/|' \
    -e 's|http://ferroehr-d:8080/|http://127.0.0.1:8084/|' "$1"
}

# idle_table PLAN: the [idle] table of PLAN, comments and blank lines off.
idle_table() {
  awk '/^\[/ { inside = ($0 == "[idle]") ; next }
       inside && !/^[[:space:]]*(#|$)/ { print }' "$1"
}

wait_for() {
  local what="$1" tries="$2"
  shift 2
  for _ in $(seq 1 "$tries"); do
    "$@" > /dev/null 2>&1 && return 0
    sleep 1
  done
  die "$what did not come up"
}

# base_rev [BASE] [AT]: the commit the base is built from, as `build` names
# it, resolved from AT (HEAD by default).
base_rev() {
  local base="${1:-}" at="${2:-HEAD}" tag
  # The commit before the push that created a branch is all zeros.
  [[ "$base" =~ ^0+$ ]] && base=""
  if [[ -n "$base" ]]; then
    git -C "$root" merge-base "$at" "$base"
  elif tag="$(git -C "$root" describe --tags --abbrev=0 --match 'v*' "$at^" 2> /dev/null)"; then
    git -C "$root" rev-parse "$tag^{commit}"
  else
    git -C "$root" rev-parse "$at^1"
  fi
}

# build [BASE]: the base's release gateway into target/perf-base/ferrofed,
# then the head's into target/release/ferrofed.
build() {
  local rev
  cd "$root"
  rev="$(base_rev "${1:-}")" || die "the base of ${1:-the previous release} cannot be resolved"
  echo "perf-gate: the base is $rev"
  rm -rf "$BASE_TREE"
  mkdir -p "$BASE_TREE/src"
  git archive "$rev" | tar -x -C "$BASE_TREE/src"
  printf '%s\n' "$rev" > "$BASE_TREE/rev"
  # From inside the base's tree, so its own toolchain pin builds it.
  (cd "$BASE_TREE/src" && cargo build --release --locked -p ferrofed-server --bin ferrofed --target-dir "$root/target")
  cp target/release/ferrofed "$BASE_TREE/ferrofed"
  cargo build --release --locked -p ferrofed-server --bin ferrofed
}

compose() {
  FERROFED__USAGE_REPORT__ENABLED=false docker compose -f compose.yaml -f "$OVERRIDE" "$@"
}

head_pid=""
base_pid=""
poller_pid=""
cleanup() {
  [[ -n "$poller_pid" ]] && kill "$poller_pid" 2> /dev/null || true
  local pid
  for pid in "$head_pid" "$base_pid"; do
    [[ -n "$pid" ]] && kill "$pid" 2> /dev/null || true
    [[ -n "$pid" ]] && wait "$pid" 2> /dev/null || true
  done
  docker rm --force "$STORE_NAME" > /dev/null 2>&1 || true
  compose down --volumes > /dev/null 2>&1 || true
}

# store_tls: a CA and the store server's certificate for 127.0.0.1, under
# $WORK/tls, the CA also at $WORK/store-ca.pem.
store_tls() {
  local tls="$WORK/tls"
  mkdir -p "$tls"
  openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=ferrofed perf CA" \
    -keyout "$tls/ca.key" -out "$tls/ca.crt" 2> /dev/null
  openssl req -newkey rsa:2048 -nodes -subj "/CN=127.0.0.1" \
    -keyout "$tls/server.key" -out "$tls/server.csr" 2> /dev/null
  printf 'subjectAltName = IP:127.0.0.1, DNS:localhost\n' > "$tls/ext.cnf"
  openssl x509 -req -in "$tls/server.csr" -CA "$tls/ca.crt" -CAkey "$tls/ca.key" \
    -CAcreateserial -days 1 -extfile "$tls/ext.cnf" -out "$tls/server.crt" 2> /dev/null
  # The image's entrypoint sources this after the database is made, and the
  # server it restarts serves TLS (https://www.postgresql.org/docs/18/ssl-tcp.html).
  cat > "$tls/30-tls.sh" << 'EOF'
install -m 0600 /perf-tls/server.key "$PGDATA/server.key"
install -m 0644 /perf-tls/server.crt "$PGDATA/server.crt"
printf "ssl = on\nssl_cert_file = 'server.crt'\nssl_key_file = 'server.key'\n" >> "$PGDATA/postgresql.conf"
EOF
  chmod 0644 "$tls"/*
  cp "$tls/ca.crt" "$WORK/store-ca.pem"
}

# serve NAME BINARY CONFIG DB PORT: starts one gateway in the background,
# its store URL under $WORK/NAME, and prints its pid.
serve() {
  local binary="$2" config="$3" db="$4" port="$5" dir="$WORK/$1"
  mkdir -p "$dir"
  chmod 0700 "$dir"
  printf 'postgres://ferrofed:%s@127.0.0.1:%s/%s?sslmode=require' \
    "$STORE_PASSWORD" "$STORE_PORT" "$db" > "$dir/store-url"
  FERROFED__USAGE_REPORT__ENABLED=false FERROFED_CONFIG="$config" \
    FERROFED__SERVER__LISTEN="127.0.0.1:$port" \
    FERROFED__SIGNING__JWKS_URI="http://127.0.0.1:$port/.well-known/jwks.json" \
    FERROFED__STORE__URL_FILE="$dir/store-url" \
    "$binary" serve > "$dir/server.log" 2>&1 &
  echo "$!"
}

start() {
  local store_image base_config=$CONFIG db
  store_image="$(pin "FerroPIX database image")"
  [[ -n "$store_image" ]] || die "docs/VERSIONS.md pins no FerroPIX database image"
  [[ -x target/release/ferrofed ]] || die "target/release/ferrofed is not built"
  [[ -x "$BASE_TREE/ferrofed" ]] || die "$BASE_TREE/ferrofed is not built: run gate.sh build first"
  [[ -f "$BASE_TREE/src/$CONFIG" ]] && base_config="$BASE_TREE/src/$CONFIG"
  rm -rf "$WORK"
  mkdir -p "$WORK"
  chmod 0700 "$WORK"

  bash scripts/quickstart/signing-key.sh > /dev/null
  compose up --detach --wait "${SERVICES[@]}" > "$WORK/compose.log" 2>&1 ||
    die "the quickstart's nodes and FerroPIX did not come up: $(tail -n 20 "$WORK/compose.log")"
  compose run --rm ferropix-seed > "$WORK/seed-ferropix.log" 2>&1 ||
    die "FerroPIX could not be seeded: $(tail -n 20 "$WORK/seed-ferropix.log")"
  bash scripts/quickstart/seed.sh > "$WORK/seed-nodes.log" 2>&1 ||
    die "the nodes could not be seeded: $(tail -n 20 "$WORK/seed-nodes.log")"
  registry docker/quickstart/registry.toml > "$WORK/registry.toml"

  store_tls
  docker run --detach --name "$STORE_NAME" --publish "127.0.0.1:${STORE_PORT}:5432" \
    --env POSTGRES_PASSWORD="$STORE_PASSWORD" --env POSTGRES_DB=ferrofed \
    --volume "$root/$WORK/tls:/perf-tls:ro" \
    --volume "$root/$WORK/tls/30-tls.sh:/docker-entrypoint-initdb.d/30-tls.sh:ro" \
    "$store_image" > /dev/null
  wait_for "the store's PostgreSQL" 60 docker exec "$STORE_NAME" \
    psql --username postgres --dbname ferrofed --host 127.0.0.1 --command 'SHOW ssl' --tuples-only
  # A role of its own with CREATE on each database, as the store page asks;
  # the gate counts transactions as the superuser. Each gateway has a
  # database of its own, so each arm counts its own transactions.
  docker exec "$STORE_NAME" psql --quiet -v ON_ERROR_STOP=1 --username postgres --dbname ferrofed \
    --command "CREATE ROLE ferrofed LOGIN PASSWORD '${STORE_PASSWORD}'" \
    --command "CREATE DATABASE ferrofed_base" > /dev/null
  for db in ferrofed ferrofed_base; do
    docker exec "$STORE_NAME" psql --quiet -v ON_ERROR_STOP=1 --username postgres --dbname ferrofed \
      --command "GRANT CREATE ON DATABASE ${db} TO ferrofed" > /dev/null
  done

  head_pid="$(serve head target/release/ferrofed "$CONFIG" ferrofed 8080)"
  base_pid="$(serve base "$BASE_TREE/ferrofed" "$base_config" ferrofed_base 8070)"
  wait_for "the head gateway" 60 curl --fail --silent "$HEAD_URL/health/readiness"
  wait_for "the base gateway" 60 curl --fail --silent "$BASE_URL/health/readiness"
}

# busy_loop: the planted poller must fail the idle phase on its
# transactions.
busy_loop() {
  local code=0 summary="$WORK/busy-loop.md"
  yes "$POLL" | docker exec --interactive --env PGAPPNAME=perf-busy-loop "$STORE_NAME" \
    psql --quiet --no-psqlrc --username ferrofed --dbname ferrofed > /dev/null 2>&1 &
  poller_pid=$!
  GITHUB_STEP_SUMMARY='' ferrohealth-perf run --plan "$BUSY_PLAN" --pid "$head_pid" \
    --base-url "$HEAD_URL" --database-url "$(admin_url ferrofed)" --baseline "$WORK/no-baseline.toml" \
    --out-dir target/perf/busy-loop --summary "$summary" > /dev/null || code=$?
  kill "$poller_pid" 2> /dev/null || true
  poller_pid=""
  docker exec "$STORE_NAME" psql --quiet --username postgres --dbname ferrofed --command \
    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = 'perf-busy-loop'" > /dev/null
  local breaches
  breaches="$(awk '/^### Breaches/ { on = 1; next } /^### / { on = 0 } on' "$summary")"
  if [[ "$code" -eq 1 ]] && grep -qF 'idle.transactions_per_second' <<< "$breaches"; then
    echo "perf-gate: the planted store poller fails the idle phase on its transactions"
    report_busy_loop "failed the gate, as it must" "$summary"
    return 0
  fi
  echo "perf-gate: the planted store poller was not failed on idle transactions (exit $code)" >&2
  cat "$summary" >&2
  report_busy_loop "was NOT failed on idle transactions (exit $code)" "$summary"
  return 1
}

report_busy_loop() {
  [[ -n "${GITHUB_STEP_SUMMARY:-}" ]] || return 0
  {
    printf '## The planted idle busy-loop\n\nA store poller that never waits %s.\n\n' "$1"
    awk '/^### Breaches/ { on = 1 } /^### Checks/ { on = 0 } on' "$2"
  } >> "$GITHUB_STEP_SUMMARY"
}

run() {
  [[ $# -eq 0 ]] || die "usage: gate.sh run"
  cd "$root"
  command -v ferrohealth-perf > /dev/null || die "ferrohealth-perf is not on PATH"
  trap cleanup EXIT
  start
  local token code=0 server
  token="$(bash scripts/quickstart/token.sh)" || die "the development token could not be minted"
  render "$PLAN" "$WORK/plan.toml" "$token" || exit 2
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf "The base is \`%s\`.\n\n" "$(< "$BASE_TREE/rev")" >> "$GITHUB_STEP_SUMMARY"
  fi
  ferrohealth-perf ab --plan "$WORK/plan.toml" \
    --base-url "$BASE_URL" --base-pid "$base_pid" --base-database-url "$(admin_url ferrofed_base)" \
    --head-url "$HEAD_URL" --head-pid "$head_pid" --head-database-url "$(admin_url ferrofed)" \
    --out-dir target/perf || code=$?
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    for server in head base; do
      echo "perf-gate: the $server gateway's log follows" >&2
      tail -n 50 "$WORK/$server/server.log" >&2
    done
  fi
  exit "$code"
}

self_test() {
  local work n=0 failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now
  trap "rm -r -- '$work'" EXIT
  check() {
    n=$((n + 1))
    if [[ "$2" != "$3" ]]; then
      echo "  FAIL: $1: expected [$2], got [$3]" >&2
      failed=1
    fi
  }
  # shellcheck disable=SC2016 # the backticks are the row's, never expanded
  printf '| Item | Pin | Repeated in |\n|---|---|---|\n| FerroPIX database image | `postgres:1@sha256:00` | x |\n' > "$work/matrix.md"
  check "a pin is read without its backticks" "postgres:1@sha256:00" "$(pin "FerroPIX database image" "$work/matrix.md")"
  check "a missing row reads empty" "" "$(pin "Nothing" "$work/matrix.md")"
  check "the store's image is pinned" 1 "$([[ -n "$(pin "FerroPIX database image")" ]] && echo 1 || echo 0)"
  printf 'a = "Bearer {token}"\n' > "$work/plan.toml"
  render "$work/plan.toml" "$work/out.toml" t1
  check "the token is filled" 'a = "Bearer t1"' "$(< "$work/out.toml")"
  printf 'a = "Bearer {other_token}"\n' > "$work/plan.toml"
  check "a placeholder left is refused" 1 "$(render "$work/plan.toml" "$work/out.toml" t 2> /dev/null && echo 0 || echo 1)"
  printf 'url = "http://ferroehr-c:8080/ferroehr/rest/openehr"\n' > "$work/registry.toml"
  check "a node's compose address becomes its loopback port" \
    'url = "http://127.0.0.1:8083/ferroehr/rest/openehr"' "$(registry "$work/registry.toml")"
  check "every quickstart node is reached on loopback" 0 \
    "$(registry "$root/docker/quickstart/registry.toml" | grep -c 'http://ferroehr-' || true)"
  printf '[idle]\n# a note\nmax = 1.0\n\n[load]\nrate = 2.0\n' > "$work/plan.toml"
  check "the idle table is read alone" "max = 1.0" "$(idle_table "$work/plan.toml")"
  check "the gate and the busy-loop plan hold the same idle limits" \
    "$(idle_table "$root/$PLAN")" "$(idle_table "$root/$BUSY_PLAN")"
  [[ -n "$(idle_table "$root/$PLAN")" ]] || {
    echo "  FAIL: $PLAN holds no [idle] table" >&2
    failed=1
  }
  local repo="$work/repo" saved_root="$root"
  git init --quiet "$repo"
  commit() { git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false -c tag.gpgsign=false commit --quiet --allow-empty -m "$1"; }
  commit one
  commit two
  git -C "$repo" -c tag.gpgsign=false tag v0.0.1
  commit three
  git -C "$repo" branch side
  commit four
  git -C "$repo" checkout --quiet side
  commit side-only
  git -C "$repo" checkout --quiet -
  root="$repo"
  check "with no BASE the base is the newest tag before HEAD" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev "")"
  check "a BASE names its merge base with HEAD" \
    "$(git -C "$repo" rev-parse HEAD~1)" "$(base_rev side)"
  check "an all-zero BASE takes the previous release" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev 0000000000000000000000000000000000000000)"
  check "with no tag before it the base is the first parent" \
    "$(git -C "$repo" rev-parse v0.0.1~1)" "$(base_rev "" v0.0.1)"
  root="$saved_root"
  [[ "$failed" -eq 0 ]] || exit 1
  echo "perf-gate: self-test OK ($n checks)."
}

case "${1:-}" in
  build)
    shift
    build "$@"
    ;;
  run)
    shift
    run "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh build [BASE] | run | --self-test" ;;
esac
