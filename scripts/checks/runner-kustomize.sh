#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/checks/runner-kustomize.sh: the kustomize the kubeconform job renders
# with is the runner image's, and this check refuses it unless it is at or above
# the version the job states in KUSTOMIZE_MIN_VERSION (#1773).
#
# The job fetched kustomize from a GitHub release on every run until
# 2026-11-09, when that host answered 504 all day. The GitHub-hosted Ubuntu
# images ship kustomize (actions/runner-images,
# images/ubuntu/Ubuntu2404-Readme.md), so the job reads that one and fails
# here, naming both versions, when an image ever ships an older one.
# No specification governs this: our own design.
#
# Usage: KUSTOMIZE_MIN_VERSION=5.8.3 scripts/checks/runner-kustomize.sh
set -euo pipefail

min="${KUSTOMIZE_MIN_VERSION:?set KUSTOMIZE_MIN_VERSION to the oldest kustomize the job accepts, e.g. 5.8.3}"
min="${min#v}"

if ! command -v kustomize >/dev/null 2>&1; then
  echo "::error::no kustomize on PATH; this job needs kustomize ${min} or newer from the runner image"
  exit 1
fi

banner="$(kustomize version 2>&1)"
have="$(sed -nE 's/^[^0-9]*v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' <<<"$banner" | head -n1)"
if [[ -z "$have" ]]; then
  echo "::error::cannot read a version out of: ${banner}"
  exit 1
fi

oldest="$(printf '%s\n%s\n' "$min" "$have" | sort -V | head -n1)"
if [[ "$oldest" != "$min" ]]; then
  echo "::error::the runner's kustomize is ${have}; this job needs ${min} or newer"
  exit 1
fi

echo "kustomize ${have} at $(command -v kustomize) (needs ${min} or newer)"
