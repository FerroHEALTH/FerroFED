#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# Version-drift guard (docs/VERSIONS.md is the single source of truth).
#
# Every file that repeats a pin must agree with the matrix. The repository is
# in its design phase, so a check whose subject file is absent SKIPS LOUDLY
# with a printed reason, and gains teeth the moment the file appears.
#
#   1. specification pins  the Federation Tier with AQL, openEHR ITS-REST and
#                          openEHR AQL rows of the docs/architecture.md pin
#                          table against docs/VERSIONS.md.
#   2. model crates        openehr-query and openehr-its across
#                          docs/architecture.md, docs/VERSIONS.md, and the root
#                          Cargo.toml [workspace.dependencies] requirement.
#   3. toolchain           rust-toolchain.toml channel, plus the root
#                          Cargo.toml edition, rust-version and resolver.
#   4. product version     CITATION.cff version against the docs/VERSIONS.md
#                          product-version row, and against the root Cargo.toml
#                          [workspace.package] version once that exists.
#   5. CI tool pins        the zizmor, actionlint, shellcheck and hadolint
#                          versions .github/workflows/ci.yml installs, and the
#                          cargo-auditable, cargo-cyclonedx and syft versions
#                          the release workflows install.
#   6. docs toolchain      the mdBook, mdbook-toc and mdbook-mermaid defaults of
#                          .github/actions/docs-toolchain/action.yml.
#   7. testkit images      the PinnedImage constants of the testkit container
#                          harness against the docs/VERSIONS.md image rows.
#   8. vendored corpora    every docs/specs/*/PROVENANCE.md names the commit or
#                          tag its docs/VERSIONS.md corpus row pins, and the
#                          federation specification's provenance declares the
#                          version the specification row pins.
#   9. container images    the FROM of docker/Dockerfile against the base-image
#                          row, every digest-pinned compose.yaml image against
#                          a row naming the same reference, and the
#                          compose.yaml gateway tag default against the product
#                          version.
#  10. licence             LICENSE is the Business Source License 1.1 and no
#                          first-party file claims MIT or Apache-2.0 as its
#                          own.
#
# FerroFED's own database image gets a check of its own in the change that adds
# its first pin row.
#
# Usage:
#   scripts/checks/versions.sh
#
# Exit 0 = every present check agrees (skips are fine). Exit 1 = a real drift.
#
# No specification governs this file; it is FerroFED's own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

matrix=docs/VERSIONS.md

fail=0
note() { printf '  %s\n' "$*"; }
bad() {
  printf '  DRIFT: %s\n' "$*" >&2
  fail=1
}

if [ ! -f "$matrix" ]; then
  echo "versions: $matrix is missing, and it is the source of truth" >&2
  exit 1
fi

# The first whitespace-separated token of the second cell of the markdown table
# row whose first cell is ITEM, with surrounding spaces and backticks removed.
pin_of() {
  awk -F'|' -v item="$1" '
    NF >= 3 {
      k = $2; v = $3
      gsub(/`/, "", k); gsub(/`/, "", v)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == item) { split(v, w, /[[:space:]]/); print w[1]; exit }
    }
  ' "$2"
}

# The whole second cell of the row whose first cell is ITEM, backticks removed.
pin_cell_of() {
  awk -F'|' -v item="$1" '
    NF >= 3 {
      k = $2; v = $3
      gsub(/`/, "", k); gsub(/`/, "", v)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == item) { print v; exit }
    }
  ' "$2"
}

# The value of KEY inside TOML table TABLE, unquoted.
toml_val() {
  awk -v table="$1" -v key="$2" '
    /^[[:space:]]*\[/ { h = $0; gsub(/[[:space:]]/, "", h); f = (h == table); next }
    f && $0 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); exit }
      sub(/^[^=]*=[[:space:]]*/, "")
      gsub(/[[:space:]]/, "")
      print; exit
    }
  ' "$3"
}

