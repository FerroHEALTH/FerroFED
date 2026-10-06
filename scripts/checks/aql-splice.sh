#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The AQL splice guard: no Rust source outside the tests builds AQL text by
# string formatting or concatenation. AQL is parsed and printed through
# openehr-query (parser::parse_str, bind::bind, printer::to_aql), so a value
# is bound into the syntax tree and printed as an escaped literal, never
# spliced into the text (AQL §Parameters; ITS-REST Query API
# query_parameters). A clippy disallowed-macros entry cannot tell an AQL
# format! from any other, so this guard reads the literals instead.
#
# A Rust file fails when a string literal that reads as AQL stands in a
# splice: an argument of format!, format_args!, concat!, write!, writeln!,
# push_str, insert_str, replace or replacen, or an operand of + or +=. A
# literal reads as AQL when it holds SELECT followed by FROM, FROM EHR,
# CONTAINS, or an ENDPOINT or ORGANISATION directive, each an uppercase word,
# or an AQL path compared with a quoted placeholder ('{…}'), the form a
# spliced value takes in a WHERE fragment. The literals of one call are read
# together, so a query split across arguments is still found. A constant AQL
# literal that no call splices passes.
#
# Exempt: files under a tests/ or fixtures/ directory, and every item behind
# #[cfg(test)]. Comments, doc comments included, are never read. No
# specification governs this guard: our own design.
#
# Usage:
#   scripts/checks/aql-splice.sh              check every tracked .rs file
#                                             under app/, crates/ and tools/
#   scripts/checks/aql-splice.sh --self-test  prove the refusals and passes
# Exit 1 naming each splice; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# The scanner: a small Rust lexer over comments, strings, raw strings, char
# literals and brackets, enough to tell which call encloses each literal and
# which literal stands beside a +. It prints PATH:LINE for every splice.
SCANNER=''
IFS= read -r -d '' SCANNER << 'AWK' || true
function word_char(c) { return c ~ /[A-Za-z0-9_]/ }
function is_splice(name) {
  return name == "format!" || name == "format_args!" || name == "concat!" ||
    name == "write!" || name == "writeln!" || name == "push_str" ||
    name == "insert_str" || name == "replace" || name == "replacen"
}
function reads_as_aql(text) {
  gsub(/[\t\n]/, " ", text)
  text = " " text " "
  if (text ~ /[^A-Za-z0-9_]SELECT[^A-Za-z0-9_](.*[^A-Za-z0-9_])?FROM[^A-Za-z0-9_]/) return 1
  if (text ~ /[^A-Za-z0-9_]FROM[ ]+EHR[^A-Za-z0-9_]/) return 1
  if (text ~ /[^A-Za-z0-9_]CONTAINS[^A-Za-z0-9_]/) return 1
  if (text ~ /[^A-Za-z0-9_](ENDPOINT|ORGANISATION)[ ]*[A-Za-z0-9_]*[ ]*\[/) return 1
  if (text ~ /[A-Za-z0-9_]\/[A-Za-z0-9_\/]+[ ]*(=|!=|<|>|<=|>=)[ ]*\047\{/) return 1
  return 0
}
function report(line) { print FILENAME ":" line; found = 1 }
# A literal just closed: read it beside a +, add it to the innermost splice
# call around it, and hold it for a + that may follow.
function close_literal(   k) {
  state = "code"
  if (test_depth > 0) return
  if (plus_at[depth] && reads_as_aql(literal)) report(literal_line)
  for (k = depth; k >= 1; k--) {
    if (splice[k]) {
      text[k] = text[k] " " literal
      if (!line_of[k]) line_of[k] = literal_line
      break
    }
  }
  held = literal; held_line = literal_line; held_depth = depth
}
function open_literal() { state = "string"; literal = ""; literal_line = FNR; word = "" }
FNR == 1 {
  depth = 0; test_depth = 0; pending_test = 0; state = "code"; word = ""
  held = ""; held_depth = -1; plus_at[0] = 0
}
{
  line = $0
  if (state == "code" && line ~ /^[ \t]*#\[cfg\(test\)\]/) pending_test = 1
  n = length(line)
  i = 1
  while (i <= n) {
    c = substr(line, i, 1)
    if (state == "block") {
      if (c == "*" && substr(line, i + 1, 1) == "/") { state = "code"; i += 2; continue }
      i++; continue
    }
    if (state == "string") {
      if (c == "\\") { literal = literal substr(line, i, 2); i += 2; continue }
      if (c == "\"") { close_literal(); i++; continue }
      literal = literal c; i++; continue
    }
    if (state == "raw") {
      if (c == "\"" && substr(line, i + 1, hashes) == closing) { close_literal(); i += 1 + hashes; continue }
      literal = literal c; i++; continue
    }
    if (c == "/" && substr(line, i + 1, 1) == "/") break
    if (c == "/" && substr(line, i + 1, 1) == "*") { state = "block"; i += 2; continue }
    if (c == "\"") { open_literal(); i++; continue }
    if (c == "r" && !word_char(substr(line, i - 1, 1)) && substr(line, i + 1) ~ /^#*"/) {
      rest = substr(line, i + 1)
      match(rest, /^#*/)
      hashes = RLENGTH
      closing = substr(rest, 1, hashes)
      open_literal(); state = "raw"
      i += 2 + hashes; continue
    }
    if (c == "\047") {
      word = ""
      if (substr(line, i + 1, 1) == "\\") {
        rest = substr(line, i + 2)
        j = index(rest, "\047")
        i += 2 + (j > 0 ? j : length(rest)); continue
      }
      if (substr(line, i + 2, 1) == "\047") { i += 3; continue }
      i++; continue
    }
    if (word_char(c)) {
      if (!word_char(substr(line, i - 1, 1))) word = ""
      word = word c; i++; continue
    }
    if (c == " " || c == "\t") { i++; continue }
    if (c == "!" && word != "") { word = word "!"; i++; continue }
    if (c == "+") {
      if (test_depth == 0 && held != "" && held_depth == depth && reads_as_aql(held)) report(held_line)
      plus_at[depth] = 1
    } else if (c == "(" || c == "[" || c == "{") {
      depth++
      splice[depth] = is_splice(word)
      text[depth] = ""; line_of[depth] = 0; plus_at[depth] = 0
      if (c == "{" && pending_test && test_depth == 0) { test_depth = depth; pending_test = 0 }
    } else if (c == ")" || c == "]" || c == "}") {
      if (depth >= 1) {
        if (splice[depth] && test_depth == 0 && reads_as_aql(text[depth])) report(line_of[depth])
        if (depth == test_depth) test_depth = 0
        if (held_depth >= depth) { held = ""; held_depth = -1 }
        depth--
      }
    } else if (c == ";" || c == ",") {
      if (c == ";") pending_test = 0
      plus_at[depth] = 0
      if (held_depth == depth) { held = ""; held_depth = -1 }
    }
    word = ""
    i++
  }
}
END { exit found ? 1 : 0 }
AWK
readonly SCANNER

# scan FILE...: prints PATH:LINE for each splice; exits 1 when it found one.
# The C locale reads bytes, so a multi-byte character never stops the lexer.
scan() {
  LC_ALL=C awk "$SCANNER" "$@"
}

# exempt PATH: whether PATH is a test or fixture file the guard leaves alone.
exempt() {
  case "$1" in
    */tests/* | tests/* | */fixtures/*) return 0 ;;
    *) return 1 ;;
  esac
}

check_tree() {
  local fail=0 count=0 path hits hit
  local -a files=()
  while IFS= read -r path; do
    if exempt "$path"; then
      continue
    fi
    files+=("$path")
    count=$((count + 1))
  done < <(git ls-files -- 'app/*.rs' 'crates/*.rs' 'tools/*.rs')
  if [[ "$count" -eq 0 ]]; then
    echo "::error::aql-splice: no Rust file found under app/, crates/ or tools/." >&2
    return 1
  fi
  hits="$(scan ${files[@]+"${files[@]}"})" || fail=1
  if [[ "$fail" -ne 0 ]]; then
    while IFS= read -r hit; do
      [[ -z "$hit" ]] && continue
      echo "::error file=${hit%%:*},line=${hit##*:}::$hit builds AQL text by formatting or concatenation; parse a fixed template with openehr_query::parser::parse_str, bind each value with openehr_query::bind::bind or send it as an ITS-REST query parameter, and print with openehr_query::printer::to_aql." >&2
    done <<< "$hits"
    return 1
  fi
  echo "aql-splice: $count Rust files, no AQL built by splicing."
}

# The self-test drives scan against fixtures in a temporary directory: each
# refused form fails, each near miss passes.
self_test() {
  local work
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -r '$work'" EXIT
  local failed=0
  # expect WANT NAME BODY: scan over a file holding BODY exits WANT.
  expect() {
    local want="$1" name="$2" file="$work/$2.rs" status=0
    printf '%s\n' "$3" > "$file"
    scan "$file" > /dev/null || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "aql-splice: self-test failed: $name exited $status, wanted $want." >&2
      failed=1
    fi
  }
  local q="'"
  expect 1 format "fn q(id: &str) -> String {
    format!(\"SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_id/value = ${q}{id}${q}\")
}"
  expect 1 format_multiline "fn q(id: &str) -> String {
    format!(
        \"SELECT c/uid/value FROM EHR e[ehr_id/value=${q}{}${q}] \\
         CONTAINS COMPOSITION c\",
        id
    )
}"
  expect 1 format_split 'fn q(w: &str) -> String {
    format!("{}{}", "SELECT c ", "FROM COMPOSITION c WHERE {w}")
}'
  expect 1 concat 'const Q: &str = concat!("SELECT c FROM ", "EHR e");'
  expect 1 contains_only 'fn q(f: &str) -> String { format!("{f} CONTAINS COMPOSITION c") }'
  expect 1 plus_after 'fn q(id: &str) -> String { "SELECT c FROM EHR e WHERE x = ".to_owned() + id }'
  expect 1 plus_before 'fn q(head: String) -> String { head + " CONTAINS COMPOSITION c" }'
  expect 1 plus_assign 'fn q(mut head: String) -> String { head += " CONTAINS COMPOSITION c"; head }'
  expect 1 push_str 'fn q(s: &mut String) { s.push_str(" CONTAINS OBSERVATION o"); }'
  expect 1 replace 'fn q(id: &str) -> String { T.replace("FROM EHR e", id) }'
  expect 1 write "fn q(f: &mut F, id: &str) { write!(f, \"SELECT x FROM EHR e[ehr_id/value=${q}{id}${q}]\"); }"
  expect 1 predicate "fn p(v: &str) -> String {
    format!(
        \"e/ehr_status/subject/external_ref/id/value = ${q}{}${q} \\
         AND e/ehr_status/subject/external_ref/namespace = ${q}{}${q}\",
        v, N
    )
}"
  expect 1 directive 'fn d(e: &str) -> String { format!("ENDPOINT [\"{e}\"]") }'
  expect 1 directive_named 'fn d(o: &str) -> String { format!("ORGANISATION o [\"{o}\"]") }'
  expect 0 masked "fn m() -> String { format!(\"x = ${q}{MASK}${q}\") }"
  expect 1 raw 'fn q(id: &str) -> String { format!(r#"SELECT c FROM EHR e WHERE x = "{id}""#) }'
  expect 1 after_tests '#[cfg(test)]
mod tests {
    fn t() -> String { format!("SELECT c FROM EHR e") }
}
fn q(id: &str) -> String { format!("SELECT c FROM EHR e WHERE x = {id}") }'
  expect 1 after_test_use '#[cfg(test)]
use std::fmt;
fn q(id: &str) -> String { format!("SELECT c FROM EHR e WHERE x = {id}") }'
  # shellcheck disable=SC2016 # Rust source holding an AQL parameter, never expanded.
  expect 0 constant 'const Q: &str = "SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_id/value = $ehr_id";'
  expect 0 parsed 'fn q() -> Result<SelectQuery, ParseError> { parse_str("SELECT c FROM EHR e CONTAINS COMPOSITION c") }'
  expect 0 message 'fn m(n: usize) -> String { format!("the query of the EHR scoped by e/ehr_id/value answered {n} row(s)") }'
  expect 0 lowercase 'fn m(x: &str) -> String { format!("select one from {x}, which contains it") }'
  expect 0 sum 'fn m(a: usize) -> usize { let q = "SELECT c FROM EHR e"; a + 1 }'
  expect 0 comment 'fn q() {
    // format!("SELECT c FROM EHR e CONTAINS COMPOSITION c")
    /* format!("SELECT c FROM EHR e") */
}
/// format!("SELECT c FROM EHR e")
fn r() {}'
  expect 0 test_module "#[cfg(test)]
mod tests {
    fn t(id: &str) -> String { format!(\"SELECT c FROM EHR e WHERE x = ${q}{id}${q}\") }
}"
  expect 0 test_fn "fn q() {}
#[cfg(test)]
fn t(id: &str) -> String { format!(\"SELECT c FROM EHR e[ehr_id/value=${q}{id}${q}]\") }"
  expect 0 char_brace "fn q(s: &mut String) { s.push(${q}{${q}); }
fn r(id: &str) -> String { format!(\"{id}\") }"
  expect 0 identifier 'fn q() -> String { format!("{}", CONTAINS_LIMIT) }'
  local status=0
  exempt app/ferrofed-server/tests/it/node_profile.rs || status=1
  exempt crates/ihe-iti/tests/fixtures/x.rs || status=1
  if exempt app/ferrofed-server/src/conformance/fixture.rs; then status=1; fi
  if [[ "$status" -ne 0 ]]; then
    echo "aql-splice: self-test failed: the exemption reads the wrong paths." >&2
    failed=1
  fi
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "aql-splice: self-test OK."
}

case "${1:-}" in
  --self-test) self_test ;;
  "") check_tree ;;
  *)
    echo "aql-splice: unknown argument $1" >&2
    exit 2
    ;;
esac
