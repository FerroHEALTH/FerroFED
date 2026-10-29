#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# The observability assets guard (no specification governs this: our own
# design). deploy/observability/ ships a Grafana dashboard and a Prometheus
# rule file with every release; this guard holds both to what they claim:
#
#   1. the rule file is one Prometheus rule file: promtool check rules when
#      promtool is on PATH (or named by PROMTOOL), and in every run a
#      structure check: each rule names an alert, an expr, a severity of
#      page or ticket, and a summary;
#   2. the dashboard is one Grafana dashboard: a uid, a datasource variable,
#      unique panel ids, and every panel a timeseries with a PromQL target
#      over that variable;
#   3. every metric either file names is one the gateway exports: its name,
#      without the Prometheus suffixes, is an instrument name of
#      app/ferrofed-server/src, or one of the HTTP server metrics the request
#      log of ferrohealth-telemetry records (HTTP_SERVER), with each `.`
#      written `_`.
#
# Usage:
#   scripts/checks/observability.sh              check the shipped assets
#   scripts/checks/observability.sh --self-test  prove a missing tool skips
#                                                outside CI and fails in it
# Needs jq and yq (mikefarah), and promtool in CI. Outside CI (CI unset), a
# missing jq or yq prints a SKIP naming it and exits 0, and a missing
# promtool leaves the structure check alone; in CI each fails the run, since
# CI installs them. Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly RULES=deploy/observability/ferrofed-alerts.yaml
readonly DASHBOARD=deploy/observability/ferrofed-dashboard.json

# missing TOOL: outside CI, print a SKIP naming TOOL and exit 0; in CI,
# where every tool is installed, fail naming it.
missing() {
  if [[ -n "${CI:-}" ]]; then
    echo "::error::observability: $1 is not on PATH, and CI installs it" >&2
    exit 1
  fi
  echo "SKIP: observability: $1 is not on PATH; install it to run this check"
  exit 0
}

# self_test: runs this script under a PATH without one tool, with CI unset
# and with CI=true, and exits 1 when a run skips or fails the wrong way.
self_test() {
  local work failed=0 script
  script="$PWD/scripts/checks/observability.sh"
  work="$(mktemp -d)"
  # shellcheck disable=SC2064 # expand $work now: the local is gone at EXIT.
  trap "rm -rf '$work'" EXIT
  mkdir -p "$work/nojq" "$work/noyq" "$work/nopromtool"
  printf '#!/bin/sh\nexit 0\n' > "$work/stub"
  chmod +x "$work/stub"
  ln -s "$(command -v dirname)" "$work/nojq/dirname"
  ln -s "$(command -v dirname)" "$work/noyq/dirname"
  ln -s "$work/stub" "$work/noyq/jq"
  ln -s "$(command -v dirname)" "$work/nopromtool/dirname"
  ln -s "$work/stub" "$work/nopromtool/jq"
  ln -s "$work/stub" "$work/nopromtool/yq"

  # expect BIN TOOL CI WANT: a run on PATH=BIN with CI set to CI exits WANT
  # and names TOOL, as a SKIP when WANT is 0.
  expect() {
    local bin="$1" tool="$2" ci="$3" want="$4" status=0 pattern
    CI="$ci" PROMTOOL="" PATH="$work/$bin" "$BASH" "$script" > "$work/out" 2>&1 || status=$?
    pattern="::error::observability: $tool is not on PATH"
    if [[ "$want" -eq 0 ]]; then
      pattern="SKIP: observability: $tool is not on PATH"
    fi
    if [[ "$status" -eq "$want" ]] && grep -q -F "$pattern" "$work/out"; then
      echo "  ok: without $tool, CI='$ci' exits $want naming it"
    else
      echo "  FAIL: without $tool, CI='$ci' exited $status, wanted $want naming it" >&2
      sed 's/^/    /' "$work/out" >&2
      failed=1
    fi
  }
  expect nojq jq "" 0
  expect nojq jq true 1
  expect noyq yq "" 0
  expect noyq yq true 1
  expect nopromtool promtool true 1
  if [[ "$failed" -ne 0 ]]; then
    exit 1
  fi
  echo "observability self-test: OK"
}

case "${1:-}" in
  --self-test)
    self_test
    exit
    ;;
  '') ;;
  *)
    echo "usage: $0 [--self-test]" >&2
    exit 2
    ;;
esac

