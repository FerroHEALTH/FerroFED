#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/report.sh: the per-track and per-point report of a test
# run, the Gazelle-style logging of section 16.4 ("each executed test records
# the conformance points it exercised"), keyed by the section 16.3 tracks and
# the section 17 points (#92).
#
#   scripts/conformance/report.sh [--offline JUNIT]... [--gated JUNIT]... [--node-profile DIR] [--out DIR]
#   scripts/conformance/report.sh --run [--out DIR]
#   scripts/conformance/report.sh --self-test
#
# A JUnit file is what `cargo nextest run --profile ci` writes to
# target/nextest/ci/junit.xml. A test under an e2e module returns early unless
# FERROFED_E2E is 1, so it counts only from a --gated file, a run with the
# gate set; every other test counts from either. --run is that gated run of
# the whole workspace, which needs Docker, reported as --gated with the
# node-profile findings it wrote.
#
# Every `// conformance:` marker above a test names the points and tracks it
# scores. A covered point or track passes when every test carrying its
# marker ran and passed, fails when one failed, and is not-run when one did
# not run and none failed. A deferred row is reported deferred and a planned
# row open, each with its issue and reason from conformance/matrix.tsv and
# conformance/tracks.tsv.
#
# A Node or Operator point is scored against that actor, never the gateway
# (section 16.2), so it is a class of its own and never a gateway pass. The
# node profile checks under tools/ write one findings file per check and
# harness CDR product into the --node-profile DIR
# (target/conformance/node-profile by default): rows of product, point,
# check, verdict (pass, fail, not-observable) and evidence. Such a row reads
# node-fail or operator-fail when a finding of any product on it fails,
# node-pass or operator-pass when one passes and none fails, and
# node-not-observable or operator-not-observable when none decides it;
# check-failed when a marked check itself failed, not-run when one did not
# run, not-reported when the checks ran and wrote no finding, and unchecked
# (a Node row) or not-applicable (an Operator row) when no check is marked.
# The findings section shows each product's verdict per check side by side,
# then each product's findings with their evidence. No specification governs
# the report's form, or which products the harness runs: our own design.
#
# It writes DIR/report.tsv, DIR/report.md and DIR/node-profile.tsv (DIR
# defaults to target/conformance) and exits 1 when a covered row failed or
# did not run; a Node or Operator row never sets the exit status.

set -euo pipefail

usage() {
  sed -n '9,11p' "${BASH_SOURCE[0]}" | sed 's/^# //' >&2
  exit 2
}

# package_of CRATE_DIR: the package name in CRATE_DIR/Cargo.toml.
package_of() {
  awk -F'"' '/^\[package\]/ { inside = 1; next } /^\[/ { inside = 0 } inside && /^name[[:space:]]*=/ { print $2; exit }' "$1/Cargo.toml"
}

# marks ROOT: one "binary <TAB> module prefix <TAB> function <TAB> token" row
# per point or track a marker in a tracked Rust file of ROOT names.
marks() {
  local root="$1" path crate rest binary module package
  while IFS= read -r path; do
    grep -q '// conformance:' "$root/$path" || continue
    case "$path" in
      */tests/*)
        crate="${path%%/tests/*}"
        rest="${path#"$crate"/tests/}"
        if [[ "$rest" == */* ]]; then
          binary="${rest%%/*}"
          module="${rest#*/}"
        else
          binary="${rest%.rs}"
          module=""
        fi
        ;;
      */src/*)
        crate="${path%%/src/*}"
        binary=""
        module="${path#"$crate"/src/}"
        ;;
      *) continue ;;
    esac
    module="${module%.rs}"
    module="${module%/mod}"
    case "$module" in main | lib | mod) module="" ;; esac
    module="${module//\//::}"
    package="$(package_of "$root/$crate")"
    [[ -n "$binary" ]] && binary="$package::$binary" || binary="$package"
    awk -v binary="$binary" -v module="$module" '
      /^[[:space:]]*\/\/[[:space:]]*conformance:/ {
        tokens = $0
        sub(/^[[:space:]]*\/\/[[:space:]]*conformance:[[:space:]]*/, "", tokens)
        pending = 1
        next
      }
      pending && match($0, /fn [A-Za-z0-9_]+/) {
        name = substr($0, RSTART + 3, RLENGTH - 3)
        n = split(tokens, token, /[[:space:]]+/)
        for (i = 1; i <= n; i++) if (token[i] != "") print binary "\t" module "\t" name "\t" token[i]
        pending = 0
      }
    ' "$root/$path"
  done < <(git -C "$root" ls-files --cached --others --exclude-standard -- '*.rs' ':!docs/specs/**')
}

