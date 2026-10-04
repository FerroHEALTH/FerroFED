<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Tracing

The gateway can export its own spans to an OpenTelemetry collector over
OTLP, so a federated query shows up as one trace: the client request at the
root, the resolution and the fan-out under it, and one span per request to a
member node. Each node request also carries a W3C `traceparent`, so a node
that traces joins the same trace. The export is off by default, and with it
off no node request carries a `traceparent`.

## Turning it on

```toml
[telemetry]
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector; unset, no span is exported
```

The export shares the resource of the [metrics](metrics.md) push
(`service.name = "ferrofed"` and the version) and usually points at the same
collector. `serve` and `config check` refuse:

- an `otlp_endpoint` that is not an `http://` URL. The export speaks gRPC
  without TLS, so run the collector beside the gateway, on the same host or
  in the same pod, and let it forward over TLS;
- an `otlp_endpoint` with a user name or a password in it, outside
  `profile = "development"`, since that credential would travel in cleartext
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).

Spans leave in batches. On `SIGTERM` the gateway flushes the spans it still
holds after the drain. An export that fails is logged at `WARN` under the
target `opentelemetry-otlp`; the gateway serves on. The export takes effect
on a restart only: a [reload](registry.md#reloading-the-registry) that
changes it logs `telemetry.otlp_endpoint` as needing a restart.

`telemetry.filter` decides what the console logs and nothing else, so a
quieter log never thins a trace. A trace a client started is sampled as the
client sampled it, and every other trace is sampled.

## The spans

| Span | Under | Attributes |
|---|---|---|
| `{method} {route}`, such as `POST /v1/query/aql` | the client's span, when it sent a `traceparent` | `http.request.method`, `http.route` (the route template), `http.response.status_code`, `request_id` (the id the gateway minted) |
| `localize` | the request | `members` |
| `consent_prefilter` | the request | `members` |
| `resolve` | the request | `members`, `resolved` |
| `fan_out` | the request | `endpoints`, `http.response.status_code` |
| `merge` | `fan_out` | `endpoints`, `rows` |
| `probe` | the request | `endpoints`, `holding` |
| `template_fan_out`, `stored_query_distribution`, `stored_query_drift` | the request | `members` |
| `node_request` | whichever of the above sent it, or the request for a routed request | `endpoint_id`, `operation` (the ITS-REST `operationId`), `contact` (`answered`, `silent` or `unsent`), `http.response.status_code`, `outcome` (the §11.1 status, for a query or a stored definition) |

The export layer adds `target` and the span's busy and idle time to each.
Every value is a route template, an id from your registry document, an
ITS-REST operation, a status or a count. No span carries a patient
identifier, query text, a header value, a body, a token or a path with its
ids, and no `tracing` event is exported: a log line stays in the log. A path
`ehr_id` is not recorded either, because the gateway cannot tell an `ehr_id`
it was handed from a patient identifier, and the route template already says
which resource was read.

## The trace context a node receives

ITS-REST declares no trace header, and a node that does not trace ignores
it (RFC 9110 §5.1). With the export on, each node request carries a
`traceparent` that names the client's trace, when the client sent one, and
the span of that node request as the parent. The gateway reads only the
client's `traceparent`. It never reads or forwards the client's
`tracestate`, which is free text, and on a routed request the client's own
`traceparent` is stripped like every header the ITS-REST operation does not
declare.

A client chooses the trace id it sends, and a random one can hold a short
identifier by chance. So the gateway checks its `traceparent` against the
patient identifier it resolved, as it checks every other part of a node
request. When the identifier occurs in it, the gateway leaves the header off,
logs that it did so at `WARN` without the value, and still sends the request.
