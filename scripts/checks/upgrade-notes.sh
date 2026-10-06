#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The upgrade-notes guard (no specification governs this: our own design). A
# change that removes a configuration key, renames one past its deprecation,
# makes one mandatory or refuses a value the last release accepted stops a
# gateway that upgrades, so the release must say what to change in an
# "Upgrade notes" fragment, changelog.d/<issue>-<slug>.upgrade.md. This guard
# runs `ferrofed config check`, built from this tree, over the example
# configuration the last release attached (deploy/compose/ferrofed.toml and
# registry.toml at that tag), with a synthetic file for each `_file` secret
# and a synthetic ES384 key for each `key_file`, the mount paths rewritten to
# a temporary directory. When the check refuses it, at least one upgrade
# fragment must be pending.
#
# The last release is the newest tag vX.Y.Z with no pre-release part. When no
# such tag is local, as in a shallow CI checkout, it is read from origin and
# fetched alone.
#
# Usage:
#   scripts/checks/upgrade-notes.sh <ferrofed binary> [<tag>]
#   scripts/checks/upgrade-notes.sh --self-test
# Needs git, sed and openssl. Exit 1 naming the refusal; 2 on a usage error;
# 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly RELEASE=deploy/compose
readonly FRAGMENT_DIR=changelog.d

usage() {
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
}

# last_release: the newest vX.Y.Z tag, fetched from origin when none is local.
last_release() {
  local tag
  tag="$(git tag --list 'v[0-9]*' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -t. -k1,1V -k2,2n -k3,3n | tail -n1 || true)"
  if [[ -z "$tag" ]]; then
    tag="$(git ls-remote --tags --refs origin 'v*' |
      sed -nE 's|.*refs/tags/(v[0-9]+\.[0-9]+\.[0-9]+)$|\1|p' |
      sort -t. -k1,1V -k2,2n -k3,3n | tail -n1 || true)"
    [[ -n "$tag" ]] || return 1
    git fetch --quiet --no-tags --depth=1 origin "refs/tags/$tag:refs/tags/$tag"
  fi
  printf '%s' "$tag"
}

