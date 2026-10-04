#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/conformance/report.sh: the per-track and per-point report of a test
# run, the Gazelle-style logging of section 16.4 ("each executed test records
# the conformance points it exercised"), keyed by the section 16.3 tracks and
# the section 17 points (#92).
#
#   scripts/conformance/report.sh [--offline JUNIT]... [--gated JUNIT]... [--out DIR]
#   scripts/conformance/report.sh --run [--out DIR]
#   scripts/conformance/report.sh --self-test
#
# A JUnit file is what `cargo nextest run --profile ci` writes to
# target/nextest/ci/junit.xml. A test under an e2e module returns early unless
# FERROFED_E2E is 1, so it counts only from a --gated file, a run with the
# gate set; every other test counts from either. --run is that gated run of
# the whole workspace, which needs Docker, reported as --gated.
#
# Every `// conformance:` marker above a test names the points and tracks it
# scores. A covered point or track passes when every test carrying its
# marker ran and passed, fails when one failed, and is not-run when one did
# not run and none failed. A deferred row is reported deferred, a node-profile or operator
# row not-applicable to the gateway (section 16.2), and a planned row open,
# each with its issue and reason from conformance/matrix.tsv and
# conformance/tracks.tsv. No specification governs the report's form: our own
# design.
#
# It writes DIR/report.tsv and DIR/report.md (DIR defaults to
# target/conformance) and exits 1 when a covered row failed or did not run.

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

# table FILE KIND KEY_PREFIX RESULTS: one report row per data row of the
# hand-kept table FILE, its result joined from RESULTS.
table() {
  local file="$1" kind="$2" prefix="$3" results="$4"
  awk -F'\t' -v kind="$kind" -v prefix="$prefix" '
    FILENAME == ARGV[1] {
      if ($3 == "pass") passed[$1]++
      else if ($3 == "fail") failed[$1]++
      else unrun[$1]++
      next
    }
    /^#/ { next }
    !header { header = 1; next }
    {
      id = $1; key = prefix id; status = $5
      p = passed[key] + 0; f = failed[key] + 0; u = unrun[key] + 0
      if (status == "covered") result = (f > 0 ? "fail" : (u > 0 || p == 0 ? "not-run" : "pass"))
      else if (status == "deferred") result = "deferred"
      else if (status == "node-profile" || status == "operator") result = "not-applicable"
      else result = "open"
      if (kind == "track") { title = $2; actor = "-" } else { title = "-"; actor = $2 }
      print kind "\t" id "\t" title "\t" actor "\t" status "\t" result "\t" p "\t" f "\t" u "\t" $6 "\t" $7
    }
  ' "$results" "$file"
}

# render TSV SOURCES: the markdown report of the rows in TSV.
render() {
  local tsv="$1" sources="$2" commit
  commit="$(git rev-parse --short HEAD 2> /dev/null || echo unknown)"
  printf '# Conformance report\n\n'
  printf 'The section 16.3 tracks and the section 17 conformance points of the Federation Tier with AQL, scored from the tests that carry each marker (section 16.4). Commit %s; %s.\n\n' "$commit" "$sources"
  printf 'A deferred track or point is not run, by a recorded decision. A Node or Operator point is not applicable to the gateway (section 16.2). An open point has no test yet.\n\n'
  awk -F'\t' '
    function cell(text) { gsub(/\|/, "\\|", text); return text }
    $1 == "track" && !tracks { print "## Tracks\n\n| Track | Title | Result | Tests passed, failed, not run | Issue | Reason |\n|---|---|---|---|---|---|"; tracks = 1 }
    $1 == "cp" && !cps { print "\n## Conformance points\n\n| Point | Actor | Result | Tests passed, failed, not run | Issue | Reason |\n|---|---|---|---|---|---|"; cps = 1 }
    {
      reason = ($6 == "pass" ? "-" : cell($11))
      name = ($1 == "track" ? $2 " | " cell($3) : $2 " | " $4)
      print "| " name " | " $6 " | " $7 ", " $8 ", " $9 " | " $10 " | " reason " |"
    }
  ' "$tsv"
}

