#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The openEHR class guard: every openEHR type a tracked file under crates/,
# app/, tools/, docs/ (the vendored corpora aside), conformance/ or fuzz/, or
# the README or the changelog, names is a class the vendored Reference Model
# or BASE defines, and every attribute it names on such a class is one that
# class or an ancestor carries (no specification governs this guard: our own
# design). The classes are the definition tables of the RM and BASE releases
# docs/VERSIONS.md pins, docs/specs/openehr-rm/docs/UML/classes/ and
# docs/specs/openehr-base/docs/UML/classes/, with the inheritance each table
# records. ITS-REST 1.1.0 adds the types its OpenAPI documents title
# (`UPDATE_AUDIT`, `RESULT_SET`), which canonical JSON names in `_type` too.
#
# A citation is one of three forms, read from the forms the tree carries:
#
#   AQL class      the class of an AQL class expression, `CONTAINS X v` or
#                  `FROM X v`, `[` in place of the variable admitted (AQL
#                  1.1.0 §3.2 and §4.2.3). `FROM ENDPOINT` and
#                  `FROM ORGANISATION` are the federation specification's
#                  §8.1 directive, never a class.
#   _type          the type a canonical JSON object names in its `_type`
#                  member, in a fixture, a test or a doc.
#   class.member   `X.member` in a comment or a Markdown file, where X is an
#                  RM or BASE class: the member must be an attribute, a
#                  constant or a function of X or of a class X inherits.
#                  An archetype id (`openEHR-EHR-COMPOSITION.encounter.v1`)
#                  is not one.
#
# Usage:
#   scripts/checks/openehr-classes.sh              check the tracked tree
#   scripts/checks/openehr-classes.sh --self-test  prove each refusal and pass
# Exit 1 naming each citation the vendored definitions do not hold; 0
# otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# build_index ROOT: one line per fact of the vendored definitions under ROOT,
# `C<TAB>CLASS<TAB>SOURCE` for a class, `P<TAB>CLASS<TAB>PARENT` for an
# inheritance edge and `M<TAB>CLASS<TAB>MEMBER` for a member, and
# `F<TAB>NAME` for a file at ROOT whose name reads like `CLASS.member`
# (`CITATION.cff`), which is a file name and never a citation.
build_index() {
  local root="$1" dir source
  for source in RM BASE; do
    case "$source" in
      RM) dir="$root/docs/specs/openehr-rm/docs/UML/classes" ;;
      *) dir="$root/docs/specs/openehr-base/docs/UML/classes" ;;
    esac
    if [[ ! -d "$dir" ]]; then
      echo "openehr-classes: $dir is missing; run the vendor script docs/VERSIONS.md names" >&2
      return 1
    fi
    find "$dir" -type f -name '*.adoc' -exec awk -v source="$source" '
        function bare(t) { sub(/<.*/, "", t); return t }
        FNR == 1 { cur = ""; section = ""; inherit = 0 }
        /^=== [^ ]+ (Class|Enumeration|Interface)[ \t]*$/ {
          cur = bare($2); print "C\t" cur "\t" source; section = ""; inherit = 0; next
        }
        cur == "" { next }
        /^h\|\*Inherit\*/ { inherit = 1; next }
        inherit && /[^ \t]/ {
          line = $0; inherit = 0
          while (match(line, /,[A-Za-z][A-Za-z0-9_]*>>|\[[A-Za-z][A-Za-z0-9_]*(<[^]]*>)?\^\]|`[A-Za-z][A-Za-z0-9_]*(<[^`]*>)?`/)) {
            t = substr(line, RSTART, RLENGTH); line = substr(line, RSTART + RLENGTH)
            gsub(/^[,[`]|>>$|\^\]$|`$/, "", t)
            print "P\t" cur "\t" bare(t)
          }
          next
        }
        /^h\|\*(Attributes|Constants|Functions)\*/ { section = "member"; next }
        /^h\|\*[A-Z][a-z]+\*/ { section = ""; next }
        section == "member" && match($0, /^\|\*[a-z_][a-z0-9_]*\*/) {
          print "M\t" cur "\t" substr($0, RSTART + 2, RLENGTH - 3)
        }
      ' {} +
  done
  local oas="$root/docs/specs/its-rest/computable/OAS"
  if [[ -d "$oas" ]]; then
    find "$oas" -type f -name '*.yaml' -exec \
      sed -n -E 's/^[[:space:]]+title:[[:space:]]+([A-Z][A-Z0-9_]*)[[:space:]]*$/C	\1	ITS-REST/p' {} +
  fi
  find "$root" -maxdepth 1 -type f -name '[A-Z]*.[a-z]*' -exec basename {} \; \
    | sed -n -E 's/^([A-Z][A-Z0-9_]*\.[a-z][a-z0-9_]*)$/F	\1/p'
}

# citations: reads `PATH:LINE:TEXT` lines and prints one line per openEHR
# type citation, `PATH:LINE<TAB>FORM<TAB>CLASS<TAB>MEMBER<TAB>AS-WRITTEN`.
citations() {
  LC_ALL=C awk '
    function emit(where, form, class, member, written) {
      print where "\t" form "\t" class "\t" member "\t" written
    }
    {
      p = index($0, ":"); path = substr($0, 1, p - 1); rest = substr($0, p + 1)
      q = index(rest, ":"); line = substr(rest, 1, q - 1); text = substr(rest, q + 1)
      where = path ":" line

      t = text
      while (match(t, /(CONTAINS|FROM)[ \t]+[A-Z][A-Z0-9_]*([ \t]+[a-z]|[ \t]*\[)/)) {
        tok = substr(t, RSTART, RLENGTH); t = substr(t, RSTART + RLENGTH)
        kw = tok; sub(/[ \t].*/, "", kw)
        cls = tok; sub(/^[A-Z]+[ \t]+/, "", cls); sub(/[^A-Z0-9_].*$/, "", cls)
        if (kw == "FROM" && (cls == "ENDPOINT" || cls == "ORGANISATION")) { continue }
        emit(where, "AQL class", cls, "", kw " " cls)
      }

      t = text
      while (match(t, /"_type\\?"[ \t]*:[ \t]*\\?"[A-Za-z][A-Za-z0-9_]*/)) {
        tok = substr(t, RSTART, RLENGTH); t = substr(t, RSTART + RLENGTH)
        cls = tok; sub(/.*"/, "", cls)
        emit(where, "_type", cls, "", "_type " cls)
      }

      prose = ""
      if (path ~ /\.md$/) { prose = text }
      else if (path ~ /\.rs$/ && index(text, "//") > 0) { prose = substr(text, index(text, "//")) }
      t = prose
      while (match(t, /[A-Z][A-Z0-9]*(_[A-Z0-9]+)*\.[a-z][a-z0-9_]*/)) {
        before = (RSTART > 1) ? substr(t, RSTART - 1, 1) : ""
        tok = substr(t, RSTART, RLENGTH); t = substr(t, RSTART + RLENGTH)
        if (before ~ /[-A-Za-z0-9_.\/]/) { continue }
        d = index(tok, ".")
        emit(where, "member", substr(tok, 1, d - 1), substr(tok, d + 1), tok)
      }
    }'
}

# judge INDEX CITATIONS: prints a finding per citation INDEX does not hold and
# exits 1 when there is one. A class.member citation whose class is no RM or
# BASE class is not an openEHR citation and is passed over.
judge() {
  awk -F '\t' '
    FNR == 1 { part++ }
    part == 1 && $1 == "C" { if (!($2 in source)) { source[$2] = $3 } else if (index(source[$2], $3) == 0) { source[$2] = source[$2] ", " $3 }; next }
    part == 1 && $1 == "P" { parents[$2] = parents[$2] " " $3; next }
    part == 1 && $1 == "M" { member[$2, $3] = 1; next }
    part == 1 && $1 == "F" { file[$2] = 1; next }
    part == 1 { next }
    {
      where = $1; form = $2; class = $3; name = $4; written = $5
      if (form == "member") {
        if (!(class in source) || source[class] == "ITS-REST" || (class "." name) in file) { next }
        checked++
        n = 1; queue[1] = class; delete seen; seen[class] = 1; found = 0
        for (i = 1; i <= n && !found; i++) {
          c = queue[i]
          if ((c SUBSEP name) in member) { found = 1; break }
          k = split(parents[c], ps, " ")
          for (j = 1; j <= k; j++) { if (!(ps[j] in seen)) { seen[ps[j]] = 1; queue[++n] = ps[j] } }
        }
        if (!found) {
          printf "::error::%s cites %s, but %s (%s) and the classes it inherits carry no member %s\n", where, written, class, source[class], name
          bad++
        }
        next
      }
      checked++
      if (!(class in source)) {
        printf "::error::%s names %s (%s), which the vendored RM, BASE and ITS-REST define no class for\n", where, class, written
        bad++
      }
    }
    END {
      if (bad > 0) { printf "openehr-classes: %d of %d openEHR citation(s) name what the vendored definitions do not hold\n", bad, checked; exit 1 }
      printf "openehr-classes: %d openEHR citation(s), every one in the vendored RM, BASE and ITS-REST definitions\n", checked
    }' "$1" "$2"
}

# check ROOT: builds the index under ROOT and judges the citations read from
# stdin as `PATH:LINE:TEXT`.
check() {
  local root="$1" work status=0
  work="$(mktemp -d)"
  build_index "$root" > "$work/index.tsv" || status=$?
  if [[ "$status" -eq 0 ]] && ! grep -q '^C	' "$work/index.tsv"; then
    echo "openehr-classes: no class definition under $root/docs/specs/openehr-rm or openehr-base" >&2
    status=1
  fi
  if [[ "$status" -ne 0 ]]; then
    rm -rf "$work"
    return "$status"
  fi
  citations > "$work/citations.tsv"
  judge "$work/index.tsv" "$work/citations.tsv" || status=$?
  rm -rf "$work"
  return "$status"
}

check_tree() {
  git grep -n -I -E '(CONTAINS|FROM)[[:space:]]+[A-Z]|_type|[A-Z]\.[a-z]' -- \
    crates app tools docs conformance fuzz README.md CHANGELOG.md \
    ':(exclude)docs/specs/**' ':(glob,exclude)**/vendor/**' | check "$PWD"
}

# The self-test builds a small vendored corpus in a temporary directory and
# judges one citation per case: each refused form fails, each near miss and
# each form the grammar leaves alone passes.
self_test() {
  local work failed=0 n=0 want path text status
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  local rm="$work/docs/specs/openehr-rm/docs/UML/classes" base="$work/docs/specs/openehr-base/docs/UML/classes"
  mkdir -p "$rm" "$base" "$work/docs/specs/its-rest/computable/OAS"
  cat > "$rm/locatable.adoc" << 'EOF'
=== LOCATABLE Class

h|*Inherit*
2+|`<<_pathable_class,PATHABLE>>`

h|*Attributes*
^h|*Signature*

h|*1..1*
|*name*: `DV_TEXT`

h|*Invariants*
|*Name_valid*: `name /= Void`
EOF
  cat > "$rm/pathable.adoc" << 'EOF'
=== PATHABLE Class

h|*Inherit*
2+|`Any`

h|*Functions*
^h|*Signature*

h|*1..1*
|*parent* (): `PATHABLE`
EOF
  cat > "$rm/composition.adoc" << 'EOF'
=== COMPOSITION Class

h|*Inherit*
2+|`<<_locatable_class,LOCATABLE>>`

h|*Attributes*

h|*1..1*
|*composer*: `PARTY_PROXY`
EOF
  cat > "$rm/ehr.adoc" << 'EOF'
=== EHR Class

h|*Attributes*

h|*1..1*
|*ehr_id*: `HIER_OBJECT_ID`
EOF
  cat > "$base/party_ref.adoc" << 'EOF'
=== PARTY_REF Class

h|*Inherit*
2+|`link:/releases/BASE/{base_release}/base_types.html#_object_ref_class[OBJECT_REF^]`

