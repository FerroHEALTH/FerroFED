#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Withdraws a released FerroFED version its manufacturer found not to
# conform, the route Regulation (EU) 2025/327 Art 30(1)(i) and (j) asks for
# (the steps are our own design). A published release is immutable and its
# tag protected, so nothing published is edited or deleted. The script:
#
#   1. refuses a version with no finding row naming it in
#      docs/registers/non-conforming-versions.tsv, a version already listed
#      as withdrawn, and a replacement that is not a later published release
#      of the same minor line;
#   2. moves the image tags <major>.<minor> and latest of the gateway and
#      console images from the withdrawn version to the replacement, where
#      they point at the withdrawn version; the <version> tag stays, because
#      GHCR removes a tag only by deleting the image digest, which would break
#      every deployment pinned by digest and the attestations that name it;
#   3. publishes the advisory: appended to the GitHub security advisory named
#      with --security-advisory, published with it, or else written as
#      docs/advisories/v<version>-withdrawn.md;
#   4. writes the upgrade note the next release's notes carry,
#      changelog.d/<issue>-withdraw-v<version>.upgrade.md;
#   5. lists the version under "Withdrawn versions" in SECURITY.md;
#   6. records the withdrawal in the finding's row: the action, and the date
#      and route users were told;
#   7. prints the notices to send to the national authorities and to
#      distributors, importers and users.
#
# Usage:
#   scripts/release/withdraw.sh --version X.Y.Z --replacement X.Y.Z \
#     --finding N-YYYY-NNN --issue N [--security-advisory GHSA-…] [--apply]
#   scripts/release/withdraw.sh --self-test
#
# Without --apply it is a dry run: it reads everything it checks and prints
# what it would do, and writes nothing. Exit 1 on a refusal, 2 on a usage
# error. WITHDRAW_ROOT names the repository root (the tests point it at a
# stub tree), WITHDRAW_STUB a directory standing in for the registry and
# GitHub, and WITHDRAW_TODAY the date written (YYYY-MM-DD).
set -euo pipefail
export LC_ALL=C

readonly REPO=FerroHEALTH/FerroFED
readonly IMAGES='ghcr.io/ferrohealth/ferrofed ghcr.io/ferrohealth/ferrofed-viewer'
readonly REGISTER=docs/registers/non-conforming-versions.tsv
readonly MARKER='<!-- withdrawn versions: rows above this line -->'
readonly CONTACT=info@cadasto.com

die() {
  echo "withdraw: $*" >&2
  exit 1
}

usage() {
  echo "usage: withdraw.sh --version X.Y.Z --replacement X.Y.Z --finding N-YYYY-NNN --issue N [--security-advisory GHSA-ID] [--apply] | --self-test" >&2
  exit 2
}

# say TEXT: one line of the plan, marked as such in a dry run.
say() {
  if [[ "$APPLY" -eq 1 ]]; then
    echo "withdraw: $*"
  else
    echo "withdraw: would $*"
  fi
}

# --- The registry and GitHub, or their stubs ------------------------------

# digest IMAGE TAG: the digest IMAGE:TAG names, or nothing when the tag does
# not exist.
digest() {
  local image="$1" tag="$2"
  if [[ -n "${WITHDRAW_STUB:-}" ]]; then
    cat "$WITHDRAW_STUB/registry/${image##*/}/$tag" 2> /dev/null || true
    return
  fi
  docker buildx imagetools inspect "$image:$tag" --format '{{json .Manifest}}' 2> /dev/null \
    | jq -r '.digest // empty' || true
}

# retag IMAGE TAG DIGEST: points IMAGE:TAG at IMAGE@DIGEST.
retag() {
  local image="$1" tag="$2" target="$3"
  if [[ -n "${WITHDRAW_STUB:-}" ]]; then
    printf '%s\n' "$target" > "$WITHDRAW_STUB/registry/${image##*/}/$tag"
    return
  fi
  docker buildx imagetools create --tag "$image:$tag" "$image@$target"
}

# published TAG: whether TAG is a published release, not a draft.
published() {
  local tag="$1" draft
  if [[ -n "${WITHDRAW_STUB:-}" ]]; then
    [[ -f "$WITHDRAW_STUB/releases/$tag" ]]
    return
  fi
  draft="$(gh release view "$tag" --repo "$REPO" --json isDraft --jq '.isDraft' 2> /dev/null)" || return 1
  [[ "$draft" == false ]]
}

