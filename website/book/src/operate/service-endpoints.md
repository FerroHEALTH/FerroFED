<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Service endpoints and their TLS

**Audience:** operators who connect the identity services, the audit repository and the other services the gateway calls. Every such service is one endpoint table; this page sets its address, its timeouts, its credentials and its TLS. [Configuration](configuration.md) covers the file as a whole.

## The endpoint table of a service

Every outbound service a binding names is written as one `endpoint` table,
the `EndpointSection` of `ferrohealth-smart` that every FerroHEALTH product
reads for an outbound integration: `[pixm.manager.endpoint]` under each
`[[pixm.manager]]`, `[pdqm.endpoint]`, `[pmir.endpoint]`,
`[registry.mcsd.endpoint]`, `[xcpd.gateway.endpoint]` under each
`[[xcpd.gateway]]`, `[nl_gf.nvi.endpoint]` and `[nl_gf.mitz.endpoint]`, as
`[audit.repository.endpoint]` and `[eehrxf.terminology.endpoint]` are:

```toml
[pdqm.endpoint]
url = "https://pdq.example.org/fhir/"   # required
timeout = "1s"                          # one whole call, every try and wait included
# connect_timeout = "500ms"             # opening the connection; unset, timeout bounds it
# retry = { max_attempts = 1, initial_backoff = "200ms", max_backoff = "5s" }

[pdqm.endpoint.tls]
client_identity_file = "/run/secrets/pdq-client.pem"

[pdqm.endpoint.credentials]
user = "ferrofed-pdq-consumer"
password_file = "/run/secrets/pdq-password"
```

- `url` is the service's base. It carries no credential: one with a user
  name or a password in it is refused naming the key, as an endpoint URL in
  the registry document is.
- `timeout` bounds one call. Each table keeps its own default, so a key you
  leave out keeps the value the service had before it moved under
  `endpoint`: `1s` for `[pdqm]` and `[nl_gf.mitz]`, `5s` for `[pmir]`, and
  `10s` for the others.
- `retry` tries a call again when the service could not be reached, the try
  timed out, or it answered `5xx`. Each identity, localization and consent
  table tries a call once unless you raise `max_attempts`, so a failure
  fails the step at once within the query's budget.
