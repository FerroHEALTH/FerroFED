#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# The obligations guard, tier 1 (#278): conformance/obligations.tsv, one row
# per normative statement of the pinned Federation Tier with AQL
# specification, held to the vendored text, to the conformance matrix and to
# the tests it names. Offline and static: the Rust tier runs the tests.
#
# It fails when:
#   1. the header row is not the ten columns, or a row has another count;
#   2. a row id is duplicated;
#   3. a status, actor or keyword is outside its vocabulary, or a node or
#      operator status sits on another actor;
#   4. a planned, missing, built-untested, new-gap, contradiction or deferred
#      row names no #N, or an n/a row gives no reason;
#   5. a tested row names no test as ::function, or names a function that no
#      Rust file of the tree defines with `fn <name>`;
#   6. a row's point is not in conformance/matrix.tsv, or one of its
#      requirements is not in conformance/requirements.tsv;
#   7. a row's page is not a vendored page or schema, or a vendored source
#      with a line carrying a keyword has no row;
#   8. conformance/obligation-sources.tsv differs from a fresh derivation
#      from the vendored text (a re-pin added, removed or reworded a line
#      carrying a keyword, and the rows of that source wait to be
#      reclassified);
#   9. the book page differs from `scripts/conformance/obligations.sh --render`.
#
# No specification governs the checklist form: our own design.
#
# Usage:
#   scripts/checks/obligations.sh              check the tree
#   scripts/checks/obligations.sh --self-test  prove each refusal and its near misses
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

readonly OBLIGATIONS=conformance/obligations.tsv
readonly SOURCES=conformance/obligation-sources.tsv
readonly MATRIX=conformance/matrix.tsv
readonly REQUIREMENTS=conformance/requirements.tsv
readonly PAGE=website/book/src/evaluate/obligations.md
readonly GENERATOR=scripts/conformance/obligations.sh
readonly HEADER=$'id\tpage\tanchor_or_section\tactor\tkeyword\tn\tcp\tstatus\tevidence\tclause'

# The Rust sources of the tree a tested row may name a function in, outside
# the vendored corpora: the git index in a work tree, a walk otherwise.
rust_sources() {
  if git rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    git ls-files --cached --others --exclude-standard -- '*.rs' ':!docs/specs/**'
  else
    find . -name '*.rs' -not -path './docs/specs/*' -not -path './target/*' | sed 's|^\./||'
  fi
  return
}

