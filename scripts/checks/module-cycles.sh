#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Module-cycle guard: no two top-level modules of ferrofed-server import each
# other outside tests (#1580, the target shape of #1482: modules point down).
#
# tools/module-cycles parses the crate with syn. The top-level modules are
# the `mod` items of src/lib.rs, and a module imports another when one of its
# files names it through `crate::` (a path, a `use` tree with its groups, or
# the tokens of a macro or an attribute), or through `super::` in a file
# directly under src/. An item under a `#[cfg(...)]` that names `test`, a
# module only such an item declares, and a file under tests/ are left out.
#
# Usage:
#   scripts/checks/module-cycles.sh              # ferrofed-server
#   scripts/checks/module-cycles.sh --self-test  # a pair, a group, a macro and test code
#
# Exit 0 = no pair imports each other, 1 = a pair does (each named), 2 =
# usage or the tool failed.
#
# No specification governs this: our own design.
set -Eeuo pipefail

# shellcheck source=scripts/lib/guard-args.sh
. "$(dirname "$0")/../lib/guard-args.sh"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly GRAMMAR="[--self-test]"
readonly CRATE_SRC=app/ferrofed-server/src
export LC_ALL=C

# Lists the pairs of top-level modules of the crate at $1 that import each
# other, one `a b` per line.
pairs() {
  cargo run --manifest-path "$root/Cargo.toml" --locked --quiet --release -p module-cycles -- "$1"
}

check() {
  local found
  found="$(pairs "$1")" || {
    echo "module-cycles: the tool failed on $1" >&2
    return 2
  }
  if [[ -n "$found" ]]; then
    echo "module-cycles: these top-level modules of $1 import each other outside tests:" >&2
    printf '%s\n' "$found" | sed 's/ / and /; s/^/  /' >&2
    echo "Give each pair one direction: move what both read below both, or let the lower one own a seam the upper one implements (.claude/rules/rust-style.md)." >&2
    return 1
  fi
  echo "module-cycles: OK, no two top-level modules of $1 import each other."
}

self_test() {
  local work src found
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now, while it is still in scope
  trap "rm -rf '$work'" RETURN
  src="$work/src"
  mkdir -p "$src/b" "$src/c"
  printf 'pub mod a;\npub mod b;\npub mod c;\npub mod d;\n#[cfg(test)]\nmod t;\n' >"$src/lib.rs"
  printf 'use crate::b::B;\npub struct A;\n' >"$src/a.rs"
  printf 'mod inner;\npub struct B;\n' >"$src/b/mod.rs"
  printf 'use crate::{a::A, d};\n' >"$src/b/inner.rs"
  printf '#[cfg(test)]\nmod tests;\npub fn f() { tracing::warn!(x = crate::d::g()); }\n' >"$src/c.rs"
  printf 'use crate::d;\n' >"$src/c/tests.rs"
  printf '#[cfg(all(test, feature = "x"))]\nfn h() { crate::c::f(); }\npub fn g() {}\n' >"$src/d.rs"
  found="$(pairs "$src")"
  if [[ "$found" != "a b" ]]; then
    echo "module-cycles: self-test FAILED: expected exactly the pair \"a b\", got: ${found:-nothing}" >&2
    return 1
  fi
  if check "$src" >/dev/null 2>&1; then
    echo "module-cycles: self-test FAILED: a crate with a pair passed" >&2
    return 1
  fi
  printf 'pub struct A;\n' >"$src/a.rs"
  if ! check "$src" >/dev/null 2>&1; then
    echo "module-cycles: self-test FAILED: a crate with no pair was refused" >&2
    return 1
  fi
  echo "module-cycles: self-test OK."
}

case "${1:-}" in
  --self-test)
    [[ $# -eq 1 ]] || guard_usage "$GRAMMAR"
    self_test
    ;;
  "")
    check "$CRATE_SRC"
    ;;
  *)
    guard_usage "$GRAMMAR"
    ;;
esac
