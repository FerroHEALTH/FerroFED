#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every first-party file names one copyright holder, the Licensor of LICENSE
# (no specification governs this: our own design). A header that names any
# other holder, typed by hand or carried in from an older branch, fails here.
# Vendored trees keep their upstream terms and are skipped: every directory
# that holds a PROVENANCE.md.
#
#   scripts/checks/copyright-holder.sh
#
# Exit 0 when every SPDX-FileCopyrightText header line names the holder. Exit
# 1 naming each file and line that does not.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly HOLDER="Cadasto B.V."

# The vendored trees, as git pathspec exclusions.
excludes=()
while IFS= read -r provenance; do
  excludes+=(":(exclude)$(dirname "$provenance")/**")
done < <(git ls-files '*PROVENANCE.md')

# A header line is the tag behind nothing but a comment marker: `//`, `#`,
# `<!--`, `/*` or `*`. Prose that quotes the tag, and an emitter that prints
# one, are not headers.
header='^[[:space:]]*(//|#|<!--|/\*|\*)[[:space:]]*SPDX-FileCopyrightText:'
fail=0
while IFS= read -r hit; do
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  text=${rest#*:}
  holder=$(sed -E 's/.*SPDX-FileCopyrightText:[[:space:]]*//; s/[[:space:]]*(-->|\*\/)[[:space:]]*$//; s/[[:space:]]+$//' <<<"$text")
  if [[ "$holder" != "$HOLDER" ]]; then
    echo "copyright-holder: $file:$line names \"$holder\", not $HOLDER" >&2
    fail=1
  fi
done < <(git grep -nE "$header" -- . "${excludes[@]}" || true)

if [[ $fail -ne 0 ]]; then
  exit 1
fi
echo "copyright-holder: every header names $HOLDER"