# outcomes SOURCE JUNIT: one "binary <TAB> test <TAB> pass|fail <TAB> SOURCE"
# row per test case in the nextest JUnit file JUNIT.
outcomes() {
  awk -v source="$1" '
    function attribute(line, key,    found) {
      if (match(line, key "=\"[^\"]*\"")) {
        found = substr(line, RSTART + length(key) + 2, RLENGTH - length(key) - 3)
        return found
      }
      return ""
    }
    function emit() { print binary "\t" name "\t" outcome "\t" source; open = 0 }
    /<testcase / {
      name = attribute($0, "name"); binary = attribute($0, "classname")
      outcome = "pass"; open = 1
      if ($0 ~ /<testcase [^>]*\/>/) { emit(); next }
    }
    open && /<(failure|error)[ >\/]/ { outcome = "fail" }
    open && /<skipped/ { outcome = "skip" }
    open && /<\/testcase>/ { emit() }
  ' "$2"
}

# results MARKS OUTCOMES: one "token <TAB> test <TAB> pass|fail|not-run" row
# per marked test. A test under e2e:: counts only from a gated source.
results() {
  awk -F'\t' '
    FILENAME == ARGV[1] {
      if ($3 == "skip") next
      if ($2 ~ /^e2e::/ && $4 != "gated") next
      n = split($2, part, "::")
      key = $1 "\t" part[n]
      seen[key] = seen[key] "\034" $2 "\035" $3
      next
    }
    {
      binary = $1; module = $2; fn = $3; token = $4
      split(seen[binary "\t" fn], candidate, "\034")
      result = "not-run"; test = binary " " (module == "" ? "" : module "::") fn
      for (i in candidate) {
        if (candidate[i] == "") continue
        split(candidate[i], pair, "\035")
        if (module != "" && index(pair[1], module "::") != 1) continue
        test = binary " " pair[1]
        if (pair[2] == "fail") result = "fail"
        else if (result != "fail") result = "pass"
      }
      print token "\t" test "\t" result
    }
  ' "$2" "$1" | sort -u
}

# table FILE KIND KEY_PREFIX RESULTS FINDINGS: one report row per data row of
# the hand-kept table FILE, its result joined from RESULTS and, for a Node or
# Operator point, from the node-profile FINDINGS. A track row is of kind
# track; a point row's kind is its class: gateway, node or operator.
table() {
  local file="$1" kind="$2" prefix="$3" results="$4" findings="$5"
  awk -F'\t' -v kind="$kind" -v prefix="$prefix" '
    FILENAME == ARGV[1] {
      if ($3 == "pass") passed[$1]++
      else if ($3 == "fail") failed[$1]++
      else unrun[$1]++
      next
    }
    FILENAME == ARGV[2] {
      if ($4 == "fail") nfail[$2]++
      else if ($4 == "pass") npass[$2]++
      else nnone[$2]++
      next
    }
    /^#/ { next }
    !header { header = 1; next }
    {
      id = $1; key = prefix id; status = $5
      p = passed[key] + 0; f = failed[key] + 0; u = unrun[key] + 0
      class = (status == "node-profile" ? "node" : (status == "operator" ? "operator" : "gateway"))
      if (status == "covered") result = (f > 0 ? "fail" : (u > 0 || p == 0 ? "not-run" : "pass"))
      else if (status == "deferred") result = "deferred"
      else if (class != "gateway") {
        if (f > 0) result = "check-failed"
        else if (nfail[id] > 0) result = class "-fail"
        else if (npass[id] > 0) result = class "-pass"
        else if (nnone[id] > 0) result = class "-not-observable"
        else if (u > 0) result = "not-run"
        else if (p > 0) result = "not-reported"
        else result = (class == "node" ? "unchecked" : "not-applicable")
      }
      else result = "open"
      if (kind == "track") { title = $2; actor = "-"; out = kind } else { title = "-"; actor = $2; out = class }
      print out "\t" id "\t" title "\t" actor "\t" status "\t" result "\t" p "\t" f "\t" u "\t" $6 "\t" $7
    }
  ' "$results" "$findings" "$file"
}

