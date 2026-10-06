#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The script mode guard: every shell program the repository runs is committed
# executable (no specification governs this: our own design). A program
# committed as mode 100644 runs from a checkout only as `bash script.sh`, and
# the documentation, the vendored-inputs rule and each script's own usage line
# call it as `scripts/vendor/x.sh`.
#
# It reads the index (`git ls-files -s`) under scripts/ and the session hooks
# and refuses two kinds of file:
#
#   not executable  a file whose first line is a `#!` interpreter line and
#                   whose mode is not 100755.
#   no interpreter  a scripts/<area>/<name>.sh program, the layout of every
#                   check, vendor, release and tracker script, with no `#!`
#                   first line. A shared library lives one level deeper
#                   (scripts/vendor/lib/) and is sourced, never run.
#
# Usage:
#   scripts/checks/script-modes.sh              check the index
#   scripts/checks/script-modes.sh --self-test  prove each refusal and pass
# Exit 1 naming each file; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# judge: reads `MODE<TAB>PATH<TAB>FIRST-LINE` lines and prints a finding per
# refused file, exiting 1 when there is one.
judge() {
  awk -F '\t' '
    {
      n++
      mode = $1; path = $2; first = $3
      shebang = (substr(first, 1, 2) == "#!")
      if (shebang && mode != "100755") {
        printf "::error::%s is a program (%s) committed as mode %s; run git update-index --chmod=+x %s\n", path, first, mode, path
        bad++
      } else if (!shebang && path ~ /^scripts\/[^\/]+\/[^\/]+\.sh$/) {
        printf "::error::%s is a program with no #! interpreter line\n", path
        bad++
      }
    }
    END {
      if (bad > 0) { printf "script-modes: %d of %d file(s) refused\n", bad, n; exit 1 }
      printf "script-modes: %d file(s), every program executable\n", n
    }'
}

# index: one `MODE<TAB>PATH<TAB>FIRST-LINE` line per tracked file under
# scripts/ and the session hooks, the first line read from the working tree.
index() {
  local meta mode path first
  git ls-files -s -- scripts .claude/hooks | while IFS=$'\t' read -r meta path; do
    mode="${meta%% *}"
    first=""
    if [[ -f "$path" ]]; then
      IFS= read -r first < "$path" || true
    fi
    first="${first//$'\t'/ }"
    printf '%s\t%s\t%s\n' "$mode" "$path" "$first"
  done
}

self_test() {
  local failed=0 n=0 out status

  # expect WANT LINE: the index line LINE exits WANT.
  expect() {
    local want="$1"
    n=$((n + 1))
    status=0
    out="$(printf '%s\n' "$2" | judge 2>&1)" || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "script-modes: self-test failed: '$2' exited $status, wanted $want." >&2
      printf '%s\n' "$out" | sed 's/^/  /' >&2
      failed=1
    fi
  }
  expect 0 $'100755\tscripts/vendor/aql.sh\t#!/usr/bin/env bash'
  expect 0 $'100755\t.claude/hooks/session-start.sh\t#!/usr/bin/env bash'
  expect 0 $'100644\tscripts/vendor/lib/corpus.sh\t# SPDX-FileCopyrightText: Cadasto B.V.'
  expect 0 $'100644\tscripts/checks/file-length-allow.txt\tcrates/x/src/lib.rs'
  expect 1 $'100644\tscripts/vendor/openehr-rm.sh\t#!/usr/bin/env bash'
  expect 1 $'100644\t.claude/hooks/guard.py\t#!/usr/bin/env python3'
  expect 1 $'100644\tscripts/checks/new-guard.sh\t# SPDX-FileCopyrightText: Cadasto B.V.'
  expect 1 $'100755\tscripts/release/notes.sh\t'
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "script-modes: self-test OK ($n cases)."
}

case "${1:-}" in
  --self-test) self_test ;;
  '') index | judge ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
