#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Comment-style guard (RFC 505 and RFC 1574, plus the citation rule of 9-10).
#
# Checks HAND-WRITTEN .rs files (a file carrying the `@generated` marker is
# skipped: its comments are fixed in the generator that emitted it):
#
#   1. block comments      `/* … */` is banned; line comments only (RFC 505).
#   2. TODO form           every TODO names its issue: `TODO(#NNNN):`.
#   3. marker vocabulary   only TODO(#N)/NOTE/SAFETY are sanctioned; FIXME,
#                          HACK, XXX, WIP and the (port) forms all fail.
#   4. NOTE budget         a `// NOTE:` block is a citation + one sentence:
#                          at most $NOTE_MAX physical comment lines.
#   5. essay budget        a plain `//` comment run is at most $RUN_MAX lines;
#                          longer prose belongs in doc comments, the PR
#                          description, or the tracker: not in code.
#   6. orphaned lines      a comment whose whole content is punctuation
#                          (`//.`, `///:` …) is sweep residue, not prose.
#   7. quoted markers      a doc line carrying a backtick-quoted marker
#                          (`` `// NOTE:` `` …) reads as a marker to a human
#                          and is invisible to checks 2-4: describe the
#                          marker in words instead.
#   8. empty sections      a bare `/// # Errors` / `# Panics` heading with no
#                          body satisfies the doc lints and tells a caller
#                          nothing.
#   9. internal citations  a comment, a doc comment, a trailing `//` comment
#                          or a lint `reason = "…"` string names an internal
#                          markdown file: the architecture document under
#                          docs, any path into the `.claude` tree, the root
#                          agent instructions file, or the bare file name of a
#                          rule or memory file (read from that tree, so a new
#                          rule is covered). Any other markdown file under
#                          docs, and any README.md of the tree, outside the
#                          vendored docs/specs and vendor trees, fails in the
#                          citation form only: the path opening a
#                          parenthetical, or followed by `section` or `§`.
#  10. decision markers    the same text names a decision-register entry:
#                          the word `decision` (or `decisions`) followed by an
#                          `A` and a digit, or an `A` with one or two digits
#                          standing as its own token. The bare form is judged
#                          outside backtick code spans and outside fenced doc
#                          code blocks, and a letter, digit, `_`, `-`, `.` or
#                          `#` on its left, or a letter, digit, `_` or `-` on
#                          its right, makes it part of another token, so hex
#                          (`0xA1`), `HbA1c`, Annex section numbers such as
#                          `A.1` and an AQL alias quoted in backticks pass.
#
# Checks 9 and 10 also read the HASH files: the full-line `#` comments of
# every shell script under scripts/, every Cargo.toml, clippy.toml, the YAML
# under .github (workflows, composite actions and the repository
# configuration) and the TOML under docker/. In the YAML they also read the
# text of every `echo` or `printf` in a `run:` block, shell escapes removed,
# every `description:` scalar, and in the TOML every `reason = "…"` string.
# In the YAML and the TOML they read the trailing `#` comment after a value
# too, outside quoted strings, run blocks and other block scalars. Heredoc
# bodies are skipped, since they are document text a script writes. One
# difference is deliberate: a HASH file may NAME the architecture document as
# a file a script reads (the versions guard compares its pin table), so only
# the citation form fails there, the document followed by `section` or `§`,
# or opening a parenthetical.
#
# Checks 9 and 10 also read the conformance tables, conformance/*.tsv: their
# full-line `#` comments and, on every data row, the cells of the columns the
# header row names `reason` or `evidence`. The book renders those cells, so
# any mention of the architecture document fails there, as in a Rust comment.
#
# Citation rule: no specification governs this: our own design. A comment
# cites the specification it rests on (section, N, CP) or official external
# documentation, or says that no specification governs the decision.
#
# NOT machine-checked, deliberately: module-doc (`//!`) block LENGTH. The
# longest module docs are governing-section maps and matching-rule contracts,
# longer than any essay; a cap loose enough to keep them catches nothing, and
# a cap tight enough to catch essays would force condensing legitimate
# reference docs. Essay-vs-reference is judgment; review carries it.
#
# Usage:
#   scripts/checks/comment-style.sh --all                 # whole tree
#   scripts/checks/comment-style.sh --diff <base> [head]  # changed files only
#   scripts/checks/comment-style.sh --files <file>...     # named files (hook)
#   scripts/checks/comment-style.sh --self-test           # prove checks 9-10
#
# Exit 0 = clean, 1 = violations (listed as file:line: message), 2 = usage.

set -euo pipefail

NOTE_MAX=3
RUN_MAX=8

cd "$(dirname "$0")/../.."

