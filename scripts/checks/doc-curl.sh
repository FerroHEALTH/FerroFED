#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The documentation curl guard (no specification governs this: our own
# design). curl sends the value of -d, --data, --data-binary, --data-raw or
# --data-ascii as the request body, so a curl example whose body follows in a
# here-document reads it only through -d @-, and one that passes a number
# sends that number. Every curl command in README.md, the book's pages and
# the landing page is read with its continuation lines joined, and refused
# when:
#
#   - a data option's value is a bare number, which curl posts as the body;
#   - the command takes a here-document and a data option names anything but
#     @- or @<file>, so the here-document never reaches the body.
#
# Usage:
#   scripts/checks/doc-curl.sh              check every documentation page
#   scripts/checks/doc-curl.sh --self-test  prove the refusals and passes
# Exit 1 naming each refused command by file and line; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

# scan FILE...: prints FILE:LINE and the reason for each refused command.
scan() {
  awk '
    function reset() { command = ""; start = 0 }
    function judge(text, file, line,    n, i, token, value, words, heredoc) {
      gsub(/&lt;/, "<", text)
      if (text !~ /(^|[^[:alnum:]_-])curl[[:space:]]/) return
      heredoc = (text ~ /<<-?[[:space:]]*["\047]?[A-Za-z_]+/)
      n = split(text, words, /[[:space:]]+/)
      for (i = 1; i <= n; i++) {
        token = words[i]
        value = ""
        if (token ~ /^(-d|--data|--data-binary|--data-raw|--data-ascii)$/ && i < n) {
          value = words[i + 1]
        } else if (token ~ /^-d./) {
          value = substr(token, 3)
        } else if (token ~ /^--data(-binary|-raw|-ascii)?=/) {
          value = substr(token, index(token, "=") + 1)
        } else {
          continue
        }
        gsub(/^["\047]|["\047]$/, "", value)
        if (value ~ /^[0-9]+$/) {
          printf "%s:%d: curl sends the number %s as its body; read the body with -d @-\n", file, line, value
        } else if (heredoc && value !~ /^@/) {
          printf "%s:%d: curl takes a here-document but sends %s as its body; use -d @-\n", file, line, value
        }
      }
    }
    FNR == 1 { reset() }
    {
      text = $0
      if (command == "") start = FNR
      if (text ~ /\\[[:space:]]*$/) {
        sub(/\\[[:space:]]*$/, " ", text)
        command = command text
        next
      }
      command = command text
      judge(command, FILENAME, start)
      reset()
    }
  ' "$@"
}

# pages: every documentation page a reader copies a command from.
pages() {
  printf '%s\n' README.md website/landing/index.html
  find website/book/src -type f -name '*.md' | LC_ALL=C sort
}

self_test() {
  local dir found
  dir="$(mktemp -d)"
  # shellcheck disable=SC2064 # the directory is fixed when the trap is set
  trap "rm -rf '$dir'" RETURN
  printf '%s\n' \
    "curl -s http://127.0.0.1:8080/v1/query/aql \\" \
    "  -H 'Content-Type: application/json' -d 13152 <<'EOF'" \
    '{"q": "SELECT 1"}' 'EOF' > "$dir/number.md"
  printf '%s\n' \
    "curl -s http://127.0.0.1:8080/v1/query/aql -d '{\"q\": 1}' <<EOF" \
    '{"q": "SELECT 1"}' 'EOF' > "$dir/inline.md"
  printf '%s\n' "curl -X POST http://127.0.0.1:8080/x --data-binary=42" > "$dir/equals.md"
  printf '%s\n' \
    "curl -s http://127.0.0.1:8080/v1/query/aql \\" \
    "  -H 'Content-Type: application/json' -d @- &lt;&lt;'EOF'" \
    '{"q": "SELECT 1"}' 'EOF' > "$dir/stdin.html"
  printf '%s\n' \
    "curl -s http://127.0.0.1:8080/v1/query/aql -d @query.json" \
    "curl -s http://127.0.0.1:8080/v1/query/aql -d '{\"q\": \"SELECT 1\"}'" \
    "the -d 5 flag in prose is no command" > "$dir/file.md"
  for name in number inline equals; do
    found="$(scan "$dir/$name.md")"
    if [[ -z "$found" ]]; then
      echo "doc-curl: self-test failed: $name.md was not refused." >&2
      exit 1
    fi
  done
  found="$(scan "$dir/number.md")"
  if [[ "$found" != "$dir/number.md:1: "* ]]; then
    echo "doc-curl: self-test failed: number.md was not named at its first line: $found" >&2
    exit 1
  fi
  for name in stdin.html file.md; do
    found="$(scan "$dir/$name")"
    if [[ -n "$found" ]]; then
      echo "doc-curl: self-test failed: $name was refused: $found" >&2
      exit 1
    fi
  done
  echo "doc-curl: self-test OK."
}

case "${1:-}" in
  --self-test) self_test ;;
  "")
    files=()
    while IFS= read -r page; do
      files+=("$page")
    done < <(pages)
    found="$(scan "${files[@]}")"
    if [[ -n "$found" ]]; then
      printf '%s\n' "$found" >&2
      echo "doc-curl: FAILED" >&2
      exit 1
    fi
    echo "doc-curl: OK, ${#files[@]} pages"
    ;;
  *)
    sed -n '/^# Usage:/,/^# Exit/p' "$0" >&2
    exit 2
    ;;
esac
