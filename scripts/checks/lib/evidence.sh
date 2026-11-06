# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# shellcheck shell=bash
# Shared helpers of the conformity guards (scripts/checks/annex-ii.sh and
# scripts/checks/chapter-iii.sh, #1675), sourced, never run.
#
# evidence_refusals reads the evidence column of a conformity record in one
# awk, reading each file the record names once, so a guard and its
# self-test start no process per row. links_awk is the awk library both
# renderers under scripts/conformance/ put in front of their programs, so a
# book page links an evidence item the same way whichever record it renders.
#
# No specification governs this file: our own design.

# evidence_refusals RECORD COLUMN COLUMNS HEADER: one line per refusal of
# the evidence column COLUMN of the tab-separated RECORD, for each data row
# with COLUMNS columns (comments, blank lines and the header row, whose
# first field is HEADER, are skipped), read from the repository root:
#
#   line N names no evidence.
#   line N: "PATH" is not a path inside the tree.
#   line N: PATH does not exist.
#   line N: PATH has no heading "ANCHOR".
#   line N: PATH has no test "ANCHOR".
#   line N: PATH takes no anchor; only a Markdown heading or a Rust test does.
#
# An item is a file of the tree, followed by "::" and a heading of it for a
# Markdown file or a test function for a Rust file. A heading is a line
# opening with #, outside a fenced code block. A test is a function whose
# attributes, their continuation lines and the comments directly above it,
# up to a blank line or a closing brace, hold a test attribute.
evidence_refusals() {
  awk -F'\t' -v column="$2" -v columns="$3" -v header="$4" '
    # load PATH: reads PATH once into heads[] and tests[]; 0 when it cannot.
    function load(path,   n, k, j, line, fence, name, h, got) {
      if (path in loaded) return loaded[path]
      n = 0
      while ((got = (getline line < path)) > 0) lines[++n] = line
      close(path)
      loaded[path] = (got == 0)
      fence = 0
      for (k = 1; k <= n; k++) {
        if (lines[k] ~ /^```/) { fence = !fence; continue }
        if (!fence && lines[k] ~ /^#+ /) { h = lines[k]; sub(/^#+ /, "", h); heads[path SUBSEP h] = 1 }
        if (match(lines[k], /fn [A-Za-z0-9_]+\(/)) {
          name = substr(lines[k], RSTART + 3, RLENGTH - 4)
          for (j = k - 1; j > 0 && lines[j] !~ /^[[:space:]]*(\}.*)?$/; j--)
            if (lines[j] ~ /^[[:space:]]*#\[.*test(\]|\()/) tests[path SUBSEP name] = 1
        }
      }
      return loaded[path]
    }
    /^#/ || /^$/ || NF != columns || $1 == header { next }
    {
      rest = $column
      if (rest == "-" || rest == "") { printf "line %d names no evidence.\n", FNR; next }
      n = split(rest, items, "; ")
      for (i = 1; i <= n; i++) {
        item = items[i]
        at = index(item, "::")
        path = at > 0 ? substr(item, 1, at - 1) : item
        anchor = at > 0 ? substr(item, at + 2) : ""
        if (path == "" || substr(path, 1, 1) == "/" || index(path, "..") > 0) {
          printf "line %d: \"%s\" is not a path inside the tree.\n", FNR, path
          continue
        }
        if (!load(path)) { printf "line %d: %s does not exist.\n", FNR, path; continue }
        if (anchor == "") continue
        if (path ~ /\.md$/) {
          if (!((path SUBSEP anchor) in heads)) printf "line %d: %s has no heading \"%s\".\n", FNR, path, anchor
        } else if (path ~ /\.rs$/) {
          if (!((path SUBSEP anchor) in tests)) printf "line %d: %s has no test \"%s\".\n", FNR, path, anchor
        } else {
          printf "line %d: %s takes no anchor; only a Markdown heading or a Rust test does.\n", FNR, path
        }
      }
    }
  ' "$1"
}

# links_awk: the awk functions the renderers share, printed for a renderer
# to put in front of its own program. They read the variables book (the
# book's source directory, with its trailing slash) and repo (the
# repository's URL):
#
#   slug(TEXT)      the anchor mdBook gives a heading reading TEXT;
#   evidence(TEXT)  the evidence column TEXT as Markdown links: a book page
#                   relative to website/book/src/evaluate/, a Rust test as
#                   its path and name, any other file as a link to main;
#   issues(TEXT)    the issue column TEXT, #N or Owner/Repo#N separated by
#                   one space, as links, or - for -.
links_awk() {
  cat << 'AWK'
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
AWK
}