# advisory_text ID: the description of the security advisory ID.
advisory_text() {
  local id="$1"
  if [[ -n "${WITHDRAW_STUB:-}" ]]; then
    cat "$WITHDRAW_STUB/advisories/$id"
    return
  fi
  gh api "repos/$REPO/security-advisories/$id" --jq '.description'
}

# advisory_publish ID TEXT: sets the description of advisory ID to TEXT and
# publishes it.
advisory_publish() {
  local id="$1" text="$2"
  if [[ -n "${WITHDRAW_STUB:-}" ]]; then
    printf '%s\n' "$text" > "$WITHDRAW_STUB/advisories/$id"
    : > "$WITHDRAW_STUB/advisories/$id.published"
    return
  fi
  gh api --method PATCH "repos/$REPO/security-advisories/$id" \
    -f description="$text" -f state=published > /dev/null
}

# --- The checks ------------------------------------------------------------

# finding_row FINDING VERSION: the register row with id FINDING, when its
# versions column names VERSION.
finding_row() {
  awk -F '\t' -v id="$1" -v version="$2" '
    /^#/ || $1 != id { next }
    {
      n = split($3, listed, /[ ,;]+/)
      for (i = 1; i <= n; i++) {
        sub(/^v/, "", listed[i])
        if (listed[i] == version) { print; exit }
      }
    }' "$ROOT/$REGISTER"
}

