<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Hardening

This checklist takes a gateway from a working configuration to one you can
put in front of real CDRs. Each item names the setting or the deployment
control, and why it matters. The why is the [threat model](../evaluate/threat-model.md),
whose boundaries (B1 to B8) the items cite. No specification governs this
page: our own design.

## Network placement

- [ ] **Put a reverse proxy in front of the gateway, on the same host or in
  the same pod, or serve TLS on the listener.** By default the listener speaks
  plain HTTP, so the hop from the proxy to the gateway carries bearer tokens
  and patient identifiers in the clear (B1). Keep `server.listen` on a
  loopback address, its default `127.0.0.1:8080`, when the proxy runs beside
  it; in a container, bind the address the proxy reaches and nothing wider.
  Where the hop crosses a shared network, set `[server.tls]` with a
  `client_ca_file` that admits the proxy alone, and `[metrics.tls]` for the
  admin listener
  ([TLS on the listeners](configuration.md#tls-on-the-listeners)).
- [ ] **Publish container ports on one address.** A port published on
  `0.0.0.0` is reachable from the network even when the host firewall says
  otherwise ([The quickstart](container.md#the-quickstart));
  `FERROFED_BIND_HOST` in the release `compose.yaml` defaults to
  `127.0.0.1`.
- [ ] **Decide which open routes the network may reach.** The gateway
  answers without a token on the health family, `GET {base}/` and
  `GET {base}/.well-known/jwks.json`, and the PMIR feed route checks its own
  token. `GET {base}/health/dependencies` names every member endpoint and
  its state, and `GET {base}/` the version. At the proxy, keep the health
  routes to your orchestrator and monitoring, the JWK Set reachable from
  every node's authorization server, and the feed route reachable from the
  PMIR Registry (B1, B3).
- [ ] **Restrict ingress.** Admit client traffic to the gateway port from
  the proxy alone, and the admin port from your scraper and your operators
  alone. The Kubernetes example's NetworkPolicy admits the scraper alone to
  the admin port (`deploy/kubernetes/networkpolicy.yaml`); it holds only
  where the cluster's network plugin enforces NetworkPolicy (B5).
- [ ] **Restrict egress.** The gateway connects only to the URLs in your
  configuration: the members and their token endpoints, the identity
  services, the issuers' key sets, the audit repository and the collector.
  Allow those and nothing else. The Kubernetes example leaves egress open
  (B2, B3, B7).

## TLS

- [ ] **Terminate TLS at the proxy** with the protocol versions and cipher
  suites of RFC 9325 (BCP 195). Set `Strict-Transport-Security` (RFC 6797)
  there for the gateway and for the operator console, which sends none of
  its own (B1, B4).
- [ ] **Keep the production profile.** Outside `profile = "development"`,
  the gateway refuses to start when a credential or a patient identifier
  would travel over plain `http`, and a stored-query database password
  without `sslmode=require`. The startup banner and a `WARN` line name each
  unencrypted credential the development profile admits
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).
- [ ] **Use `https` for every node that receives a credential**, and mutual
  TLS where a node or a national service asks for it: `client_identity_file`
  and `trust_roots_file` on the endpoint's credentials or the service's
  table ([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705),
  [Mutual TLS to the identity services](configuration.md#mutual-tls-to-the-identity-services)).
- [ ] **Send the IHE audit over TLS.** The FHIR Feed URL is `https` and the
  ITI-55 syslog repository is TLS outside development
  ([The audit trail](audit.md)).
- [ ] **Run the OpenTelemetry collector beside the gateway.** The metrics
  push and the span export are gRPC without TLS; let the collector forward
  over TLS ([Tracing](tracing.md),
  [#644](https://github.com/FerroHEALTH/FerroFED/issues/644)).

## Callers and issuers

- [ ] **Trust as few issuers as you can.** Every token an issuer signs is
  admitted for the scopes it carries, so an issuer you list is trusted for
  every caller it vouches for (B7). Give each `jwks_uri` or
  `introspection_endpoint` over `https`.
- [ ] **Name the privileged clients per issuer.** `backend_clients` for
  `system/aql-*`, `demographic_clients` for the DEMOGRAPHIC API, and an
  `operator_scope` only on the issuer your operators sign in at
  ([Client authentication](authentication.md)).
- [ ] **Require an assurance level per issuer.** Set
  `[auth.issuer.assurance]` on every issuer whose tokens reach patient data,
  with the values it writes for each level and the least level you accept;
  `config check` names each issuer without one. Declare
  `client_tokens_act_for_professional` only for an issuer whose client
  tokens name the professional they act for
  ([Professionals and assurance](authentication.md#professionals-and-assurance)).
- [ ] **Keep `auth.purpose_of_use.required = true`**, its default, unless
  your §13.4 decisions say why not
  ([Purpose of use](authentication.md#purpose-of-use)).
- [ ] **Bind a patient grant only with a cross-reference you trust.** An
  `[auth.issuer.patient]` binding confines a `patient/` grant through your
  identity service's links; leave it unset otherwise
  ([Patient grants](authentication.md#patient-grants)).
- [ ] **Use the edge mode only with a signing proxy.** `auth.mode = "edge"`
  verifies a signed assertion and nothing else; never configure a proxy that
  forwards an unsigned identity header (RFC 7239 §8.1).
- [ ] **Know how to remove an issuer.** A change to `[auth]` takes a restart
  today ([#636](https://github.com/FerroHEALTH/FerroFED/issues/636)); write
  the steps into your incident runbook.

## Secrets

- [ ] **Read every secret from a file.** Each credential has a `_file` key:
  `bearer_token_file`, `password_file`, `client_secret_file`,
  `client_identity_file`, `key_file`, `feed_token_file`,
  `scrape_token_file`. Setting a secret both inline and by file is refused
  ([Configuration](configuration.md#running-it)).
- [ ] **Make each file readable by the gateway's user alone.** The image
  runs as `65532:65532`; mount the files read-only with mode `0400`, or
  `0440` with the gateway's group, as the Kubernetes example does
  ([The container image](container.md#the-gateway-from-a-release)).
- [ ] **Keep secrets out of the environment and the configuration file.** A
  `FERROFED__…_FILE` variable names a file, never the secret.
- [ ] **Give each endpoint its own credential.** The gateway sends an
  endpoint's credential to that endpoint alone; one shared credential lets
  one node replay it at another (B2).
- [ ] **Bind onward tokens where the node allows it**, with DPoP
  (`dpop_key_file`) or mutual TLS (RFC 8705), so a token taken from a node
  cannot be replayed from elsewhere
  ([Onward credentials](onward-credentials.md)).

## Signing keys and rotation

- [ ] **Generate the signing key for the nodes you serve.** P-384 signs
  ES384; choose P-256 when a node holds to the FAPI 2.0 Security Profile
  ([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)).
- [ ] **Set `node_jwks_cache_s` to the longest time a node caches the JWK
  Set.** `config check` then refuses a `rotation_overlap_s` too short for a
  safe rotation.
- [ ] **Rotate in three rolling restarts:** publish the new key as
  `next_key_file`, sign with it as `key_file` with the old one as
  `previous_key_file`, then retire the old one
  ([Rotating the signing key](onward-credentials.md#rotating-the-signing-key)).
  Rotate on a schedule, and at once when a key may have leaked.
- [ ] **Rotate onward credentials and the PMIR feed token** with the party
  that issued them; each is read at start, and a registry reload re-reads
  the endpoint credentials.

## The admin listener

- [ ] **Leave `[metrics] listen` unset unless you scrape.** Unset, nothing
  listens.
- [ ] **Keep it on loopback.** Set `allow_remote = true` only when the
  address is reachable from your scraper and your operators and nothing
  else ([Metrics](metrics.md#who-the-admin-listener-serves)).
- [ ] **Authenticate the scrape off loopback.** Set `scrape_token_file` to
  a file holding a long random token, such as `openssl rand -hex 32`, and
  give your Prometheus the same token with `authorization.credentials_file`;
  or set `[metrics.tls]` with a `client_ca_file` that admits the scraper.
  Outside the development profile the gateway refuses to start with a
  listener off loopback and neither. Rotate the token with your scraper:
  both read it at start.
- [ ] **Name an `operator_scope` on the issuer your operators use**, and
  grant it to them alone. A write action, such as the stored-query
  distribution, needs a token carrying it from every peer, loopback
  included; an issuer that names none admits no operator
  ([The operator surface](authentication.md#the-operator-surface)).
- [ ] **Never run a production gateway under `profile = "development"`.**
  It admits any process that reaches the listener's loopback address to the
  write actions without a credential.

## The audit spool

- [ ] **Send the records to your Audit Record Repository.** A gateway with
  a registry records every access to patient data with the caller, so
  outside `profile = "development"` it needs `[audit] destination =
  "repository"` with an `[audit.repository]` table: `config check`, `serve`
  and a reload refuse an unset destination, `off` and `log`, naming
  `audit.destination`. The `log` destination names no caller and no patient,
  so it cannot hold the access records (B6,
  [The access log](audit.md#the-access-log)).
- [ ] **Give each replica a spool of its own on durable storage.** Two
  gateways must never drain one directory, and a lost spool loses records
  that name patients for good. Use a named volume under Docker and a
  persistent volume claim per replica under Kubernetes, as the StatefulSet
  example does ([Losing a spool](container.md#losing-a-spool)).
- [ ] **Encrypt the volume at rest.** The gateway holds no key to encrypt the
  spool, and the records in it name patients and callers. Replace the
  example's `encrypted` storage class with one of yours that encrypts.
- [ ] **Leave the directory modes alone.** The gateway creates the spool
  `0700` with files `0600` and refuses to start when it is open to others.
- [ ] **Watch the backlog.** Alert on `FerroFEDAuditSpoolBacklog` and
  `FerroFEDAuditRefused`; a full spool fails the transactions it cannot
  record, and the gateway then answers every access to patient data `503
  access-unrecorded`. Remove a quarantined record only after you have read why the
  repository refused it ([The audit trail](audit.md)).

## Identity services

- [ ] **Use `https` and, where offered, mutual TLS** to the PIX Manager,
  the PDQm Supplier, the XCPD gateways, the NVI and Mitz (B3).
- [ ] **Agree the PMIR feed token with the Registry's operator** and keep it
  in `feed_token_file`; a message without it changes nothing.
- [ ] **Set `federation.binding_ttl_ms`** no longer than you would accept a
  follow-up routed on a superseded identity
  ([Resolution bindings](registry.md#resolution-bindings)).

## Resource limits

- [ ] **Size the overload limits from what the members can take.**
  `server.max_concurrent_requests` (512), `[server.caller_rate]` (off by
  default), `federation.max_in_flight_per_node` (64) and
  `federation.max_node_answer_bytes` (16 MiB)
  ([Overload protection](overload.md)). Turn the per-caller rate on.
- [ ] **Keep the request limits.** `server.body_limit_bytes` (1 MiB) and
  `server.request_timeout_ms` (30 s), with the proxy's own timeout longer,
  so the client sees the gateway's answer.
- [ ] **Rate-limit `/login` on the console at the edge.** The console bounds
  its pending sign-ins and sessions and cannot tell clients apart behind a
  balancer ([The operator console](operator-console.md)).
- [ ] **Set container limits.** CPU, memory and ephemeral storage, as the
  release `compose.yaml` and the Kubernetes example do.
- [ ] **Let the drain finish.** Keep the runtime's grace period above
  `server.drain_delay_ms` plus `server.shutdown_timeout_ms` plus
  `server.bindings_drain_timeout_ms`
  ([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).

## Logs, metrics and traces

- [ ] **Ship the JSON log to a store with access control and a retention
  period.** The log carries no patient identifier, but it does carry routing
  ids, `ehr_id`s in integrity incidents, and the security events you
  investigate from. Record the retention in your
  [records of processing](../evaluate/data-protection.md#retention).
- [ ] **Load the shipped alert rules** (`ferrofed-alerts.yaml`) and route
  their pages: caller refusals, an unavailable issuer, a request the
  outbound gate stopped, an integrity incident, the audit spool
  ([Dashboard and alert rules](metrics.md#dashboard-and-alert-rules)). Add
  an alert on the scrape job's `up`.
- [ ] **Treat an `outbound-gate-stopped` event as an incident.** It means a
  request would have carried a patient identifier to a node and was stopped.
- [ ] **Do not raise the log filter to `debug` in production** unless you
  are debugging, and return it after; `telemetry.filter` changes what is
  logged, never what is traced.

## Operator console

- [ ] **Keep `secure_cookie = true`.** The console refuses `false` unless
  `redirect_uri` is on a loopback host.
- [ ] **Register the console as a confidential client** with its secret in
  `client_secret_file`, and an exact `redirect_uri` at the provider.
- [ ] **Run one replica, or a balancer that keeps an operator on one
  replica.** Sessions live in the console's memory.
- [ ] **Give operators the operator scope only.** The console calls the
  gateway with the operator's own token, so that token's scopes are what the
  console can do.

## The container

- [ ] **Run the published image unchanged.** It is distroless and has no
  shell; it runs as `65532` with a read-only root filesystem, every
  capability dropped, and writes only to the spool volume
  ([The image](container.md#the-image)). Keep `runAsNonRoot`,
  `allowPrivilegeEscalation: false` and the `RuntimeDefault` seccomp
  profile, as the Kubernetes example sets them.
- [ ] **Verify what you run.** Pin the image by digest and check its
  attestation with `gh attestation verify`, and verify a release tarball the
  same way ([The container image](container.md#kubernetes)).
- [ ] **Never run the development profile near real data.** It admits the
  static cross-reference and cleartext credentials, and its banner says so
  in red.
- [ ] **Check every configuration before it ships.** Run
  `ferrofed config check` in your deployment pipeline; a refused file exits
  `78` naming the key.
- [ ] **Take security fixes from the latest release.** Fixes go to the
  latest release only ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md)).