# check_tree DIR: runs every check against the tree rooted at DIR and prints
# each problem on stderr. Returns 1 when there is any.
check_tree() {
  local dir="$1"
  (
    cd "$dir" || exit 2
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    fail=0
    problem() {
      echo "obligations: $*" >&2
      fail=1
      return 0
    }
    for f in "$OBLIGATIONS" "$SOURCES" "$MATRIX" "$REQUIREMENTS" "$PAGE" "$GENERATOR"; do
      [[ -f "$f" ]] || problem "$f is missing"
    done
    [[ "$fail" -eq 0 ]] || exit 1

    grep -v '^#' "$OBLIGATIONS" > "$work/rows"
    if [[ "$(head -n 1 "$work/rows")" != "$HEADER" ]]; then
      problem "$OBLIGATIONS: the header row is not: ${HEADER//$'\t'/ }"
    fi
    grep -v '^#' "$MATRIX" | tail -n +2 | cut -f1 > "$work/cps"
    grep -v '^#' "$REQUIREMENTS" | tail -n +2 | cut -f1 > "$work/ns"
    grep -v '^#' "$SOURCES" | tail -n +2 > "$work/sources"
    rust_sources | while IFS= read -r src; do
      [[ -f "$src" ]] && grep -h -o -E '\bfn [a-z_][a-z0-9_]*' "$src"
    done | sed 's/^fn //' | LC_ALL=C sort -u > "$work/fns"

    # Checks 1 to 7, one message per line.
    findings="$(
      awk -F'\t' -v file="$OBLIGATIONS" '
        function issue(s,   rest, before) {
          rest = s
          while (match(rest, /#[0-9]+/)) {
            before = substr(rest, 1, RSTART - 1)
            if (before !~ /[A-Za-z0-9.#-]$/ && substr(rest, RSTART + RLENGTH, 1) !~ /[A-Za-z0-9]/) return 1
            rest = substr(rest, RSTART + RLENGTH)
          }
          return 0
        }
        FILENAME == ARGV[1] { cp[$1] = 1; next }
        FILENAME == ARGV[2] { req[$1] = 1; next }
        FILENAME == ARGV[3] { src[$1] = 1; lines[$1] = $2; next }
        FILENAME == ARGV[4] { fn[$1] = 1; next }
        ++row == 1 { next }
        {
          id = $1
          if (NF != 10) { print file ": " id " has " NF " columns, not 10"; next }
          if (id in seen) print file ": " id " is listed twice"
          seen[id] = 1
          s = $8; a = $4; ev = $9
          if (s !~ /^(tested|built-untested|planned|missing|deferred|new-gap|contradiction|node|operator|n\/a)$/) print file ": " id " has status \"" s "\", outside the vocabulary"
          if (a !~ /^(Gateway|Node|Operator|Client|Editor)$/) print file ": " id " has actor \"" a "\", outside Gateway, Node, Operator, Client, Editor"
          if ($5 !~ /^(MUST|MUST NOT|SHALL|SHALL NOT|REQUIRED|SHOULD|SHOULD NOT|RECOMMENDED|MAY)$/) print file ": " id " has keyword \"" $5 "\", outside RFC 2119"
          if (s == "node" && a != "Node") print file ": " id " is " a "'"'"'s, so its status cannot be node"
          if (s == "operator" && a != "Operator") print file ": " id " is " a "'"'"'s, so its status cannot be operator"
          if (s ~ /^(planned|missing|built-untested|new-gap|contradiction|deferred)$/ && !issue(ev)) print file ": " id " is " s " and names no issue as #N"
          if (s == "n/a" && (ev == "" || ev == "-")) print file ": " id " is n/a and gives no reason"
          if (s == "tested") {
            named = 0
            rest = ev
            while (match(rest, /::[a-z_][a-z0-9_]*/)) {
              name = substr(rest, RSTART + 2, RLENGTH - 2)
              rest = substr(rest, RSTART + RLENGTH)
              named++
              if (!(name in fn)) print file ": " id " names the test " name ", which no Rust file defines"
            }
            if (named == 0) print file ": " id " is tested and names no test as file::function"
          }
          if ($7 != "-" && !($7 in cp)) print file ": " id " cites " $7 ", which is not in conformance/matrix.tsv"
          if ($6 != "-") {
            k = split($6, ns, ",")
            for (i = 1; i <= k; i++) if (!(ns[i] in req)) print file ": " id " cites " ns[i] ", which is not in conformance/requirements.tsv"
          }
          page = $2
          path = (page ~ /\.json$/ ? "attachments/" : "pages/") page
          if (!(path in src)) print file ": " id " is on " page ", which is no vendored page or schema in conformance/obligation-sources.tsv"
          else rows[path]++
        }
        END {
          for (p in src) if (lines[p] > 0 && !(p in rows)) print file ": " p " carries a keyword and has no row"
        }
      ' "$work/cps" "$work/ns" "$work/sources" "$work/fns" "$work/rows" | LC_ALL=C sort
    )"
    if [[ -n "$findings" ]]; then
      while IFS= read -r line; do problem "$line"; done <<< "$findings"
    fi

    # 8. The source digests against a fresh derivation.
    if bash "$GENERATOR" --derived "$work/derived"; then
      grep -v '^#' "$SOURCES" > "$work/held"
      if ! diff -u "$work/held" "$work/derived" > "$work/diff" 2>&1; then
        problem "the vendored text differs from the text $OBLIGATIONS was classified against; reclassify the rows of each source below, then run $GENERATOR --derive:"
        sed 's/^/  /' "$work/diff" >&2
      fi
    else
      problem "the source digests could not be derived"
    fi

    # 9. The rendered book page.
    if bash "$GENERATOR" --render > "$work/page.md"; then
      if ! diff -u "$PAGE" "$work/page.md" > "$work/diff" 2>&1; then
        problem "$PAGE is stale (run $GENERATOR --render-write):"
        sed 's/^/  /' "$work/diff" | head -40 >&2
      fi
    else
      problem "the book page could not be rendered"
    fi

    [[ "$fail" -eq 0 ]] || exit 1
    total="$(tail -n +2 "$work/rows" | wc -l | tr -d '[:space:]')"
    tested="$(tail -n +2 "$work/rows" | cut -f8 | grep -c -x tested)"
    echo "obligations: OK ($total statements, $tested tested)"
  )
}

# The self-test builds a small tree that passes, then breaks one thing at a
# time: each refused form must fail and each near miss must pass.
self_test() {
  local work failed=0
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  local spec=docs/specs/federation-spec/modules/ROOT

  # fixture DIR: a tree with two pages, one schema, a test and the generator.
  fixture() {
    local dir="$1"
    rm -rf "$dir"
    mkdir -p "$dir/$spec/pages" "$dir/$spec/attachments" "$dir/conformance" "$dir/scripts/conformance" \
      "$dir/website/book/src/evaluate" "$dir/docs" "$dir/app/x/tests"
    cp "$here/../conformance/obligations.sh" "$dir/$GENERATOR"
    printf '| Federation Tier with AQL | 0.9.0 |\n' > "$dir/docs/VERSIONS.md"
    printf '= Requirements\n\n[[n1]]\nThe gateway MUST answer.\nThe node SHOULD log.\n' > "$dir/$spec/pages/requirements.adoc"
    printf '= Contributors\n\nNo keyword here.\n' > "$dir/$spec/pages/contributors.adoc"
    printf '{ "description": "It MAY be absent." }\n' > "$dir/$spec/attachments/options-root.schema.json"
    printf '#[test]\nfn the_gateway_answers() {}\n' > "$dir/app/x/tests/it.rs"
    printf '# comment\ncp\tactor\nCP-1\tGateway\nCP-33a\tOperator\n' > "$dir/$MATRIX"
    printf '# comment\nrequirement\tcps\nN1\tCP-1\nN27a\t-\n' > "$dir/$REQUIREMENTS"
    {
      printf '# comment\n%s\n' "$HEADER"
      printf 'requirements#n1.1\trequirements.adoc\tn1\tGateway\tMUST\tN1\tCP-1\ttested\tapp/x/tests/it.rs::the_gateway_answers\tThe gateway MUST answer\n'
      printf 'requirements#n1.2\trequirements.adoc\tn1\tNode\tSHOULD\tN27a\tCP-33a\tnode\tthe member nodes, #93\tThe node SHOULD log\n'
      printf 'options-root.schema#/description\toptions-root.schema.json\t/description\tGateway\tMAY\t-\t-\tplanned\t#12 (rest-facade#7a.1 is no issue)\tIt MAY be absent\n'
    } > "$dir/$OBLIGATIONS"
    printf '# comment\nsource\tlines\tsha256\n' > "$dir/$SOURCES"
    bash "$dir/$GENERATOR" --derive > /dev/null
    bash "$dir/$GENERATOR" --render-write > /dev/null
    return
  }

  # expect WANT NAME: check_tree over the fixture exits WANT.
  expect() {
    local want="$1" name="$2" status=0
    check_tree "$work/tree" > /dev/null 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "obligations: self-test failed: $name exited $status, wanted $want." >&2
      failed=1
    fi
    return 0
  }
  # edit SED: rewrites the checklist of a fresh fixture and renders it again,
  # so only the edit can fail.
  edit() {
    local script="$1"
    fixture "$work/tree"
    sed -i.bak "$script" "$work/tree/$OBLIGATIONS" && rm -f "$work/tree/$OBLIGATIONS.bak"
    bash "$work/tree/$GENERATOR" --render-write > /dev/null
    return
  }

  fixture "$work/tree"
  expect 0 "the clean fixture"
  edit $'s/\ttested\t/\tverified\t/'
  expect 1 "an unknown status"
  edit $'s/\tGateway\tMUST\t/\tServer\tMUST\t/'
  expect 1 "an unknown actor"
  edit $'s/\tNode\tSHOULD/\tOperator\tSHOULD/'
  expect 1 "a node status on an Operator row"
  edit $'s/#12 (rest-facade#7a.1 is no issue)/rest-facade#7a.1 is no issue/'
  expect 1 "a planned row naming no issue"
  edit $'s/\tplanned\t#12 (rest-facade#7a.1 is no issue)/\tnew-gap\tsee section 7a/'
  expect 1 "a new-gap row naming no issue"
  edit $'s/\tplanned\t#12 (rest-facade#7a.1 is no issue)/\tmissing\tissue #12a/'
  expect 1 "a missing row naming #12a, which is no issue"
  edit $'s/\tplanned\t#12 (rest-facade#7a.1 is no issue)/\tbuilt-untested\t(#290) the code/'
  expect 0 "a built-untested row naming its issue"
  edit $'s/::the_gateway_answers/::the_gateway_answers_twice/'
  expect 1 "a tested row naming a function no file defines"
  edit $'s/app\/x\/tests\/it.rs::the_gateway_answers/app\/x\/tests\/it.rs/'
  expect 1 "a tested row naming no test"
  edit $'s/\tCP-1\ttested/\tCP-99\ttested/'
  expect 1 "a point the matrix does not hold"
  edit $'s/\tN1\tCP-1/\tN1,N2\tCP-1/'
  expect 1 "a requirement the requirements table does not hold"
  edit $'s/^requirements#n1.2\t/requirements#n1.1\t/'
  expect 1 "a duplicated row id"
  edit $'s/\toptions-root.schema.json\t/\tfederated-result-set.schema.json\t/'
  expect 1 "a page that is no vendored source"
  edit $'/^options-root.schema/d'
  expect 1 "a source with a keyword line and no row"

  fixture "$work/tree"
  printf 'The node MUST NOT log the identifier.\n' >> "$work/tree/$spec/pages/requirements.adoc"
  expect 1 "a re-pin that adds a keyword line"
  fixture "$work/tree"
  sed -i.bak 's/MUST answer/MUST answer at once/' "$work/tree/$spec/pages/requirements.adoc"
  expect 1 "a re-pin that rewords a keyword line"
  fixture "$work/tree"
  printf 'A line with no keyword.\n' >> "$work/tree/$spec/pages/requirements.adoc"
  expect 0 "a re-pin that adds prose with no keyword"

  fixture "$work/tree"
  printf '\nA hand edit.\n' >> "$work/tree/$PAGE"
  expect 1 "a stale book page"

  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "obligations: self-test OK."
}

case "${1:-}" in
  --self-test) self_test ;;
  "") check_tree "$here/../.." ;;
  *)
    echo "obligations: unknown argument $1" >&2
    exit 2
    ;;
esac
