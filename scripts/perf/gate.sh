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
#       BASE the newest v* tag before HEAD (the oldest commit after it that
#       carries the gate contract when the tag carries none), else HEAD's
#       first parent. The base is built from `git archive` of that commit
#       under target/, with its own toolchain pin, into the same target
#       directory, so the dependencies build once, and its
#       perf/ferrofed.toml and perf/arm.env (or, from before that file, its
#       own gate) are kept beside it. target/perf-base/base.md names the
#       base and why.
#   scripts/perf/gate.sh pack FILE
#       Packs what `run` reads into the tar FILE: the head's release
#       gateway and token endpoint, the base's gateway, its rev, base.md and
#       configuration contract, and the ferrohealth-perf on PATH as
#       target/perf-tools/. The perf
#       workflow's build job writes it once for every attempt (#1807).
#   scripts/perf/gate.sh unpack FILE
#       Unpacks FILE into the checkout, so `run` measures with nothing built;
#       put target/perf-tools on PATH for the tool.
#   scripts/perf/gate.sh run
#       Starts the quickstart's nodes and FerroPIX, seeds them, starts the
#       store's PostgreSQL over TLS and both gateways, the head on 8080 and
#       the base on 8070, each under its own tree's configuration contract:
#       its perf/ferrofed.toml and the overrides of its perf/arm.env, or for
#       a tree from before that file the overrides its own gate set. A base
#       that refuses its own configuration stops the run with exit 2, naming
#       the refusal. It renders perf/plan.toml with the quickstart's
#       development token and the synthetic contact point's DPoP-bound
#       tokens, and runs `ferrohealth-perf ab`: the head's idle
#       phase, then pairs of load phases in turn. Then it plants a poller of
#       the head's store that never waits and runs the idle phase of
#       perf/busy-loop.toml beside it, which must fail on idle transactions.
#       Node A is reached through an onward lane at the test kit's token
#       endpoint (ferrofed-perf-token, #1712): after the A/B run the head
#       must answer ONWARD_REQUESTS requests to node A with at most
#       ONWARD_LIMIT_PERCENT % as many token exchanges, and a third gateway,
#       the head with the lane's `cache = false`, must fail that limit.
#       Exit 0 when all hold, 1 when the gate fails or a planted case
#       passes, 2 when the run cannot be judged or set up.
#   scripts/perf/gate.sh --self-test
#       Proves the pin reader, the plan rendering, the registry rewrite, the
#       base resolution, each arm's overrides from perf/arm.env and from an
#       older tree's gate, the agreement of the two plans' idle limits and
#       the contact point's keys and tokens offline.
#
# Needs Docker with Compose, curl, git, openssl and ferrohealth-perf on PATH,
# on Linux, where the tool reads the servers' CPU and memory from /proc. No
# specification governs this: our own design.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
readonly PLAN=perf/plan.toml
readonly BUSY_PLAN=perf/busy-loop.toml
readonly CONFIG=perf/ferrofed.toml
readonly ARM_ENV=perf/arm.env
readonly GATE=scripts/perf/gate.sh
readonly OVERRIDE=perf/compose.yaml
readonly WORK=target/perf-run
readonly BASE_TREE=target/perf-base
readonly TOOLS=target/perf-tools
readonly HEAD_URL=http://127.0.0.1:8080
readonly BASE_URL=http://127.0.0.1:8070
readonly STORE_NAME=ferrofed-perf-store
readonly STORE_PORT=5433
readonly STORE_PASSWORD=perf
readonly SERVICES=(ferroehr-a ferroehr-b ferroehr-c ferroehr-d ferropix)
# The onward lane of node A (perf/ferrofed.toml, perf/ferroehr-onward.toml):
# the token endpoint's address, the key sets it writes for node A, the
# client key it writes for the gateway's private_key_jwt, and the third
# gateway that runs the lane uncached.
readonly ONWARD_URL=http://127.0.0.1:8095
readonly ONWARD_DIR=$WORK/onward
readonly ONWARD_KEY=$WORK/onward-client.pem
readonly ONWARD_LANE='[credentials."node-a-query".onward]'
readonly UNCACHED_CONFIG=$WORK/ferrofed-uncached.toml
readonly UNCACHED_URL=http://127.0.0.1:8060
readonly ONWARD_REQUESTS=200
readonly ONWARD_LIMIT_PERCENT=5
readonly QUICKSTART_ISSUER=https://quickstart-issuer.ferrofed.invalid
# The synthetic national contact point of perf/ferrofed.toml, its constants
# those of tools/ferrofed-testkit/src/national_connector.rs. NCP_DIR holds its
# signing key and the DPoP key its tokens are bound to, which the plan's
# dpop_key_file names.
readonly NCP_ISSUER=https://perf-ncp.ferrofed.invalid
readonly NCP_AUDIENCE=ferrofed-perf
readonly NCP_DIR=$WORK/ncp
readonly NCP_JWKS=$WORK/ncp-jwks.json
readonly NCP_CLIENT=synthetic-national-connector
# The TRC patient (eHDSI SAML Profile 10.0.0 §4.3), as the gateway reads it.
readonly NCP_PATIENT='urn:oid:2.999.1.1|ffd-test-0001'
# The eHDSI PurposeOfUse system (SAML Profile 10.0.0 §2.3) and HL7 ActReason.
readonly EHDSI_PURPOSE=urn:uuid:3bc18518-d305-46c2-a8d6-94bd59856e9e
readonly ACT_REASON=http://terminology.hl7.org/CodeSystem/v3-ActReason
# The plan's emergency access group, and the attempts the store's emergency
# count has written in a database: one insert or update of
# ferrofed.emergency_access per attempt (app/ferrofed-server/src/store/
# emergency.rs), read from PostgreSQL's cumulative statistics.
readonly EMERGENCY_GROUP=contact-point-emergency
readonly ATTEMPTS="SELECT coalesce(sum(n_tup_ins + n_tup_upd), 0) FROM pg_stat_user_tables WHERE schemaname = 'ferrofed' AND relname = 'emergency_access';"
readonly STATS_WAIT=30
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

