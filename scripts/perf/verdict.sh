#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The verdict of the performance gate (#1687, #1807), run by the perf
# workflow's verdict job over the three attempt jobs.
#
#   scripts/perf/verdict.sh
#       Reads RESULT_1 to RESULT_3, each attempt job's result (success,
#       failure, cancelled, skipped or empty), CODE_1 to CODE_3, each
#       attempt's gate exit code, and RUN_CANCELLED, "true" when the run
#       itself was cancelled. Passes when the last attempt that ran ended
#       with exit 0. An attempt job that failed fails the gate. A cancelled
#       attempt that a later attempt followed lost its runner while the run
#       went on, and was retried; the last attempt cancelled fails the gate.
#       Names every attempt in $GITHUB_STEP_SUMMARY when it is set.
#   scripts/perf/verdict.sh --self-test
#       Proves the verdict over every shape of the three attempts, offline.
#
# No specification governs this: our own design.
set -euo pipefail

say() {
  echo "$1"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    echo "$1" >> "$GITHUB_STEP_SUMMARY"
    echo >> "$GITHUB_STEP_SUMMARY"
  fi
}

refuse() {
  echo "::error title=Performance gate::$1"
  say "The gate fails: $1."
  exit 1
}

verdict() {
  local results=("${RESULT_1:-}" "${RESULT_2:-}" "${RESULT_3:-}")
  local codes=("${CODE_1:-}" "${CODE_2:-}" "${CODE_3:-}")
  local i last=0 result code
  for i in 1 2 3; do
    result="${results[i - 1]}"
    [[ -n "$result" && "$result" != skipped ]] && last="$i"
  done
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '## Performance gate, verdict\n\n' >> "$GITHUB_STEP_SUMMARY"
  fi
  for i in 1 2 3; do
    result="${results[i - 1]}"
    code="${codes[i - 1]}"
    case "$result" in
      "" | skipped) ;;
      cancelled)
        if ((i < last)); then
          say "Attempt $i was cancelled while the run went on: its runner shut down or the job reached its time limit, which says nothing about the code, so attempt $((i + 1)) ran on a fresh runner."
        else
          say "Attempt $i was cancelled."
        fi
        ;;
      *) say "Attempt $i ended ${result}${code:+ with exit $code}." ;;
    esac
  done
  if [[ "${RUN_CANCELLED:-false}" == true ]]; then
    refuse "the run was cancelled, so no attempt is retried"
  fi
  ((last > 0)) || refuse "no attempt ran"
  for i in 1 2 3; do
    [[ "${results[i - 1]}" == failure ]] && refuse "attempt $i did not finish"
  done
  result="${results[last - 1]}"
  code="${codes[last - 1]}"
  if [[ "$result" == cancelled ]]; then
    if ((last == 3)); then
      refuse "attempt 3 was cancelled and no attempt is left to retry it"
    fi
    refuse "attempt $last was cancelled and no retry followed"
  fi
  [[ "$code" == 0 ]] || refuse "the last attempt ended with exit ${code:-none}"
  say "The gate passes: attempt $last passed."
}

self_test() {
  local failures=0
  # expect WANT RUN_CANCELLED R1 C1 R2 C2 R3 C3
  expect() {
    local want="$1" got=0
    (
      unset GITHUB_STEP_SUMMARY
      RUN_CANCELLED="$2" RESULT_1="$3" CODE_1="$4" RESULT_2="$5" CODE_2="$6" RESULT_3="$7" CODE_3="$8"
      verdict
    ) > /dev/null 2>&1 || got=$?
    if [[ "$got" != "$want" ]]; then
      echo "FAIL: $* gave $got" >&2
      failures=$((failures + 1))
    fi
  }
  expect 0 false success 0 skipped "" skipped ""
  expect 1 false success 1 skipped "" skipped ""
  expect 0 false success 2 success 0 skipped ""
  expect 1 false success 2 success 2 success 2
  expect 0 false success 1 success 2 success 0
  # A runner that shut down is followed by a fresh one.
  expect 0 false cancelled "" success 0 skipped ""
  expect 0 false cancelled "" cancelled "" success 0
  expect 1 false cancelled "" success 1 skipped ""
  expect 1 false cancelled "" cancelled "" cancelled ""
  expect 1 false success 2 cancelled "" skipped ""
  # A run cancelled by its concurrency group or a person retries nothing.
  expect 1 true cancelled "" skipped "" skipped ""
  expect 1 true success 2 cancelled "" skipped ""
  # A job that failed outside the gate step fails the gate.
  expect 1 false failure "" skipped "" skipped ""
  expect 1 false failure "" success 0 skipped ""
  expect 1 false skipped "" skipped "" skipped ""
  expect 1 false "" "" "" "" "" ""
  if ((failures > 0)); then
    echo "verdict self-test: $failures case(s) failed" >&2
    exit 1
  fi
  echo "verdict self-test: ok"
}

case "${1:-}" in
  --self-test) self_test ;;
  "") verdict ;;
  *)
    echo "usage: scripts/perf/verdict.sh [--self-test]" >&2
    exit 2
    ;;
esac