# The version requirement of dependency NAME in the root Cargo.toml, in either
# the `name = "x.y.z"` or the `name = { version = "x.y.z" }` form.
manifest_req() {
  awk -v name="$1" '
    $0 ~ "^[[:space:]]*" name "[[:space:]]*=" {
      if (match($0, /version[[:space:]]*=[[:space:]]*"[^"]+"/)) {
        s = substr($0, RSTART, RLENGTH)
      } else if (match($0, /=[[:space:]]*"[^"]+"/)) {
        s = substr($0, RSTART, RLENGTH)
      } else { next }
      match(s, /"[^"]+"/)
      print substr(s, RSTART + 1, RLENGTH - 2); exit
    }
  ' Cargo.toml
}

# The `default:` of composite-action input KEY, unquoted. An input key sits at
# two spaces of indentation and its own keys at four, which is what the exact
# prefix comparisons below rely on.
action_default() {
  awk -v key="  $1:" '
    $0 == key { inside = 1; next }
    inside && index($0, "    default:") == 1 {
      sub(/^[[:space:]]*default:[[:space:]]*/, "")
      gsub(/"/, "")
      gsub(/[[:space:]]/, "")
      print
      exit
    }
    inside && $0 ~ /^[^[:space:]]/ { exit }
  ' "$2"
}

echo "== specification pins (docs/architecture.md <-> $matrix)"
specs=("Federation Tier with AQL" "openEHR ITS-REST" "openEHR AQL")
if [ -f docs/architecture.md ]; then
  agreed=0
  for item in "${specs[@]}"; do
    arch="$(pin_of "$item" docs/architecture.md)"
    want="$(pin_of "$item" "$matrix")"
    if [ -z "$arch" ]; then
      bad "docs/architecture.md has no '$item' pin row"
    elif [ -z "$want" ]; then
      bad "$matrix has no '$item' pin row"
    elif [ "$arch" != "$want" ]; then
      bad "$item: docs/architecture.md says $arch, $matrix pins $want"
    else
      agreed=$((agreed + 1))
    fi
  done
  [ "$agreed" -eq "${#specs[@]}" ] && note "OK: all ${#specs[@]} specification pins agree"
else
  for item in "${specs[@]}"; do
    [ -n "$(pin_of "$item" "$matrix")" ] || bad "$matrix has no '$item' pin row"
  done
  note "no docs/architecture.md yet, skipped the comparison (the research program writes it)"
fi

echo "== model crate pins (docs/architecture.md <-> $matrix <-> Cargo.toml)"
# The openehr-* family is released in lockstep, so its rows are one group: a
# member that moves alone is drift even when its own file pair agrees.
family_pin=""
for crate in openehr-query openehr-its openehr-base openehr-rm openehr-sdt; do
  want="$(pin_of "$crate" "$matrix")"
  if [ -z "$want" ]; then
    bad "$matrix has no $crate row"
    continue
  fi
  if [ -z "$family_pin" ]; then
    family_pin="$want"
  elif [ "$want" != "$family_pin" ]; then
    bad "$crate: $matrix pins $want, the rest of the openehr-* family $family_pin; the family moves together"
  fi
  case "$crate" in
  openehr-query | openehr-its) ;;
  *)
    if [ -f Cargo.toml ]; then
      req="$(manifest_req "$crate")"
      if [ -z "$req" ]; then
        note "root Cargo.toml has no $crate requirement yet, skipped"
      elif [ "$req" != "$want" ]; then
        bad "$crate: root Cargo.toml requires $req, $matrix pins $want"
      else
        note "OK: $crate $want (root Cargo.toml agrees)"
      fi
    fi
    continue
    ;;
  esac
  if [ -f docs/architecture.md ]; then
    arch="$(pin_of "$crate" docs/architecture.md)"
    if [ -z "$arch" ]; then
      bad "docs/architecture.md has no $crate row"
      continue
    elif [ "$arch" != "$want" ]; then
      bad "$crate: docs/architecture.md says $arch, $matrix pins $want"
      continue
    fi
  fi
  if [ -f Cargo.toml ]; then
    req="$(manifest_req "$crate")"
    if [ -z "$req" ]; then
      note "root Cargo.toml has no $crate requirement yet, skipped"
    elif [ "$req" != "$want" ]; then
      bad "$crate: root Cargo.toml requires $req, $matrix pins $want"
    else
      note "OK: $crate $want (root Cargo.toml agrees)"
    fi
  else
    note "no root Cargo.toml yet, skipped the $crate requirement ($matrix pins $want)"
  fi
done

