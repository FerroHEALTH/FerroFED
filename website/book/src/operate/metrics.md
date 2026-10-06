<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Metrics

The gateway counts what it already observes: the requests its clients send
it, by route and status, and how long they take; the requests it sends to
each member node, and to its resolver, localizer and demographics service,
and how long they take; the integrity incidents it raises; the security
events it logs; the requests its limits refuse; and the registry reloads. One OpenTelemetry meter provider holds the
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
scrape_token_file = "/run/secrets/ferrofed/metrics-scrape-token"   # the bearer token a scrape must carry; unset, the scrape is open
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector; unset, nothing is pushed
```

The admin listener serves `GET /metrics` and the operator's
[stored-query distribution](../integrate/stored-queries.md#repairing-drift)
(`POST /admin/stored-queries/{name}/{version}/distribute`), answers every
other path `404`, and never sits under the base path. It is never the
gateway's own listener, so no client of the federation reaches it.

### Who the admin listener serves

| Request | Admitted | Refused |
|---|---|---|
| `GET /metrics`, no `scrape_token` set | every peer | never |
| `GET /metrics`, `scrape_token` set | a scrape with `Authorization: Bearer <the scrape token>` | `401 unauthenticated` with a `WWW-Authenticate: Bearer` challenge, for a scrape with no token, another token or another scheme |
| A write action, such as the distribution | a caller whose access token carries the `operator_scope` of its [`[[auth.issuer]]`](authentication.md#the-operator-surface), from any peer | `401 unauthenticated` without a token that verifies, `403 scope-insufficient` without the operator scope |
| A write action under `profile = "development"` | as above, and a loopback peer (`127.0.0.0/8`, `::1`) that sends no credential | as above |

A write action is verified by the same client authentication as the
gateway's own listener, against the issuers of `[auth]` and in its mode,
and needs no purpose of use. An issuer that names no `operator_scope`
admits no operator, so with none the write actions are refused to every
caller outside the development profile. The scrape token admits the scrape
alone, never a write action. Every refusal is counted under
`ferrofed_security_events_total` as `admin-write-refused` or
`scrape-refused`, and logged under the target `ferrofed::security` with
its reason, never the credential.

Every write action that runs is recorded too: one line under
`ferrofed::security` with `event` `admin-write-admitted`, counted under the
same name. It names the operator by the `issuer` and `subject` of its
token, so the action stays attributable after a restart, with
`admitted_by` (`token`, or `development-loopback` for a loopback peer the
development profile admitted without one, which names no issuer or
subject), the `method`, the route template as `action`, the `status` it
was answered, the time as `at` (RFC 3339) and the `request_id`. The line
never carries the token or a value from the path. Keep these lines as long
as you keep your other security records.

`scrape_token`, or `scrape_token_file` with the token in a file read at
start, sets the scrape token; give one or the other. Prometheus sends it
with `authorization` in the scrape job:

```yaml
scrape_configs:
  - job_name: ferrofed
    authorization:
      type: Bearer
      credentials_file: /etc/prometheus/secrets/ferrofed-scrape-token
    static_configs:
      - targets: ["gateway.example.org:9464"]
```

Mutual TLS authenticates the scrape as well: with `[metrics.tls]` and its
`client_ca_file`, every client of the listener must present a certificate
that CA signed ([TLS on the listeners](configuration.md#tls-on-the-listeners)).
Give the scraper such a certificate with `tls_config` in its job.

`config check` says who a listener that is not on a loopback address
answers. `serve` and `config check` refuse:

- a `listen` address that is not a loopback address, such as `0.0.0.0:9464`,
  unless `allow_remote = true` is set. Set it only when the address is
  reachable from your scraper and your operators and nothing else, for
  example inside a pod network a network policy closes; the policy is
  defence in depth, and holds only where the cluster's network plugin
  enforces it;
- outside `profile = "development"`, a `listen` address that is not a
  loopback address with nothing to authenticate the scrape: neither
  `scrape_token` nor `[metrics.tls] client_ca_file`;
- `scrape_token` and `scrape_token_file` both set, and a
  `scrape_token_file` that cannot be read or holds nothing;
- a `listen` address equal to `server.listen`;
- an `otlp_endpoint` that is not an `http://` URL. The push speaks gRPC
  without TLS, so run the collector beside the gateway, on the same host or
  in the same pod, and let it forward over TLS;