# report OUT SOURCES RESULTS ROOT: writes OUT/report.tsv and OUT/report.md and
# returns 1 when a covered row failed or did not run.
report() {
  local out="$1" sources="$2" results="$3" root="$4"
  mkdir -p "$out"
  {
    printf 'kind\tid\ttitle\tactor\tstatus\tresult\tpassed\tfailed\tnot_run\tissue\treason\n'
    table "$root/conformance/tracks.tsv" track track- "$results"
    table "$root/conformance/matrix.tsv" cp "" "$results"
  } > "$out/report.tsv"
  {
    tail -n +2 "$out/report.tsv" | render /dev/stdin "$sources"
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

# generate ROOT OUT OFFLINE GATED: the report of the JUnit files listed in the
# newline-separated OFFLINE and GATED over the tree ROOT.
generate() {
  local root="$1" out="$2" offline="$3" gated="$4" work file sources=""
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
  marks "$root" > "$work/marks"
  results "$work/marks" "$work/outcomes" > "$work/results"
  report "$out" "JUnit: ${sources% }" "$work/results" "$root"
}

# self_test: a fixture tree with one crate, a matrix and a track table, and
# JUnit files that pass, fail and omit its marked tests.
self_test() {
  local work failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  local tree="$work/tree"
  mkdir -p "$tree/conformance" "$tree/crates/demo/tests/it/e2e" "$tree/crates/demo/src"
  printf '[package]\nname = "demo"\n' > "$tree/crates/demo/Cargo.toml"
  printf '// conformance: CP-1 track-1\n#[test]\nfn offline_case() {}\n' > "$tree/crates/demo/tests/it/plain.rs"
  printf '// conformance: CP-2 track-1\n#[tokio::test]\nasync fn gated_case() {}\n' > "$tree/crates/demo/tests/it/e2e/scenario.rs"
  printf 'mod tests {\n    // conformance: CP-3\n    #[test]\n    fn inline_case() {}\n}\n' > "$tree/crates/demo/src/inner.rs"
  printf '# c\ncp\tactor\trequirements\ttracks\tstatus\tissue\treason\nCP-1\tGateway\tN1\t1\tcovered\t#1\t-\nCP-2\tGateway\tN2\t1\tcovered\t#2\t-\nCP-3\tGateway\tN3\t2\tcovered\t#3\t-\nCP-4\tNode\tN4\t2\tnode-profile\t#4\tthe node\nCP-5\tGateway\tN5\t2\tplanned\t#5\t-\n' > "$tree/conformance/matrix.tsv"
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
  # expect WANT ROW RESULT OFFLINE GATED: the report exits WANT and its ROW
  # (kind and id) reads RESULT.
  expect() {
    local want="$1" row="$2" result="$3" status=0 got
    generate "$tree" "$work/out" "$4" "$5" > /dev/null 2>&1 || status=$?
    got="$(awk -F'\t' -v row="$row" '($1 " " $2) == row { print $6 }' "$work/out/report.tsv")"
    if [[ "$status" -ne "$want" || "$got" != "$result" ]]; then
      echo "conformance-report: self-test failed: $row read $got (exit $status), wanted $result (exit $want)." >&2
      failed=1
    fi
  }
  expect 0 "cp CP-1" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "cp CP-2" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "cp CP-3" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "track 1" pass "$work/offline.xml" "$work/gated.xml"
  expect 0 "track 2" deferred "$work/offline.xml" "$work/gated.xml"
  expect 0 "cp CP-4" not-applicable "$work/offline.xml" "$work/gated.xml"
  expect 0 "cp CP-5" open "$work/offline.xml" "$work/gated.xml"
  expect 1 "cp CP-2" not-run "$work/offline.xml" ""
  expect 1 "track 1" not-run "" "$work/gated.xml"
  expect 1 "cp CP-2" fail "$work/offline.xml" "$work/failing.xml"
  expect 1 "track 1" fail "$work/offline.xml" "$work/failing.xml"
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "conformance-report: self-test OK."
}

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="$root/target/conformance"
offline=""
gated=""
run=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --self-test)
      self_test
      exit 0
      ;;
    --offline | --gated | --out)
      [[ $# -ge 2 ]] || usage
      case "$1" in
        --offline) offline="$offline$2"$'\n' ;;
        --gated) gated="$gated$2"$'\n' ;;
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
  (cd "$root" && FERROFED_E2E=1 cargo nextest run --profile ci --locked --workspace --all-features --no-fail-fast) || true
  gated="$gated$root/target/nextest/ci/junit.xml"$'\n'
fi
[[ -n "$offline$gated" ]] || usage
generate "$root" "$out" "$offline" "$gated"
