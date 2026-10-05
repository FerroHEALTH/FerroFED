#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Every CI tool, corpus and test image pin no ecosystem watches, compared
# with its newest upstream release. Dependabot's `github-actions` ecosystem reads `uses:`
# references; it does not read an image named in a `run:` block, a version
# handed to an installer as an input, or a commit a vendor script fetches from.
# scripts/checks/versions.sh catches drift between two files we control and
# cannot tell you that upstream published a newer release, which is what this
# reads.
#
#   scripts/checks/pin-freshness.sh
#
# Reads each pin from docs/VERSIONS.md and each newest release tag from the
# upstream project's GitHub releases, which is the tag the container image and
# the installer both carry. A corpus pinned by commit on a repository with no
# releases is read against the newest commit of the branch it follows, and the
# Chrome for Testing release against the newest stable one of its availability
# feed. Each container image the testkit harness starts, a `PinnedImage`
# constant under tools/ferrofed-testkit/src, is read against the newest
# stable tag of its shape in the registry that serves it. Needs an
# authenticated `gh`, awk, curl, find, jq and sed.
#
#   scripts/checks/pin-freshness.sh --self-test
#
# Runs the tag comparison and the constant reader over fixtures, offline.
#
# Exit 0 when every pin is current, 1 when at least one is behind (each such
# line starts with STALE), 2 when a release could not be read, so a network
# failure never reads as a fresh pin.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly MATRIX=docs/VERSIONS.md

# The container images the testkit harness starts, each a `PinnedImage`
# constant under tools/ferrofed-testkit/src, read against the tags of the
# registry that serves it. Dependabot's docker ecosystem reads Dockerfiles
# and compose files, never a Rust constant.
readonly TESTKIT_SRC=tools/ferrofed-testkit/src
readonly REGISTRY_UA=ferrofed-pin-check

# One "constant<TAB>reason" record per line: a constant compared only within
# the major line its tag names, because the consumer needs that line. The
# newest tag of a later line is still printed beside it.
readonly HELD_LINES="\
TEMURIN_JRE	the Java release the reference implementation's build declares (java.version 21)
SANTEMPI_POSTGRES	the PostgreSQL line SanteMPI's own compose file was published against"

