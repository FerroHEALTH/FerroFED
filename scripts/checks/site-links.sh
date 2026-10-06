#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The site link guard: every internal link and anchor of the published site
# resolves (no specification governs this: our own design). It assembles the
# site the way the Docs workflow does (scripts/site/assemble.sh, the landing
# page at the root and the book under /docs/) into a temporary directory, with
# the roadmap block left empty so nothing is read from the network, and runs
# lychee offline over every page with --include-fragments, so a link to a page
# that does not exist and a link to an anchor a page does not carry both fail.
# It then checks README.md the same way, against the repository tree GitHub
# renders it in. --offline skips every http(s) link: the guard sends no
# request anywhere. llms.txt, which the site serves at its root, links only
# the public pages: the README, the documentation, the governance and policy
# files and the specifications, never the coding assistant's working files
# (the root instruction file and the dot-claude tree).
#
# Usage:
#   scripts/checks/site-links.sh              assemble the site and check it
#   scripts/checks/site-links.sh --self-test  prove a broken anchor and a
#                                             missing page fail, and a sound
#                                             page passes
# Needs mdbook, mdbook-toc and mdbook-mermaid (the docs toolchain) and lychee
# on PATH, and stops with exit 1 naming any of them that is missing. Exit 1
# when a link or an anchor does not resolve; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# require TOOL...: stop with exit 1, naming the tool, when one is not on PATH.
require() {
  local tool
  for tool in "$@"; do
    if ! command -v "$tool" > /dev/null 2>&1; then
      echo "site-links: $tool is not on PATH; install it or add its directory (often ~/.cargo/bin) to PATH" >&2
      exit 1
    fi
  done
}

# check_html ROOT: lychee over every HTML page under the absolute path ROOT,
# resolving a root-relative link (/docs/...) against ROOT.
check_html() {
  local root=$1
  lychee --offline --include-fragments --no-progress --format compact \
    --root-dir "$root" "$root/**/*.html"
}

# check_llms FILE: exit 1 naming each line of FILE that links a working file.
check_llms() {
  local found
  found="$(grep -n -E '\]\([^)]*(/\.claude/|/CLAUDE\.md)' "$1" || true)"
  if [[ -n "$found" ]]; then
    printf '%s\n' "$found" | sed "s|^|::error::$1:|; s|\$| links a working file, not a public page|" >&2
    return 1
  fi
}

self_test() {
  local work failed=0
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  mkdir -p "$work/sound" "$work/anchor" "$work/page"
  printf '<html><body><h2 id="here">x</h2><a href="#here">a</a><a href="/b.html#there">b</a></body></html>\n' > "$work/sound/a.html"
  printf '<html><body><h2 id="there">y</h2><a href="a.html">a</a></body></html>\n' > "$work/sound/b.html"
  cp "$work/sound/"*.html "$work/anchor/"
  printf '<html><body><a href="b.html#nowhere">b</a></body></html>\n' > "$work/anchor/c.html"
  cp "$work/sound/"*.html "$work/page/"
  printf '<html><body><a href="missing.html">m</a></body></html>\n' > "$work/page/c.html"

  if check_html "$work/sound" > /dev/null 2>&1; then
    echo "  ok: a site whose every link and anchor resolves passes"
  else
    echo "  FAIL: a sound site was refused" >&2
    failed=1
  fi
  for case in anchor page; do
    if check_html "$work/$case" > /dev/null 2>&1; then
      echo "  FAIL: a site with a broken $case passed" >&2
      failed=1
    else
      echo "  ok: a site with a broken $case fails"
    fi
  done
  printf -- '- [README](https://github.com/o/r/blob/main/README.md): x.\n' > "$work/llms-public.txt"
  printf -- '- [Rules](https://github.com/o/r/blob/main/.claude/rules/x.md): x.\n' > "$work/llms-rules.txt"
  printf -- '- [Discipline](https://github.com/o/r/blob/main/CLAUDE.md): x.\n' > "$work/llms-claude.txt"
  if check_llms "$work/llms-public.txt" > /dev/null 2>&1; then
    echo "  ok: an llms.txt that links public pages passes"
  else
    echo "  FAIL: an llms.txt that links public pages was refused" >&2
    failed=1
  fi
  for case in rules claude; do
    if check_llms "$work/llms-$case.txt" > /dev/null 2>&1; then
      echo "  FAIL: an llms.txt that links a $case working file passed" >&2
      failed=1
    else
      echo "  ok: an llms.txt that links a $case working file fails"
    fi
  done
  [[ "$failed" -eq 0 ]] && echo "site-links self-test: OK"
  return "$failed"
}

case "${1:-}" in
--self-test)
  require lychee
  self_test
  exit
  ;;
"") ;;
*)
  echo "usage: $0 [--self-test]" >&2
  exit 2
  ;;
esac

echo "== llms.txt (public pages only)"
check_llms llms.txt

require mdbook mdbook-toc mdbook-mermaid lychee

site="$(mktemp -d)"
trap 'rm -rf "$site"' EXIT

SITE_ROADMAP=off scripts/site/assemble.sh "$site/out"
echo "== site links (the assembled site, offline, with anchors)"
check_html "$site/out"
echo "== README links (the repository tree, offline, with anchors)"
lychee --offline --include-fragments --no-progress --format compact README.md
echo "site-links: OK"
