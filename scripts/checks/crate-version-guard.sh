#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crate bump rule (no specification governs it: our own design): a change
# that alters the PACKAGED content of a `crates/*` member (what its `include`
# ships: `src/**`, `README.md`, `LICENSE`, `Cargo.toml`) bumps THAT member's
# version in the same change, because a published version is immutable. A root
# `[workspace.dependencies]` entry a member consumes is packaged content too:
# `cargo package` renders the concrete requirement.
#
# Versions are per crate, not lockstep: a member that holds its crates.io name
# sits at the 0.0.0 placeholder until its first real version, as the pin
# matrix, docs/VERSIONS.md, records.
#
#   crate-version-guard.sh <base-ref> [head-ref|WORKTREE]
#   crate-version-guard.sh --self-test
#
# The change is what the head added since it forked from the base: the paths
# come from the diff against the merge base of the two, so a base that moved
# on after the fork (a bump on main a pull request is behind) is never read as
# the change's own. The version a changed member must move away from is the
# one the base holds now, so two changes cannot both claim one version.
#
# The head ref may be the literal WORKTREE, which compares the base with the
# tree as it stands rather than with a commit. That is what the pre-commit hook
# passes: the manifests and Cargo.lock are read from disk either way, so the
# changed set has to come from the same place to agree with them. In CI the
# disk holds the pull request merged into its base.
#
# Exit 0 when no packaged content changed, or every member whose packaged
# content changed also moved its version, with the root requirement and
# Cargo.lock following. Exit 1 otherwise, and 2 on a usage error or a base and
# head with no merge base, with the reason on stderr. The `no-crate-bump`
# pull-request label is the CI escape for a diff that provably does not alter
# packaged bytes; this script does not read labels.
set -euo pipefail

# The self-test builds a stub repository in a temporary directory, with a copy
# of this script in it, and runs the guard there as CI does: the base is main's
# tip, the head is the branch, and the disk holds the branch merged into main.
self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'chmod -R u+w "$work" && rm -r "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$work/scripts/checks"
  cp "$script" "$work/scripts/checks/crate-version-guard.sh"

  # A repository of its own, so no global hook, signing key or identity of the
  # caller's reaches it.
  stub_git() {
    git -C "$work" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  crate() {
    local name="$1" version="$2"
    mkdir -p "$work/crates/$name/src"
    printf '[package]\nname = "%s"\nversion = "%s"\n' "$name" "$version" > "$work/crates/$name/Cargo.toml"
  }
  lock() {
    local x="$1" y="$2"
    printf '[[package]]\nname = "x"\nversion = "%s"\n\n[[package]]\nname = "y"\nversion = "%s"\n' "$x" "$y" > "$work/Cargo.lock"
  }
  commit() {
    local message="$1"
    stub_git add -A
    stub_git commit -q -m "$message"
  }
  # expect NAME WANT BRANCH: the guard over BRANCH merged into main exits WANT.
  expect() {
    local name=$1 want=$2 branch=$3
    stub_git checkout -q --detach main
    stub_git merge -q --no-edit "$branch" > /dev/null
    status=0
    (cd "$work" && bash scripts/checks/crate-version-guard.sh main "$branch") > "$work/out" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "crate-version-guard: self-test failed: $name exited $status, wanted $want." >&2
      cat "$work/out" >&2
      exit 1
    fi
  }

  stub_git init -q -b main
  printf '[workspace]\nmembers = ["crates/x", "crates/y"]\n\n[workspace.dependencies]\nserde = "1"\n' > "$work/Cargo.toml"
  crate x 0.0.1
  crate y 0.0.1
  echo 'pub fn x() {}' > "$work/crates/x/src/lib.rs"
  echo 'pub fn y() {}' > "$work/crates/y/src/lib.rs"
  lock 0.0.1 0.0.1
  commit "the fork point"

  stub_git checkout -q -b untouched
  echo 'notes' > "$work/NOTES.md"
  commit "a change outside every crate"
  stub_git checkout -q main
  stub_git checkout -q -b unbumped
  echo 'pub fn x2() {}' >> "$work/crates/x/src/lib.rs"
  commit "packaged content of x, no bump"
  stub_git checkout -q main
  stub_git checkout -q -b bumped
  echo 'pub fn x2() {}' >> "$work/crates/x/src/lib.rs"
  crate x 0.0.2
  lock 0.0.2 0.0.1
  commit "packaged content of x, bumped"

  stub_git checkout -q main
  echo 'pub fn y2() {}' >> "$work/crates/y/src/lib.rs"
  crate y 0.0.2
  lock 0.0.1 0.0.2
  commit "main bumps y after every branch forked"

  expect "a branch behind a main that bumped a crate it never touched" 0 untouched
  expect "a branch that changed packaged content without a bump" 1 unbumped
  if ! grep -q 'packaged content of x changed but its version is still 0.0.1' "$work/out"; then
    echo "crate-version-guard: self-test failed: the unbumped branch failed without naming x." >&2
    cat "$work/out" >&2
    exit 1
  fi
  if grep -q 'of y changed' "$work/out"; then
    echo "crate-version-guard: self-test failed: main's own bump of y was read as the branch's." >&2
    exit 1
  fi
  expect "a branch behind main that bumped what it changed" 0 bumped
  echo "crate-version-guard: self-test passed."
}

