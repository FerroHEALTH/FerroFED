#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Test launchers switch the usage report off.
#
# `ferrofed serve` sends the installation report by default once it has a
# state directory, so a gateway a test lane starts could report from a
# throwaway runner and count as an installation. A launcher is a tracked
# shell script or workflow that starts the gateway with `serve`: the binary
# itself (`target/release/ferrofed`, `"$bin/ferrofed"`,
# `/usr/local/bin/ferrofed`, `cargo run -p ferrofed-server`), a `docker run`
# of the gateway image, the production guide's `gateway_run` wrapper, or
# `docker compose ... up`. Each one carries the switch
# (`FERROFED__USAGE_REPORT__ENABLED=false`, or the same key in a workflow
# `env:` map). A `docker run` of the image carries it on its own command line
# (`--env FERROFED__USAGE_REPORT__ENABLED=false`): a container inherits
# nothing from the shell. A Rust test that starts the binary with `serve`
# sets `.env("FERROFED__USAGE_REPORT__ENABLED", "false")` on that `Command`.
# Any other job (`config check`, `healthcheck`, `--version`) sends no report
# and is no launch. The gateway also stays silent when `GITHUB_ACTIONS` or
# `CI` is set; this guard covers what that misses, a container that does not
# inherit the runner's environment (#1122). The quickstart compose file at
# the repository root is a launcher too, started by hand with `docker
# compose up`: a service there that runs the gateway image carries the
# switch in its `environment:` map (#1280). The release compose file under
# `deploy/compose/` is an installation and reports by default, so it is no
# launcher. No specification governs this: our own design.
#
# Exempt: this file, which reads the commands as data.
#
#   scripts/checks/usage-report-off.sh [--self-test]
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly SELF='scripts/checks/usage-report-off.sh'
readonly SWITCH='FERROFED__USAGE_REPORT__ENABLED(=|:[[:space:]]*["'\'']?)["'\'']?false'
# shellcheck disable=SC2016 # a regular expression, never expanded
readonly BINARY='(target/(release|debug)/|/usr/local/bin/|\$bin/|\$\{bin\}/)ferrofed(["'\''[:space:]]|$)|cargo[[:space:]]+run[[:space:]].*(-p|--package)[[:space:]=]+ferrofed-server|gateway_run[[:space:]]'
readonly DOCKER_RUN='docker[[:space:]]+run[[:space:]].*(ghcr\.io/ferrohealth/ferrofed[:@]|GATEWAY_IMAGE|FERROFED_IMAGE)'
readonly SERVE='[[:space:]"'\'']serve(["'\''[:space:]]|$)'
readonly COMPOSE_UP='docker[ -]compose([[:space:]].*)?[[:space:]]up([[:space:]]|$)'
# The switch handed to a container on its own `docker run` command line.
readonly RUN_SWITCH='(-e|--env)[[:space:]=]+["'\'']?FERROFED__USAGE_REPORT__ENABLED=false'
# The quickstart compose file, and a service image that is the gateway's.
readonly QUICKSTART='compose.yaml'
readonly GATEWAY_IMAGE='^[[:space:]]*image:[[:space:]]*["'\'']?ghcr\.io/ferrohealth/ferrofed[:@]'
# A Rust test that starts the binary as a server, and the switch it carries.
readonly RUST_BINARY='CARGO_BIN_EXE_ferrofed"'
readonly RUST_LAUNCH='"serve"'
readonly RUST_SWITCH='\.env\( ?"FERROFED__USAGE_REPORT__ENABLED", ?"false",? ?\)'

# Prints the file's content without comment lines, one logical line per
# backslash-continued command.
logical_lines() {
  awk '/^[[:space:]]*#/ { next }
       { l = $0; if (sub(/\\$/, "", l)) { buf = buf l " "; next } print buf l; buf = "" }
       END { if (buf != "") print buf }' "$1"
}

# The predicates grep a captured copy, never a pipe: `grep -q` exits at the
# first match, and under pipefail the writer's SIGPIPE would read as no match.
is_launcher() {
  local lines starts
  lines="$(logical_lines "$1")"
  grep -qE "$COMPOSE_UP" <<< "$lines" && return 0
  starts="$(grep -E "$BINARY|$DOCKER_RUN" <<< "$lines" || true)"
  [[ -n "$starts" ]] && grep -qE "$SERVE" <<< "$starts"
}

# Whether $1 starts the gateway with a `docker run` that does not hand the
# container the switch itself.
bare_docker_run() {
  local lines runs
  lines="$(logical_lines "$1")"
  runs="$(grep -E "$DOCKER_RUN" <<< "$lines" | grep -E "$SERVE" || true)"
  [[ -n "$runs" ]] && grep -qvE "$RUN_SWITCH" <<< "$runs"
}

