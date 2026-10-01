#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/gh/labels.sh: bootstrap the FerroFED issue-label taxonomy.
#
# Creates (idempotently) the labels the tracker workflow assumes:
#   * exactly one TYPE label per issue, mapped to the conventional-commit types
#     (bug/enhancement/documentation/chore/refactor/perf/test/ci);
#   * a PRIORITY (P0 to P3);
#   * the DOMAIN labels, one per conformance surface;
#   * a few workflow labels.
#
# `gh label create --force` updates an existing label in place, so re-running
# this is safe and converges the colours and descriptions to the values below.
# Run it ONCE after the repository is created (owner action).
#
# Taxonomy policy: .claude/rules/issue-workflow.md
# Official docs: https://cli.github.com/manual/gh_label_create
#
# Usage: scripts/gh/labels.sh

set -euo pipefail

die() {
  echo "gh-labels: $*" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 || die "the GitHub CLI (gh) is not installed"
gh repo view --json nameWithOwner --jq .nameWithOwner >/dev/null 2>&1 ||
  die "could not resolve the current repository (run inside a gh-authenticated clone)"

# label <name> <hex-color> <description>
label() {
  gh label create "$1" --color "$2" --description "$3" --force >/dev/null
  echo "ok: $1"
}

echo "== type labels (exactly one per issue; maps to the conventional-commit type) =="
label bug           d73a4a "A defect. Maps to a fix/ branch and a fix: commit."
label enhancement   a2eeef "A new capability. Maps to a feat/ branch and a feat: commit."
label documentation 0075ca "Docs-only work. Maps to a docs/ branch and a docs: commit."
label chore         fef2c0 "Maintenance with no product change. chore/ and chore:."
label refactor      cfd3d7 "Behaviour-preserving code change. refactor/ and refactor:."
label perf          fbca04 "Performance work. perf/ and perf:."
label test          c2e0c6 "Test-only work. test/ and test:."
label ci            ededed "CI, build, and tooling. ci/ and ci:."

echo "== priority labels (P0 critical to P3 backlog) =="
label P0 b60205 "Critical, drop everything."
label P1 d93f0b "High, current focus."
label P2 fbca04 "Normal."
label P3 0e8a16 "Backlog."

echo "== domain labels (one per conformance surface) =="
label spec:federation 5319e7 "The Federation Tier with AQL specification: sections, N# requirements, CP# conformance points."
label spec:openEHR    006b75 "The openEHR ITS-REST and AQL wire between a client, the gateway, and a node."
label spec:IHE        1d76db "The IHE identity and directory binding: PIXm, PDQm, XCPD, PMIR, mCSD (Annex A)."
label spec:NL-GF      0e8a16 "The Dutch Generic Functions regional binding (Annex B)."

echo "== workflow and meta labels =="
label research         c5def5 "A design-phase investigation; the deliverable is cited evidence."
label conformance      bfd4f2 "The conformance-point matrix, the Connectathon test tracks, and the harness."
label dependencies     0366d6 "Dependency updates (used by Dependabot)."
label security         ee0701 "Security fix or hardening."
label blocked-upstream 6f42c1 "Waiting on an upstream specification or tool release."
label upstream-report  990000 "An outbound report of a defect in a published specification."
label no-changelog     bfdadc "PR escape hatch: the change has no user-visible effect."
label no-crate-bump    bfdadc "PR escape hatch: the diff provably does not alter packaged crate bytes."

echo "done."