# render TSV SOURCES: the markdown report of the rows in TSV.
render() {
  local tsv="$1" sources="$2" commit
  commit="$(git rev-parse --short HEAD 2> /dev/null || echo unknown)"
  printf '# Conformance report\n\n'
  printf 'The section 16.3 tracks and the section 17 conformance points of the Federation Tier with AQL, scored from the tests that carry each marker (section 16.4). Commit %s; %s.\n\n' "$commit" "$sources"
  printf 'A deferred track or point is not run, by a recorded decision. An open point has no test yet. A Node or Operator point is scored against that actor and never the gateway (section 16.2): its result comes from the node profile checks run against the harness CDR products, and is never a gateway pass.\n\n'
  awk -F'\t' '
    function cell(text) { gsub(/\|/, "\\|", text); return text }
    function heading(text) { print "\n## " text "\n\n| Point | Actor | Result | Tests passed, failed, not run | Issue | Reason |\n|---|---|---|---|---|---|" }
    $1 == "track" && !tracks { print "## Tracks\n\n| Track | Title | Result | Tests passed, failed, not run | Issue | Reason |\n|---|---|---|---|---|---|"; tracks = 1 }
    $1 == "gateway" && !gateway { heading("Gateway points"); gateway = 1 }
    $1 == "node" && !node { heading("Node points"); node = 1 }
    $1 == "operator" && !operator { heading("Operator points"); operator = 1 }
    {
      reason = ($6 == "pass" ? "-" : cell($11))
      name = ($1 == "track" ? $2 " | " cell($3) : $2 " | " $4)
      print "| " name " | " $6 " | " $7 ", " $8 ", " $9 " | " $10 " | " reason " |"
    }
  ' < <(awk -F'\t' '$1 == "track"' "$tsv"; awk -F'\t' '$1 == "gateway"' "$tsv"; awk -F'\t' '$1 == "node"' "$tsv"; awk -F'\t' '$1 == "operator"' "$tsv")
}

# render_findings FINDINGS: the markdown tables of the node-profile findings:
# one row per point and check with a verdict column per product, products in
# name order and checks in the order first recorded, then one table per
# product with the evidence. Where a product recorded one check on one point
# twice, the summary shows the worse verdict (fail, then not-observable).
render_findings() {
  local findings="$1"
  printf '\n## Node profile findings\n\n'
  if [[ ! -s "$findings" ]]; then
    printf 'No node profile finding was recorded in this run.\n'
    return
  fi
  printf 'What the node profile checks observed at each harness CDR product, per point they assist (section 16.2). A Node point above reads the worst verdict any product earned on it; this table shows each product apart, and a dash marks a check that product recorded no finding for.\n\n'
  awk -F'\t' '
    function cell(text) { gsub(/\|/, "\\|", text); return text }
    function rank(verdict) { return verdict == "fail" ? 3 : (verdict == "not-observable" ? 2 : (verdict == "" ? 0 : 1)) }
    !($1 in seen) { seen[$1] = 1; products[++np] = $1 }
    {
      key = $2 "\t" $3
      if (!(key in known)) { known[key] = 1; keys[++nk] = key }
      if (rank($4) > rank(verdict[key, $1])) verdict[key, $1] = $4
      rows[$1] = rows[$1] "| " $2 " | " cell($3) " | " $4 " | " cell($5) " |\n"
    }
    END {
      for (i = 2; i <= np; i++) {
        name = products[i]
        for (j = i - 1; j >= 1 && products[j] > name; j--) products[j + 1] = products[j]
        products[j + 1] = name
      }
      header = "| Point | Check |"; rule = "|---|---|"
      for (i = 1; i <= np; i++) { header = header " " cell(products[i]) " |"; rule = rule "---|" }
      print header; print rule
      for (k = 1; k <= nk; k++) {
        split(keys[k], part, "\t")
        line = "| " part[1] " | " cell(part[2]) " |"
        for (i = 1; i <= np; i++) {
          v = verdict[keys[k], products[i]]
          line = line " " (v == "" ? "-" : v) " |"
        }
        print line
      }
      for (i = 1; i <= np; i++) {
        printf "\n### %s\n\n| Point | Check | Verdict | Evidence |\n|---|---|---|---|\n%s", cell(products[i]), rows[products[i]]
      }
    }
  ' "$findings"
}

