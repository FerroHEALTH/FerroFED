#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Writes the Kubernetes package a release attaches: the kustomize tree of
# deploy/kubernetes (the base, the components and the example overlay) with
# the gateway and console images pinned to the digests the release pushed,
# packed into one tarball, and the SHA-256 of that tarball in `sha256sum`
# form beside it.
#
#   <out-dir>/ferrofed-<tag>-kubernetes.tar.gz
#   <out-dir>/ferrofed-<tag>-kubernetes.tar.gz.sha256sum
#
# The base's kustomization gains an `images` entry for the gateway and the
# console component's one for the console, each keeping the tag the
# manifests name and adding the digest, so `kubectl apply -k` runs exactly
# the image the release scanned and signed. A manifest whose tag is not the
# release's version is refused, since the digest would then pin another
# release's tag.
#
# Usage:
#   scripts/release/kubernetes-package.sh <tag> <gateway-digest> <console-digest> <out-dir>
#   scripts/release/kubernetes-package.sh --self-test
#
# Needs tar and `sha256sum` or `shasum`. No specification governs this
# package: our own design.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly TREE=deploy/kubernetes

die() {
  echo "kubernetes-package: $*" >&2
  exit 1
}

# sha256 FILE: the SHA-256 line of FILE in sha256sum form, run where FILE is.
sha256() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi
}

# pin KUSTOMIZATION IMAGE TAG DIGEST: appends an images entry pinning IMAGE
# at TAG to DIGEST to KUSTOMIZATION.
pin() {
  local file="$1" image="$2" tag="$3" digest="$4"
  grep -qE '^images:' "$file" && die "$file already carries an images list"
  printf 'images:\n  - name: %s\n    newTag: %s\n    digest: %s\n' \
    "$image" "$tag" "$digest" >> "$file"
}

# manifest_tag FILE IMAGE: the one tag FILE runs IMAGE at.
manifest_tag() {
  sed -nE "s|^[[:space:]]*image:[[:space:]]*ghcr\\.io/ferrohealth/$2:([^@[:space:]]+)[[:space:]]*\$|\\1|p" "$1" | sort -u
}

# package TAG GATEWAY CONSOLE OUT: writes the package of TAG into OUT.
package() {
  local tag="$1" gateway="$2" console="$3" out="$4" version name work
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "$tag is not a release tag"
  version="${tag#v}"
  for digest in "$gateway" "$console"; do
    [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || die "'$digest' is not a sha256 image digest"
  done
  [[ "$(manifest_tag "$TREE/base/statefulset.yaml" ferrofed)" == "$version" ]] ||
    die "$TREE/base/statefulset.yaml does not run the gateway at $version"
  [[ "$(manifest_tag "$TREE/components/console/deployment.yaml" ferrofed-console)" == "$version" ]] ||
    die "$TREE/components/console/deployment.yaml does not run the console at $version"
  name="ferrofed-${tag}-kubernetes"
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now, while it is still in scope
  trap "rm -rf '$work'" RETURN
  mkdir -p "$work/$name" "$out"
  cp -R "$TREE/." "$work/$name/"
  pin "$work/$name/base/kustomization.yaml" ghcr.io/ferrohealth/ferrofed "$version" "$gateway"
  pin "$work/$name/components/console/kustomization.yaml" ghcr.io/ferrohealth/ferrofed-console "$version" "$console"
  tar -C "$work" -czf "$out/$name.tar.gz" "$name"
  (cd "$out" && sha256 "$name.tar.gz" > "$name.tar.gz.sha256sum")
  echo "kubernetes-package: wrote $out/$name.tar.gz"
}

self_test() {
  local out version gateway console listing
  out="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $out now, while it is still in scope
  trap "rm -rf '$out'" RETURN
  version="$(manifest_tag "$TREE/base/statefulset.yaml" ferrofed)"
  gateway="sha256:$(printf '1%.0s' {1..64})"
  console="sha256:$(printf '2%.0s' {1..64})"
  package "v$version" "$gateway" "$console" "$out" > /dev/null
  listing="$(tar -xzOf "$out/ferrofed-v$version-kubernetes.tar.gz" \
    "ferrofed-v$version-kubernetes/base/kustomization.yaml" \
    "ferrofed-v$version-kubernetes/components/console/kustomization.yaml")"
  grep -qF "digest: $gateway" <<< "$listing" || die "self-test: the gateway digest is not pinned"
  grep -qF "digest: $console" <<< "$listing" || die "self-test: the console digest is not pinned"
  (cd "$out" && if command -v sha256sum > /dev/null 2>&1; then sha256sum -c --quiet "ferrofed-v$version-kubernetes.tar.gz.sha256sum"; else shasum -a 256 -c --quiet "ferrofed-v$version-kubernetes.tar.gz.sha256sum"; fi) ||
    die "self-test: the checksum does not match"
  if (package "v0.0.0-not-this" "$gateway" "$console" "$out") > /dev/null 2>&1; then
    die "self-test: a tag the manifests do not run is accepted"
  fi
  if (package "v$version" "sha256:short" "$console" "$out") > /dev/null 2>&1; then
    die "self-test: a malformed digest is accepted"
  fi
  echo "kubernetes-package: self-test OK"
}

case "${1:-}" in
  --self-test)
    self_test
    ;;
  *)
    [[ "$#" -eq 4 ]] || die "usage: $0 <tag> <gateway-digest> <console-digest> <out-dir>"
    package "$@"
    ;;
esac
