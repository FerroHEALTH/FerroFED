#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/obligations.sh: the obligations checklist's source
# digests and its rendering into the book (#278).
#
#   scripts/conformance/obligations.sh --derived FILE  write the source digests into FILE
#   scripts/conformance/obligations.sh --derive        refresh conformance/obligation-sources.tsv
#   scripts/conformance/obligations.sh --render        print the book page to stdout
#   scripts/conformance/obligations.sh --render-write  write the book page
#
# conformance/obligations.tsv is kept by hand: a person reads the vendored
# text and classifies each statement. What a script can hold to the text is
# the set of lines that carry an RFC 2119 keyword, so the source table keeps,
# for every vendored page and schema, the number of such lines and the
# SHA-256 of them in order. A re-pin that adds, removes or rewords one of
# them changes that page's digest, and scripts/checks/obligations.sh fails
# until the page's rows are reclassified and `--derive` is run again.
#
# No specification governs the digest or the page: our own design.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

readonly SPEC=docs/specs/federation-spec/modules/ROOT
readonly OBLIGATIONS=conformance/obligations.tsv
readonly SOURCES=conformance/obligation-sources.tsv
readonly PAGE=website/book/src/evaluate/obligations.md
readonly SPEC_SITE=https://syntaric.github.io/openehr-federation-spec/federation-aql
readonly REPO=https://github.com/FerroHEALTH/FerroFED
# The keywords of RFC 2119 and RFC 8174 that mark a normative line.
readonly KEYWORDS='MUST|SHALL|SHOULD|REQUIRED|RECOMMENDED|MAY|OPTIONAL'

die() {
  echo "obligations: $*" >&2
  exit 1
}

sha256() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum | cut -d' ' -f1
  elif command -v shasum > /dev/null 2>&1; then
    shasum -a 256 | cut -d' ' -f1
  else
    die "neither sha256sum nor shasum is installed"
  fi
}

# The source table: one row per vendored page and schema, in path order,
# with the number of keyword lines and their digest.
derive_into() {
  local out="$1" file lines
  [ -d "$SPEC/pages" ] || die "the vendored specification is missing ($SPEC)"
  {
    printf 'source\tlines\tsha256\n'
    find "$SPEC/pages" "$SPEC/attachments" -type f \( -name '*.adoc' -o -name '*.json' \) | LC_ALL=C sort |
      while IFS= read -r file; do
        lines="$( (grep -w -E "$KEYWORDS" "$file" || true) | wc -l | tr -d '[:space:]')"
        printf '%s\t%s\t%s\n' "${file#"$SPEC"/}" "$lines" "$( (grep -w -E "$KEYWORDS" "$file" || true) | sha256)"
      done
  } > "$out"
}

# The Federation Tier version docs/VERSIONS.md pins, as MAJOR.MINOR: the
# specification site keeps one version per minor release.
spec_site() {
  local version
  version="$(awk -F'|' '
    NF >= 3 {
      k = $2; v = $3
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", k)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", v)
      if (k == "Federation Tier with AQL") { split(v, w, /[[:space:]]/); print w[1]; exit }
    }
  ' docs/VERSIONS.md)"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "docs/VERSIONS.md pins the Federation Tier as \"$version\", not MAJOR.MINOR.PATCH"
  printf '%s/%s\n' "$SPEC_SITE" "${version%.*}"
}