# newer A B: whether version A is later than version B.
newer() {
  local a="$1" b="$2" i x y
  for i in 1 2 3; do
    x="$(cut -d. -f"$i" <<< "$a")"
    y="$(cut -d. -f"$i" <<< "$b")"
    if ((10#$x != 10#$y)); then
      ((10#$x > 10#$y))
      return
    fi
  done
  return 1
}

# --- The texts -------------------------------------------------------------

# withdrawal_text: the advisory's account of the withdrawal, in Markdown.
withdrawal_text() {
  cat << EOF
FerroFED $VERSION is withdrawn as of $TODAY by its manufacturer, Cadasto B.V.,
under Regulation (EU) 2025/327 Art 30(1)(i). The finding is $FINDING in the
register of non-conforming versions,
https://github.com/$REPO/blob/main/$REGISTER: $SUMMARY

Move every deployment of $VERSION to $REPLACEMENT or later.

- The image tags \`$LINE\` and \`latest\` of \`ghcr.io/ferrohealth/ferrofed\` and
  \`ghcr.io/ferrohealth/ferrofed-viewer\` point at $REPLACEMENT.
- The \`$VERSION\` tag, its digest, the release and its attestations stay: a
  published release and its tag cannot be changed or deleted, so a
  deployment pinned to \`$VERSION\` or to its digest keeps running the
  withdrawn version until it is moved.
- $VERSION is not supported, whatever its age.

Questions go to $CONTACT.
EOF
}

# --- The run ---------------------------------------------------------------

run() {
  VERSION='' REPLACEMENT='' FINDING='' ISSUE='' ADVISORY='' APPLY=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version) VERSION="${2:-}"; shift 2 || usage ;;
      --replacement) REPLACEMENT="${2:-}"; shift 2 || usage ;;
      --finding) FINDING="${2:-}"; shift 2 || usage ;;
      --issue) ISSUE="${2:-}"; shift 2 || usage ;;
      --security-advisory) ADVISORY="${2:-}"; shift 2 || usage ;;
      --apply) APPLY=1; shift ;;
      *) usage ;;
    esac
  done
  local semver='^[0-9]+\.[0-9]+\.[0-9]+$'
  [[ "$VERSION" =~ $semver && "$REPLACEMENT" =~ $semver ]] || usage
  [[ "$FINDING" =~ ^N-[0-9]{4}-[0-9]+$ && "$ISSUE" =~ ^[0-9]+$ ]] || usage
  [[ -z "$ADVISORY" || "$ADVISORY" =~ ^GHSA(-[23456789cfghjmpqrvwx]{4}){3}$ ]] || usage
  ROOT="${WITHDRAW_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
  TODAY="${WITHDRAW_TODAY:-$(date -u +%Y-%m-%d)}"
  LINE="${VERSION%.*}"

  local row
  row="$(finding_row "$FINDING" "$VERSION")"
  [[ -n "$row" ]] || die "no row $FINDING in $REGISTER names $VERSION: enter the finding first (Art 30(1)(i), (o))."
  SUMMARY="$(cut -f5 <<< "$row")"
  if grep -q -F "| $VERSION |" "$ROOT/SECURITY.md"; then
    die "$VERSION is already listed as withdrawn in SECURITY.md."
  fi
  grep -q -F "$MARKER" "$ROOT/SECURITY.md" || die "SECURITY.md has no withdrawn-versions table."
  newer "$REPLACEMENT" "$VERSION" || die "the replacement $REPLACEMENT is not later than $VERSION."
  [[ "${REPLACEMENT%.*}" == "$LINE" ]] \
    || die "the replacement $REPLACEMENT is not in the $LINE line, so the $LINE tag has nothing to move to: release a $LINE patch first."
  published "v$VERSION" || die "v$VERSION is not a published release."
  published "v$REPLACEMENT" || die "the replacement v$REPLACEMENT is not a published release."

  local fragment="changelog.d/$ISSUE-withdraw-v${VERSION//./-}.upgrade.md"
  local page="docs/advisories/v$VERSION-withdrawn.md"
  [[ ! -e "$ROOT/$fragment" ]] || die "$fragment exists already."
  local route
  if [[ -n "$ADVISORY" ]]; then
    advisory_text "$ADVISORY" > /dev/null || die "the security advisory $ADVISORY cannot be read."
    route="security advisory $ADVISORY"
  else
    [[ ! -e "$ROOT/$page" ]] || die "$page exists already."
    route="advisory $page"
  fi

  # The image tags, read in full before any moves.
  local image withdrawn target tag current moves=''
  for image in $IMAGES; do
    withdrawn="$(digest "$image" "$VERSION")"
    if [[ -z "$withdrawn" ]]; then
      echo "withdraw: $image has no $VERSION tag; nothing to move there."
      continue
    fi
    target="$(digest "$image" "$REPLACEMENT")"
    [[ -n "$target" ]] || die "$image has $VERSION but no $REPLACEMENT tag to move to."
    for tag in "$LINE" latest; do
      current="$(digest "$image" "$tag")"
      if [[ "$current" == "$withdrawn" ]]; then
        moves="$moves$image $tag $target"$'\n'
      else
        echo "withdraw: $image:$tag does not point at $VERSION; it stays."
      fi
    done
    echo "withdraw: $image:$VERSION stays at $withdrawn: removing it would delete the digest."
  done

  while IFS=' ' read -r image tag target; do
    [[ -n "$image" ]] || continue
    say "move $image:$tag to $REPLACEMENT ($target)"
    if [[ "$APPLY" -eq 1 ]]; then
      retag "$image" "$tag" "$target"
    fi
  done <<< "$moves"

  if [[ -n "$ADVISORY" ]]; then
    say "append the withdrawal to the security advisory $ADVISORY and publish it"
    if [[ "$APPLY" -eq 1 ]]; then
      local text
      text="$(advisory_text "$ADVISORY")"
      advisory_publish "$ADVISORY" "$text"$'\n\n'"## Withdrawal"$'\n\n'"$(withdrawal_text)"
    fi
  else
    say "write $page"
    if [[ "$APPLY" -eq 1 ]]; then
      mkdir -p "$ROOT/docs/advisories"
      {
        printf '<!-- SPDX-FileCopyrightText: Cadasto B.V. -->\n'
        printf '<!-- SPDX-License-Identifier: BUSL-1.1 -->\n\n'
        printf '# FerroFED %s is withdrawn\n\n' "$VERSION"
        withdrawal_text
      } > "$ROOT/$page"
    fi
  fi

  say "write the upgrade note $fragment"
  if [[ "$APPLY" -eq 1 ]]; then
    # shellcheck disable=SC2016 # the backticks are Markdown, never expanded
    printf -- '- FerroFED %s is withdrawn (finding %s, Regulation (EU) 2025/327 Art 30(1)(i)) and is not supported: move every deployment of it to %s or later. The image tags `%s` and `latest` point at %s; the `%s` tag and its digest stay, so an image pinned to either keeps running the withdrawn version until it is moved. The %s says why.\n' \
      "$VERSION" "$FINDING" "$REPLACEMENT" "$LINE" "$REPLACEMENT" "$VERSION" "$route" > "$ROOT/$fragment"
  fi

  local listed="| $VERSION | $TODAY | $FINDING | $route |"
  say "list $VERSION as withdrawn in SECURITY.md"
  if [[ "$APPLY" -eq 1 ]]; then
    awk -v row="$listed" -v marker="$MARKER" '$0 == marker { print row } { print }' \
      "$ROOT/SECURITY.md" > "$ROOT/SECURITY.md.withdraw"
    mv "$ROOT/SECURITY.md.withdraw" "$ROOT/SECURITY.md"
  fi

  local action="withdrawn $TODAY, replaced by $REPLACEMENT" told="$TODAY: $route, the upgrade note of the next release"
  say "record in $FINDING: action \"$action\", users_told \"$told\""
  if [[ "$APPLY" -eq 1 ]]; then
    awk -F '\t' -v OFS='\t' -v id="$FINDING" -v action="$action" -v told="$told" '
      !/^#/ && $1 == id {
        $7 = ($7 == "" ? action : $7 "; " action)
        $10 = ($10 == "" ? told : $10 "; " told)
      }
      { print }' "$ROOT/$REGISTER" > "$ROOT/$REGISTER.withdraw"
    mv "$ROOT/$REGISTER.withdraw" "$ROOT/$REGISTER"
  fi

  cat << EOF

--- Notice to the national authorities (Art 30(1)(i)) ---
Each Member State where FerroFED $VERSION was made available or put into
service. Record the date and the Member States in authorities_told.

FerroFED $VERSION does not conform: $SUMMARY
Corrective action: the version is withdrawn as of $TODAY, and $REPLACEMENT
brings FerroFED into conformity. Finding $FINDING in the register of
non-conforming versions.

--- Notice to distributors, importers and users (Art 30(1)(j)) ---
$(withdrawal_text)
EOF
  if [[ "$APPLY" -eq 0 ]]; then
    echo
    echo "withdraw: dry run, nothing was changed; run again with --apply."
  fi
}

# --- The self-test ---------------------------------------------------------

self_test() {
  local work failed=0 n=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -r '$work'" EXIT
  local good='N-2026-1	2026-10-01	0.0.9	Art 30(1)(i)	A synthetic finding.	no					'
  local other='N-2026-2	2026-10-02	0.0.7, 0.0.8	Annex II 1.1	Another synthetic finding.	no					'
  # fixture: a fresh stub repository and registry under $work/case.
  fixture() {
    local case="$work/case"
    if [[ -d "$case" ]]; then rm -r "$case"; fi
    mkdir -p "$case/repo/docs/registers" "$case/repo/changelog.d" \
      "$case/stub/releases" "$case/stub/advisories" \
      "$case/stub/registry/ferrofed" "$case/stub/registry/ferrofed-viewer"
    printf '# a comment\nid\tfound\tversions\trequirement\tsummary\tserious_incident\taction\tcorrected_in\tauthorities_told\tusers_told\tclosed\n%s\n%s\n' \
      "$good" "$other" > "$case/repo/$REGISTER"
    printf '# Security Policy\n\n## Withdrawn versions\n\n| Version | Withdrawn | Finding | Advisory |\n| --- | --- | --- | --- |\n%s\n' \
      "$MARKER" > "$case/repo/SECURITY.md"
    : > "$case/stub/releases/v0.0.9"
    : > "$case/stub/releases/v0.0.10"
    printf 'The original advisory.\n' > "$case/stub/advisories/GHSA-2345-6789-cfgh"
    local image
    for image in ferrofed ferrofed-viewer; do
      printf 'sha256:aaa\n' > "$case/stub/registry/$image/0.0.9"
      printf 'sha256:bbb\n' > "$case/stub/registry/$image/0.0.10"
      printf 'sha256:aaa\n' > "$case/stub/registry/$image/0.0"
      printf 'sha256:aaa\n' > "$case/stub/registry/$image/latest"
    done
  }
  # attempt WANT ARGS...: runs the script on the fixture and checks the exit.
  attempt() {
    local want="$1" status=0
    shift
    n=$((n + 1))
    WITHDRAW_ROOT="$work/case/repo" WITHDRAW_STUB="$work/case/stub" WITHDRAW_TODAY=2026-10-06 \
      "$BASH" "$0" "$@" > "$work/out" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "withdraw: self-test case $n ($*) exited $status, wanted $want:" >&2
      cat "$work/out" >&2
      failed=1
    fi
  }
  # check WHAT COMMAND...: records a failure when COMMAND fails.
  check() {
    local what="$1"
    shift
    if ! "$@"; then
      echo "withdraw: self-test: $what" >&2
      failed=1
    fi
  }
  local base='--version 0.0.9 --replacement 0.0.10 --finding N-2026-1 --issue 812'
  local snapshot

  fixture
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 1 --version 0.0.6 --replacement 0.0.10 --finding N-2026-1 --issue 812
  check 'a version the finding does not name is refused' grep -q 'names 0.0.6' "$work/out"
  attempt 1 --version 0.0.9 --replacement 0.0.10 --finding N-2026-9 --issue 812
  check 'a finding with no row is refused' grep -q 'no row N-2026-9' "$work/out"
  attempt 1 --version 0.0.9 --replacement 0.0.8 --finding N-2026-1 --issue 812
  check 'an earlier replacement is refused' grep -q 'not later' "$work/out"
  attempt 1 --version 0.0.9 --replacement 0.1.0 --finding N-2026-1 --issue 812
  check 'a replacement in another line is refused' grep -q 'not in the 0.0 line' "$work/out"
  rm "$work/case/stub/releases/v0.0.10"
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 1 $base
  check 'an unpublished replacement is refused' grep -q 'not a published release' "$work/out"
  attempt 2 --version 0.0.9
  attempt 2 --version 0.0.9 --replacement 0.0.10 --finding N-2026-1 --issue 812 --security-advisory nope

  fixture
  snapshot="$(cd "$work/case" && find . -type f -exec cksum {} + | sort)"
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 0 $base
  check 'a dry run changes nothing' \
    test "$snapshot" = "$(cd "$work/case" && find . -type f -exec cksum {} + | sort)"
  check 'a dry run says so' grep -q 'dry run, nothing was changed' "$work/out"
  check 'a dry run prints the authorities notice' grep -q 'Art 30(1)(i)) ---' "$work/out"

  fixture
  printf 'sha256:ccc\n' > "$work/case/stub/registry/ferrofed-viewer/latest"
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 0 $base --apply
  local repo="$work/case/repo" registry="$work/case/stub/registry"
  check 'the line tag moves' grep -q bbb "$registry/ferrofed/0.0"
  check 'latest moves' grep -q bbb "$registry/ferrofed/latest"
  check 'the viewer line tag moves' grep -q bbb "$registry/ferrofed-viewer/0.0"
  check 'a latest pointing elsewhere stays' grep -q ccc "$registry/ferrofed-viewer/latest"
  check 'the version tag stays' grep -q aaa "$registry/ferrofed/0.0.9"
  check 'the advisory page is written' test -f "$repo/docs/advisories/v0.0.9-withdrawn.md"
  check 'the page carries its licence header' grep -q 'SPDX-License-Identifier: BUSL-1.1' "$repo/docs/advisories/v0.0.9-withdrawn.md"
  check 'the upgrade note is written' test -f "$repo/changelog.d/812-withdraw-v0-0-9.upgrade.md"
  check 'the upgrade note is one list item' \
    test "$(head -c 2 "$repo/changelog.d/812-withdraw-v0-0-9.upgrade.md")" = '- '
  check 'SECURITY.md lists the version' grep -q -F '| 0.0.9 | 2026-10-06 | N-2026-1 |' "$repo/SECURITY.md"
  check 'the marker stays last' test "$(tail -n 1 "$repo/SECURITY.md")" = "$MARKER"
  check 'the action is recorded' \
    test "$(awk -F '\t' '$1 == "N-2026-1" { print $7 }' "$repo/$REGISTER")" = 'withdrawn 2026-10-06, replaced by 0.0.10'
  check 'users_told is recorded' \
    grep -q -F '2026-10-06: advisory docs/advisories/v0.0.9-withdrawn.md' "$repo/$REGISTER"
  check 'every row keeps eleven columns' \
    test "$(awk -F '\t' '!/^#/ && NF != 11' "$repo/$REGISTER" | wc -l | tr -d ' ')" = 0
  check 'the other finding is untouched' grep -q -F "$other" "$repo/$REGISTER"
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 1 $base --apply
  check 'a second withdrawal is refused' grep -q 'already listed' "$work/out"

  fixture
  # shellcheck disable=SC2086 # $base is a word list on purpose.
  attempt 0 $base --security-advisory GHSA-2345-6789-cfgh --apply
  check 'the advisory is published' test -f "$work/case/stub/advisories/GHSA-2345-6789-cfgh.published"
  check 'the advisory keeps its text' grep -q 'The original advisory.' "$work/case/stub/advisories/GHSA-2345-6789-cfgh"
  check 'the advisory gains the withdrawal' grep -q '## Withdrawal' "$work/case/stub/advisories/GHSA-2345-6789-cfgh"
  check 'no page is written for a security advisory' test ! -e "$work/case/repo/docs/advisories"

  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "withdraw: self-test OK ($n runs)."
}

case "${1:-}" in
  --self-test) self_test ;;
  '') usage ;;
  *) run "$@" ;;
esac
