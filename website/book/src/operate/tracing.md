<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Tracing

The gateway can export its own spans to an OpenTelemetry collector over
OTLP, so a federated query shows up as one trace: the client request at the
root, the resolution and the fan-out under it, and one span per request to a
member node. Each node request also carries a W3C `traceparent` of the
gateway's own trace, so a node that traces joins the same trace. The export
is off by default, and with it
off no node request carries a `traceparent`.

## Turning it on

```toml
[telemetry]
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector; unset, no span is exported
trace_sample_ratio = 1.0                   # the share of requests whose spans are exported, 0.0 to 1.0
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
quieter log never thins a trace.

## Sampling

`trace_sample_ratio` sets the share of client requests whose spans are
exported, from `0.0`, none, to `1.0`, every one. It is `1.0` unless set,
and `serve` and `config check` refuse a value outside `0.0` to `1.0`. Every
trace is the gateway's own, so the decision is made once, at the request
span, from its random trace id, and every span under it follows that
decision: a request's span tree is exported whole or not at all. A node
request of a trace that is not sampled still carries a `traceparent`, with
its sampled flag off (`-00`), so a node that traces knows the gateway did
not keep that trace. Like the export itself, the ratio takes effect on a
restart only.

## The spans

| Span | Under | Attributes |
|---|---|---|
| `{method} {route}`, such as `POST /v1/query/aql` | nothing: the root of the gateway's own trace | `http.request.method`, `http.route` (the route template), `http.response.status_code`, `request_id` (the id the gateway minted) |
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

## The trace context

Every client request starts a trace of the gateway's own, with a random
trace id, as W3C Trace Context §3.4 and §6.1 allow a service to do. The
gateway never reads a client's `traceparent` or `tracestate`: it neither
continues the client's trace nor links to it, and no exported span records
any part of it. A client chooses the trace id it sends, and 32 hexadecimal
characters can encode anything, a patient identifier included
(`3132333435` is `12345`). A trace id taken from a client would carry that
identifier to every node, and a link to the client's span would carry it
into your collector, and the gateway lets no patient identifier into a span
any more than into a node request (§5.4.1, N33). Every trace id and span id
your collector receives is one the gateway generated.

ITS-REST declares no trace header, and a node that does not trace ignores
it (RFC 9110 §5.1). With the export on, each node request carries a
`traceparent` that names the gateway's trace and the span of that node
request as the parent, so a node that traces joins the gateway's trace. No
part of it comes from the client. On a routed request, the client's own
`traceparent` and `tracestate` are stripped like every header the ITS-REST
operation does not declare.
