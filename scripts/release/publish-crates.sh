#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The crates.io lane behind the workspace `publish` switch (the Cargo reference,
# Publishing on crates.io; no specification governs the policy: our own design),
# one implementation shared by publish-crates.yml and the pull-request
# `publish-dry-run` job of ci.yml.
#
# The publishable set is read from `cargo metadata`, never listed by hand: a
# member is publishable when its `publish` field is unset or names crates-io.
# While the root `[workspace.package] publish = false`, every member reports
# `publish = []`, the set is empty, and `publish` and `verify` are successful
# no-ops that say so. The library crates are the members under `crates/`; they
# are packaged on every pull request whatever the switch says, so flipping it
# never meets a crate that cannot be packaged.
#
# Upload is per crate, in dependency order, because `cargo publish
# --workspace` refuses the whole run when any member version already exists
# while being non-atomic at the end: a partial publish could not be finished by
# re-running it. One crate at a time, "already exists" counted as done, makes
# the lane resumable and idempotent; the registry is read back before success
# is reported.
#
# Usage:
#   publish-crates.sh libraries        # the crates/* members, one per line
#   publish-crates.sh select           # the publishable members, dependency order
#   publish-crates.sh package          # package every library crate (no upload)
#   publish-crates.sh publish          # upload each publishable crate
#   publish-crates.sh verify           # read the registry back, with retries
#   publish-crates.sh version CRATE    # print one member's manifest version
#
# Requires cargo, curl and jq; `publish` also needs CARGO_REGISTRY_TOKEN.
set -euo pipefail
cd "$(dirname "$0")/../.."

metadata() {
  cargo metadata --format-version 1 --no-deps --locked
}

# The crates/* members, the libraries a third party could use.
libraries() {
  metadata | jq -r '
    .workspace_root as $root
    | .packages[]
    | select(.manifest_path | startswith($root + "/crates/"))
    | .name' | LC_ALL=C sort
}

# The publishable members, each after every publishable member it depends on
# (normal and build dependencies; a dev-dependency is stripped from the
# packaged manifest and orders nothing).
select_crates() {
  metadata | jq -r '
    [ .packages[]
      | select(.publish == null or (.publish | index("crates-io") != null)) ] as $pub
    | ($pub | map(.name)) as $names
    | [ $pub[] | {name, deps: [ .dependencies[]
        | select(.kind != "dev" and (.name as $d | $names | index($d) != null))
        | .name ] } ] as $nodes
    | def order($done; $left):
        if ($left | length) == 0 then $done
        else
          ($left | map(select(.deps - $done == [])) | map(.name) | sort) as $ready
          | if ($ready | length) == 0 then error("a dependency cycle among the publishable crates")
            else order($done + $ready; $left | map(select(.name as $n | $ready | index($n) == null)))
            end
        end;
      order([]; $nodes)[]'
}

# The `[package]` table's own `version` in one member's manifest.
manifest_version() {
  local crate="${1:?crate name}"
  metadata | jq -r --arg c "$crate" '.packages[] | select(.name == $c) | .version'
}

# One `--config` patch per crates/* member another member depends on, pointing
# crates.io at its directory. Cargo resolves a library's dependency on another
# library through its local overlay only when a target registry is known, and
# while the switch is off there is none, so the patch stands in for the overlay.
library_patches() {
  metadata | jq -r '
    .workspace_root as $root
    | [ .packages[] | select(.manifest_path | startswith($root + "/crates/")) ] as $libs
    | ($libs | map(.name)) as $names
    | ([ $libs[].dependencies[]
        | select(.kind != "dev" and (.name as $d | $names | index($d) != null))
        | .name ] | unique) as $used
    | $libs[]
    | select(.name as $n | $used | index($n) != null)
    | "--config", "patch.crates-io.\(.name).path=\"\(.manifest_path | rtrimstr("/Cargo.toml"))\""'
}

