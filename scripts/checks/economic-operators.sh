#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The register of economic operators guard (#787), for Regulation (EU)
# 2025/327 Art 35 and Regulation (EU) 2024/2847 Art 23, which have the
# register shown on request for 10 years. How it checks is our own design: no
# specification governs it. It holds three things.
#
#   format     the column row is exactly the one of the public copy, every
#              row has that many tab-separated fields, ids are unique and
#              E-<year>-<number>, each field is of its documented form, and a
#              filled keep_until is at least 10 years after the supply.
#   public     the copy in the repository, docs/registers/economic-operators.tsv,
#              holds no row: the rows are the manufacturer's business data,
#              kept outside the public repository.
#   retention  with --register FILE, the private register, kept under git:
#              every row an earlier commit of FILE held is still in FILE,
#              unchanged but for a keep_until filled in, until a keep_until it
#              carried has passed.
#
# Usage:
#   scripts/checks/economic-operators.sh                  the public copy
#   scripts/checks/economic-operators.sh --public FILE    FILE as the public copy
#   scripts/checks/economic-operators.sh --register FILE  the private register
#   scripts/checks/economic-operators.sh --self-test      prove each refusal and pass
# Needs git, awk, date and sort. Exit 1 naming each refusal; 2 on a usage
# error; 0 otherwise.
set -euo pipefail
export LC_ALL=C

# absolute PATH: PATH made absolute against the directory the script was called from.
absolute() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s/%s\n' "$PWD" "$1" ;;
  esac
}

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
readonly ROOT
readonly PUBLIC="$ROOT/docs/registers/economic-operators.tsv"
readonly COLUMNS='id	direction	supplied	operator	address	role	versions	form	member_states	contract	keep_until'

# check_format FILE: the format refusals of FILE, one ::error:: line each;
# exits 1 when there is one. The data rows go to stdout when ROWS_OUT is set.
check_format() {
  awk -F '\t' -v columns="$COLUMNS" -v name="$1" -v rows_out="${ROWS_OUT:-}" '
    function bad(msg) { printf "::error::economic-operators: %s:%d: %s\n", name, NR, msg > "/dev/stderr"; refused++ }
    function is_date(d,   m, day) {
      if (d !~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) return 0
      m = substr(d, 6, 2) + 0; day = substr(d, 9, 2) + 0
      return m >= 1 && m <= 12 && day >= 1 && day <= 31
    }
    function is_list(v, re,   n, i, part) {
      if (v == "") return 0
      n = split(v, part, ",")
      for (i = 1; i <= n; i++) if (part[i] !~ re) return 0
      return 1
    }
    /^#/ { next }
    !header {
      header = 1
      if ($0 != columns) bad("the column row is not: " columns)
      want = split(columns, unused, "\t")
      next
    }
    {
      if (NF != want) { bad("a row of " NF " fields, wanted " want); next }
      if ($1 !~ /^E-[0-9][0-9][0-9][0-9]-[0-9]+$/) bad("id " $1 " is not E-<year>-<number>")
      else if ($1 in seen) bad("id " $1 " is entered twice")
      seen[$1] = 1
      if ($2 != "supplied-to" && $2 != "supplied-by") bad($1 ": direction is not supplied-to or supplied-by")
      if (!is_date($3)) bad($1 ": supplied is not a YYYY-MM-DD date")
      if ($4 == "") bad($1 ": operator is empty")
      if ($5 == "") bad($1 ": address is empty")
      if ($6 !~ /^(manufacturer|authorised-representative|importer|distributor|fulfilment-service-provider|other)$/) bad($1 ": role " $6 " is not a role of Regulation (EU) 2019/1020 Art 3(13)")
      if (!is_list($7, "^[0-9]+\\.[0-9]+\\.[0-9]+$")) bad($1 ": versions is not a comma-separated list of X.Y.Z")
      if (!is_list($8, "^(source|binaries|gateway-image|console-image)$")) bad($1 ": form is not a comma-separated list of source, binaries, gateway-image, console-image")
      if ($9 == "" && $2 == "supplied-to") bad($1 ": member_states is empty on a supplied-to row")
      if ($9 != "" && !is_list($9, "^[A-Z][A-Z]$")) bad($1 ": member_states is not a comma-separated list of two-letter country codes")
      if ($10 == "") bad($1 ": contract is empty")
      if ($11 != "") {
        if (!is_date($11)) bad($1 ": keep_until is not a YYYY-MM-DD date")
        else if (is_date($3) && $11 < (substr($3, 1, 4) + 10) substr($3, 5)) bad($1 ": keep_until " $11 " is less than 10 years after " $3 " (Art 23(2))")
      }
      if (rows_out != "") print
    }
    END {
      if (!header) bad("no column row")
      exit refused > 0
    }
  ' "$1"
}