- an `otlp_endpoint` with a user name or a password in it, outside
  `profile = "development"`, since that credential would travel in cleartext
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).

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
| `ferrofed_consent_prefilter_requests_total` | `ferrofed.consent.prefilter.requests` | counter | `outcome`, `reason` | the calls to the [consent pre-filter](consent.md), by `denied`, `no-signal`, `not-asked`, `unavailable` or `partial`; a `not-asked` call, one that never reached the consent service, also carries a `reason`: `namespace` (the patient is named in a namespace the service is not asked by, such as a pseudonymised BSN for Mitz) `caller-claims` (the caller's token does not state the claims the service is asked on behalf of), `caller-claims-invalid` (the token states them in a form the service's question does not take) or `patient-value` (the patient's value is not one the identifier the service is asked by takes) |
| `ferrofed_localizer_requests_total` | `ferrofed.localizer.requests` | counter | `outcome` | the calls to the [localizer](registry.md#node-selection), by `candidates`, `no-records`, `not-configured`, `unavailable`, or `audit-failed` for an XCPD exchange whose audit message could not be recorded |
| `ferrofed_demographics_requests_total` | `ferrofed.demographics.requests` | counter | `outcome` | the calls to the [demographics step](identity.md#demographics-first-pdqm), by `identified`, `no-match`, `ambiguous`, `unavailable`, or `audit-failed` for an exchange whose audit record could not be stored |
| `ferrofed_http_requests_total` | `ferrofed.http.requests` | counter | `http_request_method`, `http_route`, `status_class` | the requests the gateway answered, by the route template and the status class `1xx` to `5xx`, a refused one included |
| `http_server_request_duration_seconds` | `http.server.request.duration` (unit `s`) | histogram | `http_request_method`, `http_route`, `http_response_status_code`, `url_scheme`, and `error_type` on a `5xx` | the time from receiving a request to answering it, the OpenTelemetry HTTP server metric (<https://opentelemetry.io/docs/specs/semconv/http/http-metrics/>) |
| `http_server_active_requests` | `http.server.active_requests` | gauge | `http_request_method`, `url_scheme` | the requests being served now |
| `ferrofed_resolver_requests_total` | `ferrofed.resolver.requests` | counter | `outcome` | the calls to the [cross-reference resolver](identity.md), a PIX Manager or the development cross-reference, one per patient lookup across the members asked, by `resolved`, `not-resolved`, `unavailable` (the service failed for a member) or `time-out` (its budget ran out) |
| `ferrofed_resolver_request_duration_seconds` | `ferrofed.resolver.request.duration` (unit `s`) | histogram | none | the time each resolver call took |
| `ferrofed_localizer_request_duration_seconds` | `ferrofed.localizer.request.duration` (unit `s`) | histogram | none | the time each localizer call took, an XCPD or NVI exchange; a `not-configured` call asks nothing and is not timed |
| `ferrofed_demographics_request_duration_seconds` | `ferrofed.demographics.request.duration` (unit `s`) | histogram | none | the time each PDQm call took |
| `ferrofed_security_events_total` | `ferrofed.security.events` | counter | `event`, and `reason` on `caller-refused` | the security events of the log targets `ferrofed::security`: a caller refused at [client authentication](authentication.md) by its reason, an issuer's key set or introspection endpoint that cannot be had, and every identifier-hygiene event: a query refused before dispatch, a patient predicate stripped, a request the outbound gate stopped, a query parameter or a declared value refused, a probe refused, a stored-query definition refused, a patient grant's confinement, an admin listener write action or scrape refused at the listener's authentication, and an admin listener write action an admitted operator ran |
| `ferrofed_overload_refusals_total` | `ferrofed.overload.refusals` | counter | `limit`, and `endpoint` on `node-in-flight` | the requests a limit refused ([Overload protection](overload.md)) |
| `ferrofed_registry_reloads_total` | `ferrofed.registry.reloads` | counter | `result` | the registry reloads `SIGHUP` asked for |
| `ferrofed_identity_feed_messages_total` | `ferrofed.identity_feed.messages` | counter | `result` | the ITI-93 messages the [identity feed](identity.md#the-identity-feed-pmir) received, by `applied`, `refused` (not held to the PMIR profiles), `unauthenticated` (no feed token) or `audit-failed` (its audit record could not be stored, and nothing was applied) |
| `ferrofed_audit_spool_events` | `ferrofed.audit.spool.events` | gauge | none | the ITI-20 audit messages waiting in the spools for the [audit repository](localization.md#the-audit-repository) and the FHIR Feed repository of [`[audit]`](audit.md), summed; present only with one configured |
| `ferrofed_audit_spool_bytes` | `ferrofed.audit.spool.bytes` | gauge | none | the bytes the spool holds, its quarantine included |
| `ferrofed_audit_quarantined` | `ferrofed.audit.quarantined` | gauge | none | the audit messages in the spool's quarantine, which could not be read or were no whole frame |
| `ferrofed_audit_delivered_total` | `ferrofed.audit.delivered` | counter | none | the ITI-20 audit messages delivered to the audit repository since the process started |
| `ferrofed_audit_retries_total` | `ferrofed.audit.retries` | counter | none | the failed attempts to deliver, each followed by a backoff |
| `ferrofed_audit_refused_total` | `ferrofed.audit.refused` | counter | none | the audit messages the spool refused for want of room under `spool_max_events` or `spool_max_bytes`, the messages queued for a write counted with those stored; each fails its exchange as an audit failure |
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
| `http_route` | a route template of the gateway, such as `/v1/query/aql` or `/v1/ehr/{ehr_id}/composition`, under the base path, or `<unmatched>` for a path no route names; a path identifier is written as its parameter name, never its value |
| `http_request_method` | `GET`, `POST`, `PUT`, `DELETE`, `OPTIONS`, `HEAD`, `PATCH`, `CONNECT`, `TRACE`, or `_OTHER` for any other method |
| `http_response_status_code`, `error_type` | the HTTP status the gateway answered |
| `status_class` | `1xx`, `2xx`, `3xx`, `4xx`, `5xx` |
| `url_scheme` | `http` where the listener speaks plain HTTP and TLS ends in front of it, `https` where it serves `[server.tls]` itself |
| `event` | `caller-refused`, `key-set-unavailable`, `introspection-unavailable`, `aql-refused`, `patient-predicate-stripped`, `subject-parameters-consumed`, `outbound-gate-stopped`, `query-parameter-refused`, `parameter-value-refused`, `ehr-id-probe-refused`, `definition-subject-literal`, `held-definition-refused`, `patient-confinement`, `patient-context-unavailable`, `admin-write-refused`, `scrape-refused`, `admin-write-admitted`: the `event` field of the log line |
| `reason` | on `ferrofed_security_events_total`, the reason the `WWW-Authenticate` challenge names: `missing`, `malformed`, `algorithm`, `type`, `issuer`, `key`, `signature`, `expired`, `not-yet-valid`, `audience`, `inactive`, `unavailable`, `operation`, `scope`, `demographic-client`, `purpose-of-use`, `patient-context`, `patient-demographic` |
| `limit` | `concurrency`, `caller-rate`, `node-in-flight` |
| `le` | a bucket bound in seconds: on the member, resolver, localizer and demographics histograms `0.005`, `0.01`, `0.025`, `0.05`, `0.1`, `0.25`, `0.5`, `1`, `2.5`, `5`, `10`, `30`, `+Inf`; on `http_server_request_duration_seconds` the bounds the OpenTelemetry HTTP conventions advise, `0.005`, `0.01`, `0.025`, `0.05`, `0.075`, `0.1`, `0.25`, `0.5`, `0.75`, `1`, `2.5`, `5`, `7.5`, `10`, with `30` added for the request timeout, and `+Inf` |

The incident, reload and security event counters, and the `concurrency` and
`caller-rate` refusals, show every label value at `0` from the start, so an
alert on their increase works from the first scrape. A node
request series appears with the first request to that endpoint, and an
endpoint a reload removes keeps its series until a restart.

### Which calls the node request series cover

Six kinds of call send a request to a member. Each request one of them
sends is counted once in `ferrofed_node_requests_total` and timed once in
`ferrofed_node_request_duration_seconds`, under the same `endpoint`, so the
histogram's `_count` equals the counter summed over `outcome`. A request
that never left the gateway is in neither: a member that was
`not-resolved`, `excluded` or `not-localized`, and a request the gateway
could not send, for want of a client or a credential, or because the
identifier-hygiene gate withheld it. §11.1 has no status for a request the
gateway could not send, so a member record in `meta.federation` still
reports that member `offline`; the series do not count it. A request whose
deadline passed before it left is not counted either, in any of the six
calls, because the node was never asked. The client still sees the budget
run out: a `time-out` in the member record, or a `504` (`node-timeout`) for
a routed request or a probe. A call the gateway fails with a `500` because
one of its own tasks panicked counts no member at all, since a defect in
the gateway is never a node's `time-out`.

| Call | Requests counted | `outcome` read from | Time recorded |
|---|---|---|---|
| A federated query, `GET` or `POST {base}/v1/query/aql`, and a stored-query invocation | one per member sent the query | its §11.1 status in `meta.federation` | its `latency_ms` |
| A fan-out template upload | one per member sent the upload | its §11.1 status in `meta.federation` | its `latency_ms` |
| A stored-query distribution, a `PUT` naming members or the admin listener's repair | one per member sent the definition | its §11.1 status in `meta.federation` | its `latency_ms` |
| A stored-query drift check, a `GET` naming members | one per member asked for its copy | its §11.1 status in `meta.federation` | its `latency_ms` |
| A request routed to one node: the `{base}/v1/ehr/` area, `GET {base}/v1/ehr?subject_id=…`, a definition request naming one endpoint, a demographic request | one | the node's answer | from sending it to the answer |
| The ask-all probe, `GET /ehr/{ehr_id}` at every member for a read whose owner no earlier step named | one per member probed | the node's answer | from sending it to the answer, or to the moment the overall budget ran out |

The per-member record a federated query, a template upload, a
distribution and a drift check write is the one the client reads in
`meta.federation`, and the counter follows it exactly. A routed request and
a probe have no such record, so their `outcome` is read from the node's
answer by the same rules. Each outcome therefore covers these calls:

| `outcome` | Covers |
|---|---|
| `active` | a member that answered with success; for a routed request or a probe, any answer below `500`, a `404` included, so a probe that finds no EHR at a member is `active` |
| `node-error` | in a member record, any answer that is not a success, a `4xx` included (except a consent refusal the registry names), and a drift check whose copy differs from the registry's definition or is missing; for a routed request or a probe, a `5xx` answer; in every call, a node that refused the gateway's onward credentials |
| `time-out` | a member that gave no answer before its per-node deadline, and a member still being waited on when the overall budget ran out |
| `offline` | a member the gateway sent a request to and could not reach |
| `consent-denied` | a request whose node answered `403` with a consent refusal code the registry lists for it ([Consent](consent.md)): a federated query member, a routed or by-subject read, or an ask-all probe, counted so whether or not the client's answer withholds it ([Withholding consent exclusions](consent-exclusions.md)); a member a consent pre-filter dropped is sent no request and is not counted |

## Dashboard and alert rules

Every release attaches a Grafana dashboard, `ferrofed-dashboard.json`, and a
Prometheus rule file, `ferrofed-alerts.yaml`; both are in the repository
under `deploy/observability/`. Import the dashboard and pick your Prometheus
as its `datasource`: it charts the requests by status class and route, the
members by outcome and answer time, the resolver, localizer and demographics
calls, the security events, the limits' refusals, the integrity incidents
and the audit spool. Load the rule file through `rule_files` in
`prometheus.yml`, or wrap its groups in a `PrometheusRule` for the Prometheus
Operator. Its alerts carry `severity: page` or `severity: ticket`:

| Alert | Fires when |
|---|---|
| `FerroFEDServerErrors` | over 5% of the answers are `5xx` for 10 minutes |
| `FerroFEDSlowAnswers` | the 95th percentile answer takes over 20 seconds for 10 minutes |
| `FerroFEDOverloaded` | the concurrency limit refuses requests for 5 minutes |
| `FerroFEDCallerRateLimited` | a caller is rate limited for 15 minutes |
| `FerroFEDMemberFailing` | a member endpoint fails over 10% of its requests for 10 minutes |
| `FerroFEDMemberCapSaturated` | a member's in-flight cap stays full for 5 minutes |
| `FerroFEDResolverFailing`, `FerroFEDLocalizerFailing` | the resolver or the localizer does not answer for 5 minutes |
| `FerroFEDIntegrityIncident` | an integrity incident is raised |
| `FerroFEDRegistryReloadRefused` | a registry reload is refused |
| `FerroFEDAuditSpoolBacklog`, `FerroFEDAuditRefused` | the audit spool holds over 1000 records for 15 minutes, or refuses one |
| `FerroFEDOutboundGateStopped` | the outbound gate stops a request that would have carried a patient identifier |
| `FerroFEDCallerRefusals` | callers are refused at authentication over once a second for 10 minutes, by reason |
| `FerroFEDAuthenticationUnavailable` | an issuer's key set or introspection endpoint cannot be had |

The thresholds are a starting point; tune them to your traffic. Add an alert
on your scrape job's `up` as well, since a gateway that does not answer the
scrape counts nothing. `scripts/checks/observability.sh` holds both files to
the metrics the gateway exports, and runs `promtool check rules`.

The Kubernetes example (`deploy/kubernetes/`) serves the admin listener on
port `9464` of each pod, with the scrape token in the `metrics-scrape-token`
key of the `ferrofed-secrets` Secret, annotates the pods for a Prometheus
that reads `prometheus.io/scrape`, and opens the port to the Prometheus pods
of the `monitoring` namespace alone with a network policy. Give the
Prometheus job the same token with `authorization.credentials_file`. The
gateway authenticates the scrape and every write action by itself, so the
policy is defence in depth.

The gateway does not call a webhook. An incident is counted, and its log
line under `ferrofed::integrity` carries the routing ids you act on; route
the alert from your Prometheus or your collector.
