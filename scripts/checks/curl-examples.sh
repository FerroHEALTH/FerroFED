#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The curl example guard: a `curl` example in the documentation that takes
# its body from a here-document reads that body with `-d @-` (no
# specification governs this: our own design). curl sends the value of `-d`
# as the body, so `-d 5820 <<'EOF'` posts the number and the here-document
# goes nowhere, and the gateway refuses the first query a reader runs.
#
# It reads every tracked Markdown file outside the vendored specifications
# and refuses a line that opens a here-document and passes `-d`, `--data`,
# `--data-binary`, `--data-raw` or `--json` a value that does not start with
# `@` (`@-` for standard input, `@<file>` for a file).
#
# Usage:
#   scripts/checks/curl-examples.sh              check the documentation
#   scripts/checks/curl-examples.sh --self-test  prove each refusal and pass
# Exit 1 naming each line; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# judge: reads `PATH:LINE:TEXT` lines and prints a finding per refused line,
# exiting 1 when there is one.
judge() {
  local bad=0 n=0 path rest number text value
  local heredoc="<<-?[[:space:]]*['\"]?[A-Za-z_]+"
  local data="(^|[[:space:]])(-d|--data|--data-binary|--data-raw|--json)([[:space:]]+|=)([^[:space:]]+)"
  while IFS= read -r line; do
    path="${line%%:*}"
    rest="${line#*:}"
    number="${rest%%:*}"
    text="${rest#*:}"
    [[ "$text" =~ $heredoc ]] || continue
    n=$((n + 1))
    [[ "$text" =~ $data ]] || continue
    value="${BASH_REMATCH[4]}"
    value="${value#\'}"
    value="${value#\"}"
    if [[ "$value" != @* ]]; then
      echo "::error file=$path,line=$number::$path:$number passes curl the body '$value' and opens a here-document the command never reads; write -d @- to send the here-document."
      bad=$((bad + 1))
    fi
  done
  if [[ "$bad" -gt 0 ]]; then
    echo "curl-examples: $bad of $n here-document line(s) refused"
    return 1
  fi
  echo "curl-examples: $n here-document line(s), every curl body read from @"
}

# lines: every here-document line of the tracked Markdown, as PATH:LINE:TEXT.
lines() {
  git ls-files -z -- '*.md' ':!docs/specs/**' | xargs -0 grep -n -H -E '<<' -- || true
}

self_test() {
  local failed=0 n=0 out status

  # expect WANT LINE: the PATH:LINE:TEXT line LINE exits WANT.
  expect() {
    local want="$1"
    n=$((n + 1))
    status=0
    out="$(printf '%s\n' "$2" | judge 2>&1)" || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "curl-examples: self-test failed: '$2' exited $status, wanted $want." >&2
      printf '%s\n' "$out" | sed 's/^/  /' >&2
      failed=1
    fi
  }
  expect 0 "README.md:1:  -H 'Content-Type: application/json' -d @- <<'EOF'"
  expect 0 "a.md:2:  --data-binary @body.json <<EOF"
  expect 0 "a.md:3:cat > x.toml <<'EOF'"
  expect 0 "a.md:4:curl -d 5820 https://example.org"
  expect 1 "README.md:5:  -H 'Content-Type: application/json' -d 5820 <<'EOF'"
  expect 1 "a.md:6:  --data='{}' <<EOF"
  expect 1 "a.md:7:  --json \"\$body\" <<-EOF"
  expect 1 "a.md:8:  --data-raw - << \"EOF\""
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "curl-examples: self-test OK ($n cases)."
}

case "${1:-}" in
  --self-test) self_test ;;
  '') lines | judge ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
