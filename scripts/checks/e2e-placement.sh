#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The gated-test placement guard: a test behind an opt-in gate lives in the
# module the CI job of that gate selects, because a gated test anywhere else
# returns early in every job and never runs (no specification governs this:
# our own design). Two gates exist:
#
#   FERROFED_E2E       the container tests. A gated file passes only at
#                      <dir>/<crate>/tests/it/e2e.rs or under
#                      <dir>/<crate>/tests/it/e2e/, the paths whose tests
#                      nextest names e2e::…, and the e2e job must set the gate
#                      and select test(/^e2e::/) across the workspace.
#   FERROFED_JOURNEYS  the operator console's browser journeys, which start
#                      the gateway and the console and so live in the
#                      testkit's test binary. A gated file passes only under
#                      tools/ferrofed-testkit/tests/it/journeys/, and the
#                      journeys job must set the gate and select
#                      test(/^journeys::/) in the testkit.
#
# A Rust file is gated when a line outside a comment calls the gate's check
# (e2e_enabled, journeys_enabled) or names its variable or constant. The
# testkit's src/, which defines the e2e gate, is outside the rule.
#
# Usage:
#   scripts/checks/e2e-placement.sh              check every tracked .rs file
#   scripts/checks/e2e-placement.sh --self-test  prove the refusals and passes
# Exit 1 naming each misplaced file or a workflow that stopped selecting the
# gated tests; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly WORKFLOW=.github/workflows/ci.yml
readonly E2E_PATTERN='e2e_enabled[[:space:]]*\(|FERROFED_E2E|E2E_GATE'
readonly JOURNEYS_PATTERN='journeys_enabled[[:space:]]*\(|FERROFED_JOURNEYS|JOURNEYS_GATE'

# is_gated FILE PATTERN: whether FILE matches PATTERN on a line that is not a
# comment.
is_gated() {
  local file="$1" pattern="$2"
  grep -v -E '^[[:space:]]*//' "$file" | grep -q -E "$pattern"
}

