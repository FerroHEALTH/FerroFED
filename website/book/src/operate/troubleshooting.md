<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Troubleshooting

This page starts from what you see: a status a client reports, a line in the
log, a metric that moves, a gateway that does not start. Each entry names
the cause, how to confirm it from what the gateway already emits, and the
fix, with a link to the page that explains the setting. No specification
governs troubleshooting: our own design.

The page has four parts. [Where to look](#where-to-look) says what each
signal carries. [Symptoms](#symptoms) covers the failures an operator meets
most. [The gateway does not start](#the-gateway-does-not-start) covers every
refusal of `config check` and `serve`. [Error codes](#error-codes) has one
row for every code the gateway answers with, held to the gateway's own table
by a test.

## Where to look

- **The answer.** Every error the gateway reports itself is the ITS-REST
  `Error` body with a stable `code` and the `request_id`
  ([Errors and status codes](../integrate/errors.md)). A federated query that
  fails under all-or-nothing completeness answers a result set instead, whose
  `meta.federation.endpoints[]` names each node with its `status` and its
  `error`.
- **The request line.** Every request writes one line, `request`, with the
  `method`, the route template as `route`, the `status`, `latency_ms` and the
  gateway's `request_id`: at `INFO` for a success, `WARN` for a `4xx` and
  `ERROR` for a `5xx`
  ([What the log records](configuration.md#what-the-log-records)). Every
  other line about the same request carries the same `request_id`.
- **The log target.** With `telemetry.format = "json"`, each line carries a
  `target`; the `pretty` format leaves it out, so search by the message
  there. Four targets are named on purpose: `ferrofed::security` (client
  authentication, identifier hygiene and the admin listener, never quieted
  by `telemetry.filter`), `ferrofed::integrity` (integrity incidents),
  `ferrofed::audit` (audit records written to the log) and
  `opentelemetry-otlp` (a failed trace export). Every other line's target is
  the Rust module that wrote it, such as `ferrofed_server::reload`,
  `ferrofed_server::access` or `ferrofed_engine::dispatch::reported`, as the
  entries below name it ([Tracing](tracing.md#turning-it-on)).
- **The metrics** on the admin listener: `ferrofed_node_requests_total` by
  `endpoint` and `outcome`, `ferrofed_security_events_total` by `event` and
  `reason`, `ferrofed_overload_refusals_total` by `limit`, the resolver,
  localizer, audit spool and reload series ([Metrics](metrics.md#the-metrics)).
- **The dependencies.** `GET {base}/health/dependencies` names the state the
  gateway last saw of each member endpoint and each identity service: `up`,
  `failing`, `down` or `unknown` ([Health probes](health.md#the-routes)).

## Symptoms

### Every query answers `424`

**Symptom.** Federated queries answer `424` with a result set and no rows.

**Cause.** Under all-or-nothing completeness, one node that answered with an
error fails the whole query, and so does a member the cross-reference
service could not answer for (§11.4, N37).

**Confirm.** Read `meta.federation.endpoints[]` of one answer. A member with
`status` `node-error` names the node's own status and message in `error`,
such as `the node answered 500 Internal Server Error: …`. A member with
`not-resolved` and an `error` is one the resolver failed for; the resolver
counts it as `ferrofed_resolver_requests_total{outcome="unavailable"}`, and
`GET {base}/health/dependencies` reports `resolver` `failing` or `down`. The
failing node counts under `ferrofed_node_requests_total{outcome="node-error"}`.

**Fix.** Fix the node the `error` names, or the cross-reference service
([Identity resolution](identity.md)). To serve the members that answer while
one is down, a client sends `openEHR-federation-completeness: partial`, which
`federation.best_effort` must offer
([Completeness](queries-and-areas.md#completeness)).

### Every node is `offline` or `time-out`

**Symptom.** Queries answer `504`, and every endpoint is `offline` or
`time-out`.

**Cause.** The gateway cannot reach the nodes (a wrong URL in the registry
document, DNS, a firewall, TLS), or they do not answer inside
`federation.per_node_timeout_ms`, or the gateway already has
`federation.max_in_flight_per_node` requests open at each of them.

**Confirm.** The endpoint's `error` reads `the node could not be reached: …`
with the HTTP client's reason, or `no answer before the deadline`. The
dependency state is `down`, and the node counts under
`ferrofed_node_requests_total{outcome="offline"}` or `{outcome="time-out"}`.
A request that waited for a slot is counted by
`ferrofed_overload_refusals_total{limit="node-in-flight", endpoint="…"}`.
Compare `ferrofed_node_request_duration_seconds` with the timeout.

**Fix.** Correct the endpoint `url` in the registry document and reload
([The registry document](registry.md#the-registry-document)), open the path
from the gateway to the node, raise `per_node_timeout_ms` above the node's
99th percentile ([Timeouts](queries-and-areas.md#timeouts)), or size
`max_in_flight_per_node` to what the node can serve
([The per-member cap](overload.md#the-per-member-cap)).

### Nodes answer `401`

**Symptom.** Inside a query, endpoints are `node-error` with `error`
`the node answered 401 Unauthorized`. On a request routed to one node, the
client gets `424 node-refused`.

**Cause.** The node does not accept the gateway's onward credential: the
`[credentials."<endpoint>"]` section is missing or wrong, the node does not
trust the gateway's signing key, or the token endpoint refused the gateway.

**Confirm.** When the gateway could not get a token at all, the endpoint's
`error` is `no onward credential could be obtained, so nothing was sent`,
followed by the token endpoint's RFC 6749 code when it sent one, and the
gateway logs `no onward credential could be obtained, so nothing was sent to
the node` at `WARN` under `ferrofed_engine::dispatch::reported`, with the
`endpoint` and the token endpoint's own account in `error`. A `401` from the
node itself leaves the dependency state `up`, because the node answered.

**Fix.** Set or correct the endpoint's credentials section, its `scope`, its
`token_endpoint` and its `client_id`
([Onward credentials](onward-credentials.md#oauth-20-to-a-node)). The
token endpoint verifies the gateway's client assertion, and a node its
signed caller token, against the gateway's JWK Set at
`{base}/.well-known/jwks.json`; check that each can reach it and holds the
current key
([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)).

### Every caller is refused `401`

**Symptom.** Clients get `401 unauthenticated`, `403 scope-insufficient` or
`403 purpose-of-use-required`, and no node is asked.

**Cause.** The token is not one the `[auth]` configuration accepts.

**Confirm.** Each refusal logs `a request was refused at client
authentication` at `WARN` under `ferrofed::security`, with `event`
`caller-refused`, the `reason` and the `status`, and counts
`ferrofed_security_events_total{event="caller-refused", reason="…"}`. The
`WWW-Authenticate` challenge of the answer names the same reason.

**Fix.** By `reason`: `issuer` is an `iss` no `[[auth.issuer]]` lists;
`audience` is an `aud` other than `auth.audience`; `expired` and
`not-yet-valid` past `auth.clock_skew_s` point at a clock out of step;
`key` is a `kid` the issuer does not publish; `algorithm`, `type` and
`malformed` are tokens that are no RFC 9068 access tokens; `scope` and
`purpose-of-use` are grants the token lacks
([Client authentication](authentication.md#configuration)).

### Every caller is refused `503 authentication-unavailable`

**Symptom.** Every request of one issuer's callers answers `503
authentication-unavailable`.

**Cause.** The gateway cannot fetch or read that issuer's key set, or its
introspection endpoint does not answer.

**Confirm.** `an issuer's key set could not be had, so its tokens cannot be
verified` (`event` `key-set-unavailable`) or `an issuer's introspection
endpoint did not answer, so its tokens cannot be verified` (`event`
`introspection-unavailable`) at `WARN` under `ferrofed::security`, naming
the `issuer` and the fetch `error`.

**Fix.** Make `jwks_uri` or `introspection_endpoint` reachable from the
gateway, or raise `auth.fetch_timeout_ms`
([The key set](authentication.md#the-key-set)).

### Requests are refused `503 overloaded` or `429 rate-limited`

**Symptom.** Clients get `503 overloaded` or `429 rate-limited` with a
`Retry-After`.

**Cause.** The gateway serves `server.max_concurrent_requests` requests at
once, or one caller sent more than `[server.caller_rate]` admits.

**Confirm.** `a request was refused: the gateway serves as many requests as
it takes at once` or `a request was refused: its caller sent more requests
than one caller may`, at `WARN` under `ferrofed_server::overload`, and
`ferrofed_overload_refusals_total{limit="concurrency"}` or
`{limit="caller-rate"}`.

**Fix.** Size the limits to your members and callers
([Overload protection](overload.md#the-concurrency-limit)).

### Requests are refused `503 access-unrecorded`

**Symptom.** Queries and routed reads answer `503 access-unrecorded`.

**Cause.** The gateway could not store the record of the access, so it
withholds the answer.

**Confirm.** `the access could not be recorded, so its answer is withheld`
at `ERROR` under `ferrofed_server::access`, with the cause in `error`. The
spool refused the record when `an audit message was refused: …` naming the
spool's bounds appears at `WARN` under `ihe_iti::atna::spool` and
`ferrofed_audit_refused_total` rises. Two other lines under the same target
point at a defect to report with the `request_id`: `an access was served with
no verified caller, so its answer is withheld` and `an answer of a
patient-data operation carried no access, so it is withheld`.

**Fix.** Free the spool by bringing the audit repository back
([the next entry](#the-audit-spool-grows)), give the spool room
(`spool_max_events`, `spool_max_bytes`) or a faster disk
(`spool_write_timeout_ms`) ([The spool and the failure
policy](audit.md#the-spool-and-the-failure-policy)). A write may have
reached the node before the record failed, so a client reads before writing
again ([Failing closed](audit.md#failing-closed)).

### The audit spool grows

**Symptom.** `ferrofed_audit_spool_events` and `ferrofed_audit_spool_bytes`
rise, and `GET {base}/health/dependencies` reports `audit_repository` or
`audit_feed` `degraded`.

**Cause.** The audit repository is not taking the records, so they wait in
the spool, or it refused some, which wait in the quarantine.

**Confirm.** Under `ihe_iti::atna::forwarder`: `the audit repository could
not be reached; its messages stay spooled` or `an audit message could not be
delivered; the connection is dropped and the message stays spooled` for an
ITI-20 syslog repository, and `an AuditEvent could not be delivered; it
stays spooled` for a FHIR Feed repository, each at `WARN` with
`ferrofed_audit_retries_total` rising. `the audit repository refused an
AuditEvent; it is quarantined` at `ERROR`, with its `sequence` and HTTP
`status`, raises `ferrofed_audit_quarantined`.

**Fix.** Bring the repository back; the spool drains in order once it
answers. Read why a quarantined record was refused, fix that, and remove it
([The audit spool](hardening.md#the-audit-spool)). A spool that fills turns
every access into `503 access-unrecorded`.

### A reload is refused

**Symptom.** After `SIGHUP`, the gateway still serves the old registry.

**Cause.** The new configuration does not load, or changes something only a
restart applies.

**Confirm.** ``registry reload refused, the running registry stays;
`ferrofed config check` names the fault`` at `ERROR` under
`ferrofed_server::reload`, with the failure `class`, and
`ferrofed_registry_reloads_total{result="refused"}`. A changed setting a
reload does not apply logs `changed settings take effect only on a restart;
the running values stay` at `WARN`, naming the keys in `settings`. A listener
certificate that does not read logs `listener certificate reload refused,
the running certificate stays` at `ERROR`.

**Fix.** Run `ferrofed config check` against the same file, which prints the
key at fault, correct it, and send `SIGHUP` again
([Reloading the registry](registry.md#reloading-the-registry)).

### A request is refused `409 ehr-id-collision`

**Symptom.** Reads and writes that name one `ehr_id` answer `409
ehr-id-collision`, and the message lists the endpoints that claim it.

**Cause.** Two members or more claim the same `ehr_id`, a defect in the
federation (§12.5.2, N42).

**Confirm.** `integrity incident: an ehr_id is claimed by more than one
member, and the request was refused` at `ERROR` under `ferrofed::integrity`,
with `kind` `EhrIdCollision`, the `detection` and the `claimants`, and
`ferrofed_integrity_incidents_total{kind="EhrIdCollision"}`.

**Fix.** Have the member that issued or adopted the `ehr_id` in error fix it,
then restart the gateway. Until then a client reaches one member by naming
it in `openEHR-federation-endpoint`
([Integrity incidents](registry.md#integrity-incidents)).

### The operator console shows a refusal

**Symptom.** A view of the operator console shows a status and a code where
its rows should be.

**Cause.** The gateway refused the operator's token. A `403
scope-insufficient` is a token without the issuer's `operator_scope`, which
the `/integrity` and `/stored-queries` views need; a `401` is a token that
does not verify or a session that ended.

**Confirm.** The gateway logs the refusal as `caller-refused` under
`ferrofed::security`, with `reason` `scope` or the `401`'s reason.

**Fix.** Set `operator_scope` on the operator's `[[auth.issuer]]` and have the
provider grant it ([The operator surface](authentication.md#the-operator-surface)).
On a `401`, sign in again. An `/integrity` view that lists no incident is
a gateway that has raised none since it started
([The operator console](operator-console.md#what-is-built)), and no view is
ever silently empty.

### Readiness answers `503`

**Symptom.** `GET {base}/health/readiness` answers `503`, and a load
balancer or the container runtime takes the gateway out.

**Cause.** The body's phase is `booting` before the gateway serves, and
`draining` from the moment `SIGTERM` or `SIGINT` arrives. No member node and
no identity service gates readiness.

**Confirm.** Read the body's phase. `ferrofed healthcheck` prints
`ferrofed: <address>: <outcome>` and exits `1`; a configuration that does
not load prints `ferrofed: not ready: …`.

**Fix.** `booting` lasts until the gateway has bound its listener and
started its bindings. A gateway that cannot start exits instead, with a
`cannot start` or `cannot serve` line
([The gateway does not start](#the-gateway-does-not-start)). A `draining`
gateway was asked to stop; size the grace period
([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).

### The admin listener answers `401`

**Symptom.** Prometheus reports the scrape target down with `401`, or the
stored-query distribution answers `401` or `403`.

**Cause.** `metrics.scrape_token` is set and the scrape does not carry it, or
a write action's caller has no token that verifies or no `operator_scope`.

**Confirm.** `a request to the admin listener was refused at its
authentication` at `WARN` under `ferrofed::security`, with `event`
`scrape-refused` and `reason` `missing` or `scrape-token`, or `event`
`admin-write-refused` and the client-authentication `reason`.

**Fix.** Give the scrape job the token in `authorization`, or give the
operator a token carrying the issuer's `operator_scope`
([Who the admin listener serves](metrics.md#who-the-admin-listener-serves)).

### Every request under `{base}/v1/` answers `501`

**Symptom.** Every ITS-REST path answers `501 not-implemented`.

**Cause.** No registry is configured, so the gateway federates nothing.

**Confirm.** The startup banner's `Registry` line reads `none`, and
`OPTIONS {base}/` answers `501` too.

**Fix.** Set `registry.document`, or `[registry.mcsd]`
([The registry](registry.md#the-registry-document)).

### Every query answers `404 no-destination`

**Symptom.** Queries answer `404 no-destination` and no node is asked.

**Cause.** No registry member is in scope: every endpoint is suspended or
excluded, or the targeting names organisations that manage no endpoint.

**Confirm.** `OPTIONS {base}/` lists each endpoint with its membership
standing.

**Fix.** Correct the membership in the registry document and reload
([The registry document](registry.md#the-registry-document)).

## The gateway does not start

`config check` and `serve` refuse the same configurations. A refusal found
while reading the configuration prints `ferrofed: cannot start:` on stderr,
followed by the fault and each cause after it, separated by `: `, and exits
`78`. A refusal found once the log has started is logged instead, as
`cannot start` at `ERROR` under `ferrofed_server::command` with the chain in
`error`, and also exits `78`. A listener that cannot be bound logs `cannot
serve` with `binding <address>` or `binding metrics.listen <address>` and
exits `1`. No refusal quotes a secret or a patient identifier; each names
the key. Run `ferrofed config check --config <file>` to see the fault without
binding a socket ([Running it](configuration.md#running-it)).

| The line says | Cause and fix |
|---|---|
| `the configuration file … could not be read` | The path is wrong or not readable; check `--config` and `FERROFED_CONFIG` ([Running it](configuration.md#running-it)). |
| `the configuration … is not valid: …` | An unknown key, a value of the wrong type or a TOML syntax fault, at the key, line and column named; `after the environment overrides` points at a `FERROFED__` variable ([The file](configuration.md#the-file)). |
| `the configuration could not be assembled` | The merged configuration could not be written back; report it with the line ([Complaints and incidents](../evaluate/post-market.md#making-a-complaint)). |
| `… names no configuration key` or `… addresses a key under a value that is not a section` | A `FERROFED__` variable names no key; spell it `FERROFED__<SECTION>__<KEY>` ([The environment](configuration.md#the-environment)). |
| `… is deprecated and replaced by …, and both are set` | Keep the new key only ([The compatibility policy](upgrading.md#the-compatibility-policy)). |
| `… is set together with …_file`, `… names …, which could not be read`, `… which holds no secret` | Set a secret inline or in a file, not both, and make the file readable and not empty ([Secrets](configuration.md#secrets-inline-or-from-a-file)). |
| `… is not set, and its section needs it` | Set the named key. |
| `… is not a URL`, `… carries a user name or password`, `… must be an http or https URL …` | Correct the URL; credentials go in their own section ([The file](configuration.md#the-file)). |
| `… is not a socket address`, `… is not a base path`, `… is zero; it must be positive` | Correct the value of the named key ([The base path](configuration.md#the-base-path)). |
| `… is not https, and … would travel over it in cleartext` | A credential or patient identifier would go over plain `http` outside the development profile; use `https` ([What must travel encrypted](configuration.md#what-must-travel-encrypted)). |
| `… is plain http to a host that is not loopback` | An issuer's key set or introspection endpoint is plain `http`; use `https` ([Client authentication](authentication.md#configuration)). |
| `auth.…` with a fault, or `the gateway federates, and [auth] names no [[auth.issuer]]` | Correct `[auth]`; a gateway with a registry lists at least one issuer ([Client authentication](authentication.md#configuration)). |
| `… is not an endpoint id`, `… names …, which is no endpoint of the registry` | A `[credentials]`, `[auth.issuer.patient]`, `federation.demographic_endpoint` or `access_log.retention.origins` key names an endpoint the registry document does not declare ([Onward credentials](onward-credentials.md)). |
| `… names more than one credentials scheme`, `… names no credentials`, `… takes a bearer token or basic credentials, not …`, `… takes a bearer token, basic credentials or an oauth2 client-credentials grant, not …` | One credentials section names exactly one scheme its service takes ([Onward credentials](onward-credentials.md)). |
| `… cannot be sent in the Authorization header`, `… cannot be sent: it holds …` | A bearer token outside the RFC 6750 `b64token` characters, or a basic user or password with a character RFC 7617 forbids ([Running it](configuration.md#running-it)). |
| `… is not a usable OAuth 2.0 client-credentials grant`, `… is not a SMART on openEHR system scope`, `… needs [signing]`, `… names a grant it cannot use`, the FAPI 2.0 and assertion audience faults | Correct the grant ([OAuth 2.0 to a node](onward-credentials.md#oauth-20-to-a-node)). |
| `… is not a space-delimited list of RFC 6749 §3.3 scope-tokens` | An identity service's `scope` ([A PIX Manager behind OAuth 2.0](identity.md#a-pix-manager-behind-oauth-20)). |
| `… is not a usable signing key`, `… holds a key that signs …`, `signing.assertion_lifetime_s is …`, `signing.rotation_overlap_s … must be at least …`, `set [signing] when a registry is configured` | Correct `[signing]` ([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)). |
| `… is not a usable DPoP key` | A P-256 or P-384 PKCS#8 key ([DPoP](onward-credentials.md#tokens-bound-to-a-key-dpop)). |
| `… is not usable in a Nuts grant`, `… names a holder that cannot present`, the Nuts client and DID document faults | Correct the Nuts grant ([The Nuts grant](onward-credentials.md#the-nuts-grant-annex-b-b4)). |
| TLS material or mutual TLS faults, `endpoint … is not an https URL, and credentials.… presents a TLS client certificate` | Correct the certificate, key or CA files, and use `https` with a client certificate ([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705)). |
| The certificate, key or client CA of a listener | Correct `[server.tls]` or `[metrics.tls]` ([TLS on the listeners](configuration.md#tls-on-the-listeners)). |
| `… is read only by ferrofed healthcheck` | Remove `healthcheck_identity_file` where no listener asks for a client certificate ([`ferrofed healthcheck`](health.md#ferrofed-healthcheck)). |
| `… is neither an IP address nor a CIDR block` | Correct `server.trusted_proxies` ([Behind a reverse proxy](public-address.md#behind-a-reverse-proxy)). |
| `server.public_url must be …`, `server.public_url names the path …`, `… is not the route the gateway serves under server.public_url` | Correct `server.public_url`, or remove the key that repeats it ([The public base URL](public-address.md#the-public-base-url)). |
| `telemetry.trace_sample_ratio is …`, `telemetry.filter is not a valid tracing filter`, `… must be an http:// URL` | Correct `[telemetry]` or the OTLP endpoint ([Tracing](tracing.md#turning-it-on)). |
| `metrics.listen is …, which is not a loopback address`, `… and nothing authenticates GET /metrics there`, `… the address server.listen binds`, the metrics exporter faults | Correct `[metrics]` ([Turning it on](metrics.md#turning-it-on)). |
| `server.request_timeout_ms … must exceed federation.overall_timeout_ms …`, `federation.localization.timeout_ms … must be below …`, `pdqm.timeout_ms … plus …`, `nl_gf.mitz.timeout_ms … plus …` | Size the budgets so each part fits inside the overall one ([Timeouts](queries-and-areas.md#timeouts)). |
| `server.shutdown_timeout_ms … must be at least server.request_timeout_ms …` | Raise the drain, or leave it unset ([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)). |
| `federation.fan_out_stored_queries distributes …` | Configure `[stored_queries]` with a backend that stores, or turn distribution off ([Distributing stored queries](queries-and-areas.md#distributing-stored-queries-to-the-members)). |
| `… does not apply to stored_queries.backend = …`, `… is not built into this binary`, `… is not a PostgreSQL connection string …`, `the … stored-query store … could not be opened` | Correct `[stored_queries]` ([Stored queries](queries-and-areas.md#stored-queries)). |
| `the registry document … could not be loaded`, `the registry could not be read from the care services directory`, `set registry.document or [registry.mcsd], not both` | Correct the registry source; `config check` names the rule the document breaks ([The registry](registry.md#the-registry-document)). |
| `set federation.node_selection …`, `set federation.id …`, `the OPTIONS {base}/ self-description cannot be built` | A gateway with a registry declares both ([Node selection](registry.md#node-selection), [Federation id](registry.md#federation-id)). |
| `… are both …; set one`, `… are all …; set one` | Two sections fill one role, such as two resolvers; keep one ([Choosing one](identity.md#choosing-one)). |
| `the [dev] table is not valid`, `the [dev] cross-reference …` | Correct `[[dev.crossref]]`, which needs a registry ([The development cross-reference](identity.md#the-development-cross-reference-dev)). |
| `the [pixm] resolver …`, `pixm.…`, `… needs a cross-reference resolver …` | Correct `[pixm]`; a patient grant and `[pdqm]` need a resolver ([A PIX Manager](identity.md#a-pix-manager-pixm)). |
| `pdqm.…`, `the [pdqm] demographics step cannot be enabled` | Correct `[pdqm]` ([Demographics first](identity.md#demographics-first-pdqm)). |
| `the PMIR …`, `… must be a path that starts with /, …` | Correct `[pmir]` ([The identity feed](identity.md#the-identity-feed-pmir)). |
| `the localizer cannot be set up`, `xcpd.audit = "off" …`, `[xcpd.audit_repository] applies only under …`, `… cannot be a syslog header field` | Correct `[xcpd]` or its audit repository ([XCPD localization](localization.md#xcpd-localization-xcpd)). |
| `nl_gf.nvi.namespaces lists …`, `… is not a credential the NVI takes` | Correct `[nl_gf.nvi]` ([Dutch localization](localization.md#dutch-localization-nl_gfnvi)). |
| `nl_gf.mitz.…`, `the [nl_gf.mitz] consent pre-filter cannot be enabled` | Correct `[nl_gf.mitz]` ([Dutch consent](consent.md#dutch-consent-nl_gfmitz)). |
| `audit.destination = "off" …`, `… = "log" records no caller and no patient …`, `[audit.repository] applies only under …`, `the [audit] trail cannot start` | Set `[audit]` with `destination = "repository"` outside development, and a spool directory the gateway may write ([`[audit]`](audit.md#audit)). |
| `no access log: …`, `[access_log] declares a category map that cannot be used`, `[access_log.retention] declares a retention that cannot be used` | Build with `binding-ihe` and correct `[access_log]` ([`[access_log]`](audit.md#access_log)). |
| `the public document at … would sit inside the ITS-REST surface`, `two public documents would be served at …` | A document the gateway serves, such as the DID document of a Nuts grant, would sit under the base path's ITS-REST surface or on another document's path; change the DID or the base path ([The Nuts grant](onward-credentials.md#the-nuts-grant-annex-b-b4)). |
| `the node clients could not be built`, `the HTTP client … could not be built` | The cause after it names the fault, such as a `[credentials]` section for an endpoint the registry document does not declare ([Onward credentials](onward-credentials.md)); report any other with the line ([Complaints and incidents](../evaluate/post-market.md#making-a-complaint)). |

## Error codes

One row for every code the gateway answers on its own behalf. The query
refusals (`not-aql`, `unreducible` and the rest) are all `400`: each logs `a
federated query was refused before dispatch` at `WARN` under
`ferrofed::security`, `event` `aql-refused`, with its `kind` and byte range,
and the client rewrites the query ([Query refusals](../integrate/errors.md#query-refusals)).
A row's "Confirm" names what the gateway writes beside the request line,
which every row has.

| Code | Status | Cause | Confirm | Fix |
|---|---|---|---|---|
| `body-invalid` | 400 | The body or query string is not the ITS-REST request the route takes. | The request line alone. | The client sends the ITS-REST request ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `completeness-invalid` | 400 | `openEHR-federation-completeness` is repeated or neither `all` nor `partial`. | The request line alone. | The client sends one valid value ([Completeness](queries-and-areas.md#completeness)). |
| `partial-unsupported` | 400 | A client asked for `partial`, and `federation.best_effort` is `false`. | `OPTIONS {base}/` offers no best-effort. | Set `best_effort = true`, or the client drops the header ([Completeness](queries-and-areas.md#completeness)). |
| `dedup-invalid` | 400 | `openEHR-federation-dedup` is repeated or names no mode. | The request line alone. | The client sends `none` or `version-identity` ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `parameter-invalid` | 400 | A query parameter is no AQL literal, or an operator route's `limit` is out of range. | The request line alone. | The client sends a string, number or boolean ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `patient-invalid` | 400 | The patient identifier or namespace forms no patient reference. | The request line alone. | The client names the patient and its namespace ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `no-destination` | 404 | No member is in scope, or every probed member answered `404`. | `OPTIONS {base}/` shows the endpoints' standing. | Correct the registry membership, or the client names an existing EHR ([Every query answers `404`](#every-query-answers-404-no-destination)). |
| `ehr-id-collision` | 409 | Two members claim one `ehr_id`. | `EhrIdCollision` under `ferrofed::integrity`. | The member at fault fixes it, then restart ([Integrity incidents](registry.md#integrity-incidents)). |
| `controlling-system-unreachable` | 409 | A versioned write amends a version another member created. | `a versioned write was refused at a node that does not control the version it amends` at `WARN` under `ferrofed_server::facade::route::ehr`. | The client writes through the member that created the version; correct a `[[creating_system]]` mapping that names the wrong node ([The registry document](registry.md#the-registry-document)). |
| `internal` | 500 | The gateway failed on its own side. | `the federated query failed` or `the routed request failed` at `ERROR`, or `outbound-gate-stopped` under `ferrofed::security`, with the `request_id`. | Report it with the `request_id` and the lines ([Complaints and incidents](../evaluate/post-market.md#making-a-complaint)). |
| `not-found` | 404 | The path is outside the base path and every surface. | The request line shows `<unmatched>`. | Point the client and probes at `server.base_path` ([The base path](configuration.md#the-base-path)). |
| `not-implemented` | 501 | The path is an ITS-REST area the gateway does not serve, or no registry is configured. | The banner's `Registry` line. | Configure a registry, `[stored_queries]` or `federation.demographic_endpoint` ([Every request answers `501`](#every-request-under-basev1-answers-501)). |
| `endpoint-unknown` | 400 | A directive or `openEHR-federation-endpoint` names no registry endpoint. | `OPTIONS {base}/` lists the endpoints. | The client names an endpoint the registry holds ([The registry document](registry.md#the-registry-document)). |
| `organisation-unknown` | 400 | A directive or header names no registry organisation. | `OPTIONS {base}/` lists the organisations. | The client names an organisation the registry holds ([The registry document](registry.md#the-registry-document)). |
| `target-required` | 400 | A write, a new EHR, a definition or a DEMOGRAPHIC request names no node. | The request line alone. | The client names the endpoint in `openEHR-federation-endpoint` ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `endpoint-several` | 400 | A request routed to one node selects several endpoints. | The request line alone. | The client names one endpoint ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `query-parameter-refused` | 400 | A routed request carries a query parameter its operation does not declare. | `query-parameter-refused` at `WARN` under `ferrofed::security`. | The client drops the parameter ([Declared values](configuration.md#declared-values)). |
| `node-timeout` | 504 | The routed node, or a probed member, did not answer in time. | `the routed request failed` at `ERROR`, `code` `node-timeout`; the endpoint's dependency state `down`. | Fix the node, or raise `per_node_timeout_ms` ([Timeouts](queries-and-areas.md#timeouts)). |
| `node-unreachable` | 504 | The routed node, or a probed member, could not be reached. | `the routed request failed` at `ERROR`, `code` `node-unreachable`; the dependency state `down`. | Correct the endpoint `url` or the network path ([Every node is `offline`](#every-node-is-offline-or-time-out)). |
| `node-refused` | 424 | The routed node refused the gateway's onward credentials with `401`. | The node's own log shows the `401`; the endpoint's dependency state stays `up`. | Correct `[credentials."<endpoint>"]` ([Nodes answer `401`](#nodes-answer-401)). |
| `targeting-conflict` | 400 | Two targeting mechanisms select different node sets. | The request line alone. | The client uses one mechanism ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `ehr-id-invalid` | 400 | The path `ehr_id` is no `HIER_OBJECT_ID`. | The request line alone. | The client sends the `ehr_id` a node issued ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `node-error` | 424 | A probed member answered neither a success nor `404`. | The message names the member and its status; a `5xx` makes its dependency state `failing`. | Fix the member the message names ([Health probes](health.md#the-routes)). |
| `probe-requires-uuid` | 400 | An unrouted read names an `ehr_id` that is no bare UUID. | `ehr-id-probe-refused` at `WARN` under `ferrofed::security`. | The client names the endpoint ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `query-name-invalid` | 400 | A stored query name breaks the ITS-REST form. | The request line alone. | The client corrects the name ([Stored queries](../integrate/stored-queries.md)). |
| `query-version-invalid` | 400 | A stored query version is no `major.minor.patch` or prefix. | The request line alone. | The client corrects the version ([Stored queries](../integrate/stored-queries.md)). |
| `query-version-required` | 400 | A definition was `PUT` with no version. | The request line alone. | The client stores at a version ([Stored queries](../integrate/stored-queries.md)). |
| `query-type-unsupported` | 400 | A definition's `query_type` is not `AQL`. | The request line alone. | The client stores AQL ([Stored queries](../integrate/stored-queries.md)). |
| `subject-literal` | 400 | A definition names its patient by a literal. | `definition-subject-literal` at `WARN` under `ferrofed::security`. | The client names the patient through a `$parameter` ([Stored queries](../integrate/stored-queries.md)). |
| `stored-query-held` | 409 | The name and version are already stored. | The request line alone. | The client stores a new version ([Stored queries](../integrate/stored-queries.md)). |
| `stored-query-unknown` | 404 | No stored query at that name and version. | `GET {base}/operator/stored-queries` lists what is held. | Store it, or the client names one held ([Stored queries](queries-and-areas.md#stored-queries)). |
| `preceding-version-invalid` | 400 | A versioned write names no single version it amends. | The request line alone. | The client sends one quoted `OBJECT_VERSION_ID` in `If-Match` ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `parameter-value-invalid` | 400 | A routed value does not match what its operation declares. | `parameter-value-refused` at `WARN` under `ferrofed::security`, with the `carrier`. | The client corrects the value ([Declared values](configuration.md#declared-values)). |
| `media-type-not-acceptable` | 406 | `Accept` admits no media type the operation answers in. | The request line alone. | The client accepts a listed type ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `media-type-unsupported` | 415 | `Content-Type` is no media type the operation takes. | The request line alone. | The client sends a listed type ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `subject-several` | 409 | The subject of `GET {base}/v1/ehr` resolves at several members. | The message lists the endpoints. | The client names one endpoint ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `resolution-unavailable` | 424 | The cross-reference service could not answer, or none is configured. | `the read of an EHR by subject was not served` at `ERROR` under `ferrofed_server::facade::subject::unserved`; `ferrofed_resolver_requests_total{outcome="unavailable"}`; `resolver` not `up`. | Bring the resolver back, or configure one ([Identity resolution](identity.md)). |
| `ehr-id-held` | 409 | A new EHR's `ehr_id` is held at another member. | `a new EHR was refused: another member holds its ehr_id` at `WARN` under `ferrofed_server::facade::route::ehr`, naming the holders. | The client creates it at the holder, or under another `ehr_id` ([Gateway codes](../integrate/errors.md#gateway-codes)). |
| `definition-endpoint-targeted` | 400 | A distributed definition carries a `FROM ENDPOINT` or `ORGANISATION` directive. | The request line alone. | The client stores it without naming members ([Distributing stored queries](queries-and-areas.md#distributing-stored-queries-to-the-members)). |
| `stored-query-fan-out-unsupported` | 400 | A stored-query request names members, and `federation.fan_out_stored_queries` is off. | `OPTIONS {base}/` declares `definition.stored_query_fan_out` as `false`. | Turn distribution on, or the client drops the header ([Distributing stored queries](queries-and-areas.md#distributing-stored-queries-to-the-members)). |
| `stored-query-read-only` | 405 | A `PUT` reached the read-only `files` backend. | `stored_queries.backend = "files"`. | Add the definition as a file and restart ([Read-only: `files`](queries-and-areas.md#read-only-files)). |
| `stored-query-reserved` | 409 | A `PUT` names the reserved namespace `eu.ferrofed.eehrxf`. | The request line alone. | The client uses its own namespace ([The gateway's own queries](../integrate/stored-queries.md#the-gateways-own-queries)). |
| `consent-denied` | 403 | The consent pre-filter denied every member that might hold the subject. | `ferrofed_consent_prefilter_requests_total{outcome="denied"}`. | The patient's consent decides; nothing to fix at the gateway ([Consent](consent.md)). |
| `unauthenticated` | 401 | No token the gateway accepts. | `caller-refused` under `ferrofed::security`, with the `reason`. | By reason ([Every caller is refused `401`](#every-caller-is-refused-401)). |
| `scope-insufficient` | 403 | No granted scope covers the operation, or the client is no demographic client. | `caller-refused`, `reason` `scope` or `demographic-client`. | Grant the scope, or list the client in `demographic_clients` ([Scopes per route](authentication.md#scopes-per-route)). |
| `purpose-of-use-required` | 403 | The token declares no purpose of use. | `caller-refused`, `reason` `purpose-of-use`. | The issuer adds it, or set `auth.purpose_of_use.required` ([Purpose of use](authentication.md#purpose-of-use)). |
| `authentication-unavailable` | 503 | The issuer's key set or introspection endpoint cannot be had. | `key-set-unavailable` or `introspection-unavailable` under `ferrofed::security`. | Make the issuer reachable ([Every caller is refused `503`](#every-caller-is-refused-503-authentication-unavailable)). |
| `operation-refused` | 403 | The request addresses the ADMIN API under `{base}/v1/admin/`. | `caller-refused`, `reason` `operation`. | None: the gateway admits no caller there ([Scopes per route](authentication.md#scopes-per-route)). |
| `localization-unavailable` | 424 | The localizer did not answer under `on_failure = "closed"`, or its XCPD exchange could not be audited. | `the localizer did not answer` at `WARN` or `the localization exchange could not be audited` at `ERROR` under `ferrofed_server::facade::localize`; `ferrofed_localizer_requests_total`. | Bring the localizer or its audit repository back ([Node selection](registry.md#node-selection)). |
| `patient-context-missing` | 403 | Only a bound `patient/` grant covers the operation, and the token has no `ehrId`. | `caller-refused`, `reason` `patient-context`. | The issuer adds the `ehrId` claim ([Patient grants](authentication.md#patient-grants)). |
| `patient-confinement` | 403 | A `patient/` grant reaches beyond its patient. | `patient-confinement` at `WARN` under `ferrofed::security`, or `caller-refused` with `reason` `patient-demographic`. | The client stays inside its patient ([Patient grants](authentication.md#patient-grants)). |
| `patient-context-unavailable` | 424 | The cross-reference could not resolve a `patient/` grant's patient. | `patient-context-unavailable` at `ERROR` under `ferrofed::security`. | Bring the resolver back, and check `ehr_id_system` ([Patient grants](authentication.md#patient-grants)). |
| `subject-unavailable` | 404 | Under `disclose = false`, no member the gateway may read holds the EHR. | The request line alone, by design. | Nothing: the answer hides a consent exclusion ([Withholding consent exclusions](consent-exclusions.md)). |
| `overloaded` | 503 | `server.max_concurrent_requests` requests were in flight. | `limit` `concurrency` under `ferrofed_server::overload`. | Size the limit ([The concurrency limit](overload.md#the-concurrency-limit)). |
| `rate-limited` | 429 | One caller sent more than `[server.caller_rate]` admits. | `limit` `caller-rate` under `ferrofed_server::overload`. | Raise the rate, or the caller slows down ([The per-caller rate](overload.md#the-per-caller-rate)). |
| `access-unrecorded` | 503 | The record of an access could not be stored. | `the access could not be recorded, so its answer is withheld` at `ERROR` under `ferrofed_server::access`. | Free the spool ([Requests are refused `503 access-unrecorded`](#requests-are-refused-503-access-unrecorded)). |
| `natural-person-required` | 401 | A patient-data request's token names no natural person. | `caller-refused`, `reason` `natural-person`. | Use a user token, or declare `client_tokens_act_for_professional` ([Professionals and assurance](authentication.md#professionals-and-assurance)). |
| `authentication-assurance-insufficient` | 401 | The token states no assurance at the issuer's minimum. | `caller-refused`, `reason` `assurance`. | The user authenticates at the level, or correct `[auth.issuer.assurance]` ([The assurance level](authentication.md#the-assurance-level)). |
| `contact-point-attributes-required` | 403 | A national contact point's token lacks an Annex attribute. | `caller-refused`, `reason` `contact-point-attributes`. | The contact point sends every attribute, or correct the claim names ([National contact points](authentication.md#national-contact-points)). |
| `correlation-invalid` | 400 | A national contact point's correlation header is malformed. | `caller-refused`, `reason` `correlation`. | The contact point sends one header of 1 to 128 visible ASCII bytes ([National contact points](authentication.md#national-contact-points)). |