echo "== toolchain (rust-toolchain.toml and Cargo.toml <-> $matrix)"
if [ -f rust-toolchain.toml ]; then
  chan="$(toml_val "[toolchain]" channel rust-toolchain.toml)"
  want="$(pin_of "Rust toolchain" "$matrix")"
  if [ -z "$chan" ]; then
    bad "rust-toolchain.toml has no [toolchain] channel"
  elif [ -z "$want" ]; then
    bad "$matrix has no 'Rust toolchain' row"
  elif [ "$chan" != "$want" ]; then
    bad "toolchain: rust-toolchain.toml channel is $chan, $matrix pins $want"
  else
    note "OK: the toolchain is $chan"
  fi
else
  note "no rust-toolchain.toml yet, skipped"
fi

if [ -f Cargo.toml ]; then
  check_row() {
    local label="$1" found="$2" row="$3" want
    want="$(pin_of "$row" "$matrix")"
    if [ -z "$found" ]; then
      note "root Cargo.toml has no $label yet, skipped"
    elif [ -z "$want" ]; then
      bad "$matrix has no '$row' row"
    elif [ "$found" != "$want" ]; then
      bad "$label: root Cargo.toml says $found, $matrix pins $want"
    else
      note "OK: $label is $found"
    fi
  }
  check_row edition "$(toml_val "[workspace.package]" edition Cargo.toml)" "Edition"
  check_row rust-version "$(toml_val "[workspace.package]" rust-version Cargo.toml)" "MSRV"
  check_row resolver "$(toml_val "[workspace]" resolver Cargo.toml)" "Cargo resolver"
else
  note "no root Cargo.toml yet, skipped the edition, MSRV and resolver rows"
fi

echo "== product version (CITATION.cff <-> $matrix <-> Cargo.toml)"
want_product="$(pin_of "Product version" "$matrix")"
[ -n "$want_product" ] || bad "$matrix has no 'Product version' row"
if [ -f CITATION.cff ]; then
  cff="$(sed -nE 's/^version:[[:space:]]*//p' CITATION.cff | head -n1 | tr -d '"'\''[:space:]')"
  if [ -z "$cff" ]; then
    bad "CITATION.cff has no version"
  elif [ "$cff" != "$want_product" ]; then
    bad "product version: CITATION.cff says $cff, $matrix pins $want_product"
  else
    note "OK: CITATION.cff and $matrix both name $cff"
  fi
else
  note "no CITATION.cff yet, skipped"
fi
if [ -f Cargo.toml ]; then
  cargo_ver="$(toml_val "[workspace.package]" version Cargo.toml)"
  if [ -z "$cargo_ver" ]; then
    bad "root Cargo.toml has no [workspace.package] version"
  elif [ "$cargo_ver" != "$want_product" ]; then
    bad "product version: root Cargo.toml says $cargo_ver, $matrix pins $want_product"
  else
    note "OK: root Cargo.toml names $cargo_ver"
  fi
else
  note "no root Cargo.toml yet, skipped its version"
fi

echo "== CI tool pins (.github/workflows/ci.yml <-> $matrix)"
ci=.github/workflows/ci.yml
if [ -f "$ci" ]; then
  # The version each analyzer is pinned to in the workflow: an installer
  # `tool: name@version` line, or the tag of a digest-pinned image.
  ci_tool_pin() {
    case "$1" in
    zizmor | shellcheck)
      sed -nE "s|^[[:space:]]*tool:[[:space:]]*$1@([^[:space:]]+).*|\1|p" "$ci" | sort -u
      ;;
    actionlint)
      sed -nE 's|.*rhysd/actionlint:([^@[:space:]]+)@sha256:.*|\1|p' "$ci" | sort -u
      ;;
    hadolint)
      sed -nE 's|.*hadolint/hadolint:v([^@[:space:]]+)@sha256:.*|\1|p' "$ci" | sort -u
      ;;
    esac
  }
  for tool in zizmor actionlint shellcheck hadolint; do
    want="$(pin_of "$tool" "$matrix")"
    found="$(ci_tool_pin "$tool")"
    if [ -z "$want" ]; then
      bad "$matrix has no '$tool' row"
    elif [ -z "$found" ]; then
      # hadolint has nothing to lint until a Dockerfile exists, so its absence
      # from the workflow is a skip; the other three always run.
      if [ "$tool" = hadolint ] && ! grep -q hadolint "$ci"; then
        note "$ci runs no hadolint yet, skipped"
      else
        bad "$ci pins no $tool version"
      fi
    elif [ "$(printf '%s\n' "$found" | wc -l | tr -d '[:space:]')" != "1" ]; then
      bad "$tool: $ci pins more than one version ($(printf '%s' "$found" | tr '\n' ' '))"
    elif [ "$found" != "$want" ]; then
      bad "$tool: $ci pins $found, $matrix pins $want"
    else
      note "OK: $tool $found"
    fi
  done