# check_public FILE: FILE is well formed and holds no row.
check_public() {
  local rows status=0
  rows="$(ROWS_OUT=1 check_format "$1")" || status=1
  if [[ -n "$rows" ]]; then
    echo "::error::economic-operators: $1 holds a row; the rows are kept outside the public repository" >&2
    status=1
  fi
  if [[ "$status" -eq 0 ]]; then
    echo "economic-operators: $1 has the column row and no entry"
  fi
  return "$status"
}

# check_register FILE: FILE is well formed, under git, and keeps every row an
# earlier commit of it held until that row's keep_until has passed.
check_register() {
  local file="$1" dir base history current commit today status=0 rows
  dir="$(dirname "$file")"
  base="$(basename "$file")"
  rows="$(ROWS_OUT=1 check_format "$file")" || status=1
  if ! git -C "$dir" rev-parse --is-inside-work-tree > /dev/null 2>&1; then
    echo "::error::economic-operators: $file is not in a git work tree, so its retention cannot be checked" >&2
    return 1
  fi
  history="$(mktemp)"
  # The data rows of every committed version, the comments and the column row dropped.
  for commit in $(git -C "$dir" log --format=%H -- "$base"); do
    git -C "$dir" show "$commit:./$base" | awk '/^#/ { next } !seen++ { next } { print }' >> "$history"
  done
  current="$(mktemp)"
  printf '%s\n' "$rows" > "$current"
  today="$(date -u +%Y-%m-%d)"
  awk -F '\t' -v today="$today" -v name="$file" -v current="$current" '
    function bad(id, msg) {
      if (id in reported) return
      printf "::error::economic-operators: %s: %s %s\n", name, id, msg > "/dev/stderr"
      reported[id] = 1; refused++
    }
    FILENAME == current { if ($0 != "") cur[$1] = $0; next }
    {
      if ($11 != "" && (!($1 in until) || $11 > until[$1])) until[$1] = $11
      old[++n] = $0; oid[n] = $1
    }
    END {
      for (i = 1; i <= n; i++) {
        id = oid[i]; o = old[i]
        if (!(id in cur)) {
          if (!(id in until) || until[id] >= today) bad(id, "was removed before its keep_until had passed")
          continue
        }
        if (cur[id] == o) continue
        split(o, of, "\t"); split(cur[id], cf, "\t")
        same = (of[11] == "")
        for (k = 1; k <= 10; k++) if (of[k] != cf[k]) same = 0
        if (!same) bad(id, "was changed; a row is only ever completed by filling its keep_until")
      }
      exit refused > 0
    }
  ' "$current" "$history" || status=1
  rm -f "$history" "$current"
  if [[ "$status" -eq 0 ]]; then
    echo "economic-operators: $file is well formed and has kept every row"
  fi
  return "$status"
}

