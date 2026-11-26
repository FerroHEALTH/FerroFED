#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The CPU instructions of each hot path's in-process work (#2003): the
# analysis and node rewrite of the façade's patient query and the merge of
# two members' ordered answers, each counted by callgrind in a release build
# of the example `hot_path_instructions` and held to its ceiling in
# perf/instructions.ceilings.
#
# A count is deterministic for one binary on one platform, so the lane runs
# on Linux, where valgrind runs. `--toggle-collect` counts inside the
# example's `measured` function and nowhere else. No specification governs
# the cost of a path: our own design.
#
# Usage:
#   scripts/perf/instructions.sh            build, measure, fail on a ceiling
#   scripts/perf/instructions.sh --no-build measure the example already built
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

ceilings="perf/instructions.ceilings"
example="target/release/examples/hot_path_instructions"

command -v valgrind >/dev/null || {
  echo "instructions: valgrind is not installed" >&2
  exit 2
}
if [[ "${1:-}" != "--no-build" ]]; then
  cargo build --locked --release -p ferrofed-server --example hot_path_instructions
fi

out="$(mktemp -d)"
trap 'rm -f "$out"/*.out; rmdir "$out"' EXIT

over=0
printf '%-16s %15s %15s\n' path instructions ceiling
while read -r path ceiling; do
  [[ -z "$path" || "$path" == \#* ]] && continue
  trace="$out/$path.out"
  valgrind --tool=callgrind --toggle-collect='*measured*' \
    --callgrind-out-file="$trace" "$example" "$path" 2>/dev/null
  count="$(awk '/^summary:/ { print $2 }' "$trace")"
  [[ -n "$count" ]] || {
    echo "instructions: no count for $path" >&2
    exit 2
  }
  verdict=""
  if [[ "$ceiling" != "report" && "$count" -gt "$ceiling" ]]; then
    verdict="  OVER"
    over=1
  fi
  printf '%-16s %15s %15s%s\n' "$path" "$count" "$ceiling" "$verdict"
done <"$ceilings"

if [[ "$over" -ne 0 ]]; then
  echo "instructions: a hot path exceeds its ceiling in $ceilings" >&2
  exit 1
fi
