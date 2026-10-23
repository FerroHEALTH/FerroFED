#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The changelog checks over the entries the next release will carry (no
# specification governs the wiring: our own design). The entries are
# fragments under changelog.d/ until the cut, so this assembles them under
# [Unreleased] with `scripts/release/changelog.sh --preview` and runs, on
# that preview:
#
#   scripts/checks/changelog-structure.sh   every section of the file holds
#       Keep a Changelog types, plus "Upgrade notes", each at most once;
#   scripts/checks/changelog-security.sh    the [Unreleased] Security section
#       names an advisory in every entry, and opens with its reason when it
#       sits beside other changes (Regulation (EU) 2024/2847 Annex I Part
#       II(2), (4), (8)).
#
# The release plan runs the same two checks on CHANGELOG.md for the section
# being cut (scripts/release/plan.sh). The preview is written beside
# CHANGELOG.md, because changelog-security.sh resolves a security/vex/ path
# from the directory of the file it reads, and removed on exit.
#
# Usage: scripts/checks/changelog-unreleased.sh
# Exit 0 when both pass, 1 on a finding, 2 on a usage error.
set -euo pipefail

# shellcheck source=scripts/lib/guard-args.sh
. "$(dirname "$0")/../lib/guard-args.sh"
guard_no_args "$@"

cd "$(dirname "$0")/../.."

export CHANGELOG_EXTRA_TYPES="${CHANGELOG_EXTRA_TYPES:-Upgrade notes}"
export CHANGELOG_RULE="${CHANGELOG_RULE:-changelog.d/README.md, Security entries}"

preview="$(mktemp ./.changelog-preview.XXXXXX)"
trap 'rm -f -- "$preview"' EXIT
bash scripts/release/changelog.sh --preview "$preview"

fail=0
bash scripts/checks/changelog-structure.sh "$preview" || fail=1
bash scripts/checks/changelog-security.sh --section Unreleased "$preview" || fail=1
exit "$fail"