- `tls` is the TLS material of [Mutual TLS to the identity
  services](#mutual-tls-to-the-identity-services).
- `credentials` takes a user and a password, or the `oauth`
  client-credentials grant authenticated by a client secret or a client
  assertion ([Identity resolution](identity.md#a-pix-manager-behind-oauth-20)),
  never a `nuts` grant. Each grant a node's section can name is described in
  [Onward credentials](onward-credentials.md). Mitz takes the same: a user
  and a password, or the `oauth` grant, whose token each question carries
  as a bearer token. A grant a table does not take is refused at load,
  naming it, and never read as no credential. `[nl_gf.nvi.endpoint.credentials]`
  takes a user and a password and refuses `oauth`; the NVI's `nuts` grant
  sits in `[nl_gf.nvi.nuts]`
  ([Dutch localization](localization.md#dutch-localization-nl_gfnvi)). An
  XCPD responding gateway takes no credential: the XUA assertion is the
  gateway's, and a `[xcpd.gateway.endpoint.credentials]` is refused.

When a service refuses a token with `401`, the gateway drops the token and
sends the call once more with a fresh one, and a second `401` fails the
call (RFC 6750 §3.1).

## A PIX Manager asked by `GET` or `POST`

ITI-83 is an HTTP `GET` with the parameters in the URL: "the HTTP GET
operation shall be used" (PIXm 3.1.0 §2:3.83.4.1.2), and FHIR R4 requires a
server to support that `GET` (FHIR R4 Operations §3.2.0.1). The patient
identifier is then in the request URL, where a proxy, a load balancer or the
Manager's access log can record it. `method = "get"` is the default, because
every PIXm Manager answers it.

FHIR R4 invokes an operation "generally" by a `POST` of a `Parameters`
resource to the operation's endpoint (FHIR R4 Operations §3.2.0.1), and the
PIXm IG gives the request in that form (its Query Parameters In profile and
request example). With `method = "post"` the gateway posts the same
`sourceIdentifier` and `targetSystem` parameters as `valueString`s, with
`Content-Type: application/fhir+json`, to `[base]/Patient/$ihe-pix`, and the
request URL carries no parameter. PIXm does not require a Manager to accept a
`POST`, so set it only for a Manager that says it does:

```toml
[[pixm.manager]]
method = "post"   # the patient identifier travels in the body, never the URL

[pixm.manager.endpoint]
url = "https://pix.example.org/fhir/"
```

Any other value is refused naming the key. The answer is read the same way
under either method, and the audit record of each exchange holds the request
as sent: the URL of a `GET`, or the request line, media type and body of a
`POST` ([The audit trail](audit.md)).

## Mutual TLS to the identity services

A national identity service often authenticates the gateway by its client
certificate instead of, or beside, a password or a token. Every outbound
service takes its TLS material in the `tls` table of its `endpoint`
([The endpoint table of a service](#the-endpoint-table-of-a-service)), and a
node in the `tls` sub-table of its `[credentials."<endpoint id>"]`:

```toml
[[pixm.manager]]

[pixm.manager.endpoint]
url = "https://pix.example.org/fhir/"

[pixm.manager.endpoint.tls]
client_identity_file = "/run/secrets/pix-client.pem"  # mutual TLS
trust_roots_file = "/etc/ferrofed/pix-roots.pem"      # optional
# min_version = "1.2"                                 # "1.3" unless lowered
```

- `client_identity`, or its `client_identity_file` sibling, holds the PEM
  client certificate chain and its private key, which the gateway presents
  in the TLS handshake. It is a secret: it shows as `***` wherever the
  configuration is rendered, and setting both forms is refused naming
  `<table>.endpoint.tls.client_identity`.
- `trust_roots_file` names a PEM bundle of trust roots the service's
  certificate chains to, added to the platform's roots.
- `min_version` is the oldest TLS version the gateway speaks to the
  service: `"1.3"` unless lowered to `"1.2"`. Lower it only for a service
  that speaks no TLS 1.3, such as a MyHealth@EU responding gateway
  ([Localization](localization.md#a-myhealtheu-route)).
- Every file is read at boot and on each reload of a reloadable table. A
  file that cannot be read, or material that holds no certificate and key
  in PEM, is refused naming its key, without quoting the file.
- The keys sat on the table itself before release 0.0.18; set there, they
  are refused as unknown keys ([Upgrading](upgrading.md)).

Each of these services is reached through the outbound client of
`ferrohealth-smart` its endpoint table builds, with no redirect followed, the
credential marked sensitive, and the TLS material above. A MyHealth@EU
responding gateway's client completes a connection only over a chain its
revocation check cleared ([Localization](localization.md#a-myhealtheu-route)).

## What must travel encrypted

Every outbound connection in the configuration is held to one of three
rules, by what it carries.

**Credentials and patient identifiers.** Outside `profile = "development"`,
a URL that a configured credential or a patient identifier is sent to must
be `https`. `serve`, `config check`, `admission check`, `conformance run`
and every [reload](registry.md#reloading-the-registry) refuse anything else, with exit
code 78 and one line naming the URL's key and what would travel over it,
never a value. The rule covers:

- the URL of a registry endpoint that has a `[credentials."<id>"]` section,
  which receives its basic credentials or the access token its `oauth`
  grant obtains;
- the `token_endpoint` of an `oauth` section, which receives the client
  secret or the client assertion;
- the `authorization_server` of a `nuts` section, which receives the
  gateway's credentials in a Verifiable Presentation;
- the `issuer` of a node's `oauth` section, whose token endpoint receives
  the client assertion and, under an `onward` table, the callers' tokens;
- the `endpoint.url` of every PIX Manager, which is asked for patient
  identifiers with or without `[pixm.manager.endpoint.credentials]`;
- the `endpoint.url` of `[pdqm]`, the PDQm Supplier, which is sent the
  patient identifier with or without `[pdqm.endpoint.credentials]`;
- the `endpoint.url` of every XCPD responding gateway, which is sent the
  patient identifier and, when one is configured, the XUA assertion;
- the `endpoint.url` of `[nl_gf.nvi]`, which is sent the pseudonymised BSN
  and, when one is configured, its credentials;
- `telemetry.otlp.endpoint` when it carries a user name or a password; an
  `https://` collector meets the rule.

```text
ferrofed: cannot start: the url of endpoint hospital-a in registry.document is not an https URL, and credentials.hospital-a would travel over it in cleartext: outside profile = "development" a credential or a patient identifier is sent only encrypted
```

A node URL no credential is sent to may stay `http`, for example a node on a
private network whose transport a sidecar protects with mutual TLS: the
gateway sends a node its own `ehr_id`, never the patient identifier (§5.4,
N33). A node the gateway presents its own client certificate to
([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705))
is `https` under every profile.

**A database password.** FerroFED's own store is reached over TLS under
every profile: its connection string must set `sslmode=require`, or
`config check`, the start and a reload refuse it, naming `store.url`
([FerroFED's own store](store.md#tls-is-required)). Outside the development
profile, the revision store's connection string, `config.url`, must set
`sslmode=require` when it carries a password and reaches its host over the
network. The driver reads `sslmode` as `disable`, `prefer` (its default,
which falls back to no TLS) or `require`, and refuses any other value, so
`disable` and `prefer` are refused, naming `config.url`, by the same
commands and on a reload. A revision store connection with no password, or
whose every host is a Unix socket and that names no `hostaddr`, crosses no
network with a secret and may leave `sslmode` as it is
([Configuration revisions](configuration-revisions.md)).

Under the development profile the same configuration starts, so the
[quickstart](container.md#the-quickstart) can reach its nodes over `http`
inside its Docker network. Each credential or patient identifier that
travels unencrypted is named by key in the startup banner, in a `WARN` log
line at boot and after each reload, and on stderr by `config check`.

**Trust anchors.** A URL the gateway verifies its callers against, an
issuer itself, the key set its metadata names, or an introspection endpoint
in [`[auth]`](authentication.md), must be `https`, or `http` to a loopback
host, under every profile, development included. A key set fetched in the
clear would let anyone on the network substitute keys and forge callers.

**The profile takes a restart.** A reload whose file changes `profile` is
refused (class `profile`), and the running configuration stays. Every
decision the development profile admits, these transport rules and an
`http` XCPD gateway, reads the profile the process started with.

The specification assumes a protected transport and leaves it to the
security profiles (§2.2, §13); no specification governs these rules, which
are FerroFED's own design.