# report OUT SOURCES RESULTS ROOT FINDINGS: writes OUT/report.tsv, OUT/report.md
# and OUT/node-profile.tsv, and returns 1 when a covered row failed or did
# not run.
report() {
  local out="$1" sources="$2" results="$3" root="$4" findings="$5"
  mkdir -p "$out"
  {
    printf 'product\tpoint\tcheck\tverdict\tevidence\n'
    cat "$findings"
  } > "$out/node-profile.tsv"
  {
    printf 'kind\tid\ttitle\tactor\tstatus\tresult\tpassed\tfailed\tnot_run\tissue\treason\n'
    table "$root/conformance/tracks.tsv" track track- "$results" "$findings"
    table "$root/conformance/matrix.tsv" cp "" "$results" "$findings"
  } > "$out/report.tsv"
  {
    render "$out/report.tsv" "$sources"
    render_findings "$findings"
    if awk -F'\t' '$3 != "pass" { found = 1 } END { exit !found }' "$results"; then
      printf '\n## Marked tests that did not pass\n\n'
      awk -F'\t' '$3 != "pass" { print "- " $3 ": `" $2 "` (" $1 ")" }' "$results" | sort -u
    fi
  } > "$out/report.md"
  local bad
  bad="$(awk -F'\t' 'NR > 1 && $5 == "covered" && $6 != "pass" { print $1 " " $2 " " $6 }' "$out/report.tsv")"
  awk -F'\t' 'NR > 1 { count[$6]++ } END { for (r in count) printf "%s %d; ", r, count[r]; print "" }' "$out/report.tsv" | sed 's/^/conformance-report: /'
  if [[ -n "$bad" ]]; then
    echo "conformance-report: covered rows that did not pass:" >&2
    local line
    while IFS= read -r line; do
      echo "  $line" >&2
    done <<< "$bad"
    return 1
  fi
}

# generate ROOT OUT OFFLINE GATED NODES: the report of the JUnit files listed
# in the newline-separated OFFLINE and GATED, and of the node-profile findings
# files in the directory NODES when it exists, over the tree ROOT.
generate() {
  local root="$1" out="$2" offline="$3" gated="$4" nodes="$5" work file sources=""
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" RETURN
  : > "$work/outcomes"
  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    outcomes offline "$file" >> "$work/outcomes"
    sources="${sources}offline $(basename "$file") "
  done <<< "$offline"
  while IFS= read -r file; do
    [[ -n "$file" ]] || continue
    outcomes gated "$file" >> "$work/outcomes"
    sources="${sources}gated $(basename "$file") "
  done <<< "$gated"
  : > "$work/findings"
  if [[ -n "$nodes" && -d "$nodes" ]]; then
    while IFS= read -r -d '' file; do
      awk -F'\t' 'FNR > 1 && NF == 5' "$file" >> "$work/findings"
    done < <(find "$nodes" -maxdepth 1 -name '*.tsv' -print0 | sort -z)
    sources="${sources}node-profile $(basename "$nodes") "
  fi
  marks "$root" > "$work/marks"
  results "$work/marks" "$work/outcomes" > "$work/results"
  report "$out" "JUnit: ${sources% }" "$work/results" "$root" "$work/findings"
}