render_page() {
  local site
  site="$(spec_site)"
  awk -F'\t' -v site="$site" -v repo="$REPO" -v spec="$SPEC" '
    # Markdown escapes for a table cell.
    function esc(s) {
      gsub(/\|/, "\\|", s)
      gsub(/\*/, "\\*", s)
      gsub(/</, "\\&lt;", s)
      gsub(/>/, "\\&gt;", s)
      return s
    }
    # Evidence: file::function tokens as code, issue and #212 comment
    # references as tracker links. A # inside a word or an id is no issue.
    function evidence(s,   out, before, tok, num, comment, prev) {
      s = esc(s)
      out = ""
      while (match(s, /[A-Za-z0-9_.\/-]*::[a-z_][a-z0-9_]*|#[0-9]+( comment [0-9]+)?/)) {
        before = substr(s, 1, RSTART - 1)
        tok = substr(s, RSTART, RLENGTH)
        s = substr(s, RSTART + RLENGTH)
        prev = substr(before, length(before), 1)
        if (tok ~ /::/) {
          out = out before "`" tok "`"
        } else if (prev ~ /[A-Za-z0-9.#-]/ || s ~ /^[A-Za-z]/) {
          out = out before tok
        } else {
          num = tok
          sub(/^#/, "", num)
          comment = ""
          if (num ~ / comment /) {
            comment = num
            sub(/^[0-9]+ comment /, "", comment)
            sub(/ comment .*/, "", num)
          }
          if (comment != "") out = out before "[" tok "](" repo "/issues/" num "#issuecomment-" comment ")"
          else out = out before "[" tok "](" repo "/issues/" num ")"
        }
      }
      return out s
    }
    # The statement id, linked to its page of the specification site, or
    # to the vendored schema in the repository.
    function idlink(id, page, anchor,   stem) {
      if (page ~ /\.json$/) return "[" esc(id) "](" repo "/blob/main/" spec "/attachments/" page ")"
      stem = page
      sub(/\.adoc$/, "", stem)
      if (anchor ~ /^[a-z]/) return "[" id "](" site "/" stem ".html#" anchor ")"
      return "[" id "](" site "/" stem ".html)"
    }
    function list(s) {
      gsub(/,/, ", ", s)
      return s
    }
    BEGIN {
      nst = split("tested built-untested planned missing deferred new-gap contradiction node operator n/a", order, " ")
      meaning["tested"] = "a test asserts it"
      meaning["built-untested"] = "the code does it and no test asserts it yet"
      meaning["planned"] = "not built yet; an open issue holds the work"
      meaning["missing"] = "not built, found missing by the audit; an issue holds the work"
      meaning["deferred"] = "not built, by a decision of the owner"
      meaning["new-gap"] = "the text contradicts itself or is silent, found by the audit and reported on #212"
      meaning["contradiction"] = "the text contradicts itself, reported on #212 before the audit"
      meaning["node"] = "a member node must meet it"
      meaning["operator"] = "the federation operator must meet it"
      meaning["n/a"] = "no gateway obligation: a client or an editor must meet it, or it is a permission the gateway does not take"
      ngap = split("missing built-untested new-gap contradiction planned deferred", gaps, " ")
      title["missing"] = "Missing"
      title["built-untested"] = "Built, not yet tested"
      title["new-gap"] = "Contradictions and silences found by the audit"
      title["contradiction"] = "Contradictions already reported"
      title["planned"] = "Planned"
      title["deferred"] = "Deferred"
    }
    /^#/ { next }
    ++row == 1 { next }
    {
      total++
      all[$8]++
      if ($4 == "Gateway") { gw++; gwn[$8]++ }
      n[$8]++
      line[$8, n[$8]] = $0
    }
    END {
      print "<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->"
      print "<!-- SPDX-License-Identifier: BUSL-1.1 -->"
      print "<!-- Generated by scripts/conformance/obligations.sh --render-write from conformance/obligations.tsv. Do not edit. -->"
      print ""
      print "# Obligations checklist"
      print ""
      print "Every normative statement of the Federation Tier with AQL specification,"
      print "on every page and in both JSON schemas, with the status FerroFED holds for"
      print "it. The [conformance matrix](conformance.md) scores the section 17 points;"
      print "this checklist also holds the obligations in the section prose that are no"
      print "point of their own. Each row of"
      print "[`conformance/obligations.tsv`](" repo "/blob/main/conformance/obligations.tsv)"
      print "quotes the statement and names the test, the code, the issue or the"
      print "report behind its status."
      print ""
      printf "**Statements:** %d, of which %d fall on the gateway.\n", total, gw
      print ""
      print "## Counts per status"
      print ""
      print "| Status | Statements | Gateway statements | Meaning |"
      print "|---|---|---|---|"
      for (i = 1; i <= nst; i++) {
        s = order[i]
        printf "| %s | %d | %d | %s |\n", s, all[s], gwn[s], evidence(meaning[s])
      }
      print ""
      print "## Gaps"
      print ""
      print "Every statement FerroFED does not yet meet as the text reads, and every"
      print "statement where the text disagrees with itself, by status."
      for (g = 1; g <= ngap; g++) {
        s = gaps[g]
        print ""
        printf "### %s\n", title[s]
        print ""
        if (n[s] == 0) { print "None."; continue }
        print "| Statement | Actor | Keyword | Requirement | Point | Statement text | Evidence |"
        print "|---|---|---|---|---|---|---|"
        for (i = 1; i <= n[s]; i++) {
          split(line[s, i], f, "\t")
          printf "| %s | %s | %s | %s | %s | %s | %s |\n", idlink(f[1], f[2], f[3]), f[4], f[5], list(f[6]), f[7], esc(f[10]), evidence(f[9])
        }
      }
    }
  ' "$OBLIGATIONS"
}

case "${1:-}" in
--derived)
  [ -n "${2:-}" ] || die "usage: $0 --derived FILE"
  derive_into "$2"
  ;;
--derive)
  work="$(mktemp)"
  trap 'rm -f "$work"' EXIT
  derive_into "$work"
  {
    if [ -f "$SOURCES" ]; then awk '/^#/ { print; next } { exit }' "$SOURCES"; fi
    cat "$work"
  } > "$work.out"
  mv "$work.out" "$SOURCES"
  echo "obligations: refreshed $SOURCES from the vendored specification."
  ;;
--render)
  render_page
  ;;
--render-write)
  render_page > "$PAGE"
  echo "obligations: wrote $PAGE."
  ;;
*)
  die "usage: $0 --derived FILE | --derive | --render | --render-write"
  ;;
esac