if [[ "${1:-}" = "--self-test" && $# -eq 1 ]]; then
  self_test
  exit 0
fi

cd "$(dirname "$0")/../.."

if [[ $# -lt 1 || $# -gt 2 || -z "${1:-}" ]]; then
  echo "usage: crate-version-guard.sh <base-ref> [head-ref|WORKTREE] | --self-test" >&2
  exit 2
fi
base="$1"
head="${2:-HEAD}"
# The head that names the working tree instead of a commit.
readonly WORKTREE=WORKTREE

# The commit the change forked from the base at; the worktree forks where its
# HEAD does.
tip="$head"
[[ "$head" != "$WORKTREE" ]] || tip=HEAD
if ! fork="$(git merge-base "$base" "$tip")"; then
  echo "crate-version-guard: $base and $head have no merge base; check out the full history (fetch-depth: 0)." >&2
  exit 2
fi

# `git diff <fork> -- …` with no second ref reads the working tree, which is
# the WORKTREE head; everything else names two commits.
changed_paths() {
  if [[ "$head" = "$WORKTREE" ]]; then git diff --name-only "$fork" --; else git diff --name-only "$fork" "$head" --; fi
}
diff_text() {
  if [[ "$head" = "$WORKTREE" ]]; then git diff "$fork" -- "$@"; else git diff "$fork" "$head" -- "$@"; fi
}
head_file() {
  local path="$1"
  if [[ "$head" = "$WORKTREE" ]]; then cat "$path"; else git show "$head:$path"; fi
}

# No `crates/*` member yet (the repository before its Cargo workspace) means no
# packaged content, and nothing to guard.
if ! compgen -G 'crates/*/Cargo.toml' > /dev/null; then
  echo "crate-version-guard: no crates/* member exists yet, nothing to guard."
  exit 0
fi

changed="$(changed_paths)"

# Only the `[workspace.dependencies]` table renders into a packaged manifest;
# the `[workspace.package]` keys (the product `version`, edition, licence) are
# read through `key.workspace = true` and change no crate's packaged bytes.
workspace_dependencies() {
  awk '/^\[workspace\.dependencies\]/{p=1; next} /^\[/{p=0} p && /^[A-Za-z0-9_-]+[[:space:]]*=/{sub(/[[:space:]]*=.*/, ""); print}'
}

# The workspace dependency names this change touched, restricted to names the
# table actually declares on either side.
touched_dependencies=""
if grep -qx 'Cargo.toml' <<<"$changed"; then
  diff_names="$(diff_text Cargo.toml |
    grep -E '^[+-][A-Za-z0-9_-]+[[:space:]]*=' |
    sed -E 's/^[+-]//; s/[[:space:]]*=.*//' | sort -u || true)"
  # The base has no Cargo.toml on the change that adds the workspace, and that
  # absence must not end the script under `set -e`.
  dependency_names="$( { head_file Cargo.toml; git show "$base:Cargo.toml" 2> /dev/null || true; } | workspace_dependencies)"
  for name in $diff_names; do
    grep -qx "$name" <<<"$dependency_names" || continue
    touched_dependencies="$touched_dependencies $name"
  done
fi

package_field() {
  local key="$1"
  awk -F'"' -v key="$key" '/^\[package\]/{p=1} p && $0 ~ "^" key " = " {print $2; exit}'
}

fail=0
bumped=""
clean=""
reserved=""
for manifest in crates/*/Cargo.toml; do
  crate="$(dirname "$manifest")"
  name="$(package_field name < "$manifest")"

  # A here-string, not a pipe: `grep -q` closes its input on the first match,
  # and under `pipefail` the SIGPIPE'd writer would fail the whole test.
  packaged=0
  # Everything a manifest's `include` can ship: src, the schemas a crate embeds,
  # the README, the licence and the manifest itself.
  if grep -qE "^${crate}/(src/|schemas/|README\.md$|LICENSE$|Cargo\.toml$)" <<<"$changed"; then
    packaged=1
  fi
  if [[ "$packaged" -eq 0 ]]; then
    for dep in $touched_dependencies; do
      if grep -qE "^${dep}(\.workspace)?[[:space:]]*=" "$manifest"; then
        echo "crate-version-guard: $name consumes workspace dependency '$dep', which this change moved."
        packaged=1
        break
      fi
    done
  fi
  if [[ "$packaged" -eq 0 ]]; then
    clean="$clean $name"
    continue
  fi

  new_ver="$(package_field version < "$manifest")"
  old_ver="$(git show "$base:$manifest" 2>/dev/null | package_field version || true)"
  # A 0.0.0 manifest is a crates.io name reservation outside the crate line
  # the pin matrix records; its content changes until the first real version.
  if [[ "$new_ver" = "0.0.0" ]] && [[ "${old_ver:-0.0.0}" = "0.0.0" ]]; then
    reserved="$reserved $name"
    continue
  fi
  if [[ -n "$old_ver" ]] && [[ "$old_ver" = "$new_ver" ]]; then
    echo "::error::packaged content of $name changed but its version is still $new_ver. Bump $manifest, move any internal requirement in the root Cargo.toml with it, refresh Cargo.lock, or apply the 'no-crate-bump' label when the diff provably does not alter packaged bytes." >&2
    fail=1
    continue
  fi

  # An internal requirement only exists for a crate other members depend on;
  # when it does, it must name the same version.
  req="$(grep -E "^${name} = \{ path = \"${crate}\", version = \"" Cargo.toml |
    sed -E 's/.*version = "([^"]+)".*/\1/' || true)"
  if [[ -n "$req" ]] && [[ "$req" != "$new_ver" ]]; then
    echo "::error::root Cargo.toml requires $name $req, its manifest is at $new_ver." >&2
    fail=1
  fi
  locked="$(awk -v n="\"$name\"" '$1 == "name" && $3 == n { hit = 1; next } hit && $1 == "version" { gsub(/"/, "", $3); print $3; exit }' Cargo.lock)"
  if [[ "$locked" != "$new_ver" ]]; then
    echo "::error::Cargo.lock records $name ${locked:-nothing}, its manifest is at $new_ver; run cargo update -w and commit the lock." >&2
    fail=1
  fi
  bumped="$bumped $name@${old_ver:-<new>}->$new_ver"
done

[[ "$fail" -eq 0 ]] || exit 1
if [[ -z "$bumped" ]]; then
  echo "crate-version-guard: no packaged content of crates/* changed."
else
  echo "crate-version-guard: packaged content changed and the version moved:$bumped"
fi
[[ -z "$clean" ]] || echo "crate-version-guard: unchanged packaged content:$clean"
[[ -z "$reserved" ]] || echo "crate-version-guard: 0.0.0 name reservations, outside the line:$reserved"
