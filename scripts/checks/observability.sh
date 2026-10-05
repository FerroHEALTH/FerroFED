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
#      app/ferrofed-server/src with each `.` written `_`.
#
# Usage:
#   scripts/checks/observability.sh
# Needs jq and yq (mikefarah). Exit 1 naming each failure; 0 otherwise.
set -euo pipefail
cd "$(dirname "$0")/../.."

readonly RULES=deploy/observability/ferrofed-alerts.yaml
readonly DASHBOARD=deploy/observability/ferrofed-dashboard.json

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
promtool="${PROMTOOL:-$(command -v promtool || true)}"
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
exported="$(grep -rhoE '"(ferrofed|http\.server)\.[a-z_.]+"' app/ferrofed-server/src |
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
