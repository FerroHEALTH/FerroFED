#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Says whether a release becomes the latest: the repository's latest GitHub
# release and the `latest` tag of both images. It prints `true` when the
# version is a release without a pre-release suffix and no vX.Y.Z tag of the
# remote names a higher version, and `false` otherwise. A security patch on
# an older supported line is published beside a newer line and so prints
# `false`; a tag that is itself the highest, re-run by a dispatch, prints
# `true`.
#
# Usage:
#   scripts/release/latest.sh <version> [<remote>]
#   scripts/release/latest.sh --self-test
#
# <version> is X.Y.Z or X.Y.Z-pre, with or without a leading v. <remote> is
# the git remote whose tags are read, origin by default. Exit 1 when the
# remote's tags cannot be read, 2 on a usage error. No specification governs
# which release is the latest; it is FerroFED's own design.
set -euo pipefail

die() {
  echo "latest: $*" >&2
  exit 1
}

usage() {
  echo "usage: $0 <version> [<remote>] | --self-test" >&2
  exit 2
}

# highest: the highest X.Y.Z on standard input, compared field by field as
# numbers, so 0.0.10 sorts above 0.0.9.
highest() {
  sort -t. -k1,1n -k2,2n -k3,3n | tail -n1
}

run() {
  local version="${1#v}" remote="${2:-origin}" refs top
  printf '%s\n' "$version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' || usage
  case "$version" in
  *-*)
    echo false
    return
    ;;
  esac
  refs="$(git ls-remote --tags --refs "$remote" 'v*')" ||
    die "the tags of $remote cannot be read, so whether $version is the latest release is unknown."
  top="$({
    printf '%s\n' "$refs" | sed -nE 's|.*refs/tags/v([0-9]+\.[0-9]+\.[0-9]+)$|\1|p'
    printf '%s\n' "$version"
  } | highest)"
  if [[ "$top" == "$version" ]]; then
    echo true
  else
    echo false
  fi
}

self_test() {
  local status out
  # Global, so the EXIT trap still sees it after the function returns.
  work="$(mktemp -d)"
  trap 'rm -rf -- "$work"' EXIT
  git init --quiet --bare "$work/remote.git"
  git init --quiet "$work/repo"
  git -C "$work/repo" -c user.name=t -c user.email=t@example.org -c commit.gpgsign=false -c core.hooksPath=/dev/null \
    commit --quiet --allow-empty -m one
  for tag in v0.0.9 v0.0.10 v1.2.3 v1.3.0 v1.3.1-rc.1 v1.9.0 v1.10.0; do
    git -C "$work/repo" -c tag.gpgsign=false tag "$tag"
  done
  git -C "$work/repo" -c core.hooksPath=/dev/null push --quiet "$work/remote.git" --tags
  expect() {
    local name="$1" want="$2" version="$3"
    status=0
    out="$(run "$version" "$work/remote.git" 2> "$work/err")" || status=$?
    [[ "$status" -eq 0 && "$out" == "$want" ]] || {
      echo "latest: self-test failed: $name printed '$out' (exit $status), wanted '$want'." >&2
      cat "$work/err" >&2
      exit 1
    }
  }
  expect "a patch above every tag" true 1.10.1
  expect "a new minor above every tag" true 1.11.0
  expect "a new major" true 2.0.0
  expect "the highest tag itself, re-run" true v1.10.0
  expect "a security patch on an older line" false 1.3.2
  expect "a patch on a line below a two-digit minor" false 1.9.1
  expect "a pre-release above every tag" false 2.0.0-rc.1
  expect "a patch below a two-digit patch" false 0.0.10
  status=0
  (run 1.0.0 "$work/missing.git") > /dev/null 2> "$work/err" || status=$?
  [[ "$status" -eq 1 ]] || {
    echo "latest: self-test failed: an unreadable remote exited $status, wanted 1." >&2
    exit 1
  }
  status=0
  (run 1.0 "$work/remote.git") > /dev/null 2> "$work/err" || status=$?
  [[ "$status" -eq 2 ]] || {
    echo "latest: self-test failed: a malformed version exited $status, wanted 2." >&2
    exit 1
  }
  echo "latest: self-test passed."
}

case "${1:-}" in
--self-test)
  [[ $# -eq 1 ]] || usage
  self_test
  ;;
"") usage ;;
*)
  [[ $# -le 2 ]] || usage
  run "$@"
  ;;
esac