else
  note "no $ci yet, skipped"
fi

release_workflows=(.github/workflows/release-build.yml .github/workflows/release-image.yml .github/workflows/fuzz.yml)
# Every version of TOOL the release and fuzz workflows install, deduplicated, so a tool
# named in both files has to carry the same pin in both.
release_tool_pins() {
  local wf
  for wf in "${release_workflows[@]}"; do
    [ -f "$wf" ] || continue
    sed -nE "s|^[[:space:]]*tool:[[:space:]]*$1@([^[:space:]]+).*|\1|p" "$wf"
  done | sort -u
}
if [ -f "${release_workflows[0]}" ] || [ -f "${release_workflows[1]}" ]; then
  for tool in cargo-auditable cargo-cyclonedx syft cargo-fuzz; do
    want="$(pin_of "$tool" "$matrix")"
    found="$(release_tool_pins "$tool")"
    if [ -z "$want" ]; then
      bad "$matrix has no '$tool' row"
    elif [ -z "$found" ]; then
      bad "the release workflows pin no $tool version"
    elif [ "$(printf '%s\n' "$found" | wc -l | tr -d '[:space:]')" != "1" ]; then
      bad "$tool: the release workflows disagree ($(printf '%s' "$found" | tr '\n' ' '))"
    elif [ "$found" != "$want" ]; then
      bad "$tool: the release workflows pin $found, $matrix pins $want"
    else
      note "OK: $tool $found"
    fi
  done
else
  note "no release-build.yml or release-image.yml yet, skipped the release tool pins"
fi

echo "== docs toolchain (.github/actions/docs-toolchain <-> $matrix)"
action=.github/actions/docs-toolchain/action.yml
if [ -f "$action" ]; then
  agreed=0
  for tool in mdbook mdbook-toc mdbook-mermaid; do
    if [ "$tool" = mdbook ]; then row=mdBook; else row="$tool"; fi
    found="$(action_default "$tool-version" "$action")"
    want="$(pin_of "$row" "$matrix")"
    if [ -z "$found" ]; then
      bad "$action has no $tool-version default"
    elif [ -z "$want" ]; then
      bad "$matrix has no '$row' row"
    elif [ "$found" != "$want" ]; then
      bad "$tool: $action installs $found, $matrix pins $want"
    else
      agreed=$((agreed + 1))
    fi
  done
  [ "$agreed" -eq 3 ] && note "OK: the three docs-toolchain pins agree"
else
  note "no $action yet, skipped"
fi