# may_be_e2e PATH: whether a file behind the e2e gate may sit at PATH.
may_be_e2e() {
  local path="$1"
  case "$path" in
    tools/ferrofed-testkit/src/*) return 0 ;;
    *) ;;
  esac
  [[ "$path" =~ ^[^/]+/[^/]+/tests/it/e2e(\.rs|/.+\.rs)$ ]]
}

# may_be_journey PATH: whether a file behind the journeys gate may sit at PATH.
may_be_journey() {
  [[ "$1" =~ ^tools/ferrofed-testkit/tests/it/journeys/.+\.rs$ ]]
}

# check_file PATH FILE: reports PATH when FILE is gated where it may not be.
check_file() {
  local path="$1" file="$2" fail=0
  if is_gated "$file" "$E2E_PATTERN" && ! may_be_e2e "$path"; then
    echo "::error file=$path::$path checks the FERROFED_E2E gate outside tests/it/e2e, so no CI job runs it; move it under the e2e module of its crate's test binary." >&2
    fail=1
  fi
  if is_gated "$file" "$JOURNEYS_PATTERN" && ! may_be_journey "$path"; then
    echo "::error file=$path::$path checks the FERROFED_JOURNEYS gate outside tools/ferrofed-testkit/tests/it/journeys, so no CI job runs it; move it under the journeys module of the testkit's test binary." >&2
    fail=1
  fi
  return "$fail"
}

# job_of FILE NAME: the lines of job NAME in workflow FILE.
job_of() {
  awk -v name="  $2:" '$0 == name { inside = 1; next } inside && /^  [^ #][^ ]*:$/ { inside = 0 } inside' "$1"
}

# check_workflow FILE: whether the e2e job in FILE sets its gate and selects
# every crate's e2e module, and the journeys job sets its gate and selects
# the testkit's journeys module.
check_workflow() {
  local fail=0 job
  job="$(job_of "$1" e2e)"
  if [[ -z "$job" ]]; then
    echo "::error file=$1::$1 has no e2e job, so no gated test runs." >&2
    fail=1
  else
    if ! grep -q -E '^[[:space:]]*FERROFED_E2E: "1"' <<< "$job"; then
      echo "::error file=$1::the e2e job of $1 no longer sets FERROFED_E2E to 1, so no gated test runs." >&2
      fail=1
    fi
    if ! grep -q -F -e '--workspace' <<< "$job" || ! grep -q -F 'test(/^e2e::/)' <<< "$job"; then
      echo "::error file=$1::the e2e job of $1 no longer runs test(/^e2e::/) across the workspace, so a gated test this guard places under e2e:: may never run." >&2
      fail=1
    fi
  fi
  job="$(job_of "$1" journeys)"
  if [[ -z "$job" ]]; then
    echo "::error file=$1::$1 has no journeys job, so no browser journey runs." >&2
    return 1
  fi
  if ! grep -q -E '^[[:space:]]*FERROFED_JOURNEYS: "1"' <<< "$job"; then
    echo "::error file=$1::the journeys job of $1 no longer sets FERROFED_JOURNEYS to 1, so no browser journey runs." >&2
    fail=1
  fi
  if ! grep -q -F -e '-p ferrofed-testkit' <<< "$job" || ! grep -q -F 'test(/^journeys::/)' <<< "$job"; then
    echo "::error file=$1::the journeys job of $1 no longer runs test(/^journeys::/) in ferrofed-testkit, so a browser journey this guard places under journeys:: may never run." >&2
    fail=1
  fi
  return "$fail"
}

check_tree() {
  local fail=0 count=0 path
  while IFS= read -r path; do
    count=$((count + 1))
    check_file "$path" "$path" || fail=1
  done < <(git ls-files -- '*.rs' ':(exclude)docs/specs/**' ':(glob,exclude)**/vendor/**')
  if [[ -f "$WORKFLOW" ]]; then
    check_workflow "$WORKFLOW" || fail=1
  fi
  if [[ "$fail" -eq 0 ]]; then
    echo "e2e-placement: $count Rust files, every gated test under e2e:: or journeys::."
  fi
  return "$fail"
}

# The self-test drives check_file and check_workflow against fixtures in a
# temporary directory: each refused form fails, each near miss passes.
self_test() {
  local work
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  local gated="$work/gated.rs" offline="$work/offline.rs" commented="$work/commented.rs"
  local by_name="$work/by_name.rs" by_const="$work/by_const.rs"
  local journey="$work/journey.rs" journey_by_name="$work/journey_by_name.rs"
  printf 'async fn t() {\n    if !containers::e2e_enabled() {\n        return;\n    }\n}\n' > "$gated"
  printf 'fn t() {\n    assert_eq!(1, 1);\n}\n' > "$offline"
  printf '//! Behind the FERROFED_E2E gate, elsewhere.\n// e2e_enabled() is checked there.\n//! FERROFED_JOURNEYS too.\nfn t() {}\n' > "$commented"
  printf 'fn t() {\n    if std::env::var("FERROFED_E2E").is_err() {}\n}\n' > "$by_name"
  printf 'fn t() {\n    let _gate = std::env::var(containers::E2E_GATE);\n}\n' > "$by_const"
  printf 'async fn t() {\n    if !journeys_enabled() {\n        return;\n    }\n}\n' > "$journey"
  printf 'fn t() {\n    if std::env::var("FERROFED_JOURNEYS").is_err() {}\n}\n' > "$journey_by_name"

  local failed=0
  # expect WANT PATH FILE: check_file over PATH holding FILE exits WANT.
  expect() {
    local want="$1" path="$2" file="$3" status=0
    check_file "$path" "$file" 2> /dev/null || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "e2e-placement: self-test failed: $path ($(basename "$file")) exited $status, wanted $want." >&2
      failed=1
    fi
  }
  expect 1 app/ferrofed-server/tests/it/e2e_attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/routing/e2e/attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/e2e_x/attributes.rs "$gated"
  expect 1 app/ferrofed-server/src/federation/build.rs "$gated"
  expect 1 app/ferrofed-engine/tests/it/dispatch.rs "$by_name"
  expect 1 crates/ihe-iti/tests/it/pixm.rs "$by_const"
  expect 1 tools/ferrofed-testkit/tests/it/journeys/sign_in.rs "$gated"
  expect 0 app/ferrofed-server/tests/it/e2e/attributes.rs "$gated"
  expect 0 app/ferrofed-server/tests/it/e2e/mod.rs "$gated"
  expect 0 tools/ferrofed-testkit/tests/it/e2e.rs "$gated"
  expect 0 crates/ihe-iti/tests/it/e2e/pixm.rs "$by_const"
  expect 0 tools/ferrofed-testkit/src/containers/mod.rs "$by_const"
  expect 0 app/ferrofed-server/tests/it/track10/mod.rs "$commented"
  expect 0 app/ferrofed-server/tests/it/e2e_attributes.rs "$offline"
  expect 1 app/ferrofed-viewer/tests/it/journeys/sign_in.rs "$journey"
  expect 1 tools/ferrofed-testkit/tests/it/journeys.rs "$journey"
  expect 1 tools/ferrofed-testkit/tests/it/e2e/journey.rs "$journey"
  expect 1 tools/ferrofed-testkit/tests/it/console.rs "$journey_by_name"
  expect 0 tools/ferrofed-testkit/tests/it/journeys/mod.rs "$journey"
  expect 0 tools/ferrofed-testkit/tests/it/journeys/sign_in.rs "$journey_by_name"
  expect 0 tools/ferrofed-testkit/tests/it/journeys/sign_in.rs "$offline"

  local job="$work/ci.yml"
  # expect_workflow WANT NAME: check_workflow over $job exits WANT.
  expect_workflow() {
    local want=$1 status=0
    check_workflow "$job" 2> /dev/null || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "e2e-placement: self-test failed: workflow $2 exited $status, wanted $want." >&2
      failed=1
    fi
  }
  # workflow ENV RUN JENV JRUN: a workflow whose e2e job sets ENV and runs
  # RUN and whose journeys job sets JENV and runs JRUN, between jobs that run
  # every test offline and name the gates in a comment.
  workflow() {
    {
      printf 'jobs:\n  test:\n    steps:\n      - run: cargo nextest run --workspace --locked\n'
      printf '  # Every gated test checks FERROFED_E2E first.\n  e2e:\n    env:\n      %s\n' "$1"
      printf '    steps:\n      - run: %s\n' "$2"
      printf '  # FERROFED_JOURNEYS: "1" -p ferrofed-testkit test(/^journeys::/)\n'
      printf '  journeys:\n    env:\n      %s\n' "$3"
      printf '    steps:\n      - run: %s\n' "$4"
      printf '  doc:\n    steps:\n      - run: cargo doc --workspace -E '"'"'test(/^e2e::/)'"'"'\n'
    } > "$job"
  }
  local filter="'package(ferrofed-testkit) or test(/^e2e::/)'"
  local e2e_ok="cargo nextest run --locked --workspace -E $filter"
  local journeys_ok="cargo nextest run --locked -p ferrofed-testkit -E 'test(/^journeys::/)'"
  workflow 'FERROFED_E2E: "1"' "$e2e_ok" 'FERROFED_JOURNEYS: "1"' "$journeys_ok"
  expect_workflow 0 "selecting e2e:: across the workspace and journeys:: in the testkit"
  workflow 'FERROFED_E2E: "1"' "cargo nextest run --locked -p ferrofed-server -E $filter" 'FERROFED_JOURNEYS: "1"' "$journeys_ok"
  expect_workflow 1 "selecting one package for e2e"
  workflow 'FERROFED_E2E: "0"' "$e2e_ok" 'FERROFED_JOURNEYS: "1"' "$journeys_ok"
  expect_workflow 1 "without the e2e gate"
  workflow 'FERROFED_E2E: "1"' "cargo nextest run --locked --workspace -E 'test(/^e2e_/)'" 'FERROFED_JOURNEYS: "1"' "$journeys_ok"
  expect_workflow 1 "with another e2e filter"
  workflow 'FERROFED_E2E: "1"' "$e2e_ok" 'FERROFED_JOURNEYS: "0"' "$journeys_ok"
  expect_workflow 1 "without the journeys gate"
  workflow 'FERROFED_E2E: "1"' "$e2e_ok" 'FERROFED_JOURNEYS: "1"' "cargo nextest run --locked -p ferrofed-viewer -E 'test(/^journeys::/)'"
  expect_workflow 1 "selecting journeys in another package"
  workflow 'FERROFED_E2E: "1"' "$e2e_ok" 'FERROFED_JOURNEYS: "1"' "cargo nextest run --locked -p ferrofed-testkit -E 'test(/^journey/)'"
  expect_workflow 1 "with another journeys filter"
  printf 'jobs:\n  test:\n    env:\n      FERROFED_E2E: "1"\n    steps:\n      - run: cargo nextest run --workspace -E %s\n' "$filter" > "$job"
  expect_workflow 1 "with no e2e job"
  printf 'jobs:\n  e2e:\n    env:\n      FERROFED_E2E: "1"\n    steps:\n      - run: %s\n' "$e2e_ok" > "$job"
  expect_workflow 1 "with no journeys job"

  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "e2e-placement: self-test OK."
}

case "${1:-}" in
  --self-test) self_test ;;
  "") check_tree ;;
  *)
    echo "e2e-placement: unknown argument $1" >&2
    exit 2
    ;;
esac