# check BINARY TAG: the guard itself.
check() {
  local binary="$1" tag="$2" work asset secret key out
  work="$(mktemp -d)"
  # Global, so the EXIT trap still sees it after the function returns.
  check_work="$work"
  trap 'rm -r -- "$check_work"' EXIT
  mkdir -p "$work/secrets" "$work/var"
  chmod 700 "$work/var"
  for asset in ferrofed.toml registry.toml; do
    if ! git show "$tag:$RELEASE/$asset" > "$work/$asset" 2> /dev/null; then
      echo "upgrade-notes: $tag attached no $RELEASE/$asset; nothing to compare." >&2
      return 0
    fi
  done
  while IFS= read -r secret; do
    printf 'synthetic-%s\n' "$secret" > "$work/secrets/$secret"
  done < <(sed -nE 's|^[a-z_]+_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
  while IFS= read -r key; do
    openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 \
      -out "$work/secrets/$key" 2> /dev/null || {
      echo "upgrade-notes: a synthetic signing key could not be generated for $key" >&2
      return 1
    }
  done < <(sed -nE 's|^key_file[[:space:]]*=[[:space:]]*"/run/secrets/ferrofed/([^"]+)"[[:space:]]*$|\1|p' "$work/ferrofed.toml")
  sed -i.orig -e "s|/run/secrets/ferrofed/|$work/secrets/|g" \
    -e "s|/etc/ferrofed/|$work/|g" -e "s|/var/lib/ferrofed|$work/var|g" "$work/ferrofed.toml"

  if out="$(env -i PATH="$PATH" "$binary" config check --config "$work/ferrofed.toml" 2>&1)"; then
    echo "upgrade-notes: this build accepts the $tag example configuration; no upgrade note is required."
    return 0
  fi
  local notes=()
  for file in "$FRAGMENT_DIR"/*.upgrade.md; do
    [[ -e "$file" ]] && notes+=("$file")
  done
  if [[ "${#notes[@]}" -eq 0 ]]; then
    echo "::error::this build refuses the configuration $tag attached, so a gateway that upgrades stops: $out" >&2
    echo "upgrade-notes: add $FRAGMENT_DIR/<issue>-<slug>.upgrade.md saying what an operator must change (changelog.d/README.md)." >&2
    return 1
  fi
  echo "upgrade-notes: this build refuses the $tag example configuration ($out); the upgrade notes are ${notes[*]}."
}

self_test() {
  local script status
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'chmod -R u+w "$work" && rm -r -- "$work"' EXIT
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  mkdir -p "$work/repo/scripts/checks" "$work/repo/$RELEASE" "$work/repo/$FRAGMENT_DIR"
  cp "$script" "$work/repo/scripts/checks/upgrade-notes.sh"
  stub_git() {
    git -C "$work/repo" -c user.name=self-test -c user.email=self-test@example.org \
      -c commit.gpgsign=false -c tag.gpgsign=false -c core.hooksPath=/dev/null "$@"
  }
  flunk() {
    echo "upgrade-notes: self-test failed: $*" >&2
    cat "$work/out" >&2
    exit 1
  }
  # A stub ferrofed: it refuses a configuration that names `retired_key`,
  # and one whose secret file was not written.
  cat > "$work/ferrofed" <<'EOF'
#!/usr/bin/env bash
config="$4"
secret="$(sed -nE 's|^token_file = "([^"]+)"$|\1|p' "$config")"
[[ -f "$secret" ]] || { echo "token_file names $secret, which could not be read"; exit 78; }
if grep -q '^retired_key' "$config"; then echo "unknown field retired_key"; exit 78; fi
echo "ferrofed: the configuration is valid"
EOF
  chmod +x "$work/ferrofed"
  expect() {
    local name="$1" want="$2"
    status=0
    (cd "$work/repo" && bash scripts/checks/upgrade-notes.sh "$work/ferrofed" v0.0.1) > "$work/out" 2>&1 || status=$?
    [[ "$status" -eq "$want" ]] || flunk "$name exited $status, wanted $want."
  }

  stub_git init -q -b main
  printf 'token_file = "/run/secrets/ferrofed/token"\n' > "$work/repo/$RELEASE/ferrofed.toml"
  printf '# members\n' > "$work/repo/$RELEASE/registry.toml"
  stub_git add -A
  stub_git commit -q -m "the release"
  stub_git tag v0.0.1
  expect "an example this build accepts" 0
  grep -q 'no upgrade note is required' "$work/out" || flunk "the pass did not say so."

  printf 'retired_key = 1\ntoken_file = "/run/secrets/ferrofed/token"\n' > "$work/repo/$RELEASE/ferrofed.toml"
  stub_git commit -q -a -m "an example with a retired key"
  stub_git tag -d v0.0.1 > /dev/null
  stub_git tag v0.0.1
  expect "a refused example with no upgrade note" 1
  grep -q 'retired_key' "$work/out" || flunk "the refusal was not quoted."
  printf -- '- Remove retired_key.\n' > "$work/repo/$FRAGMENT_DIR/1-retired.upgrade.md"
  expect "a refused example with an upgrade note" 0
  grep -q '1-retired.upgrade.md' "$work/out" || flunk "the upgrade note was not named."
  echo "upgrade-notes: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
"" | -*) usage ;;
*)
  [[ $# -le 2 ]] || usage
  binary="$1"
  [[ -x "$binary" ]] || {
    echo "upgrade-notes: $binary is not an executable ferrofed binary." >&2
    exit 2
  }
  binary="$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")"
  cd "$(dirname "$0")/../.."
  tag="${2:-}"
  if [[ -z "$tag" ]]; then
    tag="$(last_release)" || {
      echo "upgrade-notes: no vX.Y.Z release tag, locally or at origin." >&2
      exit 2
    }
  fi
  check "$binary" "$tag"
  ;;
esac