for tool in jq yq; do
  command -v "$tool" > /dev/null 2>&1 || missing "$tool"
done
promtool="${PROMTOOL:-$(command -v promtool || true)}"
if [[ -z "$promtool" ]] && [[ -n "${CI:-}" ]]; then
  missing promtool
fi

fail=0
bad() {
  local message="$1"
  echo "::error::$message" >&2
  fail=1
}

for file in "$RULES" "$DASHBOARD"; do
  [[ -f "$file" ]] || bad "$file is missing"
done
[[ "$fail" -eq 0 ]] || exit 1

echo "== the rule file"
if [[ -n "$promtool" ]]; then
  if out="$("$promtool" check rules "$RULES" 2>&1)"; then
    echo "OK: promtool: ${out##*$'\n'}"
  else
    bad "promtool check rules refuses $RULES: $out"
  fi
else
  echo "SKIP: promtool is not on PATH; the structure check stands alone"
fi
if ! rules="$(yq -o=json '.' "$RULES")"; then
  bad "$RULES is not YAML"
  rules='{}'
fi
while IFS= read -r fault; do
  bad "$RULES: $fault"
done < <(jq -r '
  if (.groups | type) != "array" or (.groups | length) == 0 then "no groups"
  else .groups[] | .name as $group
    | if (.rules | type) != "array" or (.rules | length) == 0 then "group \($group) holds no rule"
      else .rules[]
        | (.alert // "<unnamed>") as $alert
        | (if (.alert | type) != "string" then "a rule of \($group) names no alert" else empty end),
          (if (.expr | type) != "string" or (.expr | length) == 0 then "\($alert) has no expr" else empty end),
          (if (.labels.severity | IN("page", "ticket")) | not then "\($alert) has no severity of page or ticket" else empty end),
          (if (.annotations.summary | type) != "string" then "\($alert) has no summary" else empty end)
      end
  end' <<< "$rules")
if [[ "$fail" -eq 0 ]]; then
  echo "OK: every rule names an alert, an expr, a severity and a summary"
fi

echo "== the dashboard"
while IFS= read -r fault; do
  bad "$DASHBOARD: $fault"
done < <(jq -r '
  (if (.uid | type) != "string" then "no uid" else empty end),
  (if ([.templating.list[]? | select(.name == "datasource" and .type == "datasource")] | length) != 1
   then "no datasource variable" else empty end),
  (if (.panels | length) == 0 then "no panel" else empty end),
  (if ([.panels[].id] | length) != ([.panels[].id] | unique | length) then "two panels share an id" else empty end),
  (.panels[] | .title as $title
    | (if .type != "timeseries" then "panel \($title) is no timeseries" else empty end),
      (if .datasource.uid != "${datasource}" then "panel \($title) reads no datasource variable" else empty end),
      (if ([.targets[]? | select((.expr | type) == "string" and (.expr | length) > 0)] | length) == 0
       then "panel \($title) has no PromQL target" else empty end))
' "$DASHBOARD")
if [[ "$fail" -eq 0 ]]; then
  echo "OK: every panel is a timeseries with a PromQL target over the datasource variable"
fi

echo "== every metric named is one the gateway exports"
# The instruments the request log of ferrohealth-telemetry records on the
# gateway's meter provider (OpenTelemetry HTTP semantic conventions).
readonly HTTP_SERVER='"http.server.request.duration"
"http.server.active_requests"'
exported="$( { grep -rhoE '"(ferrofed|http\.server)\.[a-z_.]+"' app/ferrofed-server/src; echo "$HTTP_SERVER"; } |
  tr -d '"' | tr '.' '_' | sort -u)"
named="$( { jq -r '.panels[].targets[].expr' "$DASHBOARD"; jq -r '.groups[].rules[].expr' <<< "$rules"; } |
  grep -oE '\b(ferrofed|http_server)_[a-z_]+' | sort -u)"
count=0
while IFS= read -r metric; do
  [[ -n "$metric" ]] || continue
  count=$((count + 1))
  base="$(sed -E 's/_(bucket|sum|count)$//; s/_total$//; s/_seconds$//' <<< "$metric")"
  if ! grep -qxF "$base" <<< "$exported"; then
    bad "$metric names no instrument the gateway exports"
  fi
done <<< "$named"
echo "checked $count metric names"

if [[ "$fail" -ne 0 ]]; then
  echo "observability assets: FAILED" >&2
  exit 1
fi
echo "observability assets: OK"