# The bare names (without `.md`) of the rule and memory files, joined by `|`,
# plus CLAUDE: the alternation check 9 matches a bare file name against.
internal_names() {
  local f names=(CLAUDE)
  for f in .claude/rules/*.md .claude/memory/*.md; do
    [[ -f "$f" ]] || continue
    f="${f##*/}"
    names+=("${f%.md}")
  done
  local IFS='|'
  printf '%s' "${names[*]}"
}
CITE_NAMES="$(internal_names)"
export CITE_NAMES

# The awk functions checks 9 and 10 share between the Rust and the HASH pass.
# The regexes are built from strings in BEGIN (portable across BWK awk, mawk
# and gawk: no interval expressions, no gawk extensions). `strict` selects the
# Rust reading of the architecture document (any mention) over the HASH one
# (the citation form only).
# shellcheck disable=SC2016 # awk source: the `$` and backticks are awk's, never expanded by the shell
CITE_AWK='
  BEGIN {
    arch_any_re = "docs/architecture\\.md"
    arch_cite_re = "(docs/architecture\\.md`?[[:space:]]*(section|§)|\\(`?docs/architecture\\.md)"
    claude_re = "(\\.claude/|CLAUDE\\.md)"
    names_re = ""
    if (ENVIRON["CITE_NAMES"] != "")
      names_re = "(^|[^A-Za-z0-9_./-])(" ENVIRON["CITE_NAMES"] ")\\.md"
    dec_re = "(^|[^A-Za-z0-9_])[Dd]ecisions?[[:space:]]+A[0-9]"
    bare_re = "(^|[^A-Za-z0-9_.#-])A[0-9][0-9]?([^A-Za-z0-9_-]|$)"
    doc_re = "docs/[A-Za-z0-9_./-]*\\.md"
    readme_re = "[A-Za-z0-9_./-]*README\\.md"
    vendored_re = "(^docs/specs/|^vendor/|/vendor/|^//)"
    doc_left_re = "[A-Za-z0-9_./-]$"
    doc_open_re = "\\(`?$"
    doc_sec_re = "^`?[[:space:]]*(section|§)"
  }
  # Whether text cites a file whose path matches re, outside the vendored
  # docs/specs/ and vendor/ trees: the path opens a parenthetical or is
  # followed by `section` or `§`. A path that is the tail of a longer one (a
  # URL) is not. RSTART and RLENGTH are restored, because a caller loops on
  # its own match.
  function path_cited(text, re,   t, pre, m, start, len, found) {
    start = RSTART
    len = RLENGTH
    found = 0
    t = text
    while (!found && match(t, re)) {
      pre = substr(t, 1, RSTART - 1)
      m = substr(t, RSTART, RLENGTH)
      t = substr(t, RSTART + RLENGTH)
      if (m !~ vendored_re && pre !~ doc_left_re && (pre ~ doc_open_re || t ~ doc_sec_re))
        found = 1
    }
    RSTART = start
    RLENGTH = len
    return found
  }
  function cite_check(text, what, strict, fenced,   t) {
    if ((strict ? text ~ arch_any_re : text ~ arch_cite_re) \
        || path_cited(text, doc_re) || path_cited(text, readme_re) \
        || text ~ claude_re || (names_re != "" && text ~ names_re))
      printf ":%d: %s cites an internal markdown file: cite the specification section it rests on, or write \"no specification governs this: our own design\"\n", NR, what
    t = text
    gsub(/`[^`]*`/, "", t)
    if (text ~ dec_re || (!fenced && t ~ bare_re))
      printf ":%d: %s names a decision-register marker: cite the specification section the decision rests on, or write \"no specification governs this: our own design\"\n", NR, what
  }
'

# Prints the violations of one .rs file, one `:LINE: message` per line.
check_rs() {
  local file="$1"
  awk -v NOTE_MAX="$NOTE_MAX" -v RUN_MAX="$RUN_MAX" "$CITE_AWK"'
    function flush_note() {
      if (note_len > NOTE_MAX)
        printf ":%d: NOTE block is %d lines (max %d): a NOTE is a citation + one sentence; move the essay to the PR/issue\n", note_start, note_len, NOTE_MAX
      note_len = 0
    }
    function flush_run() {
      if (run_len > RUN_MAX)
        printf ":%d: comment run is %d lines (max %d): long prose belongs in doc comments or on the PR/issue, not in code\n", run_start, run_len, RUN_MAX
      run_len = 0
    }
    function flush_doc_note() {
      if (doc_note_len > RUN_MAX)
        printf ":%d: NOTE paragraph in a doc comment is %d lines (max %d): an adjudication essay lives on the PR/issue, not in rustdoc\n", doc_note_start, doc_note_len, RUN_MAX
      doc_note_len = 0
    }
    function flush_sec() {
      if (sec_start)
        printf ":%d: `# %s` doc section has no body: it satisfies the doc lint and tells a caller nothing; write the contract or drop the heading\n", sec_start, sec_name
      sec_start = 0
    }
    {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      is_doc  = (line ~ /^\/\/[\/!]/)
      is_line = (!is_doc && line ~ /^\/\//)

      # 9 + 10. internal citations and decision markers. A fenced doc code
      # block toggles on its fence line; any non-doc line closes it.
      if (!is_doc) in_fence = 0
      if (is_doc && line ~ /^\/\/[\/!][[:space:]]*```/) in_fence = !in_fence
      if (is_doc || is_line) {
        cite_check(line, "comment", 1, in_fence)
      } else {
        # A trailing comment: the first `//` after whitespace, outside a
        # string literal (an even count of quotes before it).
        tc = match($0, /[[:space:]]\/\//)
        if (tc > 0) {
          pre = substr($0, 1, tc)
          if (gsub(/"/, "", pre) % 2 == 0)
            cite_check(substr($0, tc + 1), "comment", 1, 0)
        }
      }
      if (match($0, /reason[[:space:]]*=[[:space:]]*"([^"\\]|\\.)*"/))
        cite_check(substr($0, RSTART, RLENGTH), "lint reason", 1, 0)

      # 1. block comments on code lines: a `/*` at line start or after
      # whitespace, before any string literal on the line. The position
      # guard skips glob/media-type/URL text inside multi-line string
      # literals (`*/*`, `dir/*.xml`, `://***@`), which an earlier-opened
      # string puts on a quote-less line.
      if (!is_doc && !is_line) {
        bc = index($0, "/*"); q = index($0, "\"")
        if (bc > 0 && (q == 0 || bc < q) \
            && (bc == 1 || substr($0, bc - 1, 1) ~ /[[:space:]]/))
          printf ":%d: block comment: use line comments (`//`) only (RFC 505)\n", NR
      }

      # 2 + 3. marker forms. TODO is judged only as the LEADING marker of a
      # comment: prose or a verbatim spec quotation mentioning the word is
      # not a marker.
      if (is_doc || is_line) {
        if (line ~ /^\/\/+[!\/]?[[:space:]]*TODO/ && line !~ /TODO\(#[0-9]+\):/)
          printf ":%d: TODO without an issue reference: the only sanctioned form is `TODO(#NNNN):`\n", NR
        if (line ~ /PORT NOTE|PORT STATUS|TODO\(port\)|PERF\(port\)|NOTE\(port\)|FIXME|HACK:|(^|[^A-Za-z0-9_])XXX([^A-Za-z0-9_]|$)|\/\/[[:space:]]*WIP[: ]/)
          printf ":%d: unsanctioned comment marker: the only forms are TODO(#NNNN): / NOTE: / SAFETY:\n", NR
      }

      # 6. orphaned punctuation-only comment lines: residue an earlier
      # rewriting sweep left behind (`//.`, `///:`, `//!,` …).
      if ((is_doc || is_line) && line ~ /^\/\/[\/!]?[[:space:]]*[.;:,)]+[[:space:]]*$/)
        printf ":%d: comment line carries punctuation only: sweep residue; delete it\n", NR

      # 7. a backtick-quoted marker USED AS a marker: leading the doc
      # line, leading a bullet, or opening a parenthetical: reads as a
      # marker to a human and is invisible to checks 2-4. A mid-sentence
      # DESCRIPTION ("the emitter writes a `// NOTE:` …") stays legal.
      if (is_doc && (line ~ /^\/\/[\/!][[:space:]]*([-*][[:space:]]+)?`\/\/[[:space:]]?(NOTE|TODO|SAFETY)/ \
          || line ~ /\(`\/\/[[:space:]]?(NOTE|TODO|SAFETY)/))
        printf ":%d: doc line uses a backtick-quoted comment marker as a marker: invisible to the marker checks; write a real NOTE/TODO or plain prose\n", NR

      # 4 + 5. NOTE / plain-run budgets
      if (is_line) {
        if (line ~ /^\/\/[[:space:]]*NOTE/) {
          flush_note(); flush_run()
          note_start = NR; note_len = 1
        } else if (note_len > 0) {
          note_len++
        } else {
          if (run_len == 0) run_start = NR
          run_len++
        }
        next
      }
      # The NOTE budget inside doc comments (`/// NOTE:` / `//! NOTE:`):
      # a doc-relocated essay is the same essay. The paragraph ends at a
      # blank doc line, per rustdoc paragraph semantics.
      if (is_doc) {
        if (line ~ /^\/\/[\/!][[:space:]]*NOTE/) {
          flush_doc_note()
          doc_note_start = NR; doc_note_len = 1
        } else if (doc_note_len > 0) {
          # The paragraph ends at a blank doc line (rustdoc paragraph
          # semantics) or at the next list item (a NOTE inside a list does
          # not swallow its sibling items).
          if (line ~ /^\/\/[\/!][[:space:]]*$/ \
              || line ~ /^\/\/[\/!][[:space:]]+([-*][[:space:]]|[0-9]+\.[[:space:]])/ \
              || line ~ /^\/\/[\/!][[:space:]]*#/)
            flush_doc_note()
          else doc_note_len++
        }
        # 8. a lint-required section heading must carry a body: a bare
        # `# Errors` / `# Panics` satisfies missing_errors_doc /
        # missing_panics_doc while documenting nothing. Pending state
        # clears on the first non-blank, non-heading doc line and trips on
        # another heading or the end of the doc block.
        if (line ~ /^\/\/[\/!][[:space:]]*#[[:space:]]*(Errors|Panics)[[:space:]]*$/) {
          flush_sec()
          sec_start = NR
          sec_name = (line ~ /Errors/) ? "Errors" : "Panics"
        } else if (sec_start) {
          if (line ~ /^\/\/[\/!][[:space:]]*#/) flush_sec()
          else if (line !~ /^\/\/[\/!][[:space:]]*$/) sec_start = 0
        }
        flush_note(); flush_run()
        next
      }
      flush_note(); flush_run(); flush_doc_note(); flush_sec()
    }
    END { flush_note(); flush_run(); flush_doc_note(); flush_sec() }
  ' "$file"
}

# Prints the check 9 and 10 violations of one HASH file, one `:LINE: message`
# per line. The kind comes from the extension: `sh` (heredoc bodies skipped),
# `yml` (workflow and action YAML: heredocs inside a `run:` block skipped, the
# text of every `echo` or `printf` in a `run:` block read, with its shell
# escapes removed, and every `description:` scalar and trailing comment read)
# or `toml` (every `reason = "…"` string and trailing comment read as well).
check_hash() {
  local file="$1" kind
  case "$file" in
  *.sh) kind="sh" ;;
  *.yml | *.yaml) kind="yml" ;;
  *) kind="toml" ;;
  esac
  awk -v kind="$kind" -v sq="'" "$CITE_AWK"'
    BEGIN {
      hd_re = "<<-?[[:space:]]*[" sq "\"]?[A-Za-z_][A-Za-z0-9_]*[" sq "\"]?"
      reason_re = "reason[[:space:]]*=[[:space:]]*\"([^\"\\\\]|\\\\.)*\""
      echo_re = "(^|[;&|({[:space:]])(echo|printf)[[:space:]]"
      block_re = ":[[:space:]]*[|>][-+0-9]*[[:space:]]*(#.*)?$"
      heredoc = ""
      run_ind = -1
      desc_ind = -1
      block_ind = -1
    }
    # The trailing comment of a TOML or YAML line: the first `#` after a space
    # or a tab outside a quoted string, to the end of the line. A quote opens
    # a string only where a value starts, so the apostrophe of a word does not.
    function trailing_comment(s,   i, n, c, prev, dq, sqs) {
      n = length(s)
      prev = " "
      dq = 0
      sqs = 0
      for (i = 1; i <= n; i++) {
        c = substr(s, i, 1)
        if (dq) {
          if (c == "\\") i++
          else if (c == "\"") dq = 0
        } else if (sqs) {
          # Two quotes are the escaped quote of a YAML single-quoted scalar.
          if (c == sq && substr(s, i + 1, 1) == sq) i++
          else if (c == sq) sqs = 0
        } else if (c == "#" && (prev == " " || prev == "\t")) {
          return substr(s, i)
        } else if (index(" \t[{,:=", prev) > 0) {
          if (c == "\"") dq = 1
          else if (c == sq) sqs = 1
        }
        prev = c
      }
      return ""
    }
    NR == 1 && /^#!/ { next }
    {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      ind = length($0) - length(line)
      # The column of a key: a sequence entry `- key:` puts it past the dash.
      key_ind = ind
      if (match(line, /^-[[:space:]]+/)) key_ind = ind + RLENGTH
    }
    # A block scalar holds the lines indented deeper than its key (the YAML
    # 1.2.2 specification, block scalars), so the first line that is not ends it.
    # A multi-line plain or quoted `description:` scalar ends the same way.
    kind == "yml" && run_ind >= 0 && line != "" && ind <= run_ind {
      run_ind = -1
      heredoc = ""
    }
    kind == "yml" && desc_ind >= 0 && line != "" && ind <= desc_ind { desc_ind = -1 }
    kind == "yml" && block_ind >= 0 && line != "" && ind <= block_ind { block_ind = -1 }
    heredoc != "" {
      t = $0
      if (heredoc_tabs) sub(/^\t+/, "", t)
      if (kind == "yml") sub(/^[[:space:]]+/, "", t)
      if (t == heredoc) heredoc = ""
      next
    }
    {
      if (line ~ /^#/) {
        cite_check(line, "comment", 0, 0)
        next
      }
      if (desc_ind >= 0) {
        cite_check(line, "description", 0, 0)
        next
      }
      if (block_ind >= 0) next
      in_run = (kind == "sh")
      if (kind == "yml") {
        if (line ~ /^(-[[:space:]]+)?run:[[:space:]]*[|>]/) {
          cite_check(trailing_comment(line), "comment", 0, 0)
          run_ind = key_ind
          next
        }
        in_run = (run_ind >= 0)
        if (match(line, /^(-[[:space:]]+)?run:[[:space:]]*/)) {
          in_run = 1
          line = substr(line, RLENGTH + 1)
        }
        if (!in_run && match(line, /^(-[[:space:]]+)?description:[[:space:]]*/)) {
          cite_check(substr(line, RLENGTH + 1), "description", 0, 0)
          desc_ind = key_ind
          next
        }
        if (!in_run) {
          cite_check(trailing_comment(line), "comment", 0, 0)
          if (line ~ block_re) block_ind = key_ind
        }
        if (in_run && match(line, echo_re)) {
          t = substr(line, RSTART)
          gsub(/\\/, "", t)
          cite_check(t, "echoed text", 0, 0)
        }
      }
      if (kind == "toml") {
        cite_check(trailing_comment(line), "comment", 0, 0)
        t = $0
        while (match(t, reason_re)) {
          cite_check(substr(t, RSTART, RLENGTH), "lint reason", 0, 0)
          t = substr(t, RSTART + RLENGTH)
        }
      }
      if (in_run && match($0, hd_re)) {
        tok = substr($0, RSTART, RLENGTH)
        heredoc_tabs = (tok ~ /^<<-/)
        sub(/^<<-?[[:space:]]*/, "", tok)
        gsub(/[^A-Za-z0-9_]/, "", tok)
        heredoc = tok
      }
    }
  ' "$file"
}

# Prints the check 9 and 10 violations of one conformance table, one
# `:LINE: message` per line: its `#` comment lines, then the `reason` and
# `evidence` cells of every row after the header row that names the columns.
check_tsv() {
  local file="$1"
  awk -F'\t' "$CITE_AWK"'
    /^#/ { cite_check($0, "comment", 1, 0); next }
    !seen_header {
      seen_header = 1
      for (i = 1; i <= NF; i++) if ($i == "reason" || $i == "evidence") read_col[i] = $i
      next
    }
    {
      for (i = 1; i <= NF; i++) if (i in read_col) cite_check($i, read_col[i] " cell", 1, 0)
    }
  ' "$file"
}

# Runs the right pass over one file and prints its violations prefixed with
# the path. Returns 1 when the file has any.
check_file() {
  local f="$1" out
  case "$f" in
  *.rs)
    # The emitter writes its banner as the FIRST line of every generated
    # file, so the skip anchors there. Matching the marker anywhere would let
    # a hand-written file exempt itself by merely mentioning it in prose.
    head -n 1 "$f" 2>/dev/null | grep -q '^// @generated' && return 0
    out="$(check_rs "$f")"
    ;;
  *.tsv) out="$(check_tsv "$f")" ;;
  *) out="$(check_hash "$f")" ;;
  esac
  [[ -z "$out" ]] && return 0
  printf '%s\n' "$out" | sed "s|^|$f|"
  return 1
}

# The HASH files of the tree, outside the vendored corpora: shell scripts
# under scripts/, every Cargo.toml, clippy.toml, the YAML under .github
# (workflows, composite actions and the repository configuration), the TOML
# under docker/ and the conformance tables. A `case` pattern's `*` also
# matches `/`.
is_hash_file() {
  case "$1" in
  docs/specs/*) return 1 ;;
  scripts/*.sh | Cargo.toml | */Cargo.toml | clippy.toml) return 0 ;;
  .github/*.yml | .github/*.yaml | docker/*.toml | conformance/*.tsv) return 0 ;;
  *) return 1 ;;
  esac
}

# The pathspecs that list the files the guard reads. A pathspec `*` also
# matches `/`, and is_hash_file narrows the set.
pathspecs=('*.rs' 'scripts/*.sh' 'Cargo.toml' '*/Cargo.toml' 'clippy.toml'
  '.github/*.yml' '.github/*.yaml' 'docker/*.toml' 'conformance/*.tsv')

# Proves checks 9 and 10: every refused form fails with its message, and
# every near miss passes. Fixtures are written to a temporary tree.
self_test() {
  local dir fails=0
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand now: the path is fixed at this point
  trap "rm -rf '$dir'" EXIT

  [[ "$CITE_NAMES" == *rust-style* ]] || {
    echo "self-test: no rule file names were read from the rules tree" >&2
    return 1
  }

  # expect <file> <refused|accepted> <message fragment> <line>...
  expect() {
    local file="$dir/$1" verdict="$2" want="$3" out
    shift 3
    printf '%s\n' "$@" >"$file"
    out="$(check_file "$file" || true)"
    case "$verdict" in
    refused)
      if [[ "$out" != *"$want"* ]]; then
        echo "self-test: not refused ($want): $*" >&2
        fails=$((fails + 1))
      fi
      ;;
    accepted)
      if [[ -n "$out" ]]; then
        echo "self-test: refused a near miss: $* => $out" >&2
        fails=$((fails + 1))
      fi
      ;;
    *)
      echo "self-test: unknown verdict $verdict: $*" >&2
      fails=$((fails + 1))
      ;;
    esac
  }
  local internal="cites an internal markdown file"
  local marker="names a decision-register marker"

  expect a.rs refused "$internal" '//! The rewrite (docs/architecture.md §4).'
  # shellcheck disable=SC2016 # a literal Rust fixture: the backticks are Rust doc text
  expect b.rs refused "$internal" '/// The seam (`.claude/rules/rust-style.md`, typed carriers).'
  expect c.rs refused "$internal" '// Rule: reliability.md, the error chain.'
  expect d.rs refused "$internal" 'let x = 1; // see docs/architecture.md'
  expect e.rs refused "$internal" '#[expect(clippy::disallowed_types, reason = "seam 4 of rust-style.md: JSON in tests")]'
  expect f.rs refused "$internal" '/// The crate CLAUDE.md records it.'
  expect g.rs refused "$marker" '// NOTE: decision A17, a resolver outage fails the query.'
  expect h.rs refused "$marker" '/// The column travels hidden (A28).'
  expect i.rs refused "$marker" '//! Decisions A3 to A10 are covered here.'
  expect j.rs refused "$marker" '    reason = "decision A5: the default namespace"'
  expect k.rs refused "$marker" '/// §11.1, A43: the node was sent LIMIT 1.'

  expect l.rs accepted "" '// The frame opens with the byte 0xA1.'
  # shellcheck disable=SC2016 # a literal Rust fixture: the backticks are Rust doc text
  expect m.rs accepted "" '/// `SELECT x AS A1 FROM EHR e` names the alias.'
  expect n.rs accepted "" '/// ```text' '/// SELECT x AS A1 FROM EHR e' '/// ```'
  expect o.rs accepted "" '// Annex A §A.1 binds PIXm (§5.4.1, N33, CP-26).'
  expect p.rs accepted "" '// The pin matrix is docs/VERSIONS.md; HbA1c is a lab value.'
  expect q.rs accepted "" 'let url = "https://example.org/.claude/x";'
  expect r.rs accepted "" '#[expect(clippy::expect_used, reason = "the test seam: a fixture is valid JSON")]'

  expect a.sh refused "$internal" '# Policy: .claude/rules/issue-workflow.md.'
  expect b.sh refused "$internal" '# THE switch (docs/architecture.md section 11).'
  expect c.sh refused "$internal" '# The harness (#39, docs/architecture.md §13).'
  expect d.sh refused "$marker" '# Nothing is published (decision A35).'
  expect Cargo.toml refused "$internal" '# The reliability bar (reliability.md).'

  expect e.sh accepted "" '#   1. the docs/architecture.md pin table against the matrix.'
  expect f.sh accepted "" "cat <""<'EOF'" '# Provenance (.claude/rules/vendored-inputs.md)' 'EOF'
  expect g.sh accepted "" 'echo "rules: .claude/rules/comments.md" >&2'
  expect h.sh accepted "" '# The byte 0xA1 opens the frame; Annex A §A.1.'
  expect i.sh accepted "" '#!/usr/bin/env bash' '# A plain comment.'

  local steps=('jobs:' '  x:' '    runs-on: ubuntu-latest' '    steps:')
  local run='      - run: |'
  expect a.yml refused "$internal" '  # through env:, never spliced into run: (.claude/rules/ci-cd.md).'
  expect b.yml refused "$internal" '  # The guard (docs/architecture.md section 12): the matrix.'
  expect c.yml refused "$marker" '# Nothing is published (decision A35).'
  # shellcheck disable=SC2016 # a literal workflow fixture: the backticks are Markdown the echo prints
  expect d.yml refused "$internal" "${steps[@]}" "$run" \
    '          echo "a no-op (\`docs/architecture.md\` section 11)."'
  expect e.yml refused "$marker" "${steps[@]}" "$run" '          set -euo pipefail' \
    '          { echo "### Nothing"; echo "a no-op (decision A35)."; } >> out.md'
  expect f.yml refused "$internal" "${steps[@]}" '      - run: echo "rules: .claude/rules/ci-cd.md"'
  expect g.yml refused "$marker" "${steps[@]}" '      - name: Print' '        run: >-' \
    "          printf '%s\\n' 'the resolver fails closed (A17)'"
  expect action.yaml refused "$internal" 'runs:' '  using: composite' '  steps:' \
    '    # A publishing lane restores no cache (.claude/rules/ci-cd.md).'

  expect h.yml accepted "" "${steps[@]}" "$run" '          echo "the pin table of docs/architecture.md"'
  expect i.yml accepted "" "${steps[@]}" '      - name: Print' '        run: |' '          echo "ok"' \
    '      - name: echo the summary (A35) is a step name, not a run line'
  expect j.yml accepted "" "${steps[@]}" "$run" "          cat > out.md <<EOF" \
    '          # Provenance (.claude/rules/vendored-inputs.md)' '          EOF' '          echo "done"'
  expect k.yml accepted "" "${steps[@]}" '      - run: echo "the byte 0xA1; Annex A §A.1; HbA1c"'
  expect l.yml accepted "" '# Pinned in docs/VERSIONS.md (the GitHub Actions security hardening guide).'

  expect clippy.toml refused "$internal" '# Clippy configuration (.claude/rules/reliability.md).'
  expect clippy.toml refused "$internal" 'disallowed-types = [' \
    '  { path = "serde_json::Value", reason = "a Value lives at one of the four seams of rust-style.md" },' ']'
  expect clippy.toml refused "$marker" '  { path = "x::Y", reason = "decision A2: typed carriers only" },'
  expect clippy.toml accepted "" '# Clippy configuration (the Clippy book, Configuration).' \
    '  { path = "serde_json::Value", reason = "typed carriers only; HbA1c and 0xA1 are words" },'
  expect registry.toml refused "$internal" '# reached at its API root (docs/architecture.md section 8).'
  expect ferrofed.toml refused "$marker" '# a patient query fails closed with 424 (decision A17).'
  expect ferrofed.toml accepted "" '# a patient query fails closed with 424 (§11.4, N37).' \
    'document = "/etc/ferrofed/registry.toml"'

  # Any markdown file under docs/ outside docs/specs/, cited by a parenthetical
  # or a section; a file a script reads, a vendored page and a URL tail pass.
  expect s.rs refused "$internal" '// The lane (docs/ci-cd.md §The fuzz lane).'
  expect t.rs refused "$internal" '/// The checklist is docs/release.md section 2.'
  expect j.sh refused "$internal" '# The fuzz lane (docs/ci-cd.md).'
  # shellcheck disable=SC2016 # a literal shell fixture: the backticks are comment text
  expect k.sh refused "$internal" '# The images: `docs/VERSIONS.md` § Container images.'
  expect u.rs accepted "" '// The test reads docs/VERSIONS.md (docs/specs/its-rest/README.md).'
  expect l.sh accepted "" '# Reads each pin from docs/VERSIONS.md, the matrix.'
  expect m.sh accepted "" '# Config (https://github.com/rhysd/actionlint/blob/main/docs/config.md).'

  # A README.md of the tree, at the root or in a member, cited by a
  # parenthetical or a section; naming one as a file a script reads or writes,
  # a vendored README and a URL tail pass.
  expect v.rs refused "$internal" '// The seeds (fuzz/README.md).'
  expect w.rs refused "$internal" '/// The features are crates/ihe-iti/README.md section 2.'
  # shellcheck disable=SC2016 # a literal shell fixture: the backticks are comment text
  expect n.sh refused "$internal" '# The favicons (`assets/brand/README.md`).'
  expect o.sh refused "$internal" '# The quickstart (README.md).'
  expect u.yml refused "$internal" '# on every query the rewrite accepts (fuzz/README.md).'
  expect Cargo.toml refused "$internal" '# The binary (app/ferrofed-server/README.md § Running).'
  expect x.rs accepted "" '// The generator writes the target table of fuzz/README.md.'
  expect p.sh accepted "" '# Reads README.md and crates/ihe-iti/README.md, then the badges.'
  expect q.sh accepted "" '# The mermaid assets (website/book/vendor/mermaid/README.md).'
  expect r.sh accepted "" '# Upstream (https://github.com/rust-fuzz/cargo-fuzz/blob/main/README.md).'
  expect v.yml accepted "" '# Upstream https://example.org/README.md section 2 of the guide.'
  expect y.rs accepted "" '// The vendored overview (docs/specs/federation-ref/README.md).'

  # A YAML description scalar: inline, folded, and plain over several lines.
  expect m.yml refused "$internal" 'inputs:' '  x:' '    description: The pin (docs/VERSIONS.md).'
  expect n.yml refused "$internal" 'name: x' 'description: >-' '  Install the toolchain, so the pin' \
    '  lives in one file (docs/VERSIONS.md).' 'inputs: {}'
  expect o.yml refused "$marker" 'inputs:' '  - description: the resolver' '      fails closed (decision A17).'
  expect p.yml accepted "" 'inputs:' '  x:' '    description: mdBook version, pinned in docs/VERSIONS.md.' \
    '    default: "(docs/ci-cd.md)"'

  # A trailing comment after a YAML or TOML value, outside any quoted string.
  expect q.yml refused "$internal" "${steps[@]}" '      - uses: actions/checkout@0123 # pinned (docs/ci-cd.md)'
  expect r.yml refused "$marker" 'permissions: {} # least privilege (decision A35)'
  expect s.yml refused "$internal" "${steps[@]}" '      - run: | # see .claude/rules/ci-cd.md' '          echo ok'
  expect t.yml accepted "" "${steps[@]}" '      - uses: actions/checkout@0123 # v7.0.1' \
    "        name: 'it''s # (docs/ci-cd.md)'" '        url: https://example.org/x#(docs/ci-cd.md)' \
    '      - uses: actions/github-script@0123 # v8.0.0' '        with:' '          script: |' \
    '            const x = 1 # (docs/ci-cd.md)' '      - run: echo done'
  expect Cargo.toml refused "$internal" 'serde = "1" # see (docs/VERSIONS.md)'
  expect ferrofed.toml refused "$marker" 'timeout_ms = 30000 # fails closed (decision A17)'
  expect Cargo.toml accepted "" 'url = "https://x.org/#(docs/ci-cd.md)"' "name = 'it # (docs/release.md)'" \
    'level = "deny" # the Clippy book, Configuration'

  # A conformance table: its comment lines and its reason column only.
  local cols=$'cp\tactor\tstatus\treason'
  expect a.tsv refused "$internal" '# One row per point (docs/architecture.md section 12).' "$cols"
  expect b.tsv refused "$marker" '# Only the owner defers a point (decision A37).' "$cols"
  expect c.tsv refused "$marker" '# A comment' "$cols" \
    $'CP-18\tNode\tnode-profile\tscored in the harness (section 16.2, decision A44)'
  expect d.tsv refused "$internal" "$cols" $'CP-20\tOperator\toperator\tsee .claude/rules/testing.md'
  expect e.tsv refused "$internal" "$cols" $'CP-27\tNode\tnode-profile\tthe harness of docs/architecture.md'
  expect f.tsv accepted "" '# The points of section 17 (section 16.4, #41); CP-33a is a point.' "$cols" \
    $'CP-18\tNode\tnode-profile\tscored against the nodes (section 16.2); no specification governs this: our own design' \
    $'CP-33a\tOperator\toperator\tAnnex A §A.1; HbA1c; the byte 0xA1' $'CP-1\tGateway\tcovered\t-'
  expect g.tsv accepted "" $'track\ttitle\tstatus' $'8\tthe A35 track\tdeferred'
  local ob=$'id\tstatus\tevidence\tclause'
  expect h.tsv refused "$marker" "$ob" $'n1.1\tdeferred\tdecision A31: no cursor\tA MUST'
  expect i.tsv refused "$internal" "$ob" $'n1.2\tplanned\t#80 (docs/architecture.md section 13)\tA MUST'
  expect j.tsv accepted "" "$ob" $'n1.3\tdeferred\towner decision on #16\tthe A35 clause'

  if [[ "$fails" -ne 0 ]]; then
    echo "comment-style self-test: $fails case(s) failed." >&2
    return 1
  fi
  echo "ok: comment-style self-test (internal citations and decision markers refused in .rs, .sh, Cargo.toml, clippy.toml, workflow and action YAML, the docker TOML and the conformance tables; near misses accepted)"
}