h|*Attributes*
EOF
  cat > "$base/object_ref.adoc" << 'EOF'
=== OBJECT_REF Class

h|*Attributes*

h|*1..1*
|*namespace*: `String`
EOF
  cat > "$base/interval.adoc" << 'EOF'
=== Interval<T> Class

h|*Attributes*

h|*0..1*
|*lower*: `T`
EOF
  : > "$work/COMPOSITION.cff"
  cat > "$work/docs/specs/its-rest/computable/OAS/ehr.yaml" << 'EOF'
components:
  schemas:
    UpdateAudit:
      title: UPDATE_AUDIT
EOF

  # One case per line: the exit it wants, a tab, the path, a tab, the line.
  cat > "$work/cases.tsv" << 'EOF'
0	crates/x/src/lib.rs	    "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
0	crates/x/src/lib.rs	    "SELECT c FROM EHR e[ehr_id/value=$id] CONTAINS COMPOSITION[openEHR-EHR-COMPOSITION.encounter.v1]",
0	crates/x/src/lib.rs	/// `FROM ENDPOINT p [x] CONTAINS EHR e` and `FROM ORGANISATION o [y]`.
0	crates/x/src/lib.rs	    "SELECT COUNT(*) FROM SEC_APP_TBL WHERE APP_PUB_ID = 1",
0	crates/x/src/lib.rs	    r#"{"_type":"COMPOSITION","name":{"value":"x"}}"#
0	crates/x/src/lib.rs	    "{\"_type\": \"UPDATE_AUDIT\"}"
0	crates/x/src/lib.rs	/// The namespace is `PARTY_REF.namespace`, inherited from `OBJECT_REF`.
0	crates/x/src/lib.rs	    let id = EHR_A.parse()?; // the `COMPOSITION.composer` and `LOCATABLE.name`
0	docs/x.md	The `COMPOSITION.parent` function comes from PATHABLE, `Interval.lower` from BASE.
0	crates/x/src/lib.rs	    let body = PATIENT.to_owned(); // `ONE_NODE.replace` is no class.
0	docs/x.md	The archetype openEHR-EHR-COMPOSITION.encounter.v1 names no member.
0	crates/x/src/lib.rs	    let status = EHR_STATUS.to_owned();
0	docs/x.md	The citation metadata is `COMPOSITION.cff`, a file at the root.
0	crates/x/src/lib.rs	    r#"{"token_type":"Bearer","_type":"EHR"}"#
1	crates/x/src/lib.rs	    "SELECT c FROM EHR e CONTAINS COMPOSTION c",
1	crates/x/src/lib.rs	    "SELECT o FROM EHR e CONTAINS OBSERVATION o",
1	crates/x/src/lib.rs	    "SELECT x FROM EHR_RECORD x",
1	crates/x/src/lib.rs	    r#"{"_type":"COMPOSITION_REF"}"#
1	crates/x/src/lib.rs	/// The id is `PARTY_REF.identifier`.
1	docs/x.md	The `COMPOSITION.encounter` attribute.
1	docs/x.md	The `LOCATABLE.uid` attribute, which this corpus does not define.
EOF

  while IFS=$'\t' read -r want path text; do
    n=$((n + 1))
    status=0
    printf '%s:%d:%s\n' "$path" "$n" "$text" | check "$work" > "$work/out" 2>&1 || status=$?
    if [[ "$status" -ne "$want" ]]; then
      echo "openehr-classes: self-test failed: '$text' exited $status, wanted $want." >&2
      sed 's/^/  /' "$work/out" >&2
      failed=1
    elif [[ "$want" -ne 0 ]] && ! grep -q -F "::error::$path:$n " "$work/out"; then
      echo "openehr-classes: self-test failed: the refusal of '$text' does not name its file and line." >&2
      failed=1
    fi
    if [[ -n "${OPENEHR_CLASSES_VERBOSE:-}" ]]; then
      sed 's/^/  /' "$work/out"
    fi
  done < "$work/cases.tsv"
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "openehr-classes: self-test OK ($n cases)."
}

case "${1:-}" in
  --self-test) self_test ;;
  '') check_tree ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac
