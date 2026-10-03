<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Metrics

The gateway counts what it already observes: the integrity incidents it
raises, the requests it sends to each member node and how long they take,
and the registry reloads. One OpenTelemetry meter provider holds the
counts. A Prometheus server scrapes them from `GET /metrics` on an admin
listener of their own, and the gateway can also push them to an
OpenTelemetry collector over OTLP. Both surfaces read the same provider, so
a metric never exists on one and not the other. Both are off by default.

`GET {base}/health/dependencies` stays as it is: it reports the last state
the gateway observed of each member, and the metrics count every request
over time.

## Turning it on

```toml
[metrics]
listen = "127.0.0.1:9464"     # the admin listener; unset, nothing listens
allow_remote = false          # true lets listen name a non-loopback address
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector; unset, nothing is pushed
```

The admin listener serves `GET /metrics` and nothing else, answers every
other path `404`, and never sits under the base path. It has no
authentication, and it is never the gateway's own listener, so no client of
the federation reaches it. `serve` and `config check` refuse:

- a `listen` address that is not a loopback address, such as `0.0.0.0:9464`,
  unless `allow_remote = true` is set. Set it only when the address is
  reachable from your scraper and nothing else, for example inside a pod
  network a network policy closes;
- a `listen` address equal to `server.listen`;
- an `otlp_endpoint` that is not an `http://` URL. The push speaks gRPC
  without TLS, so run the collector beside the gateway, on the same host or
  in the same pod, and let it forward over TLS.

The push sends every 60 seconds; the standard `OTEL_METRIC_EXPORT_INTERVAL`
environment variable, in milliseconds, changes the interval. A push that
fails is logged at `WARN` under the target `opentelemetry-otlp`, and the
counts stay readable on `/metrics`. On `SIGTERM` the gateway pushes once
more after the drain.

`[metrics]` takes effect on a restart only: a [reload](registry.md#reloading-the-registry)
that changes it logs the key as needing a restart, like `[server]`.

## The metrics

The gateway names its instruments the OpenTelemetry way, and the Prometheus
exporter renders each name with `_` for `.`, `_total` after a counter, and
the unit after a histogram.

| Prometheus name | Instrument | Type | Labels | Counts |
|---|---|---|---|---|
| `ferrofed_integrity_incidents_total` | `ferrofed.integrity.incidents` | counter | `kind` | the [integrity incidents](registry.md#integrity-incidents) the gateway emitted, one per incident line under `ferrofed::integrity` |
| `ferrofed_node_requests_total` | `ferrofed.node.requests` | counter | `endpoint`, `outcome` | the requests the gateway sent to a member endpoint |
| `ferrofed_node_request_duration_seconds` | `ferrofed.node.request.duration` (unit `s`) | histogram | `endpoint` | the time a member endpoint took to answer, with the series `_bucket` (and `le`), `_sum` and `_count` |
| `ferrofed_registry_reloads_total` | `ferrofed.registry.reloads` | counter | `result` | the registry reloads `SIGHUP` asked for |
| `target_info` | the resource | gauge | `service_name`, `service_version`, `telemetry_sdk_*` | always `1`: the gateway and its version |

Every label value comes from a closed set or from your registry document,
never from a request, so no patient identifier, query text, header value or
path reaches the surface:

| Label | Values |
|---|---|
| `kind` | `EhrIdCollision`, `IndexInsertCollision`, `LearnedCreatingSystemConflict`, `RegisteredCreatingSystemConflict` |
| `endpoint` | an endpoint `id` of the registry document |
| `outcome` | `active`, `node-error`, `time-out`, `offline`, `consent-denied` |
| `result` | `applied`, `refused` |
| `le` | a bucket bound in seconds: `0.005`, `0.01`, `0.025`, `0.05`, `0.1`, `0.25`, `0.5`, `1`, `2.5`, `5`, `10`, `30`, `+Inf` |

The incident and reload counters show every label value at `0` from the
start, so an alert on their increase works from the first scrape. A node
request series appears with the first request to that endpoint, and an
endpoint a reload removes keeps its series until a restart.

### What a node request's outcome is

A request's `outcome` is the §11.1 status the per-endpoint report of
`meta.federation` gives it, for a federated query, a fan-out template
upload and a stored-query distribution or drift check alike. Only the
statuses of a request the gateway sent are counted, the ones that carry a
`latency_ms`: a member that was `not-resolved`, `excluded` or
`not-localized` was not asked, and is not counted. The counter follows the
report exactly, so a member the report marks `offline` or `node-error` is
counted under that outcome whatever the cause, and a stored-query copy that
drifted from the registry's definition is `node-error` here as it is there.

A request routed to one node, and the ask-all probe that finds the node
holding an `ehr_id`, have no per-endpoint report, so their outcome is read
from the answer the same way: a `5xx` answer, or a node refusing the
gateway's onward credentials, is `node-error`; a timeout is `time-out`; a
node that cannot be reached is `offline`; any other answer, a `404`
included, is `active`. The probe is counted and not timed, because a probe
answer carries no measurement of its own; the duration histogram therefore
counts the fan-out members and the routed requests.

## Alerting

Alert on the counters rather than on the log:

```text
increase(ferrofed_integrity_incidents_total[15m]) > 0
increase(ferrofed_registry_reloads_total{result="refused"}[15m]) > 0
sum by (endpoint) (rate(ferrofed_node_requests_total{outcome!="active"}[5m]))
  / sum by (endpoint) (rate(ferrofed_node_requests_total[5m])) > 0.1
```

The gateway does not call a webhook. An incident is counted, and its log
line under `ferrofed::integrity` carries the routing ids you act on; route
the alert from your Prometheus or your collector.
