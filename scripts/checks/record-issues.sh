#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The open-issue guard of the conformity records (#2043): a row that is not
# met names at least one open issue, so every gap has a tracker. It reads
# the four records and asks GitHub for the state of each issue a row names:
#
#   conformance/annex-ii.tsv                     status column 3, issues 5
#   conformance/chapter-iii.tsv                  status column 4, issues 6
#   conformance/cra-technical-documentation.tsv  status column 5, issues 6
#   conformance/technical-documentation.tsv      status column 5, issues 6
#
# A row is settled when it is met, documented or not-applicable; any other
# row whose issues are all closed is refused. An issue is #N of this
# repository or Owner/Repo#N. Each record's own guard checks the issue's form
# and that a gap names one; this guard adds only the state. No specification
# governs this: our own design.
#
# Usage:
#   scripts/checks/record-issues.sh              check the records
#   scripts/checks/record-issues.sh --self-test  prove each refusal offline
# Needs awk and gh with a token that reads issues (GH_TOKEN in CI). Exit 1
# naming each refused row; 2 when GitHub cannot be read or on a usage error;
# 0 otherwise.
set -euo pipefail
export LC_ALL=C

readonly RECORDS='conformance/annex-ii.tsv 3 5
conformance/chapter-iii.tsv 4 6
conformance/cra-technical-documentation.tsv 5 6
conformance/technical-documentation.tsv 5 6'

# repository: Owner/Repo of this checkout.
repository() {
  if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    echo "$GITHUB_REPOSITORY"
    return
  fi
  git remote get-url origin | sed -E 's#^(git@github\.com:|https://github\.com/)##; s#\.git$##'
}

# gaps: one "file<TAB>line<TAB>key<TAB>issues" line per row that is not settled.
gaps() {
  local file status_col issue_col
  while read -r file status_col issue_col; do
    [[ -f "$file" ]] || {
      echo "record-issues: $file is missing" >&2
      exit 2
    }
    awk -F'\t' -v file="$file" -v s="$status_col" -v i="$issue_col" '
      /^#/ || /^$/ { next }
      !header { header = 1; next }
      $s !~ /^(met|documented|not-applicable)$/ && $i != "-" && $i != "" {
        print file "\t" FNR "\t" $1 "\t" $i
      }
    ' "$file"
  done <<< "$RECORDS"
}

run() {
  local here repo file line key issues ref owner number state any failures=0 cache
  here="$(repository)"
  cache="$(mktemp)"
  # shellcheck disable=SC2064 # expand now: cache is local
  trap "rm -f -- '$cache'" EXIT
  while IFS=$'\t' read -r file line key issues; do
    any=0
    for ref in $issues; do
      if [[ "$ref" == *"/"*"#"* ]]; then
        repo="${ref%%#*}"
      else
        repo="$here"
      fi
      number="${ref##*#}"
      [[ "$number" =~ ^[0-9]+$ ]] || {
        echo "::error::record-issues: $file line $line ($key) names $ref, which is no issue." >&2
        failures=$((failures + 1))
        continue
      }
      owner="$repo#$number"
      state="$(awk -v key="$owner" '$1 == key { print $2; exit }' "$cache")"
      if [[ -z "$state" ]]; then
        state="$(gh api "repos/$repo/issues/$number" --jq .state 2> /dev/null)" || {
          echo "record-issues: GitHub did not answer for $owner" >&2
          exit 2
        }
        printf '%s %s\n' "$owner" "$state" >> "$cache"
      fi
      [[ "$state" == open ]] && any=1
    done
    if [[ "$any" -eq 0 ]]; then
      echo "::error::record-issues: $file line $line ($key) is not met and names only closed issues ($issues); name the open issue for what is missing, or settle the row." >&2
      failures=$((failures + 1))
    fi
  done < <(gaps)
  if [[ "$failures" -gt 0 ]]; then
    echo "record-issues: $failures refusals." >&2
    exit 1
  fi
  echo "record-issues: every row that is not met names an open issue."
}

self_test() {
  local work status
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand now: work is local
  trap "rm -rf -- '$work'" EXIT
  mkdir -p "$work/repo/conformance" "$work/bin"
  # A stub gh: #1 and Other/Repo#3 are open, every other issue closed, #9 fails.
  cat > "$work/bin/gh" << 'STUB'
#!/usr/bin/env bash
case "$2" in
*/issues/9) exit 1 ;;
repos/Owner/Repo/issues/1 | repos/Other/Repo/issues/3) echo open ;;
*) echo closed ;;
esac
STUB
  chmod +x "$work/bin/gh"
  fixture() {
    printf '# c\npoint\trequirement\tstatus\tevidence\tissue\tnote\n1.1\tr\t%s\te\t%s\tn\n' "$1" "$2" > "$work/repo/conformance/annex-ii.tsv"
    printf 'provision\trequirement\tapplies\tstatus\tevidence\tissue\tnote\nArt 1\tr\t2027\tmet\te\t-\tn\n' > "$work/repo/conformance/chapter-iii.tsv"
    printf 'item\telement\ttarget\tsection\tstatus\tissue\n6\te\tt\t-\tdocumented\t-\n' > "$work/repo/conformance/cra-technical-documentation.tsv"
    printf 'item\telement\ttarget\tsection\tstatus\tissue\n4\te\tt\t-\tpartial\t#1 #2\n' > "$work/repo/conformance/technical-documentation.tsv"
  }
  expect() {
    local name="$1" want="$2"
    status=0
    (cd "$work/repo" && GITHUB_REPOSITORY=Owner/Repo PATH="$work/bin:$PATH" bash "$script" > /dev/null 2>&1) || status=$?
    [[ "$status" -eq "$want" ]] || {
      echo "record-issues: self-test failed: $name exited $status, wanted $want." >&2
      exit 1
    }
  }
  script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
  fixture partial '#1'
  expect "a partial row naming an open issue" 0
  fixture partial '#2'
  expect "a partial row naming only a closed issue" 1
  fixture partial '#2 Other/Repo#3'
  expect "a closed issue beside an open one in another repository" 0
  fixture partial 'Other/Repo#4'
  expect "a closed issue of another repository" 1
  fixture met '#2'
  expect "a met row naming a closed issue" 0
  fixture not-applicable '-'
  expect "a not-applicable row naming none" 0
  fixture partial '#9'
  expect "an issue GitHub does not answer for" 2
  echo "record-issues: self-test OK."
}

case "${1:-}" in
'') run ;;
--self-test) self_test ;;
*)
  sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
  exit 2
  ;;
esac
