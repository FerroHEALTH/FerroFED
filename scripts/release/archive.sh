#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# Archives the repository at Software Heritage for a release (Regulation
# (EU) 2025/327 Art 30(3), the copy kept outside GitHub) and writes the
# record a release attaches:
#
#   <out-dir>/ferrofed-<tag>-archive.json
#
# It asks the archive's "save code now" API to visit the public git
# repository, waits until the visit succeeded, checks that the archive holds
# the tagged commit, and records the commit's SWHID, swh:1:rev:<commit>
# (SWHID specification v1.2 §5.4 and §5.8, a revision id computed as git
# computes a commit id), with the save request and the visit. A request the
# archive rejects, a visit that fails, or a commit the archive does not hold
# after the visit refuses the release.
#
# Usage:
#   scripts/release/archive.sh <tag> <commit> <repository-url> <out-dir>
#   scripts/release/archive.sh --self-test
#
# Needs curl and jq. SWH_API overrides the API root for the self-test. No
# specification governs the record's form: our own design.

set -euo pipefail

SWH_API="${SWH_API:-https://archive.softwareheritage.org/api/1}"
# The visit is polled every POLL seconds, at most TRIES times.
POLL="${SWH_POLL:-120}"
TRIES="${SWH_TRIES:-55}"

die() {
  echo "archive: $*" >&2
  exit 1
}

# api METHOD PATH: the JSON body the archive answers METHOD on PATH with.
api() {
  local method="$1" path="$2"
  curl --fail --silent --show-error --retry 3 --request "$method" \
    --header 'Accept: application/json' "$SWH_API$path"
}

# record TAG COMMIT REQUEST OUT: writes the record of TAG at COMMIT from the
# save REQUEST, the archive's JSON of a succeeded save request.
record() {
  local tag="$1" commit="$2" request="$3" out="$4"
  mkdir -p "$out"
  jq --arg tag "$tag" --arg swhid "swh:1:rev:$commit" --arg api "$SWH_API" '{
      tag: $tag,
      swhid: $swhid,
      browse: ("https://archive.softwareheritage.org/" + $swhid),
      origin: .origin_url,
      save_request: { id: .id, url: .request_url, date: .save_request_date },
      visit: { date: .visit_date, status: .visit_status, snapshot: .snapshot_swhid }
    }' <<< "$request" > "$out/ferrofed-${tag}-archive.json"
}

# archive TAG COMMIT REPOSITORY OUT: saves REPOSITORY, waits for the visit
# and writes the record.
archive() {
  local tag="$1" commit="$2" repository="$3" out="$4" request id task status tries=0
  [[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "'$commit' is not a commit id"
  [[ "$repository" =~ ^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] ||
    die "'$repository' is not a public GitHub repository URL"
  request="$(api POST "/origin/save/git/url/$repository/")" || die "the archive refused the save request"
  [[ "$(jq -r '.save_request_status' <<< "$request")" != rejected ]] ||
    die "the archive rejected the save request: $(jq -r '.note // "no reason given"' <<< "$request")"
  id="$(jq -r '.id' <<< "$request")"
  [[ "$id" =~ ^[0-9]+$ ]] || die "the archive answered no save request id"
  while :; do
    task="$(jq -r '.save_task_status' <<< "$request")"
    status="$(jq -r '.visit_status // empty' <<< "$request")"
    case "$task" in
      succeeded) break ;;
      failed) die "the visit of save request $id failed" ;;
    esac
    tries=$((tries + 1))
    [[ "$tries" -le "$TRIES" ]] || die "save request $id did not finish ($task)"
    sleep "$POLL"
    request="$(api GET "/origin/save/$id/")" || die "the archive does not answer save request $id"
  done
  [[ "$status" = full ]] || die "the visit of save request $id is $status, not full"
  api GET "/revision/$commit/" > /dev/null || die "the archive holds no revision $commit after the visit"
  record "$tag" "$commit" "$request" "$out"
  echo "archive: $tag is archived as swh:1:rev:$commit (save request $id)"
}

self_test() {
  local work out written
  work="$(mktemp -d)"
  out="$work/out"
  record v9.9.9 0123456789abcdef0123456789abcdef01234567 '{
      "id": 7, "request_url": "https://example.org/api/1/origin/save/7/",
      "origin_url": "https://github.com/example/example",
      "save_request_date": "2026-11-28T00:00:00+00:00",
      "save_task_status": "succeeded", "visit_date": "2026-11-28T00:05:00+00:00",
      "visit_status": "full", "snapshot_swhid": "swh:1:snp:0000000000000000000000000000000000000000"
    }' "$out"
  written="$out/ferrofed-v9.9.9-archive.json"
  [[ "$(jq -r '.swhid' "$written")" = swh:1:rev:0123456789abcdef0123456789abcdef01234567 ]] ||
    die "self-test: the record names another SWHID"
  [[ "$(jq -r '.save_request.id' "$written")" = 7 ]] || die "self-test: the record names no save request"
  [[ "$(jq -r '.visit.status' "$written")" = full ]] || die "self-test: the record names no visit"
  if (archive v9.9.9 not-a-commit https://github.com/example/example "$work/refused") > /dev/null 2>&1; then
    die "self-test: a malformed commit id is accepted"
  fi
  if (archive v9.9.9 0123456789abcdef0123456789abcdef01234567 'https://example.org/x?y' "$work/refused") > /dev/null 2>&1; then
    die "self-test: a URL that is no public GitHub repository is accepted"
  fi
  rm -r "$work"
  echo "archive: self-test OK"
}

case "${1:-}" in
  --self-test)
    self_test
    ;;
  *)
    [[ $# -eq 4 ]] || die "usage: $0 <tag> <commit> <repository-url> <out-dir> | --self-test"
    archive "$1" "$2" "$3" "$4"
    ;;
esac