carries_switch() {
  local lines
  lines="$(logical_lines "$1")"
  grep -qE "$SWITCH" <<< "$lines"
}

# Lists the candidate files: shell scripts and workflow and action YAML.
candidates() {
  local f
  while IFS= read -r f; do
    case "$f" in
      "$SELF") continue ;;
      .github/*.yml | .github/*.yaml | *.sh) printf '%s\n' "$f" ;;
      *) ;;
    esac
  done < <("$@" | grep -E '^(scripts|docker|\.github/(workflows|actions))/' | sort)
}

# Prints one finding per launcher that does not carry the switch.
# $@ = the file-listing command.
findings() {
  local f
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    is_launcher "$f" || continue
    if bare_docker_run "$f"; then
      printf '%s\n' "$f"
      continue
    fi
    carries_switch "$f" || printf '%s\n' "$f"
  done < <(candidates "$@")
  rust_findings "$@"
  quickstart_findings "$@"
}

# Prints the quickstart compose file when a service in it runs the gateway
# image and the file does not carry the switch.
quickstart_findings() {
  local files lines
  files="$("$@")"
  grep -qxF "$QUICKSTART" <<< "$files" || return 0
  lines="$(logical_lines "$QUICKSTART")"
  grep -qE "$GATEWAY_IMAGE" <<< "$lines" || return 0
  grep -qE "$SWITCH" <<< "$lines" || printf '%s\n' "$QUICKSTART"
}

# Prints every tracked Rust source that starts the binary with `serve`
# without the switch. The source is read with its line breaks folded, so a
# call that rustfmt wraps still matches.
rust_findings() {
  local f text
  while IFS= read -r f; do
    [[ -n "$f" ]] || continue
    grep -qF "$RUST_BINARY" "$f" || continue
    text="$(tr '\n' ' ' < "$f" | tr -s '[:space:]' ' ')"
    grep -qF "$RUST_LAUNCH" <<< "$text" || continue
    grep -qE "$RUST_SWITCH" <<< "$text" || printf '%s\n' "$f"
  done < <("$@" | grep -E '^(app|tools|crates)/.*\.rs$' | sort)
}

self_test() {
  local tmp found failures=0 name
  tmp="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $tmp now, while it is still in scope
  trap "rm -rf '$tmp'" RETURN
  mkdir -p "$tmp/scripts/checks" "$tmp/.github/workflows" "$tmp/app/server/tests"
  printf '#!/usr/bin/env bash\nFERROFED__USAGE_REPORT__ENABLED=false target/release/ferrofed serve &\n' > "$tmp/scripts/off.sh"
  printf '#!/usr/bin/env bash\nFERROFED__SERVER__LISTEN=127.0.0.1:1 target/release/ferrofed serve > log 2>&1 &\n' > "$tmp/scripts/on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\nFERROFED_CONFIG=x \\\n  "$bin/ferrofed" serve >log &\n' > "$tmp/scripts/bin-on.sh"
  printf '#!/usr/bin/env bash\ncargo run --release -p ferrofed-server -- serve\n' > "$tmp/scripts/cargo-on.sh"
  printf '#!/usr/bin/env bash\ndocker compose -f compose.yaml up -d\n' > "$tmp/scripts/compose-on.sh"
  printf '#!/usr/bin/env bash\nexport FERROFED__USAGE_REPORT__ENABLED=false\ndocker compose up --wait\n' > "$tmp/scripts/compose-off.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\ngateway_run() {\n  docker run --read-only "$@"\n}\ngateway_run --detach "$image" serve --config c\n' > "$tmp/scripts/wrapped-on.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\ngateway_run() {\n  docker run --env FERROFED__USAGE_REPORT__ENABLED=false "$@"\n}\ngateway_run --detach "$image" serve --config c\n' > "$tmp/scripts/wrapped-off.sh"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf '#!/usr/bin/env bash\ngateway_run --rm "$image" config check --config c\n' > "$tmp/scripts/wrapped-check.sh"
  printf '#!/usr/bin/env bash\n# target/release/ferrofed serve &\necho done\n' > "$tmp/scripts/commented.sh"
  printf '#!/usr/bin/env bash\ntarget/release/ferrofed config check\n/usr/local/bin/ferrofed healthcheck\n' > "$tmp/scripts/jobs.sh"
  printf 'jobs:\n  a:\n    steps:\n      - run: |\n          docker run -d ghcr.io/ferrohealth/ferrofed:main serve\n' > "$tmp/.github/workflows/run-on.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'env:\n  FERROFED__USAGE_REPORT__ENABLED: "false"\njobs:\n  a:\n    steps:\n      - run: docker run -d "$GATEWAY_IMAGE" serve\n' > "$tmp/.github/workflows/run-job-env.yml"
  # shellcheck disable=SC2016 # the fixture is literal shell text, never expanded here
  printf 'jobs:\n  a:\n    steps:\n      - run: docker run -d --env FERROFED__USAGE_REPORT__ENABLED=false "$GATEWAY_IMAGE" serve\n' > "$tmp/.github/workflows/run-off.yml"
  printf 'jobs:\n  a:\n    steps:\n      - run: docker run --rm ghcr.io/ferrohealth/ferrofed:main --version\n' > "$tmp/.github/workflows/one-shot.yml"
  printf 'env:\n  FERROFED__USAGE_REPORT__ENABLED: "false"\njobs:\n  a:\n    steps:\n      - run: target/release/ferrofed serve &\n' > "$tmp/.github/workflows/bin-off.yml"
  printf 'jobs:\n  a:\n    steps:\n      - run: target/release/ferrofed serve &\n' > "$tmp/.github/workflows/bin-on.yml"
  printf '#!/usr/bin/env bash\ngrep "target/release/ferrofed serve &" x\n' > "$tmp/$SELF"
  printf 'Command::new(env!("CARGO_BIN_EXE_ferrofed"))\n    .args(["serve", "--config"])\n    .output()\n' > "$tmp/app/server/tests/on.rs"
  printf 'Command::new(env!("CARGO_BIN_EXE_ferrofed"))\n    .args(["serve", "--config"])\n    .env(\n        "FERROFED__USAGE_REPORT__ENABLED",\n        "false",\n    )\n' > "$tmp/app/server/tests/off.rs"
  printf 'Command::new(env!("CARGO_BIN_EXE_ferrofed"))\n    .args(["config", "check"])\n' > "$tmp/app/server/tests/job.rs"
  printf 'Command::new(env!("CARGO_BIN_EXE_ferrofed-console"))\n    .arg("serve")\n' > "$tmp/app/server/tests/console.rs"

  found="$(cd "$tmp" && findings find scripts docker .github app -type f 2> /dev/null)"
  for name in scripts/on.sh scripts/bin-on.sh scripts/cargo-on.sh scripts/compose-on.sh scripts/wrapped-on.sh \
    .github/workflows/run-on.yml .github/workflows/run-job-env.yml .github/workflows/bin-on.yml app/server/tests/on.rs; do
    grep -qxF "$name" <<< "$found" || {
      echo "self-test: $name was not caught" >&2
      failures=1
    }
  done
  for name in scripts/off.sh scripts/compose-off.sh scripts/wrapped-off.sh scripts/wrapped-check.sh scripts/commented.sh \
    scripts/jobs.sh .github/workflows/run-off.yml .github/workflows/one-shot.yml .github/workflows/bin-off.yml "$SELF" \
    app/server/tests/off.rs app/server/tests/job.rs app/server/tests/console.rs; do
    if grep -qxF "$name" <<< "$found"; then
      echo "self-test: $name was flagged" >&2
      failures=1
    fi
  done
  mkdir -p "$tmp/on" "$tmp/off" "$tmp/other"
  printf 'services:\n  ferrofed:\n    image: ghcr.io/ferrohealth/ferrofed:0.0.1\n    environment:\n      FERROEHR__USAGE_REPORT__ENABLED: "false"\n' > "$tmp/on/$QUICKSTART"
  printf 'services:\n  ferrofed:\n    image: ghcr.io/ferrohealth/ferrofed:0.0.1\n    environment:\n      FERROFED__USAGE_REPORT__ENABLED: "false"\n' > "$tmp/off/$QUICKSTART"
  printf 'services:\n  node:\n    image: ghcr.io/ferrohealth/ferroehr:5.3.2\n' > "$tmp/other/$QUICKSTART"
  found="$(cd "$tmp/on" && findings printf '%s\n' "$QUICKSTART")"
  grep -qxF "$QUICKSTART" <<< "$found" || {
    echo "self-test: a quickstart compose file without the switch was not caught" >&2
    failures=1
  }
  for name in off other; do
    if [[ -n "$(cd "$tmp/$name" && findings printf '%s\n' "$QUICKSTART")" ]]; then
      echo "self-test: the $name quickstart compose file was flagged" >&2
      failures=1
    fi
  done
  if [[ "$failures" -ne 0 ]]; then
    return 1
  fi
  echo "usage-report-off: self-test OK."
}

case "${1:-}" in
  --self-test)
    self_test
    exit
    ;;
  "") ;;
  *)
    echo "usage: $SELF [--self-test]" >&2
    exit 2
    ;;
esac

found="$(findings git ls-files)"
if [[ -n "$found" ]]; then
  while IFS= read -r f; do
    echo "::error file=$f::$f starts the gateway with serve but does not switch the usage report off; set FERROFED__USAGE_REPORT__ENABLED=false on it (#1122)." >&2
  done <<< "$found"
  exit 1
fi
echo "usage-report-off: every launcher switches the usage report off."