mode="${1:---all}"
files=()
case "$mode" in
--self-test)
  self_test
  exit $?
  ;;
--all)
  # `git ls-files` reads the index, so a file deleted from the worktree but
  # not yet staged would still be listed: skip it rather than letting awk
  # fail on a missing path.
  while IFS= read -r f; do
    [[ -f "$f" ]] || continue
    case "$f" in
    *.rs) files+=("$f") ;;
    *) is_hash_file "$f" && files+=("$f") ;;
    esac
  done < <(git ls-files -- "${pathspecs[@]}")
  ;;
--diff)
  base="${2:?usage: --diff <base> [head]}"
  head="${3:-HEAD}"
  while IFS= read -r f; do
    [[ -f "$f" ]] || continue
    case "$f" in
    *.rs) files+=("$f") ;;
    *) is_hash_file "$f" && files+=("$f") ;;
    esac
  done < <(git diff --name-only "$base" "$head" -- "${pathspecs[@]}")
  ;;
--files)
  shift
  for f in "$@"; do
    [[ -f "$f" ]] || continue
    # The hook passes an absolute path, and the patterns of is_hash_file are
    # relative to the repository root this script runs from.
    f="${f#"$PWD"/}"
    case "$f" in
    *.rs) files+=("$f") ;;
    *) is_hash_file "$f" && files+=("$f") ;;
    esac
  done
  ;;
*)
  echo "usage: $0 [--all | --diff <base> [head] | --files <file>... | --self-test]" >&2
  exit 2
  ;;
esac

[[ "${#files[@]}" -eq 0 ]] && {
  echo "comment-style: no files to check: OK."
  exit 0
}

fail=0
for f in "${files[@]}"; do
  check_file "$f" || fail=1
done

if [[ "$fail" -ne 0 ]]; then
  echo "comment-style: violations found (rules: .claude/rules/comments.md)." >&2
  exit 1
fi
echo "comment-style: OK (${#files[@]} files)."
