#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# The e2e placement guard: a test that checks the FERROFED_E2E gate lives in
# the e2e module of its crate's test binary, because the CI container job
# selects the gated tests by the nextest filter test(/^e2e::/) and a gated
# test anywhere else returns early in every job and never runs (no
# specification governs this: our own design).
#
# A Rust file is gated when a line outside a comment calls e2e_enabled, or
# names FERROFED_E2E or E2E_GATE. A gated file passes only at
# <dir>/<crate>/tests/it/e2e.rs or under <dir>/<crate>/tests/it/e2e/, the paths
# whose tests nextest names e2e::…; the testkit's src/, which defines the gate,
# is outside the rule. The guard also checks that the CI e2e job still sets
# the gate and still selects test(/^e2e::/) across the workspace.
#
# Usage:
#   scripts/checks/e2e-placement.sh              check every tracked .rs file
#   scripts/checks/e2e-placement.sh --self-test  prove the refusals and passes
# Exit 1 naming each misplaced file or a workflow that stopped selecting the
# gated tests; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly WORKFLOW=.github/workflows/ci.yml

# is_gated FILE: whether FILE reads the gate on a line that is not a comment.
is_gated() {
  grep -v -E '^[[:space:]]*//' "$1" | grep -q -E 'e2e_enabled[[:space:]]*\(|FERROFED_E2E|E2E_GATE'
}

# may_be_gated PATH: whether a gated file may sit at the repository path PATH.
may_be_gated() {
  case "$1" in
    tools/ferrofed-testkit/src/*) return 0 ;;
  esac
  [[ "$1" =~ ^[^/]+/[^/]+/tests/it/e2e(\.rs|/.+\.rs)$ ]]
}

# check_file PATH FILE: reports PATH when FILE is gated where it may not be.
check_file() {
  if is_gated "$2" && ! may_be_gated "$1"; then
    echo "::error file=$1::$1 checks the FERROFED_E2E gate outside tests/it/e2e, so no CI job runs it; move it under the e2e module of its crate's test binary." >&2
    return 1
  fi
}

# check_workflow FILE: whether the e2e job in FILE sets the gate and selects
# every crate's e2e module.
check_workflow() {
  local fail=0 job
  job="$(awk '/^  e2e:$/ { inside = 1; next } inside && /^  [^ #][^ ]*:$/ { inside = 0 } inside' "$1")"
  if [ -z "$job" ]; then
    echo "::error file=$1::$1 has no e2e job, so no gated test runs." >&2
    return 1
  fi
  if ! grep -q -E '^[[:space:]]*FERROFED_E2E: "1"' <<< "$job"; then
    echo "::error file=$1::the e2e job of $1 no longer sets FERROFED_E2E to 1, so no gated test runs." >&2
    fail=1
  fi
  if ! grep -q -F -e '--workspace' <<< "$job" || ! grep -q -F 'test(/^e2e::/)' <<< "$job"; then
    echo "::error file=$1::the e2e job of $1 no longer runs test(/^e2e::/) across the workspace, so a gated test this guard places under e2e:: may never run." >&2
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
  if [ -f "$WORKFLOW" ]; then
    check_workflow "$WORKFLOW" || fail=1
  fi
  if [ "$fail" -eq 0 ]; then
    echo "e2e-placement: $count Rust files, every gated test under e2e::."
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
  printf 'async fn t() {\n    if !containers::e2e_enabled() {\n        return;\n    }\n}\n' > "$gated"
  printf 'fn t() {\n    assert_eq!(1, 1);\n}\n' > "$offline"
  printf '//! Behind the FERROFED_E2E gate, elsewhere.\n// e2e_enabled() is checked there.\nfn t() {}\n' > "$commented"
  printf 'fn t() {\n    if std::env::var("FERROFED_E2E").is_err() {}\n}\n' > "$by_name"
  printf 'fn t() {\n    let _gate = std::env::var(containers::E2E_GATE);\n}\n' > "$by_const"

  local failed=0
  # expect WANT PATH FILE: check_file over PATH holding FILE exits WANT.
  expect() {
    local want=$1 status=0
    check_file "$2" "$3" 2> /dev/null || status=$?
    if [ "$status" -ne "$want" ]; then
      echo "e2e-placement: self-test failed: $2 ($(basename "$3")) exited $status, wanted $want." >&2
      failed=1
    fi
  }
  expect 1 app/ferrofed-server/tests/it/e2e_attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/routing/e2e/attributes.rs "$gated"
  expect 1 app/ferrofed-server/tests/it/e2e_x/attributes.rs "$gated"
  expect 1 app/ferrofed-server/src/federation.rs "$gated"
  expect 1 app/ferrofed-engine/tests/it/dispatch.rs "$by_name"
  expect 1 crates/ihe-iti/tests/it/pixm.rs "$by_const"
  expect 0 app/ferrofed-server/tests/it/e2e/attributes.rs "$gated"
  expect 0 app/ferrofed-server/tests/it/e2e/mod.rs "$gated"
  expect 0 tools/ferrofed-testkit/tests/it/e2e.rs "$gated"
  expect 0 crates/ihe-iti/tests/it/e2e/pixm.rs "$by_const"
  expect 0 tools/ferrofed-testkit/src/containers.rs "$by_const"
  expect 0 app/ferrofed-server/tests/it/track10/mod.rs "$commented"
  expect 0 app/ferrofed-server/tests/it/e2e_attributes.rs "$offline"

  local job="$work/ci.yml"
  # expect_workflow WANT NAME: check_workflow over $job exits WANT.
  expect_workflow() {
    local want=$1 status=0
    check_workflow "$job" 2> /dev/null || status=$?
    if [ "$status" -ne "$want" ]; then
      echo "e2e-placement: self-test failed: workflow $2 exited $status, wanted $want." >&2
      failed=1
    fi
  }
  # workflow ENV RUN: a workflow whose e2e job sets ENV and runs RUN, between
  # two jobs that run every test offline and name the gate in a comment.
  workflow() {
    {
      printf 'jobs:\n  test:\n    steps:\n      - run: cargo nextest run --workspace --locked\n'
      printf '  # Every gated test checks FERROFED_E2E first.\n  e2e:\n    env:\n      %s\n' "$1"
      printf '    steps:\n      - run: %s\n' "$2"
      printf '  doc:\n    steps:\n      - run: cargo doc --workspace -E '"'"'test(/^e2e::/)'"'"'\n'
    } > "$job"
  }
  local filter="'package(ferrofed-testkit) or test(/^e2e::/)'"
  workflow 'FERROFED_E2E: "1"' "cargo nextest run --locked --workspace -E $filter"
  expect_workflow 0 "selecting e2e:: across the workspace"
  workflow 'FERROFED_E2E: "1"' "cargo nextest run --locked -p ferrofed-server -E $filter"
  expect_workflow 1 "selecting one package"
  workflow 'FERROFED_E2E: "0"' "cargo nextest run --locked --workspace -E $filter"
  expect_workflow 1 "without the gate"
  workflow 'FERROFED_E2E: "1"' "cargo nextest run --locked --workspace -E 'test(/^e2e_/)'"
  expect_workflow 1 "with another filter"
  printf 'jobs:\n  test:\n    env:\n      FERROFED_E2E: "1"\n    steps:\n      - run: cargo nextest run --workspace -E %s\n' "$filter" > "$job"
  expect_workflow 1 "with no e2e job"

  if [ "$failed" -ne 0 ]; then
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