self_test() {
  local dir failed=0 n=0 status
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064 # the directory is fixed now, on purpose
  trap "rm -rf '$dir'" EXIT

  # expect WANT MODE FILE: the check MODE over FILE exits WANT.
  expect() {
    n=$((n + 1))
    status=0
    "$2" "$3" > "$dir/out" 2>&1 || status=$?
    if [[ "$status" -ne "$1" ]]; then
      echo "economic-operators: self-test case $n failed: $2 over $3 exited $status, wanted $1." >&2
      sed 's/^/  /' "$dir/out" >&2
      failed=1
    fi
  }
  # register NAME ROW...: a register NAME with the column row and ROWs.
  register() {
    local name="$1"
    shift
    { echo "# a synthetic register"; printf '%s\n' "$COLUMNS"; if (($# > 0)); then printf '%s\n' "$@"; fi; } > "$dir/$name"
  }
  local good=$'E-2026-1\tsupplied-to\t2026-10-01\tExample Integrator B.V.\tExample Street 1, 1000 AA Example\tdistributor\t0.0.9,0.0.10\tbinaries,gateway-image\tNL,BE\tK-0001\t'
  local old=$'E-1980-1\tsupplied-to\t1980-01-01\tExample Host B.V.\tExample Street 2, 1000 AA Example\tother\t0.0.1\tsource\tNL\tK-0002\t1990-01-02'

  register public-empty
  expect 0 check_public "$dir/public-empty"
  register public-row "$good"
  expect 1 check_public "$dir/public-row"
  printf '# no column row\n' > "$dir/no-columns"
  expect 1 check_public "$dir/no-columns"
  { printf '%s\textra\n' "$COLUMNS"; } > "$dir/wrong-columns"
  expect 1 check_public "$dir/wrong-columns"

  register fields $'E-2026-1\tsupplied-to\t2026-10-01'
  expect 1 check_format "$dir/fields"
  register good "$good" "$old"
  expect 0 check_format "$dir/good"
  register twice "$good" "$good"
  expect 1 check_format "$dir/twice"
  local field value i=0
  for field in 1:E-26-1 2:sold 3:2026-13-01 4: 5: 6:reseller 7:0.0 8:tarball 9:Netherlands 10: 11:2030-01-01 11:soon; do
    i=$((i + 1))
    value="${field#*:}"
    register "field-$i" "$(printf '%s\n' "$good" | awk -F '\t' -v OFS='\t' -v k="${field%%:*}" -v v="$value" '{ $k = v; print }')"
    expect 1 check_format "$dir/field-$i"
  done
  register supplied-by "$(printf '%s\n' "$good" | awk -F '\t' -v OFS='\t' '{ $2 = "supplied-by"; $9 = ""; print }')"
  expect 0 check_format "$dir/supplied-by"
  register supplied-to-empty "$(printf '%s\n' "$good" | awk -F '\t' -v OFS='\t' '{ $9 = ""; print }')"
  expect 1 check_format "$dir/supplied-to-empty"

  mkdir "$dir/loose"
  register loose/register.tsv "$good"
  expect 1 check_register "$dir/loose/register.tsv"

  local repo="$dir/repo" file="$dir/repo/register.tsv"
  git init -q "$repo"
  commit() {
    git -C "$repo" add register.tsv
    git -C "$repo" -c user.name=self-test -c user.email=self-test@example.invalid -c commit.gpgsign=false commit -q -m "$1"
  }
  register repo/register.tsv
  expect 0 check_register "$file"
  commit "the column row"
  register repo/register.tsv "$good" "$old"
  expect 0 check_register "$file"
  commit "two rows"
  register repo/register.tsv "$old"
  expect 1 check_register "$file"
  register repo/register.tsv "$(printf '%s\n' "$good" | awk -F '\t' -v OFS='\t' '{ $4 = "Another Name B.V."; print }')" "$old"
  expect 1 check_register "$file"
  local kept
  kept="$(printf '%s\n' "$good" | awk -F '\t' -v OFS='\t' '{ $11 = "2036-10-01"; print }')"
  register repo/register.tsv "$kept" "$old"
  expect 0 check_register "$file"
  commit "keep_until filled"
  register repo/register.tsv "$good" "$old"
  expect 1 check_register "$file"
  register repo/register.tsv "$kept"
  expect 0 check_register "$file"
  commit "a row past its keep_until removed"
  register repo/register.tsv "$kept"
  expect 0 check_register "$file"

  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "economic-operators: self-test OK ($n cases)."
}

case "${1:-}" in
  --self-test) self_test ;;
  '') check_public "$PUBLIC" ;;
  --public | --register)
    if [[ $# -ne 2 ]]; then
      sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
      exit 2
    fi
    if [[ "$1" == --public ]]; then
      check_public "$(absolute "$2")"
    else
      check_register "$(absolute "$2")"
    fi
    ;;
  *)
    sed -n '/^# Usage:/,/^# Needs/p' "$0" >&2
    exit 2
    ;;
esac