do_package() {
  local args=() patches=() crate selected
  while IFS= read -r crate; do args+=(-p "$crate"); done < <(libraries)
  if ((${#args[@]} == 0)); then
    echo "publish-crates: no crates/* member, nothing to package"
    return 0
  fi
  while IFS= read -r crate; do patches+=("$crate"); done < <(library_patches)
  # `cargo package` builds and verifies the exact tarball an upload would send,
  # and it works while the switch is off; `cargo publish --dry-run` refuses a
  # member whose `publish` is false, so it runs only over the publishable set,
  # and without the patches, so it meets the registry as an upload would.
  cargo package --locked "${patches[@]}" "${args[@]}"
  selected="$(select_crates)"
  if [[ -n "$selected" ]]; then
    args=()
    while IFS= read -r crate; do args+=(-p "$crate"); done <<<"$selected"
    cargo publish --dry-run --locked "${args[@]}"
  else
    echo "publish-crates: the workspace publish switch is false, so the registry dry run is skipped"
  fi
}

# cargo colours the status word, so "Uploaded" is followed by a reset sequence
# before the crate name; strip the colour before matching.
readonly ESC=$'\033'
strip_ansi() {
  sed -E "s/${ESC}\\[[0-9;]*m//g"
}

do_publish() {
  local crate out plain failed="" selected
  selected="$(select_crates)"
  if [[ -z "$selected" ]]; then
    echo "publish-crates: the workspace publish switch is false; nothing is published (no-op)"
    return 0
  fi
  while IFS= read -r crate; do
    echo "::group::$crate $(manifest_version "$crate")"
    out="$(cargo publish -p "$crate" --locked 2>&1)" || true
    printf '%s\n' "$out"
    echo "::endgroup::"
    plain="$(printf '%s' "$out" | strip_ansi)"
    case "$plain" in
    *"already exists on crates.io index"* | *"already uploaded"*)
      echo "$crate: already published at this version, nothing to do"
      ;;
    *)
      if printf '%s' "$plain" | grep -q "Uploaded $crate"; then
        echo "$crate: uploaded"
      else
        failed="$failed $crate"
      fi
      ;;
    esac
  done <<<"$selected"
  [[ -z "$failed" ]] || {
    echo "::error::failed to publish:$failed" >&2
    return 1
  }
  echo "publish-crates: every publishable crate is at its manifest version or was already there"
}

# A half-published set is worse than an unpublished one: while a line is 0.x,
# cargo treats every 0.x as its own compatibility set, so one straggler makes
# its siblings' requirements unresolvable. Read the registry, never the exit
# code alone.
do_verify() {
  local crate want got body bad="" selected count=0
  selected="$(select_crates)"
  if [[ -z "$selected" ]]; then
    echo "publish-crates: the workspace publish switch is false; nothing to verify (no-op)"
    return 0
  fi
  while IFS= read -r crate; do
    count=$((count + 1))
    want="$(manifest_version "$crate")"
    # The index is eventually consistent right after an upload: a miss is
    # retried, and a failed request counts as "not seen yet", not as a miss.
    got=""
    for _ in 1 2 3 4 5 6; do
      if body="$(curl --proto '=https' --tlsv1.2 -sSL --fail \
        -H 'User-Agent: ferrofed-publish-verify (https://github.com/FerroHEALTH/FerroFED)' \
        "https://crates.io/api/v1/crates/$crate/versions" 2>/dev/null)"; then
        got="$(printf '%s' "$body" |
          jq -r --arg v "$want" '.versions[]? | select(.num == $v) | .num' |
          head -1)" || got=""
      fi
      [[ -n "$got" ]] && break
      sleep 10
    done
    printf '%-24s want %-10s %s\n' "$crate" "$want" "${got:-MISSING}"
    [[ -n "$got" ]] || bad="$bad $crate@$want"
  done <<<"$selected"
  [[ -z "$bad" ]] || {
    echo "::error::the published set is incomplete:$bad" >&2
    return 1
  }
  echo "publish-crates: confirmed on crates.io, all $count crates at their manifest versions"
}

case "${1:-}" in
libraries) libraries ;;
select) select_crates ;;
package) do_package ;;
publish) do_publish ;;
verify) do_verify ;;
version) manifest_version "${2:-}" ;;
*)
  echo "publish-crates: expected libraries, select, package, publish, verify or version, got '${1:-<none>}'" >&2
  exit 2
  ;;
esac