# render PLAN OUT TOKEN [NCP_TOKEN NCP_EMERGENCY_TOKEN]: PLAN with its
# token placeholders replaced, refused when a placeholder is left.
render() {
  local text
  text="$(< "$1")"
  text="${text//\{token\}/$3}"
  [[ $# -lt 4 ]] || text="${text//\{ncp_token\}/$4}"
  [[ $# -lt 5 ]] || text="${text//\{ncp_emergency_token\}/$5}"
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

# b64url: standard input as unpadded base64url (RFC 7515 §2).
b64url() {
  openssl base64 -A | tr '+/' '-_' | tr -d '='
}

# unhex: the hexadecimal text on standard input as bytes.
unhex() {
  local hex i
  hex="$(tr -d '\n')"
  for ((i = 0; i < ${#hex}; i += 2)); do
    printf '%b' "\\x${hex:i:2}"
  done
}

# unb64url: base64url on standard input, padded or not, as bytes.
unb64url() {
  local text
  text="$(tr -d '\n' | tr -- '-_' '+/')"
  while ((${#text} % 4)); do text="$text="; done
  printf '%s' "$text" | openssl base64 -d -A
}

# ec_xy KEY: the base64url x and y of the P-256 key KEY, the last 64 bytes
# of its public key's DER encoding, the uncompressed point.
ec_xy() {
  local der
  der="$(mktemp)"
  openssl pkey -in "$1" -pubout -outform DER 2> /dev/null | tail -c 64 > "$der"
  printf '%s %s\n' "$(head -c 32 "$der" | b64url)" "$(tail -c 32 "$der" | b64url)"
  rm -f -- "$der"
}

# thumbprint X Y: the JWK SHA-256 thumbprint of the P-256 public key X, Y
# (RFC 7638 §3), the cnf.jkt of a token bound to it (RFC 9449 §6.1).
thumbprint() {
  printf '{"crv":"P-256","kty":"EC","x":"%s","y":"%s"}' "$1" "$2" |
    openssl dgst -sha256 -binary | b64url
}

# kid KEY: the key id of the RSA key KEY, from its modulus.
kid() {
  openssl rsa -in "$1" -noout -modulus 2> /dev/null | sed 's/^Modulus=//' |
    openssl dgst -sha256 | sed 's/^.*= *//' | cut -c1-16
}

# contact_point_keys DIR JWKS: the contact point's RSA signing key in DIR,
# its key set at JWKS, and the P-256 DPoP key in PKCS#8 PEM at
# DIR/dpop-key.pem, all made here so no key is ever committed.
contact_point_keys() {
  local dir="$1" n
  mkdir -p "$dir"
  chmod 0700 "$dir"
  openssl genrsa -out "$dir/signing-key.pem" 2048 2> /dev/null || return 1
  openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 \
    -pkeyopt ec_param_enc:named_curve -out "$dir/dpop-key.pem" 2> /dev/null || return 1
  n="$(openssl rsa -in "$dir/signing-key.pem" -noout -modulus 2> /dev/null |
    sed 's/^Modulus=//' | unhex | b64url)"
  printf '{"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"%s","n":"%s","e":"AQAB"}]}\n' \
    "$(kid "$dir/signing-key.pem")" "$n" > "$2"
}

# ncp_token DIR PURPOSE: a token of the contact point, signed RS256 with
# DIR/signing-key.pem and bound to DIR/dpop-key.pem by cnf.jkt (RFC 9449
# §6.1), relaying a professional for the eHDSI purpose of use PURPOSE with
# what a conforming identity assertion and TRC provide (eHDSI SAML Profile
# 10.0.0 §2.1, §2.3, §4), as national_connector::asserted lists it.
ncp_token() {
  local dir="$1" purpose="$2" now jti x y jkt header ehdsi payload signature
  now="$(date +%s)"
  jti="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
  read -r x y <<< "$(ec_xy "$dir/dpop-key.pem")"
  jkt="$(thumbprint "$x" "$y")"
  header="$(printf '{"alg":"RS256","typ":"at+jwt","kid":"%s"}' "$(kid "$dir/signing-key.pem")" | b64url)"
  ehdsi="$(printf '{"professional":{"hp_identifier":"urn:oid:2.999.9.71","name":"Qz9-ncp-subject-name","hp_professional_role":[{"system":"urn:oid:2.999.9","code":"Qz9-ncp-role"}],"authentication_method":"urn:oasis:names:tc:SAML:2.0:ac:classes:X509"},"provider":{"healthcare_provider_identifier":"urn:oid:2.999.9.72","healthcare_provider_name":"Qz9-ncp-hcp-name","facility_type":"Hospital","home_community_id":"urn:oid:2.999.9.73"},"point_of_care":"Qz9-ncp-point-of-care","purpose_of_use":[{"system":"%s","code":"%s"}],"assertion":{"issuer":"https://ncp-idp.example.test","id":"_Qz9-ncp-ida-1","name_id_format":"urn:oasis:names:tc:SAML:1.1:nameid-format:unspecified","not_before":%s,"not_on_or_after":%s},"trc":{"id":"_Qz9-ncp-trc-1","not_before":%s,"not_on_or_after":%s},"patient":{"id":"%s"}}' \
    "$EHDSI_PURPOSE" "$purpose" "$((now - 60))" "$((now + 3600))" "$((now - 60))" "$((now + 3600))" "$NCP_PATIENT")"
  payload="$(printf '{"iss":"%s","sub":"%s","aud":"%s","client_id":"%s","iat":%s,"exp":%s,"jti":"%s","scope":"user/aql-*.cruds user/composition-*.cruds user/template-*.cruds","extensions":{"ihe_iua":{"subject_organization_id":"urn:oid:2.999.7","purpose_of_use":[{"system":"%s","code":"TREAT"}]}},"cnf":{"jkt":"%s"},"ehdsi":%s}' \
    "$NCP_ISSUER" "$NCP_CLIENT" "$NCP_AUDIENCE" "$NCP_CLIENT" "$now" "$((now + 3600))" "$jti" "$ACT_REASON" "$jkt" "$ehdsi" | b64url)"
  signature="$(printf '%s.%s' "$header" "$payload" |
    openssl dgst -sha256 -sign "$dir/signing-key.pem" -binary | b64url)" || return 1
  printf '%s.%s.%s\n' "$header" "$payload" "$signature"
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
# it, resolved from AT (HEAD by default), with the reason on stderr. A
# previous release that carries no gate contract gives way to the oldest
# commit after it that does, the nearest point both arms boot from.
base_rev() {
  local base="${1:-}" at="${2:-HEAD}" tag tagged rev
  # The commit before the push that created a branch is all zeros.
  [[ "$base" =~ ^0+$ ]] && base=""
  if [[ -n "$base" ]]; then
    git -C "$root" merge-base "$at" "$base"
    echo "the merge base of $at with $base" >&2
  elif tag="$(git -C "$root" describe --tags --abbrev=0 --match 'v*' "$at^" 2> /dev/null)"; then
    tagged="$(git -C "$root" rev-parse "$tag^{commit}")"
    if has_contract "$tagged"; then
      echo "$tagged"
      echo "the previous release $tag" >&2
      return 0
    fi
    while IFS= read -r rev; do
      if has_contract "$rev"; then
        echo "$rev"
        echo "the oldest commit after $tag that carries the gate contract, since $tag carries neither $ARM_ENV nor a $GATE whose gateway_env() sets a gateway's overrides" >&2
        return 0
      fi
    done < <(git -C "$root" rev-list --reverse --first-parent --ancestry-path "$tagged..$at")
    echo "no commit after $tag up to $at carries the gate contract" >&2
    return 1
  else
    git -C "$root" rev-parse "$at^1"
    echo "the first parent of $at" >&2
  fi
}

# has_contract REV: REV carries the gate contract, a perf/ferrofed.toml with
# a perf/arm.env or with a gate whose gateway_env() sets the FERROFED__
# overrides a gateway's are derived from.
has_contract() {
  git -C "$root" cat-file -e "$1:$CONFIG" 2> /dev/null || return 1
  git -C "$root" cat-file -e "$1:$ARM_ENV" 2> /dev/null && return 0
  git -C "$root" show "$1:$GATE" 2> /dev/null | block gateway_env | grep -qE 'FERROFED__[A-Z0-9_]+='
}

# block NAME: the function NAME of the script on stdin.
block() {
  awk -v head="$1() {" 'index($0, head) == 1 { on = 1 } on { print } on && /^\}/ { exit }'
}

# build_base REV: the release gateway of REV into target/perf-base/ferrofed,
# REV into target/perf-base/rev, and REV's configuration contract beside
# them: its perf/ferrofed.toml as ferrofed.toml, and its perf/arm.env as
# arm.env or, in a tree from before that file, its gate as gate.sh.
build_base() {
  local rev="$1" src="$BASE_TREE/src"
  echo "perf-gate: building the base at $rev"
  rm -rf "$src"
  mkdir -p "$src"
  git archive "$rev" | tar -x -C "$src"
  printf '%s\n' "$rev" > "$BASE_TREE/rev"
  [[ -f "$src/$CONFIG" ]] || die "the base at $rev holds no $CONFIG"
  cp "$src/$CONFIG" "$BASE_TREE/ferrofed.toml"
  if [[ -f "$src/$ARM_ENV" ]]; then
    cp "$src/$ARM_ENV" "$BASE_TREE/arm.env"
  else
    [[ -f "$src/$GATE" ]] || die "the base at $rev holds neither $ARM_ENV nor $GATE"
    cp "$src/$GATE" "$BASE_TREE/gate.sh"
  fi
  # From inside the base's tree, so its own toolchain pin builds it.
  (cd "$src" && cargo build --release --locked -p ferrofed-server --bin ferrofed --target-dir "$root/target")
  cp target/release/ferrofed "$BASE_TREE/ferrofed"
}

# build [BASE]: the base's release gateway and configuration contract into
# target/perf-base/, the record of the base into target/perf-base/base.md,
# then the head's into target/release/ferrofed.
build() {
  local rev why
  cd "$root"
  why="$(mktemp)"
  rev="$(base_rev "${1:-}" 2> "$why")" || die "the base of ${1:-the previous release} cannot be resolved: $(< "$why")"
  rm -rf "$BASE_TREE"
  build_base "$rev"
  # shellcheck disable=SC2016 # the backticks are Markdown's, never expanded
  printf 'The base is `%s`, %s.\n' "$rev" "$(< "$why")" > "$BASE_TREE/base.md"
  rm -f -- "$why"
  echo "perf-gate: $(< "$BASE_TREE/base.md")"
  cargo build --release --locked -p ferrofed-server --bin ferrofed
  cargo build --release --locked -p ferrofed-testkit --bin ferrofed-perf-token
}

# contract: the base's configuration contract `build` kept, one path a line.
contract() {
  local file
  for file in ferrofed.toml arm.env gate.sh; do
    [[ -f "$BASE_TREE/$file" ]] && echo "$BASE_TREE/$file"
  done
  return 0
}

# pack FILE: what `run` reads, built by `build`, into the tar FILE.
pack() {
  [[ $# -eq 1 ]] || die "usage: gate.sh pack FILE"
  local file="$1" tool kept=()
  cd "$root"
  [[ -x target/release/ferrofed && -x target/release/ferrofed-perf-token ]] ||
    die "the head is not built: run gate.sh build first"
  [[ -x "$BASE_TREE/ferrofed" && -s "$BASE_TREE/rev" && -f "$BASE_TREE/base.md" && -f "$BASE_TREE/ferrofed.toml" ]] ||
    die "the base is not built: run gate.sh build first"
  [[ -f "$BASE_TREE/arm.env" || -f "$BASE_TREE/gate.sh" ]] || die "the base's overrides are not kept: run gate.sh build first"
  tool="$(command -v ferrohealth-perf)" || die "ferrohealth-perf is not on PATH"
  mkdir -p "$TOOLS"
  [[ "$tool" -ef "$TOOLS/ferrohealth-perf" ]] || cp "$tool" "$TOOLS/ferrohealth-perf"
  while IFS= read -r file; do kept+=("$file"); done < <(contract)
  tar -cf "$1" target/release/ferrofed target/release/ferrofed-perf-token \
    "$BASE_TREE/ferrofed" "$BASE_TREE/rev" "$BASE_TREE/base.md" ${kept[@]+"${kept[@]}"} "$TOOLS/ferrohealth-perf"
}

# unpack FILE: the tar `pack` wrote, into the checkout.
unpack() {
  [[ $# -eq 1 ]] || die "usage: gate.sh unpack FILE"
  local file
  file="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
  cd "$root"
  tar -xf "$file"
  [[ -x target/release/ferrofed && -x "$BASE_TREE/ferrofed" && -s "$BASE_TREE/rev" && -f "$BASE_TREE/ferrofed.toml" ]] ||
    die "$file holds no built arms"
}

compose() {
  FERROFED__USAGE_REPORT__ENABLED=false docker compose -f compose.yaml -f "$OVERRIDE" "$@"
}

head_pid=""
base_pid=""
poller_pid=""
token_pid=""
uncached_pid=""
cleanup() {
  [[ -n "$poller_pid" ]] && kill "$poller_pid" 2> /dev/null || true
  local pid
  for pid in "$head_pid" "$base_pid" "$uncached_pid" "$token_pid"; do
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

# fill NAME PORT FILE: the assignments of FILE, a tree's perf/arm.env, with
# the values of the gateway NAME on PORT in its placeholders: @PORT@ and
# @DIR@, the gateway's work directory. Exit 2 on a line that sets no
# FERROFED__ variable or names another placeholder.
fill() {
  local name="$1" port="$2" file="$3" line var value
  while IFS= read -r line || [[ -n "$line" ]]; do
    [[ "$line" =~ ^[[:space:]]*(#|$) ]] && continue
    [[ "$line" =~ ^(FERROFED__[A-Z0-9_]+)=(.*)$ ]] || die "$file: not a FERROFED__ assignment: $line"
    var="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"
    value="${value//@PORT@/$port}"
    value="${value//@DIR@/$WORK/$name}"
    if [[ "$value" =~ @[A-Z][A-Z0-9_]*@ ]]; then
      die "$file: $var names the unknown placeholder ${BASH_REMATCH[0]}"
    fi
    printf '%s=%s\n' "$var" "$value"
  done < "$file"
}

# derive NAME CONFIG DB PORT GATE: the FERROFED__ overrides the gateway_env()
# of GATE, a tree's own copy of this script from before perf/arm.env, gave
# the gateway NAME, evaluated with GATE's own own_urls() and the scalar
# constants of GATE this script does not define. Exit 2 when GATE has no
# gateway_env() or it sets no FERROFED__ variable.
derive() {
  local gate="$5" body out line constant
  body="$(block gateway_env < "$gate")"
  [[ -n "$body" ]] || die "$gate: it has no gateway_env()"
  out="$(
    while IFS= read -r line; do
      [[ "$line" =~ ^readonly[[:space:]]+([A-Z_][A-Z0-9_]*)=([^\(].*)$ ]] || continue
      constant="${BASH_REMATCH[1]}"
      [[ -z "${!constant+set}" ]] || continue
      eval "$constant=${BASH_REMATCH[2]}" || exit 2
    done < "$gate"
    eval "$(block own_urls < "$gate")" || exit 2
    eval "$body" || exit 2
    gateway_env "$1" "$2" "$3" "$4"
  )" || die "$gate: its gateway_env() cannot be evaluated"
  out="$(grep -E '^FERROFED__[A-Z0-9_]+=' <<< "$out" || true)"
  [[ -n "$out" ]] || die "$gate: its gateway_env() sets no FERROFED__ variable"
  printf '%s\n' "$out"
}

# arm_env NAME CONFIG DB PORT SOURCE: the FERROFED__ overrides of the
# gateway NAME on PORT, one a line, as its tree defines them in SOURCE: a
# perf/arm.env, filled, or that tree's own gate from before that file.
arm_env() {
  case "$5" in
    *.env) fill "$1" "$4" "$5" ;;
    *) derive "$@" ;;
  esac
}

# base_source: the file the base's overrides come from, as `build` kept it.
base_source() {
  if [[ -f "$BASE_TREE/arm.env" ]]; then
    echo "$BASE_TREE/arm.env"
  elif [[ -f "$BASE_TREE/gate.sh" ]]; then
    echo "$BASE_TREE/gate.sh"
  else
    die "$BASE_TREE holds neither arm.env nor gate.sh: run gate.sh build first"
  fi
}

# gateway_env NAME CONFIG DB PORT SOURCE: the environment one gateway runs
# under, one VAR=value a line, its store URL written under $WORK/NAME and its
# overrides those its tree defines in SOURCE (arm_env).
gateway_env() {
  local dir="$WORK/$1" overrides
  mkdir -p "$dir"
  chmod 0700 "$dir"
  printf 'postgres://ferrofed:%s@127.0.0.1:%s/%s?sslmode=require' \
    "$STORE_PASSWORD" "$STORE_PORT" "$3" > "$dir/store-url"
  overrides="$(arm_env "$@")" || exit 2
  printf '%s\n' FERROFED__USAGE_REPORT__ENABLED=false "FERROFED_CONFIG=$2" "$overrides"
}

# serve NAME BINARY CONFIG DB PORT SOURCE: starts one gateway in the
# background under CONFIG and the overrides of SOURCE, and prints its pid.
serve() {
  local name="$1" binary="$2" vars=() var env
  env="$(gateway_env "$name" "$3" "$4" "$5" "$6")" || exit 2
  while IFS= read -r var; do vars+=("$var"); done <<< "$env"
  env ${vars[@]+"${vars[@]}"} "$binary" serve > "$WORK/$name/server.log" 2>&1 &
  echo "$!"
}

# unknown_keys TEXT: each key a configuration refusal in TEXT names as an
# unknown field, one a line.
unknown_keys() {
  # shellcheck disable=SC2016 # the backticks are the refusal's, never expanded
  grep -oE '`[^`]+` is not valid: unknown field' <<< "$1" | sed -E 's/^`([^`]+)`.*$/\1/' || true
}

# refusal BINARY CONFIG DB PORT SOURCE: the base gateway BINARY's refusal of
# CONFIG under the overrides of SOURCE; nothing, and exit 0, when it reads it.
refusal() {
  local binary="$1" vars=() var env out
  env="$(gateway_env base "$2" "$3" "$4" "$5")" || exit 2
  while IFS= read -r var; do vars+=("$var"); done <<< "$env"
  out="$(env ${vars[@]+"${vars[@]}"} "$binary" config check 2>&1)" && return 0
  printf '%s\n' "$out"
  return 1
}

# accepts BINARY CONFIG DB PORT SOURCE: whether the base gateway BINARY reads
# its own tree's CONFIG under its own overrides; exit 2 naming each key it
# does not know, or the refusal, so a base that cannot start is never judged.
accepts() {
  local binary="$1" refused keys
  refused="$(refusal "$@")" && return 0
  keys="$(unknown_keys "$refused" | paste -sd ' ' -)"
  [[ -n "$keys" ]] && die "the base at $(< "$BASE_TREE/rev") does not know the key(s) ${keys} of its own $CONFIG: the pair is not judged"
  die "the base at $(< "$BASE_TREE/rev") refuses its own $CONFIG: $refused"
}

# onward_start: the token endpoint of node A's onward lane in the
# background, its key sets under ONWARD_DIR for node A to mount, before the
# nodes start.
onward_start() {
  mkdir -p "$ONWARD_DIR"
  chmod 0755 "$ONWARD_DIR"
  target/release/ferrofed-perf-token --listen 127.0.0.1:8095 \
    --persons-issuer "$QUICKSTART_ISSUER" --persons-jwks "$ONWARD_DIR/persons.json" \
    --machines-issuer https://perf-onward.ferrofed.invalid/machines --machines-jwks "$ONWARD_DIR/machines.json" \
    --audience https://node-a.perf.ferrofed.invalid --client-id ferrofed-perf \
    --client-key "$ONWARD_KEY" --caller-issuer "$QUICKSTART_ISSUER" \
    --caller-jwks docker/quickstart/issuer/jwks.json --lifetime-s 3600 > "$WORK/onward.log" 2>&1 &
  token_pid=$!
  wait_for "the onward token endpoint" 30 curl --fail --silent "$ONWARD_URL/calls"
}

# exchanges: the token exchanges the onward token endpoint has answered.
exchanges() {
  curl --fail --silent "$ONWARD_URL/calls" | grep -oE '"token_exchange":[0-9]+' | cut -d: -f2
}

# onward_holds EXCHANGES REQUESTS: whether REQUESTS onward requests caused at
# most ONWARD_LIMIT_PERCENT % as many token exchanges, the cache answering
# the rest.
onward_holds() {
  (($1 * 100 <= $2 * ONWARD_LIMIT_PERCENT))
}

# uncached CONFIG: CONFIG with node A's onward lane uncached, refused when
# CONFIG has no such lane.
uncached() {
  grep -qxF "$ONWARD_LANE" "$1" || {
    echo "perf-gate: $1 has no $ONWARD_LANE" >&2
    return 1
  }
  awk -v lane="$ONWARD_LANE" '{ print } $0 == lane { print "cache = false" }' "$1"
}

# onward_probe URL TOKEN: the token exchanges ONWARD_REQUESTS definition
# reads routed to node A through the gateway at URL cause; exit 2 when one
# is answered other than 200.
onward_probe() {
  local before after code i
  before="$(exchanges)" || die "the onward token endpoint did not answer"
  for ((i = 0; i < ONWARD_REQUESTS; i++)); do
    code="$(curl --silent --output /dev/null --write-out '%{http_code}' \
      --header "authorization: Bearer $2" --header 'openEHR-federation-endpoint: node-a-query' \
      "$1/v1/definition/template/adl1.4")"
    [[ "$code" == 200 ]] || die "a definition read through node A's onward lane at $1 was answered $code"
  done
  after="$(exchanges)" || die "the onward token endpoint did not answer"
  echo "$((after - before))"
}

# onward TOKEN: the head's onward lane must hold the limit, and the planted
# uncached lane must fail it; 1 when either does not.
onward() {
  local head uncached_exchanges code=0
  head="$(onward_probe "$HEAD_URL" "$1")" || exit 2
  if onward_holds "$head" "$ONWARD_REQUESTS"; then
    echo "perf-gate: the head's onward lane caused $head token exchanges for $ONWARD_REQUESTS requests"
  else
    echo "perf-gate: the head's onward lane caused $head token exchanges for $ONWARD_REQUESTS requests, over ${ONWARD_LIMIT_PERCENT} %" >&2
    code=1
  fi
  uncached "$CONFIG" > "$UNCACHED_CONFIG" || exit 2
  uncached_pid="$(serve uncached target/release/ferrofed "$UNCACHED_CONFIG" ferrofed_uncached 8060 "$ARM_ENV")"
  wait_for "the uncached gateway" 60 curl --fail --silent "$UNCACHED_URL/health/readiness"
  uncached_exchanges="$(onward_probe "$UNCACHED_URL" "$1")" || exit 2
  if onward_holds "$uncached_exchanges" "$ONWARD_REQUESTS"; then
    echo "perf-gate: the planted uncached onward lane caused $uncached_exchanges token exchanges for $ONWARD_REQUESTS requests and was NOT failed" >&2
    code=1
  else
    echo "perf-gate: the planted uncached onward lane fails on its $uncached_exchanges token exchanges for $ONWARD_REQUESTS requests"
  fi
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '## The onward lane

%s definition reads through node A'"'"'s onward lane caused %s token exchanges at the head and %s at the planted uncached lane; the limit is %s %%.

' \
      "$ONWARD_REQUESTS" "$head" "$uncached_exchanges" "$ONWARD_LIMIT_PERCENT" >> "$GITHUB_STEP_SUMMARY"
  fi
  return "$code"
}

# answered REPORT GROUP: the requests of GROUP the head answered as expected
# over every pair of the A/B report REPORT.
answered() {
  jq --arg group "$2" '[.pairs[].head.groups[$group].answered // 0] | add // 0' "$1"
}

# counted ANSWERED: whether the head's store holds at least ANSWERED emergency
# attempts, read again for up to STATS_WAIT seconds, since a backend reports
# its statistics after its transaction ends.
counted() {
  local attempts
  for _ in $(seq 1 "$STATS_WAIT"); do
    attempts="$(docker exec "$STORE_NAME" psql --quiet --no-psqlrc --tuples-only --no-align \
      --username postgres --dbname ferrofed --command "$ATTEMPTS")" || return 1
    if ((attempts >= $1)); then
      echo "$attempts"
      return 0
    fi
    sleep 1
  done
  echo "$attempts"
  return 1
}

# emergency_counted REPORT: the head answered emergency accesses in the A/B
# report REPORT and its store counted each one; 1 when either fails.
emergency_counted() {
  local answered attempts
  answered="$(answered "$1" "$EMERGENCY_GROUP")" || {
    echo "perf-gate: $1 does not read as an A/B report" >&2
    return 1
  }
  if ((answered == 0)); then
    echo "perf-gate: the head answered no request of $EMERGENCY_GROUP as expected" >&2
    return 1
  fi
  if attempts="$(counted "$answered")"; then
    echo "perf-gate: the head's store counted $attempts emergency attempts for the $answered the head answered"
  else
    echo "perf-gate: the head's store counted ${attempts:-no} emergency attempts for the $answered the head answered" >&2
    return 1
  fi
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '## Emergency access\n\nThe head answered %s emergency accesses in the measured windows, and its store counted %s attempts.\n\n' \
      "$answered" "$attempts" >> "$GITHUB_STEP_SUMMARY"
  fi
}

start() {
  local store_image
  store_image="$(pin "FerroPIX database image")"
  [[ -n "$store_image" ]] || die "docs/VERSIONS.md pins no FerroPIX database image"
  [[ -x target/release/ferrofed ]] || die "target/release/ferrofed is not built"
  [[ -x "$BASE_TREE/ferrofed" ]] || die "$BASE_TREE/ferrofed is not built: run gate.sh build first"
  rm -rf "$WORK"
  mkdir -p "$WORK"
  chmod 0700 "$WORK"

  bash scripts/quickstart/signing-key.sh > /dev/null
  # The caller issuer's key set, which the token endpoint verifies against.
  bash scripts/quickstart/token.sh > /dev/null
  onward_start
  contact_point_keys "$NCP_DIR" "$NCP_JWKS" || die "the contact point's keys could not be made"
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
    --command "CREATE DATABASE ferrofed_base" \
    --command "CREATE DATABASE ferrofed_uncached" > /dev/null
  for db in ferrofed ferrofed_base ferrofed_uncached; do
    docker exec "$STORE_NAME" psql --quiet -v ON_ERROR_STOP=1 --username postgres --dbname ferrofed \
      --command "GRANT CREATE ON DATABASE ${db} TO ferrofed" > /dev/null
  done

  # NOTE: no specification governs this: our own design; each arm boots under its own tree's
  # configuration contract, so a pull request that renames a key reaches the head alone.
  local base_contract
  base_contract="$(base_source)" || exit 2
  accepts "$BASE_TREE/ferrofed" "$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "$base_contract"
  head_pid="$(serve head target/release/ferrofed "$CONFIG" ferrofed 8080 "$ARM_ENV")"
  base_pid="$(serve base "$BASE_TREE/ferrofed" "$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "$base_contract")"
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
  [[ -f "$BASE_TREE/rev" ]] || die "$BASE_TREE/rev is missing: run gate.sh build first"
  [[ -f "$BASE_TREE/base.md" ]] || printf "The base is \`%s\`.\n" "$(< "$BASE_TREE/rev")" > "$BASE_TREE/base.md"
  echo "perf-gate: $(< "$BASE_TREE/base.md")"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n\n' "$(< "$BASE_TREE/base.md")" >> "$GITHUB_STEP_SUMMARY"
  fi
  start
  local token ncp ncp_emergency code=0 server
  token="$(bash scripts/quickstart/token.sh)" || die "the development token could not be minted"
  ncp="$(ncp_token "$NCP_DIR" TREATMENT)" || die "the contact point's token could not be minted"
  ncp_emergency="$(ncp_token "$NCP_DIR" EMERGENCY)" || die "the contact point's emergency token could not be minted"
  render "$PLAN" "$WORK/plan.toml" "$token" "$ncp" "$ncp_emergency" || exit 2
  ferrohealth-perf ab --plan "$WORK/plan.toml" \
    --base-url "$BASE_URL" --base-pid "$base_pid" --base-database-url "$(admin_url ferrofed_base)" \
    --head-url "$HEAD_URL" --head-pid "$head_pid" --head-database-url "$(admin_url ferrofed)" \
    --out-dir target/perf || code=$?
  if [[ -f target/perf/latest.json ]]; then
    emergency_counted target/perf/latest.json || { [[ "$code" -ne 0 ]] || code=1; }
  fi
  busy_loop || { [[ "$code" -ne 0 ]] || code=1; }
  onward "$token" || { [[ "$code" -ne 0 ]] || code=1; }
  if [[ "$code" -ne 0 ]]; then
    for server in head base uncached; do
      echo "perf-gate: the $server gateway's log follows" >&2
      [[ -f "$WORK/$server/server.log" ]] && tail -n 50 "$WORK/$server/server.log" >&2
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
  mkdir -p "$repo/perf"
  : > "$repo/$CONFIG"
  printf 'FERROFED__A=1\n' > "$repo/$ARM_ENV"
  git -C "$repo" add "$CONFIG" "$ARM_ENV"
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
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev "" 2> /dev/null)"
  check "a previous release with the contract is the base, and says so" "the previous release v0.0.1" \
    "$(base_rev "" 2>&1 > /dev/null)"
  check "a BASE names its merge base with HEAD" \
    "$(git -C "$repo" rev-parse HEAD~1)" "$(base_rev side 2> /dev/null)"
  check "an all-zero BASE takes the previous release" \
    "$(git -C "$repo" rev-parse v0.0.1)" "$(base_rev 0000000000000000000000000000000000000000 2> /dev/null)"
  check "with no tag before it the base is the first parent" \
    "$(git -C "$repo" rev-parse v0.0.1~1)" "$(base_rev "" v0.0.1 2> /dev/null)"
  # A release lane whose previous tag predates the gate contract.
  local release="$work/release"
  git init --quiet "$release"
  rcommit() { git -C "$release" -c user.name=t -c user.email=t@t -c commit.gpgsign=false commit --quiet --allow-empty -m "$1"; }
  rcommit before-gate
  git -C "$release" -c tag.gpgsign=false tag v0.1.0
  rcommit still-before
  root="$release"
  check "a release with no contract after it leaves no base" 1 "$( (base_rev "") > /dev/null 2>&1; echo $?)"
  mkdir -p "$release/scripts/perf" "$release/perf"
  : > "$release/$CONFIG"
  printf 'serve() {\n  FERROFED__A=1 ferrofed serve\n}\n' > "$release/$GATE"
  git -C "$release" add "$CONFIG" "$GATE"
  rcommit one-arm-gate
  printf 'gateway_env() {\n  echo FERROFED__A=1\n}\n' > "$release/$GATE"
  git -C "$release" add "$GATE"
  rcommit ab-gate
  rcommit after-gate
  rcommit release-commit
  check "a previous release without the contract gives way to the oldest commit after it with one" \
    "$(git -C "$release" rev-parse HEAD~2)" "$(base_rev "" 2> /dev/null)"
  check "the base says why it is not the previous release" \
    "the oldest commit after v0.1.0 that carries the gate contract, since v0.1.0 carries neither $ARM_ENV nor a $GATE whose gateway_env() sets a gateway's overrides" \
    "$(base_rev "" 2>&1 > /dev/null)"
  root="$repo"
  # A stand-in for a base binary: `refuses` names two keys it does not know,
  # `misses` refuses a file and `reads` reads the configuration.
  local bin="$work/bin" lane="$work/lane" kind out code
  mkdir -p "$bin" "$lane/$BASE_TREE" "$lane/perf"
  : > "$lane/$BASE_TREE/ferrofed.toml"
  printf 'FERROFED__SERVER__LISTEN=127.0.0.1:@PORT@\n' > "$lane/$BASE_TREE/arm.env"
  for kind in refuses misses reads; do
    # shellcheck disable=SC2016 # the stand-in's own parameters, expanded when it runs
    printf '#!/usr/bin/env bash\n[[ "$1 $2" == "config check" ]] || exit 64\n' > "$bin/$kind"
  done
  # shellcheck disable=SC2016 # the backticks are the refusal's, never expanded
  printf '%s\n' 'echo "refused: 2 configuration refusals; \`emergency\` is not valid: unknown field \`emergency\`; \`auth.federation[0]\` is not valid: unknown field \`federation\`" >&2' 'exit 1' >> "$bin/refuses"
  printf '%s\n' 'echo "the signing key file is missing" >&2' 'exit 1' >> "$bin/misses"
  chmod +x "$bin"/*
  printf '%s\n' "$(git -C "$repo" rev-parse v0.0.1)" > "$lane/$BASE_TREE/rev"
  code=0
  out="$(cd "$lane" && accepts "$bin/refuses" "$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "$BASE_TREE/arm.env" 2>&1)" || code=$?
  check "a base refusing a key of its own configuration stops the run with exit 2" 2 "$code"
  check "the refusal names each key and the base" 1 \
    "$(grep -cF "the base at $(git -C "$repo" rev-parse v0.0.1) does not know the key(s) emergency auth.federation[0] of its own perf/ferrofed.toml" <<< "$out")"
  code=0
  out="$(cd "$lane" && accepts "$bin/misses" "$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "$BASE_TREE/arm.env" 2>&1)" || code=$?
  check "a base refusing for another reason stops the run with its refusal" "2 1" \
    "$code $(grep -cF "refuses its own perf/ferrofed.toml: the signing key file is missing" <<< "$out")"
  check "a base reading its own configuration is accepted" 0 \
    "$( (cd "$lane" && accepts "$bin/reads" "$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "$BASE_TREE/arm.env") > /dev/null 2>&1; echo $?)"
  check "the base's overrides come from its own arm.env" "$BASE_TREE/arm.env" "$(cd "$lane" && base_source)"
  # A tree from before perf/arm.env, its gate setting the old key shape.
  local old="$work/old"
  mkdir -p "$old"
  cat > "$old/gate.sh" <<'GATE'
readonly WORK=elsewhere
readonly OLD_ONLY=old-value
readonly OLD_LIST=(a b)
own_urls() {
  grep -q '^public_url = ' "$1" && echo "FERROFED__GATEWAY__PUBLIC_URL=http://127.0.0.1:$2"
  return 0
}
gateway_env() {
  local config="$2" db="$3" port="$4" dir="$WORK/$1"
  mkdir -p "$dir"
  own_urls "$config" "$port"
  printf '%s\n' FERROFED__USAGE_REPORT__ENABLED=false "FERROFED_CONFIG=$config" \
    "FERROFED__SERVER__LISTEN=127.0.0.1:$port" "FERROFED__OLD__KEY=$OLD_ONLY/$db" \
    "FERROFED__STORE__URL_FILE=$dir/store-url"
}
other() {
  echo FERROFED__NOT__SET=1
}
GATE
  printf 'public_url = "x"\n' > "$old/ferrofed.toml"
  check "an older tree's overrides are derived from its own gate, on its own keys" \
    "$(printf '%s\n' FERROFED__GATEWAY__PUBLIC_URL=http://127.0.0.1:8070 FERROFED__USAGE_REPORT__ENABLED=false \
      FERROFED__SERVER__LISTEN=127.0.0.1:8070 FERROFED__OLD__KEY=old-value/ferrofed_base \
      "FERROFED__STORE__URL_FILE=$WORK/base/store-url")" \
    "$(cd "$lane" && arm_env base "$old/ferrofed.toml" ferrofed_base 8070 "$old/gate.sh")"
  rm -f -- "${lane:?}/${BASE_TREE:?}/arm.env"
  cp "$old/gate.sh" "$lane/$BASE_TREE/gate.sh"
  check "a base with no arm.env takes its overrides from its own gate" "$BASE_TREE/gate.sh" "$(cd "$lane" && base_source)"
  rm -f -- "${lane:?}/${BASE_TREE:?}/gate.sh"
  check "a base with neither is refused" 2 "$( (cd "$lane" && base_source) > /dev/null 2>&1; echo $?)"
  printf 'other() {\n  true\n}\n' > "$old/none.sh"
  check "an older gate with no gateway_env() is refused" 2 \
    "$( (cd "$lane" && arm_env base "$old/ferrofed.toml" ferrofed_base 8070 "$old/none.sh") > /dev/null 2>&1; echo $?)"
  printf 'gateway_env() {\n  echo FERROFED_CONFIG=x\n}\n' > "$old/unset.sh"
  check "an older gate whose gateway_env() sets no FERROFED__ variable is refused" 2 \
    "$( (cd "$lane" && arm_env base "$old/ferrofed.toml" ferrofed_base 8070 "$old/unset.sh") > /dev/null 2>&1; echo $?)"
  printf '# a note\n\nFERROFED__A__LISTEN=127.0.0.1:@PORT@\nFERROFED__B__FILE=@DIR@/x\n' > "$work/arm.env"
  check "a tree's arm.env is filled for the gateway" \
    "$(printf '%s\n' FERROFED__A__LISTEN=127.0.0.1:8060 "FERROFED__B__FILE=$WORK/uncached/x")" \
    "$(arm_env uncached "$work/x.toml" ferrofed_uncached 8060 "$work/arm.env")"
  printf 'FERROFED__A=@NOPE@\n' > "$work/arm.env"
  check "an unknown placeholder is refused" 2 "$( (arm_env head x ferrofed 8080 "$work/arm.env") > /dev/null 2>&1; echo $?)"
  printf 'server.listen=x\n' > "$work/arm.env"
  check "a line that sets no FERROFED__ variable is refused" 2 "$( (arm_env head x ferrofed 8080 "$work/arm.env") > /dev/null 2>&1; echo $?)"
  out="$(cd "$lane" && gateway_env head "$CONFIG" ferrofed 8080 "$root/$ARM_ENV")"
  check "the gate sets the configuration and switches the usage report off" 2 \
    "$(grep -cxE "FERROFED__USAGE_REPORT__ENABLED=false|FERROFED_CONFIG=$CONFIG" <<< "$out")"
  check "a gateway's store URL is written for its own database" "postgres://ferrofed:$STORE_PASSWORD@127.0.0.1:$STORE_PORT/ferrofed?sslmode=require" \
    "$(< "$lane/$WORK/head/store-url")"
  # The arms the build job packs are the ones an attempt unpacks.
  mkdir -p "$work/arms/target/release" "$work/arms/$BASE_TREE" "$work/tool" "$work/attempt"
  printf '#!/bin/sh\necho head\n' > "$work/arms/target/release/ferrofed"
  printf '#!/bin/sh\necho token\n' > "$work/arms/target/release/ferrofed-perf-token"
  printf '#!/bin/sh\necho base\n' > "$work/arms/$BASE_TREE/ferrofed"
  printf '#!/bin/sh\necho perf\n' > "$work/tool/ferrohealth-perf"
  chmod +x "$work/arms/target/release/"* "$work/arms/$BASE_TREE/ferrofed" "$work/tool/ferrohealth-perf"
  printf 'abc123\n' > "$work/arms/$BASE_TREE/rev"
  printf 'The base is abc123.\n' > "$work/arms/$BASE_TREE/base.md"
  printf '[server]\n' > "$work/arms/$BASE_TREE/ferrofed.toml"
  printf 'FERROFED__A=1\n' > "$work/arms/$BASE_TREE/arm.env"
  check "the build job packs both arms and the tool" 0 \
    "$( (root="$work/arms" PATH="$work/tool:$PATH" pack "$work/arms.tar") > /dev/null 2>&1 && echo 0 || echo 1)"
  check "an attempt unpacks them with nothing built" 0 \
    "$( (root="$work/attempt" unpack "$work/arms.tar") > /dev/null 2>&1 && echo 0 || echo 1)"
  check "the unpacked base is the packed one" "abc123 base perf" \
    "$(printf '%s %s %s' "$(< "$work/attempt/$BASE_TREE/rev")" "$("$work/attempt/$BASE_TREE/ferrofed")" "$("$work/attempt/$TOOLS/ferrohealth-perf")")"
  check "the unpacked base carries its own configuration contract" "[server] FERROFED__A=1" \
    "$(printf '%s %s' "$(< "$work/attempt/$BASE_TREE/ferrofed.toml")" "$(< "$work/attempt/$BASE_TREE/arm.env")")"
  rm -f -- "${work:?}/arms/${BASE_TREE:?}/arm.env"
  check "a base whose overrides are not kept is not packed" 1 \
    "$( (root="$work/arms" PATH="$work/tool:$PATH" pack "$work/none.tar") > /dev/null 2>&1 && echo 0 || echo 1)"
  printf 'FERROFED__A=1\n' > "$work/arms/$BASE_TREE/arm.env"
  rm -f -- "$work/arms/$BASE_TREE/rev"
  check "an unbuilt base is not packed" 1 \
    "$( (root="$work/arms" PATH="$work/tool:$PATH" pack "$work/none.tar") > /dev/null 2>&1 && echo 0 || echo 1)"
  root="$saved_root"
  # RFC 9449 §6.1: the thumbprint of the key of the example proof (§4.1).
  check "a P-256 key's thumbprint is RFC 7638's" "0ZcOCORZNYy-DWpqq30jZyJGHTN0d2HglBV3uiguA4I" \
    "$(thumbprint l8tFrhx-34tV3hRICRDY9zCkDlpBhF42UQUfWVAWBFs 9VE4jf_Ok_o64zbTTlcuNJajHmt6v9TDVrU0CdvGRDA)"
  printf 'a = "Bearer {token}"\nb = "DPoP {ncp_token}"\nc = "DPoP {ncp_emergency_token}"\n' > "$work/plan.toml"
  render "$work/plan.toml" "$work/out.toml" t1 t2 t3
  check "the contact point's tokens are filled" \
    "$(printf 'a = "Bearer t1"\nb = "DPoP t2"\nc = "DPoP t3"')" "$(< "$work/out.toml")"
  check "a contact point's placeholder left unfilled is refused" 1 \
    "$(render "$work/plan.toml" "$work/out.toml" t 2> /dev/null && echo 0 || echo 1)"
  local ncp="$work/ncp" token parts x y
  contact_point_keys "$ncp" "$work/ncp-jwks.json"
  # The DER header of a P-256 public key: id-ecPublicKey, prime256v1 and an
  # uncompressed point (RFC 5480 §2).
  check "the DPoP key is P-256 in PKCS#8 PEM" \
    "BEGIN PRIVATE KEY 3059301306072a8648ce3d020106082a8648ce3d03010703420004" \
    "$(head -n 1 "$ncp/dpop-key.pem" | tr -d -- '-') $(openssl pkey -in "$ncp/dpop-key.pem" -pubout -outform DER 2> /dev/null | head -c 27 | od -An -tx1 | tr -d ' \n')"
  check "the key set names the signing key's kid" 1 \
    "$(grep -c "\"kid\":\"$(kid "$ncp/signing-key.pem")\"" "$work/ncp-jwks.json")"
  token="$(ncp_token "$ncp" TREATMENT)"
  IFS=. read -r -a parts <<< "$token"
  check "a token has three parts" 3 "${#parts[@]}"
  local payload
  payload="$(unb64url <<< "${parts[1]}")"
  read -r x y <<< "$(ec_xy "$ncp/dpop-key.pem")"
  local bound
  bound="\"cnf\":{\"jkt\":\"$(thumbprint "$x" "$y")\"}"
  check "the token is bound to the DPoP key" 1 "$(grep -cF "$bound" <<< "$payload")"
  check "the token is the contact point's" 1 \
    "$(grep -cF "\"iss\":\"$NCP_ISSUER\",\"sub\":\"$NCP_CLIENT\",\"aud\":\"$NCP_AUDIENCE\"" <<< "$payload")"
  check "the token names the TRC patient" 1 "$(grep -cF "\"patient\":{\"id\":\"$NCP_PATIENT\"}" <<< "$payload")"
  local treatment="\"purpose_of_use\":[{\"system\":\"$EHDSI_PURPOSE\",\"code\":\"TREATMENT\"}]"
  check "the token declares treatment" 1 "$(grep -cF "$treatment" <<< "$payload")"
  local field missing=""
  for field in hp_identifier '"name"' hp_professional_role authentication_method \
    healthcare_provider_identifier healthcare_provider_name facility_type home_community_id \
    point_of_care '"issuer":"https://ncp-idp' '"id":"_Qz9-ncp-ida-1"' name_id_format \
    '"id":"_Qz9-ncp-trc-1"' not_before not_on_or_after; do
    grep -qF "$field" <<< "$payload" || missing="$missing $field"
  done
  check "the token carries every attribute national_connector::asserted lists" "" "$missing"
  printf '%s.%s' "${parts[0]}" "${parts[1]}" > "$work/signed"
  unb64url <<< "${parts[2]}" > "$work/signature"
  openssl rsa -in "$ncp/signing-key.pem" -pubout -out "$work/ncp-public.pem" 2> /dev/null
  check "the signature holds under the signing key" "Verified OK" \
    "$(openssl dgst -sha256 -verify "$work/ncp-public.pem" -signature "$work/signature" "$work/signed" 2>&1)"
  check "the emergency token declares EMERGENCY" 1 \
    "$(ncp_token "$ncp" EMERGENCY | cut -d. -f2 | unb64url | grep -cF "\"code\":\"EMERGENCY\"")"
  # shellcheck disable=SC2016 # the backticks are the refusal's, never expanded
  check "every unknown key a refusal names is read" "$(printf 'emergency\nauth.dpop.nonce')" \
    "$(unknown_keys 'refused: 2 configuration refusals; `emergency` is not valid: unknown field `emergency`, expected one of `a`; `auth.dpop.nonce` is not valid: unknown field `nonce`; `server.listen` is not valid: invalid type')"
  check "a refusal that names no unknown key reads none" "" "$(unknown_keys 'the file is missing')"
  local committed
  committed="$(fill head 8080 "$root/$ARM_ENV")"
  check "the committed perf/arm.env fills every placeholder" 0 "$(grep -c '@' <<< "$committed" || true)"
  check "the committed perf/arm.env gives the head its own URLs and store" 5 \
    "$(grep -cxE 'FERROFED__(SERVER__LISTEN=127\.0\.0\.1:8080|GATEWAY__PUBLIC_URL=http://127\.0\.0\.1:8080|AUTH__DPOP__EXTERNAL_URL=http://127\.0\.0\.1:8080|SIGNING__JWKS_URI=http://127\.0\.0\.1:8080/\.well-known/jwks\.json|STORE__URL_FILE=target/perf-run/head/store-url)' <<< "$committed")"
  # shellcheck disable=SC2016 # the pattern matches the script's own text
  check "the head starts on its own tree's contract and the base on its own" 2 \
    "$(grep -cE '^  (head_pid="\$\(serve head [^ ]+ "\$CONFIG" ferrofed 8080 "\$ARM_ENV"|base_pid="\$\(serve base [^ ]+ "\$BASE_TREE/ferrofed.toml" ferrofed_base 8070 "\$base_contract")' "$root/scripts/perf/gate.sh")"
  local config="$root/$CONFIG"
  check "the gateway trusts the contact point the gate mints for" 3 \
    "$(grep -cxF "issuer = \"$NCP_ISSUER\"" "$config")"
  check "the gateway reads the contact point's audience and key set" 2 \
    "$(grep -cxE "audience = \[\"$NCP_AUDIENCE\"\]|jwks_file = \"$NCP_JWKS\"" "$config")"
  printf '{"pairs":[{"head":{"groups":{"%s":{"answered":5},"x":{"answered":9}}}},{"head":{"groups":{"%s":{"answered":7}}}},{"head":{"groups":{}}}]}\n' \
    "$EMERGENCY_GROUP" "$EMERGENCY_GROUP" > "$work/ab.json"
  check "the emergency group's answers are summed over the head's pairs" 12 "$(answered "$work/ab.json" "$EMERGENCY_GROUP")"
  check "a report with no emergency group answered none" 0 "$(answered "$work/ab.json" absent)"
  check "the plan sends the emergency group under the emergency token and the DPoP key" 1 \
    "$(awk -v group="group = \"$EMERGENCY_GROUP\"" '$0 == group { on = 1 } on && /^dpop_key_file = "target\/perf-run\/ncp\/dpop-key.pem"$/ { print; exit }' "$root/$PLAN" | grep -c . || true)"
  check "the plan's groups keep the emergency group under the limit at the plan's rate" 1 \
    "$(awk '/^rate = / { rate = $3 } /^\[\[requests\]\]/ { n++ } END { print (rate / n < 5) ? 1 : 0 }' "$root/$PLAN")"
  check "a lane with few exchanges holds the limit" 0 "$(onward_holds 10 200 && echo 0 || echo 1)"
  check "a lane past the limit fails it" 1 "$(onward_holds 11 200 && echo 0 || echo 1)"
  check "an exchange per request fails the limit" 1 "$(onward_holds 200 200 && echo 0 || echo 1)"
  printf '[a]\n%s\nissuer = "x"\n\n[b]\n' "$ONWARD_LANE" > "$work/onward.toml"
  check "the uncached lane sets cache = false in node A's onward table" \
    "$(printf '[a]\n%s\ncache = false\nissuer = "x"\n\n[b]' "$ONWARD_LANE")" "$(uncached "$work/onward.toml")"
  check "a configuration with no onward lane is refused" 1 \
    "$(uncached "$work/base.toml" > /dev/null 2>&1 && echo 0 || echo 1)"
  check "the gate's configuration has node A's onward lane, cached" 1 \
    "$(uncached "$config" | grep -cxF 'cache = false')"
  check "the quickstart issuer signs the caller's token and the onward lane's, for node A's audience" 3 \
    "$(grep -cxE "issuer = \"$QUICKSTART_ISSUER\"|audiences = \[\"https://node-a.perf.ferrofed.invalid\"\]" "$config")"
  check "node A trusts the token endpoint's issuers" 2 \
    "$(grep -cxE 'jwks_file = "/etc/ferroehr/onward/(persons|machines).json"' "$root/perf/ferroehr-onward.toml")"
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
  pack)
    shift
    pack "$@"
    ;;
  unpack)
    shift
    unpack "$@"
    ;;
  --self-test) self_test ;;
  *) die "usage: gate.sh build [BASE] | pack FILE | unpack FILE | run | --self-test" ;;
esac