# testkit_images [ROOT]: one "constant<TAB>repository<TAB>tag" record per
# PinnedImage literal under ROOT (the testkit source by default), sorted by
# constant.
testkit_images() {
  find "${1:-$TESTKIT_SRC}" -name '*.rs' -exec awk '
    /^pub const [A-Z0-9_]+: PinnedImage = PinnedImage \{/ {
      name = $3; sub(/:$/, "", name); repo = ""; tag = ""; inside = 1; next
    }
    inside && /^[ \t]*repository:/ { repo = $0; sub(/^[^"]*"/, "", repo); sub(/".*$/, "", repo) }
    inside && /^[ \t]*tag:/ { tag = $0; sub(/^[^"]*"/, "", tag); sub(/".*$/, "", tag) }
    inside && /^\};/ { print name "\t" repo "\t" tag; inside = 0 }
  ' {} + | LC_ALL=C sort
}

# registry_tags REPOSITORY: every tag of the image, one per line, from the
# OCI distribution API (`GET /v2/<name>/tags/list`) with the anonymous pull
# token the registry's `WWW-Authenticate` challenge names. A repository with
# no registry host is a Docker Hub one, and a bare name is an official image.
registry_tags() {
  local repository="$1" host path challenge realm service token headers
  case "${repository%%/*}" in
    *.* | *:*)
      host="${repository%%/*}"
      path="${repository#*/}"
      ;;
    *)
      host=registry-1.docker.io
      path="$repository"
      [[ "$path" == */* ]] || path="library/$path"
      ;;
  esac
  challenge="$(curl --silent --show-error --user-agent "$REGISTRY_UA" --output /dev/null \
    --dump-header - "https://$host/v2/" | tr -d '\r' \
    | awk 'tolower($1) == "www-authenticate:" { sub(/^[^:]*:[ \t]*/, ""); print; exit }')" || return 1
  realm="$(sed -nE 's/.*realm="([^"]+)".*/\1/p' <<< "$challenge")"
  service="$(sed -nE 's/.*service="([^"]+)".*/\1/p' <<< "$challenge")"
  [[ -n "$realm" ]] || return 1
  token="$(curl --fail --silent --show-error --get --user-agent "$REGISTRY_UA" \
    --data-urlencode "service=$service" --data-urlencode "scope=repository:$path:pull" "$realm" \
    | jq --raw-output --exit-status '.token // .access_token')" || return 1
  headers="$registry_scratch/headers"
  curl --fail --silent --show-error --user-agent "$REGISTRY_UA" --dump-header "$headers" \
    --header "Authorization: Bearer $token" "https://$host/v2/$path/tags/list?n=100000" \
    > "$registry_scratch/tags.json" || return 1
  # A registry that pages the list past this request would hide the newest
  # tags, so a second page is a failure, never a shorter list.
  if grep -qi '^link:' "$headers"; then
    return 1
  fi
  jq --raw-output --exit-status '.tags[]' "$registry_scratch/tags.json"
}

# newest_release PINNED [LINE]: the newest tag read on stdin that is a stable
# release of the pinned tag's shape, within the major LINE when one is given.
# A release is a dotted version with an optional `_build` and the pinned
# suffix verbatim (`-eclipse-temurin-21`, `-jre-noble`), so a pre-release
# (`-rc1`, `beta1`) and a variant (`-alpine`) never match. A floating tag
# (`4.3`, `18`) names a line, not a release, so a release carries at least as
# many components as the pin, up to three: a Temurin respin adds a fourth.
newest_release() {
  awk -v pinned="$1" -v line="${2:-}" '
    function shape(tag) {
      if (!match(tag, /^[0-9]+(\.[0-9]+)*(_[0-9]+)?/)) return 0
      version = substr(tag, 1, RLENGTH); suffix = substr(tag, RLENGTH + 1)
      build = ""
      if (index(version, "_") > 0) { build = substr(version, index(version, "_") + 1); version = substr(version, 1, index(version, "_") - 1) }
      parts = split(version, part, ".")
      return 1
    }
    function key(   k, i) {
      k = ""
      for (i = 1; i <= 6; i++) k = k sprintf("%010d", (i <= parts ? part[i] : 0))
      return k sprintf("%010d", (build == "" ? 0 : build))
    }
    BEGIN {
      shape(pinned)
      want_suffix = suffix; want_build = (build != ""); want_parts = (parts < 3 ? parts : 3)
      best = ""; best_key = ""
    }
    shape($0) && suffix == want_suffix && (build != "") == want_build && parts >= want_parts \
      && (line == "" || part[1] == line) {
      k = key()
      if (best == "" || k > best_key) { best = $0; best_key = k }
    }
    END { if (best != "") print best }'
}

# The self-test runs newest_release and testkit_images over fixtures and
# reads nothing from the network, so a broken parser fails CI rather than
# reading every image as current.
self_test() {
  local failed=0 got work
  # expect WANT GOT WHAT: one comparison of the self-test.
  expect() {
    if [[ "$2" != "$1" ]]; then
      printf 'pin-freshness: self-test failed: %s gave "%s", wanted "%s".\n' "$3" "$2" "$1" >&2
      failed=1
    fi
  }
  got="$(printf '%s\n' 4.3.3 4.3 latest sha-4b271bc 4.4.0-rc1 4.3.10 4.3.9 | newest_release 4.3.3)"
  expect 4.3.10 "$got" "a release beside floating, commit and pre-release tags"
  got="$(printf '%s\n' 15.19 18 18.1 18.1-alpine 15.20 18beta1 | newest_release 15.19)"
  expect 18.1 "$got" "a two-component release across lines"
  got="$(printf '%s\n' 15.19 18 18.1 18.1-alpine 15.20 | newest_release 15.19 15)"
  expect 15.20 "$got" "a release held to its line"
  got="$(printf '%s\n' 21.0.12.1_1-jre-noble 21.0.12_7-jre-noble 21.0.13_8-jre-noble 21-jre-noble \
    21.0.13_8-jdk-noble | newest_release 21.0.12.1_1-jre-noble 21)"
  expect 21.0.13_8-jre-noble "$got" "a Temurin respin against the next update"
  got="$(printf '%s\n' 21.0.12.1_1-jre-noble 21.0.12_7-jre-noble | newest_release 21.0.12.1_1-jre-noble)"
  expect 21.0.12.1_1-jre-noble "$got" "a respin against the update it respins"
  got="$(printf '%s\n' 3.9.16-eclipse-temurin-21 3.9-eclipse-temurin-21 4.0.0-rc-4-eclipse-temurin-21 \
    3.9.17-eclipse-temurin-25 | newest_release 3.9.16-eclipse-temurin-21)"
  expect 3.9.16-eclipse-temurin-21 "$got" "a suffix that holds the Java release"
  got="$(printf '%s\n' latest main | newest_release 2.5.12)"
  expect "" "$got" "a repository with no release of the pinned shape"

  work="$(mktemp -d)"
  mkdir "$work/containers"
  printf '%s\n' '/// A.' 'pub const B_IMAGE: PinnedImage = PinnedImage {' '    repository: "example/b",' \
    '    tag: "1.2.3",' '    digest: "sha256:00",' '};' > "$work/containers.rs"
  printf '%s\n' 'pub const A_IMAGE: PinnedImage = PinnedImage {' '    repository: "ghcr.io/example/a",' \
    '    tag: "2.0.0",' '    digest: "sha256:00",' '};' > "$work/containers/nested.rs"
  got="$(testkit_images "$work" | tr '\t' ' ' | paste -sd ';' -)"
  rm "$work/containers/nested.rs" "$work/containers.rs"
  rmdir "$work/containers" "$work"
  expect "A_IMAGE ghcr.io/example/a 2.0.0;B_IMAGE example/b 1.2.3" "$got" "the PinnedImage literals of a tree"

  [[ "$failed" -eq 0 ]] || exit 1
  echo "pin-freshness: self-test OK."
}

case "${1:-}" in
  --self-test)
    self_test
    exit 0
    ;;
  '') ;;
  *)
    echo "usage: scripts/checks/pin-freshness.sh [--self-test]" >&2
    exit 2
    ;;
esac

registry_scratch="$(mktemp -d)"
trap 'rm -f "$registry_scratch/headers" "$registry_scratch/tags.json"; rmdir "$registry_scratch"' EXIT

# One "matrix label<TAB>upstream repository" record per line. The label is the
# first cell of the row in docs/VERSIONS.md, backticks and all.
readonly WATCHED="\
zizmor	zizmorcore/zizmor
actionlint	rhysd/actionlint
shellcheck	koalaman/shellcheck
hadolint	hadolint/hadolint
kubeconform	yannh/kubeconform
lychee	lycheeverse/lychee
promtool	prometheus/prometheus"

# matrix_pin LABEL: the second cell of the matrix row whose first cell is
# LABEL, with the backticks stripped and only the first token kept, the same
# shape versions.sh reads.
matrix_pin() {
  local label="$1"
  awk -F'|' -v want="$label" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      if (label == want) { split(value, f, " "); print f[1]; exit }
    }' "$MATRIX"
}

stale=0
unreadable=0
while IFS=$'\t' read -r label repo; do
  [[ -n "$label" ]] || continue

  pinned="$(matrix_pin "$label")"
  if [[ -z "$pinned" ]]; then
    printf 'UNREADABLE %s: no pin row in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! tag="$(gh api "repos/$repo/releases/latest" --jq '.tag_name' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the newest release of %s (%s)\n' "$label" "$repo" "$tag"
    unreadable=1
    continue
  fi
  # A tag may lead with the repository's name: lychee tags its releases
  # lychee-vX.Y.Z.
  latest="${tag#"${repo##*/}"-}"
  latest="${latest#v}"

  if [[ "$pinned" = "$latest" ]]; then
    printf 'current    %s %s (%s)\n' "$label" "$pinned" "$repo"
  else
    printf 'STALE      %s: pinned %s, newest upstream release %s (https://github.com/%s/releases/tag/%s)\n' \
      "$label" "$pinned" "$latest" "$repo" "$tag"
    stale=1
  fi
done <<< "$WATCHED"

# One "matrix label<TAB>upstream repository<TAB>branch" record per line: a
# corpus pinned by commit on a repository that publishes no releases, read
# against the newest commit of the branch the pin follows.
readonly WATCHED_COMMITS="\
Federation Tier with AQL specification	syntaric/openehr-federation-spec	main
Federation Tier reference implementation	syntaric/openehr-federation-ref	main"

# matrix_commit LABEL: the first 40-hex commit in the second cell of the
# matrix row whose first cell is LABEL.
matrix_commit() {
  local label="$1"
  awk -F'|' -v want="$label" '
    NF >= 3 {
      label = $2; value = $3
      gsub(/`/, "", label); gsub(/^[ \t]+|[ \t]+$/, "", label)
      if (label == want && match(value, /[0-9a-f]{40}/)) { print substr(value, RSTART, RLENGTH); exit }
    }' "$MATRIX"
}

while IFS=$'\t' read -r label repo branch; do
  [[ -n "$label" ]] || continue

  pinned="$(matrix_commit "$label")"
  if [[ -z "$pinned" ]]; then
    printf 'UNREADABLE %s: no commit pin in %s\n' "$label" "$MATRIX"
    unreadable=1
    continue
  fi

  if ! head="$(gh api "repos/$repo/commits/$branch" --jq '.sha' 2>&1)"; then
    printf 'UNREADABLE %s: could not read the head of %s %s (%s)\n' "$label" "$repo" "$branch" "$head"
    unreadable=1
    continue
  fi

  if [[ "$pinned" = "$head" ]]; then
    printf 'current    %s %s (%s %s)\n' "$label" "$pinned" "$repo" "$branch"
  else
    printf 'STALE      %s: pinned %s, newest commit on %s %s (https://github.com/%s/commit/%s)\n' \
      "$label" "$pinned" "$branch" "$head" "$repo" "$head"
    stale=1
  fi
done <<< "$WATCHED_COMMITS"

# The Chrome for Testing release the browser journeys run Chrome and
# chromedriver at, read against the newest stable release of the Chrome for
# Testing availability feed, which publishes no GitHub release.
readonly CHROME_LABEL="Chrome for Testing (Chrome and chromedriver)"
readonly CHROME_FEED=https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions.json
pinned="$(matrix_pin "$CHROME_LABEL")"
if [[ -z "$pinned" ]]; then
  printf 'UNREADABLE %s: no pin row in %s\n' "$CHROME_LABEL" "$MATRIX"
  unreadable=1
elif ! latest="$(curl --fail --silent --show-error --user-agent ferrofed-pin-check "$CHROME_FEED" \
  | jq --raw-output --exit-status '.channels.Stable.version' 2>&1)"; then
  printf 'UNREADABLE %s: could not read the newest stable release from %s (%s)\n' "$CHROME_LABEL" "$CHROME_FEED" "$latest"
  unreadable=1
elif [[ "$pinned" = "$latest" ]]; then
  printf 'current    %s %s (%s)\n' "$CHROME_LABEL" "$pinned" "$CHROME_FEED"
else
  printf 'STALE      %s: pinned %s, newest stable release %s (%s)\n' \
    "$CHROME_LABEL" "$pinned" "$latest" "$CHROME_FEED"
  stale=1
fi

# The testkit images, each against the registry that serves it.
images="$(testkit_images)"
if [[ -z "$images" ]]; then
  printf 'UNREADABLE testkit images: no PinnedImage constant under %s\n' "$TESTKIT_SRC"
  unreadable=1
fi
while IFS=$'\t' read -r constant repository tag; do
  [[ -n "$constant" ]] || continue
  if [[ -z "$repository" || -z "$tag" ]]; then
    printf 'UNREADABLE %s: no repository or tag in its PinnedImage under %s\n' "$constant" "$TESTKIT_SRC"
    unreadable=1
    continue
  fi
  if ! tags="$(registry_tags "$repository")"; then
    printf 'UNREADABLE %s: could not read the tags of %s\n' "$constant" "$repository"
    unreadable=1
    continue
  fi

  held="$(awk -F'\t' -v want="$constant" '$1 == want { print $2; exit }' <<< "$HELD_LINES")"
  newest="$(newest_release "$tag" <<< "$tags")"
  compared="$newest"
  scope="newest stable tag"
  if [[ -n "$held" ]]; then
    compared="$(newest_release "$tag" "${tag%%[!0-9]*}" <<< "$tags")"
    scope="newest stable tag of the ${tag%%[!0-9]*} line, held for $held"
  fi
  if [[ -z "$compared" ]]; then
    printf 'UNREADABLE %s: %s has no stable tag of the shape of %s\n' "$constant" "$repository" "$tag"
    unreadable=1
    continue
  fi

  later=""
  [[ "$newest" = "$compared" ]] || later="; newest stable tag of any line $newest"
  if [[ "$compared" = "$tag" ]]; then
    printf 'current    %s %s:%s%s\n' "$constant" "$repository" "$tag" "$later"
  else
    printf 'STALE      %s: %s pinned %s, %s %s%s\n' \
      "$constant" "$repository" "$tag" "$scope" "$compared" "$later"
    stale=1
  fi
done <<< "$images"

[[ "$unreadable" -eq 0 ]] || exit 2
exit "$stale"
