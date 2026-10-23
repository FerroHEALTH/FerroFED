#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Measures the operator console's release site bundle (app/ferrofed-console)
# and fails when its brotli-compressed WebAssembly is over its byte budget
# (no specification governs this: our own design).
#
# It reads the bundle scripts/release/console-site.sh --release writes, prints
# the WebAssembly and its JavaScript glue raw, gzip-compressed (gzip -9) and
# brotli-compressed (brotli -q 11) as a Markdown table, and appends the table
# to $GITHUB_STEP_SUMMARY when that is set. The budget is read from the budget
# table of the console's Leptos rule file, beside its reasoning, so the number
# lives in one place.
#
# Usage: scripts/checks/console-bundle.sh [pkg-dir]
#   pkg-dir defaults to target/site/pkg.
#
# Exit 0 = within the budget. Exit 1 = over the budget, a bundle file missing,
# a bundle the last release build did not write (target/console-site.sha256),
# a compressor missing, or the budget unreadable. Exit 2 = a usage error.
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
rule="$root/.claude/rules/leptos-ui.md"
name="ferrofed-console"

case "$#" in
  0) pkg="$root/target/site/pkg" ;;
  1) pkg="$1" ;;
  *)
    echo "usage: scripts/checks/console-bundle.sh [pkg-dir]" >&2
    exit 2
    ;;
esac

for tool in gzip brotli; do
  if ! command -v "$tool" > /dev/null; then
    echo "console-bundle: $tool is not on PATH; the bundle cannot be measured." >&2
    exit 1
  fi
done

# The bundle the last release build wrote, and no other: the stamp
# scripts/release/console-site.sh --release leaves on success names the
# digests of the two files, so a stale, a failed or a debug build is refused.
stamp="$root/target/console-site.sha256"
for file in "$name.wasm" "$name.js"; do
  if [ ! -s "$pkg/$file" ]; then
    echo "console-bundle: $pkg/$file is missing; run scripts/release/console-site.sh --release first." >&2
    exit 1
  fi
done
if [ ! -s "$stamp" ]; then
  echo "console-bundle: $stamp is missing: the last site build failed or was not a release build; run scripts/release/console-site.sh --release first." >&2
  exit 1
fi
if command -v sha256sum > /dev/null; then sum=(sha256sum); else sum=(shasum -a 256); fi
if ! (cd "$pkg" && "${sum[@]}" -c --status "$stamp"); then
  echo "console-bundle: $pkg is not the bundle the last release build wrote ($stamp); run scripts/release/console-site.sh --release first." >&2
  exit 1
fi

for file in "$name.wasm" "$name.js"; do
  if [ ! -s "$pkg/$file" ]; then
    echo "console-bundle: $pkg/$file is missing; run scripts/release/console-site.sh --release first." >&2
    exit 1
  fi
done

# The one row of the budget table whose first cell names the gating measure.
budgets="$(awk -F'|' '
  $2 ~ /^[[:space:]]*WebAssembly, brotli-compressed[[:space:]]*$/ {
    value = $3
    sub(/^[[:space:]]*/, "", value)
    sub(/[[:space:]]*bytes.*$/, "", value)
    print value
  }
' "$rule")"
if ! [[ "$budgets" =~ ^[0-9]+$ ]]; then
  echo "console-bundle: $rule has no single \"| WebAssembly, brotli-compressed | N bytes |\" budget row." >&2
  exit 1
fi
budget="$budgets"

raw() { wc -c < "$1" | tr -d '[:space:]'; }
gz() { gzip -9 -c "$1" | wc -c | tr -d '[:space:]'; }
br() { brotli -q 11 -c "$1" | wc -c | tr -d '[:space:]'; }

wasm="$pkg/$name.wasm"
js="$pkg/$name.js"
wasm_br="$(br "$wasm")"
percent=$((wasm_br * 100 / budget))

table="$(
  echo "### Operator console bundle"
  echo
  echo "| File | Raw | gzip -9 | brotli -q 11 |"
  echo "|---|---:|---:|---:|"
  echo "| \`$name.wasm\` | $(raw "$wasm") | $(gz "$wasm") | $wasm_br |"
  echo "| \`$name.js\` | $(raw "$js") | $(gz "$js") | $(br "$js") |"
  echo
  echo "Sizes in bytes. Budget: $budget bytes of brotli-compressed WebAssembly; this build is $wasm_br bytes, $percent% of it."
)"

echo "$table"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  echo "$table" >> "$GITHUB_STEP_SUMMARY"
fi

if [ "$wasm_br" -gt "$budget" ]; then
  echo "::error file=.claude/rules/leptos-ui.md::The brotli-compressed WebAssembly is $wasm_br bytes, over the $budget-byte budget of .claude/rules/leptos-ui.md §12. Shrink the bundle, or raise the budget there with the reason." >&2
  exit 1
fi
echo "console-bundle: $wasm_br of $budget bytes, within the budget"