echo "== testkit images (tools/ferrofed-testkit <-> $matrix)"
harness=tools/ferrofed-testkit/src/containers.rs
if [ -f "$harness" ]; then
  # The repository, tag and digest of the PinnedImage literal named CONST,
  # composed into the one reference the matrix row carries.
  image_pin_of() {
    awk -v name="$1" '
      $0 ~ "^pub const " name ": PinnedImage = PinnedImage \\{" { inside = 1; next }
      inside {
        if ($0 ~ /^\};/) { exit }
        if (match($0, /repository: "[^"]+"/)) { repo = substr($0, RSTART + 13, RLENGTH - 14) }
        if (match($0, /tag: "[^"]+"/)) { tag = substr($0, RSTART + 6, RLENGTH - 7) }
        if (match($0, /digest: "[^"]+"/)) { digest = substr($0, RSTART + 9, RLENGTH - 10) }
      }
      END { if (repo != "" && tag != "" && digest != "") print repo ":" tag "@" digest }
    ' "$2"
  }

  agreed=0
  expected=0
  for image in \
    "FerroEHR node image|FERROEHR" \
    "FerroEHR node database image|FERROEHR_POSTGRES" \
    "EHRbase node image|EHRBASE" \
    "EHRbase node database image|EHRBASE_POSTGRES"; do
    item="${image%%|*}"
    constant="${image##*|}"
    expected=$((expected + 1))
    want="$(pin_of "$item" "$matrix")"
    found="$(image_pin_of "$constant" "$harness")"
    if [ -z "$want" ]; then
      bad "$matrix has no '$item' row"
    elif [ -z "$found" ]; then
      bad "$harness has no $constant PinnedImage with a repository, tag and digest"
    elif [ "$found" != "$want" ]; then
      bad "$item: $harness pins $found, $matrix pins $want"
    else
      agreed=$((agreed + 1))
    fi
  done
  [ "$agreed" -eq "$expected" ] && note "OK: all $expected container image pins agree"
else
  note "no $harness yet, skipped"
fi

echo "== vendored corpora (docs/specs/*/PROVENANCE.md <-> $matrix)"
# The reference a pin cell names: its first 40-hex token, else the token after
# the word `tag`.
pinned_ref_of() {
  awk '{
    for (i = 1; i <= NF; i++) if ($i ~ /^[0-9a-f]{40}$/) { print $i; exit }
    for (i = 1; i < NF; i++) if ($i == "tag") { t = $(i + 1); gsub(/[,.;:]+$/, "", t); print t; exit }
  }' <<< "$1"
}

corpora="docs/specs/federation-spec|Federation Tier with AQL specification
docs/specs/federation-ref|Federation Tier reference implementation
docs/specs/its-rest|openEHR ITS-REST OpenAPI
docs/specs/aql|openEHR AQL specification source"

agreed=0
expected=0
while IFS='|' read -r dir item; do
  [ -n "$dir" ] || continue
  expected=$((expected + 1))
  cell="$(pin_cell_of "$item" "$matrix")"
  want="$(pinned_ref_of "$cell")"
  if [ -z "$cell" ]; then
    bad "$matrix has no '$item' row"
  elif [ -z "$want" ]; then
    bad "the $matrix pin for '$item' names no commit and no tag"
  elif [ ! -f "$dir/PROVENANCE.md" ]; then
    bad "$dir/PROVENANCE.md is missing; run the vendor script that $matrix names for '$item'"
  elif ! grep -qF "$want" "$dir/PROVENANCE.md"; then
    bad "$dir/PROVENANCE.md does not name the pin $want that $matrix records for '$item'"
  else
    agreed=$((agreed + 1))
  fi
done <<< "$corpora"
[ "$agreed" -eq "$expected" ] && note "OK: all $expected corpus provenance stamps name their pin"

# The specification row pins a version and the corpus row a commit; the
# provenance records the version that commit's antora.yml declares, so a re-pin
# that moves one and not the other is caught here.
spec_prov=docs/specs/federation-spec/PROVENANCE.md
if [ -f "$spec_prov" ]; then
  want="$(pin_of "Federation Tier with AQL" "$matrix")"
  found="$(sed -nE "s/.*spec-version: '([^']+)'.*/\1/p" "$spec_prov" | head -n1)"
  if [ -z "$found" ]; then
    bad "$spec_prov records no spec-version"
  elif [ "$found" != "$want" ]; then
    bad "Federation Tier with AQL: the vendored source declares $found, $matrix pins $want"
  else
    note "OK: the vendored federation specification declares $found"
  fi
fi

