#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/chapter-iii.sh: the book page tables of FerroFED's
# Chapter III conformity record (#525), for Regulation (EU) 2025/327.
#
#   scripts/conformance/chapter-iii.sh --page-block  print the page block
#   scripts/conformance/chapter-iii.sh --write       write it in place
#
# conformance/chapter-iii.tsv is the source. The block between the
# chapter-iii:begin and chapter-iii:end markers of
# website/book/src/evaluate/conformity-record.md holds the count of rows
# per status, the table of the Articles' rows and the table of the Annex IV
# rows. scripts/checks/chapter-iii.sh holds the page to a fresh rendering.
# No specification governs this: our own design.
set -euo pipefail
export LC_ALL=C

readonly RECORD=conformance/chapter-iii.tsv
readonly PAGE=website/book/src/evaluate/conformity-record.md
readonly BOOK=website/book/src/
readonly REPO_URL=https://github.com/FerroHEALTH/FerroFED
readonly BEGIN='<!-- chapter-iii:begin -->'
readonly END='<!-- chapter-iii:end -->'

die() {
  echo "chapter-iii: $*" >&2
  exit 2
}

usage() {
  sed -n '/^#   scripts/p' "$0" >&2
  exit 2
}

# page_block: the rendered block, markers included.
page_block() {
  [[ -f "$RECORD" ]] || die "$RECORD is missing"
  printf '%s\n' "$BEGIN"
  printf '<!-- Rendered from %s by scripts/conformance/chapter-iii.sh --write. Do not edit between the markers. -->\n' "$RECORD"
  awk -F'\t' -v book="$BOOK" -v repo="$REPO_URL" '
    function slug(text,   s) {
      s = tolower(text)
      gsub(/[^a-z0-9 _-]/, "", s)
      gsub(/ /, "-", s)
      return s
    }
    function rel(path,   p) {
      p = substr(path, length(book) + 1)
      if (p ~ /^evaluate\//) return substr(p, length("evaluate/") + 1)
      return "../" p
    }
    function title(path,   line, t) {
      t = path
      while ((getline line < path) > 0)
        if (line ~ /^# /) { t = substr(line, 3); break }
      close(path)
      return t
    }
    function item(it,   path, anchor, i) {
      i = index(it, "::")
      path = it
      anchor = ""
      if (i > 0) { path = substr(it, 1, i - 1); anchor = substr(it, i + 2) }
      if (index(path, book) == 1 && path ~ /\.md$/) {
        if (anchor == "") return "[" title(path) "](" rel(path) ")"
        return "[" anchor "](" rel(path) "#" slug(anchor) ")"
      }
      if (path ~ /\.rs$/ && anchor != "") return "`" path "` (`" anchor "`)"
      if (anchor != "") return "[`" path "`](" repo "/blob/main/" path "), " anchor
      return "[`" path "`](" repo "/blob/main/" path ")"
    }
    function evidence(text,   n, parts, k, out) {
      n = split(text, parts, "; ")
      out = ""
      for (k = 1; k <= n; k++) out = out (k > 1 ? "; " : "") item(parts[k])
      return out
    }
    function issues(text,   n, parts, k, out, owner, num) {
      if (text == "-") return "-"
      n = split(text, parts, " ")
      out = ""
      for (k = 1; k <= n; k++) {
        num = parts[k]
        sub(/^.*#/, "", num)
        owner = parts[k]
        sub(/#.*$/, "", owner)
        if (owner == "") owner = "FerroHEALTH/FerroFED"
        out = out (k > 1 ? ", " : "") "[" parts[k] "](https://github.com/" owner "/issues/" num ")"
      }
      return out
    }
    function applies(code) {
      if (code == "2027") return "26 March 2027"
      if (code == "2031") return "26 March 2031"
      if (code == "2029+2031") return "26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f)"
      return code
    }
    function line(i) {
      return "| " prov[i] " | \"" req[i] "\" | " applies(app[i]) " | " st[i] " | " evidence(ev[i]) " | " issues(iss[i]) " | " note[i] " |"
    }
    /^#/ || /^$/ { next }
    $1 == "provision" { next }
    {
      n++
      prov[n] = $1; req[n] = $2; app[n] = $3; st[n] = $4; ev[n] = $5; iss[n] = $6; note[n] = $7
      count[$4]++
    }
    END {
      print ""
      print "| Status | Rows |"
      print "|---|---|"
      split("met open awaits-act manufacturer not-applicable", order, " ")
      for (k = 1; k <= 5; k++) printf "| %s | %d |\n", order[k], count[order[k]] + 0
      printf "| all | %d |\n", n
      print ""
      print "### The obligations"
      print ""
      print "| Provision | Requirement | Applies from | Status | Evidence | Issue | Note |"
      print "|---|---|---|---|---|---|---|"
      for (i = 1; i <= n; i++) if (prov[i] !~ /^Annex IV /) print line(i)
      print ""
      print "### The declaration item by item"
      print ""
      print "| Provision | Requirement | Applies from | Status | Evidence | Issue | Note |"
      print "|---|---|---|---|---|---|---|"
      for (i = 1; i <= n; i++) if (prov[i] ~ /^Annex IV /) print line(i)
      print ""
    }
  ' "$RECORD"
  printf '%s\n' "$END"
}

# write: replace the block of the page with a fresh rendering.
write() {
  local block tmp
  [[ -f "$PAGE" ]] || die "$PAGE is missing"
  if ! grep -Fxq "$BEGIN" "$PAGE" || ! grep -Fxq "$END" "$PAGE"; then
    die "$PAGE has no $BEGIN and $END markers"
  fi
  block="$(mktemp)"
  tmp="$(mktemp)"
  page_block > "$block"
  awk -v begin="$BEGIN" -v end="$END" -v block="$block" '
    $0 == begin { while ((getline l < block) > 0) print l; skip = 1; next }
    $0 == end { skip = 0; next }
    !skip { print }
  ' "$PAGE" > "$tmp"
  mv "$tmp" "$PAGE"
  rm -f -- "$block"
}

cd "$(dirname "$0")/../.."
case "${1:-}" in
--page-block)
  [[ $# -eq 1 ]] || usage
  page_block
  ;;
--write)
  [[ $# -eq 1 ]] || usage
  write
  ;;
*) usage ;;
esac