# self_test: a fixture tree with one crate, a matrix and a track table, and
# JUnit files that pass, fail and omit its marked tests.
self_test() {
  local work failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  local tree="$work/tree"
  mkdir -p "$tree/conformance" "$tree/crates/demo/tests/it/e2e" "$tree/crates/demo/src" "$tree/tools/kit/tests/it/e2e"
  printf '[package]\nname = "demo"\n' > "$tree/crates/demo/Cargo.toml"
  printf '[package]\nname = "kit"\n' > "$tree/tools/kit/Cargo.toml"
  printf '// conformance: CP-4\n#[tokio::test]\nasync fn node_case() {}\n\n// conformance: CP-6\n#[tokio::test]\nasync fn operator_case() {}\n' > "$tree/tools/kit/tests/it/e2e/node.rs"
  printf '// conformance: CP-1 track-1\n#[test]\nfn offline_case() {}\n' > "$tree/crates/demo/tests/it/plain.rs"
  printf '// conformance: CP-2 track-1\n#[tokio::test]\nasync fn gated_case() {}\n' > "$tree/crates/demo/tests/it/e2e/scenario.rs"
  printf 'mod tests {\n    // conformance: CP-3\n    #[test]\n    fn inline_case() {}\n}\n' > "$tree/crates/demo/src/inner.rs"
  printf '# c\ncp\tactor\trequirements\ttracks\tstatus\tissue\treason\nCP-1\tGateway\tN1\t1\tcovered\t#1\t-\nCP-2\tGateway\tN2\t1\tcovered\t#2\t-\nCP-3\tGateway\tN3\t2\tcovered\t#3\t-\nCP-4\tNode\tN4\t2\tnode-profile\t#4\tthe node\nCP-5\tGateway\tN5\t2\tplanned\t#5\t-\nCP-6\tOperator\tN6\t2\toperator\t#6\tthe operator\nCP-7\tNode\tN7\t2\tnode-profile\t#7\tthe node\nCP-8\tOperator\tN8\t2\toperator\t#8\tthe operator\n' > "$tree/conformance/matrix.tsv"
  printf 'track\ttitle\trequirements\tcps\tstatus\tissue\treason\n1\tOne\tN1\tCP-1,CP-2\tcovered\t#1\t-\n2\tTwo\tN3\tCP-3\tdeferred\t#9\tprovisional\n' > "$tree/conformance/tracks.tsv"
  git -C "$tree" init -q && git -C "$tree" add -A
  local case='<testcase name="%s" classname="%s" time="0">%s</testcase>\n'
  # junit FILE ROWS...: a JUnit file of the "name|classname|body" ROWS.
  junit() {
    local file="$1" row name classname body
    shift
    {
      printf '<?xml version="1.0"?>\n<testsuites>\n'
      for row in "$@"; do
        IFS='|' read -r name classname body <<< "$row"
        # shellcheck disable=SC2059 # the format is the fixture's own constant.
        printf "$case" "$name" "$classname" "$body"
      done
      printf '</testsuites>\n'
    } > "$file"
  }
  junit "$work/offline.xml" 'plain::offline_case|demo::it|' 'e2e::scenario::gated_case|demo::it|' 'inner::tests::inline_case|demo|'
  junit "$work/gated.xml" 'e2e::scenario::gated_case|demo::it|'
  junit "$work/failing.xml" 'e2e::scenario::gated_case|demo::it|<failure message="x"/>'
  junit "$work/kit.xml" 'e2e::scenario::gated_case|demo::it|' 'e2e::node::node_case|kit::it|' 'e2e::node::operator_case|kit::it|'
  junit "$work/kit-failing.xml" 'e2e::scenario::gated_case|demo::it|' 'e2e::node::node_case|kit::it|<failure message="x"/>' 'e2e::node::operator_case|kit::it|'
  # findings DIR ROWS...: a findings directory of one file holding the
  # "point|verdict" ROWS, each of the product Demo CDR 1.0 unless the row
  # names another as "point|verdict|product".
  findings() {
    local dir="$1" row point verdict product
    shift
    mkdir -p "$dir"
    {
      printf 'product\tpoint\tcheck\tverdict\tevidence\n'
      for row in "$@"; do
        IFS='|' read -r point verdict product <<< "$row"
        printf '%s\t%s\ta check\t%s\tseen\n' "${product:-Demo CDR 1.0}" "$point" "$verdict"
      done
    } > "$dir/demo-check.tsv"
  }
  findings "$work/nodes-fail" 'CP-4|pass|Other CDR 2.0' 'CP-4|pass' 'CP-4|fail' 'CP-6|not-observable'
  findings "$work/nodes-pass" 'CP-4|pass' 'CP-4|not-observable' 'CP-6|pass'
  findings "$work/nodes-none" 'CP-4|not-observable'
  # expect WANT ROW RESULT OFFLINE GATED [NODES]: the report exits WANT and its
  # ROW (kind and id) reads RESULT.
  expect() {
    local want="$1" row="$2" result="$3" status=0 got
    generate "$tree" "$work/out" "$4" "$5" "${6:-}" > /dev/null 2>&1 || status=$?
    got="$(awk -F'\t' -v row="$row" '($1 " " $2) == row { print $6 }' "$work/out/report.tsv")"
    if [[ "$status" -ne "$want" || "$got" != "$result" ]]; then
      echo "conformance-report: self-test failed: $row read $got (exit $status), wanted $result (exit $want)." >&2
      failed=1
    fi
  }
  expect 0 "gateway CP-1" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "gateway CP-2" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "gateway CP-3" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "track 1" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "track 2" deferred "$work/offline.xml" "$work/gated.xml"
  expect 0 "gateway CP-5" open "$work/offline.xml" "$work/gated.xml"
  expect 1 "gateway CP-2" not-run "$work/offline.xml" ""
  expect 1 "track 1" not-run "" "$work/gated.xml"
  expect 1 "gateway CP-2" fail "$work/offline.xml" "$work/failing.xml"
  expect 1 "track 1" fail "$work/offline.xml" "$work/failing.xml"
  # A Node or Operator point is its own class, and never sets the exit.
  expect 0 "node CP-4" not-run "$work/offline.xml" "$work/gated.xml"
  expect 0 "node CP-4" not-reported "$work/offline.xml" "$work/kit.xml"
  expect 0 "node CP-4" node-fail "$work/offline.xml" "$work/kit.xml" "$work/nodes-fail"
  expect 0 "node CP-4" node-pass "$work/offline.xml" "$work/kit.xml" "$work/nodes-pass"
  expect 0 "node CP-4" node-not-observable "$work/offline.xml" "$work/kit.xml" "$work/nodes-none"
  expect 0 "node CP-4" check-failed "$work/offline.xml" "$work/kit-failing.xml" "$work/nodes-pass"
  expect 0 "node CP-7" unchecked "$work/offline.xml" "$work/kit.xml" "$work/nodes-pass"
  expect 0 "operator CP-6" operator-pass "$work/offline.xml" "$work/kit.xml" "$work/nodes-pass"
  expect 0 "operator CP-6" operator-not-observable "$work/offline.xml" "$work/kit.xml" "$work/nodes-fail"
  expect 0 "operator CP-8" not-applicable "$work/offline.xml" "$work/kit.xml" "$work/nodes-pass"
  generate "$tree" "$work/out" "$work/offline.xml" "$work/kit.xml" "$work/nodes-fail" > /dev/null 2>&1 || true
  # Each product is a column of the summary, the worse of two verdicts on
  # one check shown, and each has its own table with the evidence.
  if ! grep -q '^| Point | Check | Demo CDR 1.0 | Other CDR 2.0 |$' "$work/out/report.md" \
    || ! grep -q '^| CP-4 | a check | fail | pass |$' "$work/out/report.md" \
    || ! grep -q '^| CP-6 | a check | not-observable | - |$' "$work/out/report.md" \
    || ! grep -q '^### Other CDR 2.0$' "$work/out/report.md" \
    || ! grep -q '^| CP-4 | a check | fail | seen |$' "$work/out/report.md" \
    || ! grep -q '^## Node points$' "$work/out/report.md" \
    || [[ "$(grep -c 'Demo CDR' "$work/out/node-profile.tsv")" -ne 3 ]] \
    || [[ "$(grep -c 'Other CDR' "$work/out/node-profile.tsv")" -ne 1 ]]; then
    echo "conformance-report: self-test failed: the node profile findings are not reported." >&2
    failed=1
  fi
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "conformance-report: self-test OK."
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$root/target/conformance"
offline=""
gated=""
nodes=""
run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --self-test)
      self_test
      exit 0
      ;;
    --offline | --gated | --node-profile | --out)
      [[ $# -ge 2 ]] || usage
      case "$1" in
        --offline) offline="$offline$2"$'\n' ;;
        --gated) gated="$gated$2"$'\n' ;;
        --node-profile) nodes="$2" ;;
        --out) out="$2" ;;
      esac
      shift 2
      ;;
    --run)
      run=1
      shift
      ;;
    *) usage ;;
  esac
done
if [[ "$run" -eq 1 ]]; then
  # The findings of an earlier run would read as this run's.
  if [[ -d "$root/target/conformance/node-profile" ]]; then
    find "$root/target/conformance/node-profile" -maxdepth 1 -name '*.tsv' -delete
  fi
  (cd "$root" && FERROFED_E2E=1 cargo nextest run --profile ci --locked --workspace --all-features --no-fail-fast) || true
  gated="$gated$root/target/nextest/ci/junit.xml"$'\n'
  nodes="${nodes:-$root/target/conformance/node-profile}"
fi
[[ -n "$offline$gated" ]] || usage
generate "$root" "$out" "$offline" "$gated" "$nodes"