echo "== container images (docker/Dockerfile, compose.yaml <-> $matrix)"
if [ -f docker/Dockerfile ]; then
  base="$(sed -nE 's|^FROM[[:space:]]+([^[:space:]]+).*|\1|p' docker/Dockerfile | head -n1)"
  want_base="$(pin_of "Container base image" "$matrix")"
  if [ -z "$base" ]; then
    bad "docker/Dockerfile has no FROM"
  elif [ -z "$want_base" ]; then
    bad "$matrix has no 'Container base image' row"
  elif [ "$base" != "$want_base" ]; then
    bad "base image: docker/Dockerfile builds on $base, $matrix pins $want_base"
  else
    note "OK: docker/Dockerfile builds on the pinned base"
  fi
  # The digest belongs to the FROM alone; the base.name label names the tag it
  # was resolved from, and the two must name the same image.
  label_base="$(sed -nE 's|.*org\.opencontainers\.image\.base\.name="([^"]+)".*|\1|p' docker/Dockerfile | head -n1)"
  if [ -n "$base" ] && [ "${base%@*}" != "$label_base" ]; then
    bad "docker/Dockerfile labels its base as '$label_base' but builds on ${base%@*}"
  fi
else
  note "no docker/Dockerfile yet, skipped"
fi
if [ -f compose.yaml ]; then
  # Every digest-pinned image is one of the matrix's pin cells, verbatim.
  pinned="$(sed -nE 's|^[[:space:]]*image:[[:space:]]*([^[:space:]]+@sha256:[0-9a-f]{64})[[:space:]]*$|\1|p' compose.yaml | sort -u)"
  agreed=0
  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    if grep -qF "\`$ref\`" "$matrix"; then
      agreed=$((agreed + 1))
    else
      bad "compose.yaml runs $ref, which no $matrix row pins"
    fi
  done <<< "$pinned"
  [ "$agreed" -gt 0 ] && note "OK: all $agreed digest-pinned compose.yaml images are rows of $matrix"
  # An image that is neither digest-pinned nor the gateway's own is a drift
  # the line above cannot see.
  while IFS= read -r ref; do
    [ -n "$ref" ] || continue
    case "$ref" in
    *@sha256:* | ghcr.io/ferrohealth/ferrofed:*) ;;
    *) bad "compose.yaml runs $ref, which is not pinned by digest" ;;
    esac
  done < <(sed -nE 's|^[[:space:]]*image:[[:space:]]*([^[:space:]]+)[[:space:]]*$|\1|p' compose.yaml)
  tags="$(sed -nE 's|^[[:space:]]*image:[[:space:]]*ghcr\.io/ferrohealth/ferrofed:\$\{[A-Za-z_][A-Za-z0-9_]*:-([^}]+)\}[[:space:]]*$|\1|p' compose.yaml | sort -u)"
  if [ -z "$tags" ]; then
    bad "compose.yaml has no ghcr.io/ferrohealth/ferrofed image tag default"
  elif [ "$(printf '%s\n' "$tags" | wc -l | tr -d '[:space:]')" -gt 1 ]; then
    bad "compose.yaml names more than one ferrofed tag default: $(printf '%s' "$tags" | tr '\n' ' ')"
  elif [ "$tags" != "$want_product" ]; then
    bad "quickstart tag: compose.yaml runs $tags, $matrix pins the product version $want_product"
  else
    note "OK: the compose.yaml gateway tag is the product version $tags"
  fi
else
  note "no compose.yaml yet, skipped"
fi

echo "== licence (LICENSE <-> SPDX headers, manifests, badges, labels)"
if [ -f LICENSE ]; then
  stale=0
  if ! grep -q 'Business Source License 1.1' LICENSE; then
    bad "LICENSE is not the Business Source License 1.1"
    stale=1
  fi
  # The SPDX tag is anchored to the start of its line, after an optional
  # comment marker, so a header claim is caught while the same text quoted
  # inside a string literal is not. Vendored trees keep their upstream terms
  # and are outside the check.
  while IFS= read -r hit; do
    [ -n "$hit" ] || continue
    bad "stale licence claim at $hit"
    stale=1
  done < <(git grep -n -E '^[[:space:]]*([/#*]+|<!--)?[[:space:]]*SPDX-License-Identifier: (MIT|Apache-2\.0)|License-MIT|License-Apache|^license = "(MIT|Apache-2\.0)"|^license: (MIT|Apache-2\.0)|image\.licenses="?(MIT|Apache)' \
    -- ':!LICENSE' ':!CHANGELOG.md' ':!scripts/checks/versions.sh' ':(glob,exclude)**/vendor/**' \
    ':(glob,exclude)docs/specs/**' || true)
  [ "$stale" -eq 0 ] && note "OK: every first-party file names BUSL-1.1"
else
  bad "LICENSE is missing"
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "versions: DRIFT detected" >&2
  exit 1
fi
echo "versions: OK (every present check agrees)"
