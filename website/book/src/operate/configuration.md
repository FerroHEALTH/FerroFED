<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Configuration

The `ferrofed` binary reads one TOML file and the environment. It serves the
process shape (health, readiness, the request log, graceful shutdown) and, once
a registry is configured, the federated query `POST {base}/v1/query/aql`, the
EHR resources and the definition area routed to one node, and, when
`[stored_queries]` is set, the stored-query registry. The DEMOGRAPHIC area
answers `501` unless `federation.demographic_endpoint` declares the one
endpoint that serves it, which a request reaches without a targeting header,
and every other path under
`{base}/v1/` answers `501`. `{base}` is `/` unless you set
[the base path](#the-base-path).

Three pages carry the rest of the configuration: [the registry](registry.md),
its document and the state learned from it; [identity
resolution](identity.md), the cross-reference that finds each member's
`ehr_id`; and [queries and API areas](queries-and-areas.md), the settings a
federated query and each ITS-REST area run under. [What the gateway sends a
node](node-requests.md) lists the id and the headers of each request to a
node.

## Running it

```text
ferrofed serve --config /etc/ferrofed/ferrofed.toml
ferrofed config check --config /etc/ferrofed/ferrofed.toml
ferrofed healthcheck --config /etc/ferrofed/ferrofed.toml
ferrofed report --config /etc/ferrofed/ferrofed.toml
```

`--config` names the file; without it the file is the one `FERROFED_CONFIG`
names, and without that every default stands. `config check` reads and
resolves the configuration exactly as `serve` would, secrets included, loads
a read-only stored-query directory, prints one line and exits, so a deployment
pipeline can test a file without binding a socket. It opens no store file,
connects to no database and writes nothing to disk, so it runs on a
read-only root filesystem without the gateway's durable volume. An audit
spool directory is checked where it stands and never created: one that
exists must give its owner write access and its group and other users none,
and hold no file the gateway did not write; one that does not exist needs a
parent directory that does, whose mode lets a user write in it. Only the
modes are read, so whether the gateway's own user may write there, and on a
filesystem mounted writable, is left to `serve`, which creates the directory
and refuses to start, naming its key, when it cannot. `ferrofed report`
writes the archive a complaint or a serious-incident report attaches,
described on the
[Complaints and incidents](../evaluate/post-market.md#the-report-archive) page.
`ferrofed admission check --endpoint <id>` checks
one member against the admission conditions ([Admitting a node](admission.md)).
`healthcheck` asks the gateway running on this host for its readiness and
exits `0` or `1`, the two codes a container runtime's health check reads
([Health probes](health.md#ferrofed-healthcheck)).

A configuration the gateway refuses exits with code 78 (`EX_CONFIG`) and one
line naming the key at fault. It refuses an unknown key, a value of the wrong
type, a zero timeout or body limit, a log filter that does not parse, a secret
set both inline and through its `_file` sibling, a credentials section
that names no scheme or two, and a credential the `Authorization` header
cannot carry: a basic user or password holding a control character such as a newline (RFC 7617
§2), or a basic user holding a colon. That refusal names the key the value came from, and never
the value. The gateway never falls back to a default for a value you set.

A key a release renamed is still read under its old name for at least one
release. `config check` prints a `warning:` line for it, naming the key that
replaces it and the release that will refuse it, and `serve` logs the same
at start; setting both the old and the new key is refused
([the compatibility policy](upgrading.md#the-compatibility-policy)). No key
is deprecated in this release.

With a FHIR face (`[fhir]`), `config check` also prints a `note:` line for
each Art 14(1) category the face receives, naming the member its
`[[fhir.receive]]` entry declares, and one line naming the categories this
release carries that no member receives. These are the categories
`GET {fhir-base}/metadata` names
([Receiving a document](../integrate/patient-summary.md#receiving-a-document)).
A configuration with no `[fhir]` gets neither line.

```text
ferrofed: note: [[fhir.receive]] Patient-Summaries: the FHIR face receives documents of this Art 14(1) category and writes them to the member node-a
```

A face with no `[[fhir.receive]]` entry prints this line instead:

```text
ferrofed: note: [[fhir.receive]] names no receiving member for Patient-Summaries: the FHIR face refuses a document of this Art 14(1) category
```

It also prints a line for each category served resource by resource,
naming the members its `[[fhir.serve]]` entry declares, and one line naming
the categories no member serves, which the face answers as not served
([Prescriptions and dispensations](../integrate/prescriptions.md)):

```text
ferrofed: note: [[fhir.serve]] Electronic-Prescriptions: the FHIR face serves this Art 14(1) category from the members node-a, each on its own endpoint
ferrofed: note: [[fhir.serve]] names no member for Electronic-Dispensations: the FHIR face answers this Art 14(1) category as not served
```

### The startup banner

When the log renders for a person, `serve` prints a banner before the first
log line: the wordmark, the version, the maintainer, the manufacturer with
its contact and postal address, the releases the gateway serves, the end
date of its support period, and the deployment facts to check first. Built from
source and run on a terminal with the development configuration of
[the quickstart](container.md#the-quickstart), it prints this:

```text
 _____                   _____ _____ ____
|  ___|__ _ __ _ __ ___ |  ___| ____|  _ \
| |_ / _ \ '__| '__/ _ \| |_  |  _| | | | |
|  _|  __/ |  | | | (_) |  _| | |___| |_| |
|_|  \___|_|  |_|  \___/|_|   |_____|____/

  openEHR federation gateway · v0.0.9
  Maintained by Ruben Talstra · https://github.com/FerroHEALTH/FerroFED
  Manufactured by Cadasto B.V. · info@cadasto.com
  Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands

  Federation Tier  1.0.0
  ITS-REST         1.1.0
  AQL              1.1.0
  openehr-*        0.0.87
  Support          none, this build is no release

  Base path        /
  Listen           0.0.0.0:8080
  Registry         4 members, 4 endpoints
  Stored queries   off
  Unencrypted      credentials.node-a-query
                   credentials.node-b-query
                   credentials.node-c-query
                   credentials.node-d-query

  DEVELOPMENT: this deployment runs the development profile, which may resolve
  patients from a static development table. It must not hold or reach real
  patient data.
```

The `Support` line of a release from v0.0.10 on says that only the newest
release is supported and names the last day of its support period, until
which fixed releases are published, as
`newest release only, fixes until YYYY-MM-DD`, five years from its release
date ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)).
The release build compiles the date in, and the gateway never reads it from
the network. A build from source and a pre-release have no support period,
and the line says so. Once the date has passed, the line reads
`ended on YYYY-MM-DD`, and an `UNSUPPORTED` notice under it says the release
receives no security fixes and to upgrade to the newest release, in red on
a terminal with colour. `config check`
prints the same notice as a `warning:` line, `serve` logs it at `WARN`, and
`OPTIONS {base}/` names the date as `supported_until`, or `null` for a build
with none (Regulation (EU) 2024/2847 Art 13(19)).

The `Registry` line counts the members and endpoints of the registry
document. The gateway reads the document once, before the banner, and serves
that same read, so the counts are those of the registry it serves. The line
reads `none` when no document is set, and says the document does not load
when it cannot be read, in which case the boot stops on the next lines with
the reason. The development notice prints only under
`profile = "development"`, in red on a terminal with colour and in the same
words without it, and an operator reads the same profile from
`GET {base}/operator/deployment`
([The operator surface](authentication.md#the-operator-surface)). The
`Unencrypted` lines name, by key, each credential or
patient identifier that travels unencrypted, which only the development
profile allows
([What must travel encrypted](#what-must-travel-encrypted)).

Colour follows the terminal, and an explicit `format = "pretty"` keeps it
into a pipe. A `NO_COLOR` environment variable that is set and not empty
switches colour off in both the banner and the log, whatever the format
(<https://no-color.org>).

The banner prints only when the log renders as `pretty`: with
`telemetry.format = "pretty"`, or with `auto` when stdout is a terminal. With
`json`, or with `auto` and stdout piped to a collector, the first line on
stdout is a JSON log line. `config check`, `healthcheck` and
`admission check` print no banner. The banner shows counts, an address, a
path, switches and configuration keys, and never a credential, a URL, a
header value or anything from a request.

## The file

The sections, and the page that covers each:

| Key or section | What it sets | Page |
|---|---|---|
| `profile` | `production`, the default, or `development`, the only profile that admits `[dev]` | [Identity resolution](identity.md#the-development-cross-reference-dev) |
| `[server]`, `[telemetry]`, `[credentials]`, `[signing]` | the listener, the console, the onward credentials, the signing keys | this page |
| `server.max_concurrent_requests`, `[server.caller_rate]`, `federation.max_in_flight_per_node`, `federation.max_node_answer_bytes` | the overload limits and the answer bound | [Overload protection](overload.md) |
| `federation.offset_strategy`, `federation.max_offset_window`, `federation.max_node_rows` | paging across nodes, and the most rows read of one node for a query with no `LIMIT`, 9999 by default (a node is asked for one row more, so set it one under the largest page your nodes serve) | [Queries and API areas](queries-and-areas.md#paging-with-offset) |
| `[telemetry] otlp_endpoint` | the trace export | [Tracing](tracing.md) |
| `[metrics]`, `[config]` | the admin listener and the OTLP push; `config.ui`, `off` by default, and the store of the configuration revisions the admin listener serves under `managed` | [Metrics](metrics.md), [Configuration revisions](configuration-revisions.md) |
| `[registry]` | the registry document and its form, or the mCSD directory of `[registry.mcsd]` the registry is read from; a member's own exchange-format endpoint is an `hl7-fhir-rest` endpoint of the document | [The registry](registry.md), [A member's own exchange-format endpoint](registry.md#a-members-own-exchange-format-endpoint) |
| `[pixm]`, `[dev]` | the cross-reference | [Identity resolution](identity.md) |
| `[pdqm]` | the PDQm Supplier asked for the master identity of an identifier the cross-reference does not map | [Demographics first](identity.md#demographics-first-pdqm) |
| `[xcpd]` | the XCPD localizer and its audit repository | [XCPD localization](localization.md#xcpd-localization-xcpd), [The audit repository](localization.md#the-audit-repository) |
| `[nl_gf.nvi]` | the NVI localizer of the Dutch Generic Functions | [Dutch localization](localization.md#dutch-localization-nl_gfnvi) |
| `[nl_gf.mitz]` | the Mitz consent pre-filter of the Dutch Generic Functions | [Dutch consent](consent.md#dutch-consent-nl_gfmitz) |
| `[pmir]` | the PMIR identity feed: the subscription and the route the Registry sends to | [The identity feed](identity.md#the-identity-feed-pmir) |
| `[audit]` | where the access records and the audit records of the PIXm, PDQm, mCSD and PMIR transactions go; outside development a gateway with a registry needs `destination = "repository"` | [The audit trail](audit.md) |
| `[access_log]` | the category map every access record is classified with: template ids and archetype ids to Art 14(1) categories, national categories, or `none`; `[access_log.retention]`, the years each record is kept by category and origin, at least three; `[access_log.labels]`, the security labels each record carries by category and origin; and `[[access_log.emergency_purpose]]`, the purposes of use that mark an emergency access (Art 11(5)) | [The access log](access-log.md#access_log), [Labelling the access records](access-labels.md) |
| `[federation]` | the federation id, node selection, budgets, completeness, paging, aggregates and the optional facilities | [The registry](registry.md), [Queries and API areas](queries-and-areas.md) |
| `[stored_queries]` | the stored-query registry and its backend | [Queries and API areas](queries-and-areas.md#stored-queries) |
| `[fhir]` | the FHIR R4 face of the European exchange format: its base, the operator, the FHIRconnect mappings, the categories it receives (`[[fhir.receive]]`), the members that serve prescriptions and dispensations (`[[fhir.serve]]`), `receive_namespaces`, the identifier systems a received document's or dispensation's patient is resolved in, and `document_url_lifetime_secs`, how long a document URL of the document list stays valid (300 seconds unless set); it needs `[pdqm]`, which the summary header is asked of; `[eehrxf.terminology]`, the optional FHIR terminology server the face asks for `$translate`, `$lookup` and `$validate-code`, over `https` outside development | [The patient summary over FHIR](../integrate/patient-summary.md), [Terminology](../integrate/patient-summary.md#terminology), [Prescriptions and dispensations over FHIR](../integrate/prescriptions.md#declaring-the-serving-members) |

### The bindings and their features

The sections of a regional or national binding belong to that binding, and
each binding is one Cargo feature of the `ferrofed-server` crate:

| Feature | Binding | Sections |
|---|---|---|
| always built | development | `[dev]` |
| always built | the access log, no binding | `[access_log]`, `[audit]` |
| `binding-ihe` | IHE, Annex A | `[pixm]`, `[pdqm]`, `[xcpd]`, `[registry.mcsd]`, `[pmir]`, and `registry.format = "fhir"`, which reads the document with the mCSD reader |
| `binding-nl` | Dutch Generic Functions, Annex B | `[nl_gf]` (`[nl_gf.nvi]`, `[nl_gf.mitz]`), `[credentials."<id>".nuts]` |

Both features are on by default, so the released binary and the container
image carry every binding. A build without one, such as
`cargo build -p ferrofed-server --no-default-features --features binding-ihe`,
refuses that binding's sections as unknown keys. The access record needs no
binding: every build writes it through `[audit]` ([The audit
trail](audit.md)), so a build with either binding, or none, runs a registry
outside development.

```toml
[server]
listen = "127.0.0.1:8080"     # the socket address to bind
base_path = "/"               # the path of the base URL every route sits under; see The base path
# public_url = "https://gateway.example.org"   # the base URL clients reach; see The public address
trusted_proxies = []          # the proxies whose forwarded client address is taken; see The public address
forwarded_header = "forwarded"   # or "x-forwarded-for": the header those proxies name the client in
request_timeout_ms = 30000    # a request past this answers 408; see Timeouts
drain_delay_ms = 0            # after SIGTERM, readiness is 503 and the listener accepts this long; see Health probes
shutdown_timeout_ms = 30000   # then the drain is bounded by this; unset, the request timeout, and never shorter
bindings_drain_timeout_ms = 5000  # then the bindings stop their processes within this; see Health probes
body_limit_bytes = 1048576    # a body past this answers 413
max_concurrent_requests = 512 # one more at once answers 503 overloaded; see Overload protection
overload_retry_after_s = 1    # the Retry-After of that 503, in seconds
# replica = "gateway-eu-1"    # the name every operator report carries; unset, HOSTNAME, else "unnamed"; see The replica name

# A per-caller rate, off unless set; see Overload protection.
[server.caller_rate]
requests_per_second = 10      # sustained, per verified issuer and client_id
burst = 20                    # at once after a quiet spell; one more answers 429 rate-limited

# TLS on the listener, off unless set; see TLS on the listeners. Unset, the
# listener speaks plain HTTP for a proxy that terminates TLS in front of it.
# [server.tls]
# certificate_file = "/run/secrets/ferrofed/listener.crt"   # the chain, its own certificate first
# key_file = "/run/secrets/ferrofed/listener.key"
# client_ca_file = "/run/secrets/ferrofed/proxy-ca.pem"     # optional: require a client certificate it signed
# healthcheck_identity_file = "/run/secrets/ferrofed/healthcheck.pem"   # what ferrofed healthcheck presents then

[telemetry]
format = "auto"   # auto, json or pretty; auto is json unless stdout is a terminal
filter = "info,hyper=warn,tower=warn,h2=warn"   # what the console logs; never what is traced
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector the spans are exported to; see Tracing
trace_sample_ratio = 1.0   # the share of requests whose spans are exported, 0.0 to 1.0

# The metrics surface, off by default; see Metrics.
[metrics]
listen = "127.0.0.1:9464"     # the admin listener: GET /metrics and the stored-query distribution; loopback unless allow_remote
# scrape_users = ["prometheus"]   # the [[auth.basic.users]] GET /metrics admits; needed off loopback outside development, unless client_ca_file is set
otlp_endpoint = "http://127.0.0.1:4317"   # an OTLP gRPC collector the same metrics are pushed to
# [metrics.tls] takes certificate_file, key_file and client_ca_file, as [server.tls] does.

# Outbound credentials, one section per endpoint id. Each section names one
# scheme: a user and a password, or a grant; see Onward credentials.
[credentials."clinic-b"]
user = "ferrofed"
password_file = "/run/secrets/clinic-b-password"

# OAuth 2.0 client credentials with a signed JWT client assertion (§13.1).
[credentials."cdr-c".oauth]
token_endpoint = "https://auth.cdr-c.example.org/oauth2/token"
client_id = "ferrofed-gateway"
client_auth = "private_key_jwt"
private_key_file = "/run/secrets/cdr-c-client.pem"   # its public half is in the gateway's JWK Set
scope = "system/aql-*.s system/composition-*.cru"
resource = "https://cdr-c.example.org/openehr"   # optional, RFC 8707
# audience = "cdr-c"                             # optional

# The Nuts grant of the Dutch Generic Functions (Annex B §B.4): a DPoP-bound
# token for a Verifiable Presentation of the gateway's credentials.
[credentials."cdr-d".nuts]
authorization_server = "https://nuts.cdr-d.example.org/oauth2/cdr-d"
scope = "openehr-query"
# client_id = "https://nuts.gateway.example.org/oauth2/gateway"   # optional
did = "did:web:gateway.example.org"
kid = "did:web:gateway.example.org#key-1"
key_file = "/run/secrets/nuts-holder.pem"
dpop_key_file = "/run/secrets/cdr-d-dpop.pem"

[[credentials."cdr-d".nuts.credential]]
input_descriptor = "organization_credential"
file = "/run/secrets/nuts-organization-credential.jwt"

# A FAPI 2.0 authorization server, the BgZ/eOverdracht track (Annex B
# §B.4a): an ES256 private_key_jwt assertion naming the issuer, a DPoP-bound
# token, and RFC 9396 authorization_details.
[credentials."cdr-e".fapi2]
issuer = "https://as.cdr-e.example.org"
grant = "client_credentials"
client_id = "urn:oid:2.16.528.1.1007.3.3.<URA>"
client_key_file = "/run/secrets/fapi2-client.pem"
dpop_key_file = "/run/secrets/cdr-e-dpop.pem"
authorization_details = '''[{"type": "nl-gis-v1", "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT"}]'''

# The gateway's signing keys, which sign the caller's identity on every
# request to a node and every client assertion, and are published as a JWK
# Set. Required whenever a registry is configured, by registry.document or
# by [registry.mcsd].
[signing]
key_file = "/run/secrets/ferrofed-signing-key.pem"
# previous_key_file = "/run/secrets/ferrofed-signing-key-previous.pem"
# next_key_file = "/run/secrets/ferrofed-signing-key-next.pem"   # published ahead, never signs
# next_key_algorithm = "ES256"   # unset, the current key's algorithm
jwks_uri = "https://gateway.example.org/.well-known/jwks.json"
assertion_lifetime_s = 300    # at most 300
node_jwks_cache_s = 3600      # how long the nodes cache the JWK Set
rotation_overlap_s = 3900     # at least assertion_lifetime_s + node_jwks_cache_s
```

### Secrets, inline or from a file

Every secret has a `_file` sibling, read at boot and trimmed, so a secret
can come from a mounted file and never sit in the configuration or the
environment. A secret written inline, or set through a `FERROFED__`
variable, is accepted as well, under every profile, `production` included;
setting both forms of one secret is refused, naming the inline key. Prefer
the file: an inline value sits in `ferrofed.toml` and in every copy of it.
The credentials are read and checked at boot and again on each
[reload](registry.md#reloading-the-registry), and the node client of an endpoint with a
credentials section sends them on every request to that endpoint.

The gateway never prints a secret. A password or a client secret, inline, from
the environment or from a `_file` sibling, shows as `***` wherever the
configuration is rendered: a debug log line, a panic message, a test failure.
A URL that may carry a credential, such as the stored-query store's
`url` or `metrics.otlp_endpoint`, shows with its userinfo and its query
replaced: `postgres://***@db.example.org:5432/ferrofed?***`. A libpq
key/value connection string shows as `***` whole. The value itself is sent
unchanged.

A PIX Manager's `url` carries no credential: one with a user name or a
password in it is refused naming the key, as an endpoint URL in the registry
document is. Its credentials go in `[pixm.manager.credentials]`, which takes
a user and a password, or the `oauth` client-credentials grant
authenticated by a client secret or a client assertion
([Identity resolution](identity.md#a-pix-manager-behind-oauth-20)), never an
`oauth2`, `nuts` or `fapi2` grant. The credentials of `[pdqm]`, `[pmir]` and
`[registry.mcsd]` take the same. Each grant a node's section can name is
described in [Onward credentials](onward-credentials.md). The credentials of
`[nl_gf.mitz]` take the same: a user and a password, or the `oauth` grant,
whose token each question carries as a bearer token. A grant
a section does not take is refused at load, naming it, and never read as no
credential.
`[nl_gf.nvi.credentials]` takes the `nuts` grant as well, and refuses
`oauth`, `oauth2` and `fapi2`
([Dutch localization](localization.md#dutch-localization-nl_gfnvi)).

### A PIX Manager asked by `GET` or `POST`

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
url = "https://pix.example.org/fhir/"
method = "post"   # the patient identifier travels in the body, never the URL
```

Any other value is refused naming the key. The answer is read the same way
under either method, and the audit record of each exchange holds the request
as sent: the URL of a `GET`, or the request line, media type and body of a
`POST` ([The audit trail](audit.md)).

### Mutual TLS to the identity services

A national identity service often authenticates the gateway by its client
certificate instead of, or beside, a password or a token. `[[pixm.manager]]`,
`[pdqm]`, `[pmir]` and `[registry.mcsd]` take the keys `[xcpd]`,
`[nl_gf.nvi]`, `[nl_gf.mitz]` and `[audit.repository]` already take:

```toml
[[pixm.manager]]
url = "https://pix.example.org/fhir/"
client_identity_file = "/run/secrets/pix-client.pem"  # mutual TLS
trust_roots_file = "/etc/ferrofed/pix-roots.pem"      # optional
```

- `client_identity`, or its `client_identity_file` sibling, holds the PEM
  client certificate chain and its private key, which the gateway presents
  in the TLS handshake. It is a secret: it shows as `***` wherever the
  configuration is rendered, and setting both forms is refused naming
  `client_identity`.
- `trust_roots_file` names a PEM bundle of trust roots the service's
  certificate chains to, added to the platform's roots.
- Every file is read at boot and on each reload of a reloadable table. A
  file that cannot be read is refused naming its key, and material that does
  not read as PEM refuses the federation without quoting the file.

Each of these services is reached through one client build, with no
redirect followed, the credential sent as a sensitive default header, and
the TLS material above.

### What must travel encrypted

Every outbound connection in the configuration is held to one of three
rules, by what it carries.

**Credentials and patient identifiers.** Outside `profile = "development"`,
a URL that a configured credential or a patient identifier is sent to must
be `https`. `serve`, `config check`, `admission check`, `conformance run`
and every [reload](registry.md#reloading-the-registry) refuse anything else, with exit
code 78 and one line naming the URL's key and what would travel over it,
never a value. The rule covers:

- the URL of a registry endpoint that has a `[credentials."<id>"]` section,
  which receives its basic credentials or the access token its `oauth` or
  `oauth2` grant obtains;
- the `token_endpoint` of an `oauth` or `oauth2` section, which receives the
  client secret or the client assertion;
- the `authorization_server` of a `nuts` section, which receives the
  gateway's credentials in a Verifiable Presentation;
- the `issuer` of a `fapi2` section, whose token endpoint receives the
  client assertion and, under token exchange, the callers' tokens;
- the `url` of every PIX Manager, which is asked for patient identifiers
  with or without `[pixm.manager.credentials]`;
- the `url` of `[pdqm]`, the PDQm Supplier, which is sent the patient
  identifier with or without `[pdqm.credentials]`;
- the `url` of every XCPD responding gateway, which is sent the patient
  identifier and, when one is configured, the XUA assertion;
- the `url` of `[nl_gf.nvi]`, which is sent the pseudonymised BSN and, when
  one is configured, its credentials;
- `metrics.otlp_endpoint` and `telemetry.otlp_endpoint` when either carries a
  user name or a password.

```text
ferrofed: cannot start: the url of endpoint hospital-a in registry.document is not an https URL, and credentials.hospital-a would travel over it in cleartext: outside profile = "development" a credential or a patient identifier is sent only encrypted
```

A node URL no credential is sent to may stay `http`, for example a node on a
private network whose transport a sidecar protects with mutual TLS: the
gateway sends a node its own `ehr_id`, never the patient identifier (§5.4,
N33). A node the gateway presents its own client certificate to
([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705))
is `https` under every profile.

**A database password.** Outside the development profile, the stored-query
store's PostgreSQL connection string must set `sslmode=require` when it
carries a password and reaches its host over the network. The driver reads
`sslmode` as `disable`, `prefer` (its default, which falls back to no TLS) or
`require`, and refuses any other value, so `disable` and `prefer` are
refused, naming `stored_queries.url`, by the same commands and on a reload.
A connection with no password, or whose every host is a Unix socket and
that names no `hostaddr`, crosses no network with a secret and may leave
`sslmode` as it is
([Stored queries](queries-and-areas.md#stored-queries)).

Under the development profile the same configuration starts, so the
[quickstart](container.md#the-quickstart) can reach its nodes over `http`
inside its Docker network. Each credential or patient identifier that
travels unencrypted is named by key in the startup banner, in a `WARN` log
line at boot and after each reload, and on stderr by `config check`.

**Trust anchors.** A URL the gateway verifies its callers against, an
issuer itself, the key set its metadata names, or an `introspection_endpoint`
in [`[trust]`](authentication.md), must be `https`, or `http` to a loopback
host, under every profile, development included. A key set fetched in the
clear would let anyone on the network substitute keys and forge callers.

**The profile takes a restart.** A reload whose file changes `profile` is
refused (class `profile`), and the running configuration stays. Every
decision the development profile admits, these transport rules, the
development cross-reference and consent table, and an `http` XCPD gateway,
reads the profile the process started with.

The specification assumes a protected transport and leaves it to the
security profiles (§2.2, §13); no specification governs these rules, which
are FerroFED's own design.

## The environment

Any key can be set or overridden with `FERROFED__<SECTION>__<KEY>`, upper or
lower case, with `__` between the segments:

```text
FERROFED__SERVER__LISTEN=0.0.0.0:8080
FERROFED__CREDENTIALS__HOSPITAL_A__BEARER_TOKEN_FILE=/run/secrets/token
```

A value reads as TOML syntax when it is one (`9`, `true`) and as the string it
is otherwise. An override that names no key, or a key the file does not
define, is refused like any other unknown key.

## Configuration revisions

With `[config] ui = "managed"`, every change to the configuration becomes a
revision: checked as `config check` checks the file, kept in history with a
diff, applied atomically or marked pending a restart, and rolled back in one
step. Operators read and save revisions through the admin listener's API;
[Configuration revisions](configuration-revisions.md) describes it. The
listen address, the profile, `[server]`, `[metrics]`, `[auth]`, `[config]`
and `[dev]` stay file-only, and a revision holds only `_file` references,
never a secret. With `off`, the default, nothing changes.

The console manages members and stored queries (#577), and onward
credentials and binding sections with write-only secrets, a confirmation
after a change applies and a second operator's approval (#578;
[Credentials and binding sections](configuration-revisions.md#credentials-and-binding-sections)).
Any revision exports as TOML, and TOML imports as a revision you check
before you apply it (#579;
[Export and import](configuration-revisions.md#export-and-import)).

## The base path

`server.base_path` is the path of the base URL the gateway is served at,
`{base}` in the specification, and every route in the table below sits under
it (§4.1, N28). It is `/` by default, so the gateway serves at the root. Set
it to mount the gateway under a path of your choosing, for example behind a
reverse proxy that does not strip the path:

```toml
[server]
base_path = "/fed/openehr"
```

The gateway then serves `GET /fed/openehr/`, `OPTIONS /fed/openehr/`,
`GET /fed/openehr/health`, `POST /fed/openehr/v1/query/aql` and the rest of
the table, serves `{base}` without the trailing slash as `{base}/`, and
answers `404` for every path outside the base, the root included, so point a
health probe at `{base}/health`; `ferrofed healthcheck` asks under the base
by itself. The specification reserves no prefix, and
the gateway reserves none either: `/rest/openehr` is a valid base when you
choose it, and is not served unless you do. The base is checked at boot: it
starts with `/`, has no trailing `/` unless it is `/`, has no query or
fragment, and has no empty, `.` or `..` segment, or the gateway refuses to
start and names `server.base_path`. Tell clients the full base URL, scheme,
host and this path, through the registry or service discovery; nodes never
see it, because the gateway asks each node at the node's own base URL.
Write that full URL once in `server.public_url`, and the keys that repeat
it follow from it ([The public address](public-address.md)).

## The HTTP surface

Every route is under the [base path](#the-base-path); with the default `/`,
`{base}/` is `/`.

| Route | Answers |
|---|---|
| `GET {base}/` | the product name and version, and its manufacturer with the postal address and the contact |
| `OPTIONS {base}/` | the federation's self-description (§7a.2) |
| `GET {base}/health` | `200` while the process is up |
| `GET {base}/health/readiness` | `200` while the gateway serves and its own subsystems (the configuration, the registry, the outbound clients, the stored-query store) are up; `503` before boot completes and from the moment `SIGTERM` or `SIGINT` arrives, with the phase and each subsystem's state; no member node and no identity source gates it |
| `GET {base}/operator/dependencies` | to a caller with the operator scope, always `200`, with the state the gateway last observed of each member endpoint, of the resolver, of the consent pre-filter and of the mCSD directory: `up`, `degraded`, `failing`, `down` or `unknown`, with `directory_fault` while the directory is `degraded`; endpoint ids, states, the fault's class and the [replica name](#the-replica-name) only |
| `GET {base}/operator/deployment` | to a caller with the operator scope, always `200`, with the profile the process started with, whether patients resolve through the `[dev]` cross-reference, the product version and the [replica name](#the-replica-name); no setting value |
| `GET` and `POST {base}/v1/query/aql` | the federated `RESULT_SET`, the `GET` form reading the request from its query string; `501` when no registry is configured |
| `{base}/v1/ehr/{ehr_id}` and below | routed to the one node that owns the `ehr_id`, found in the order of §12.5.1: the `openEHR-federation-endpoint` header, the session's resolution binding, the `ehr_id` index, then for a read the ask-all probe; answered as that node answered; `501` when no registry is configured |
| `GET {base}/v1/ehr?subject_id=…&subject_namespace=…` | the subject resolved at the gateway, and `GET /v1/ehr/{ehr_id}` sent to the one member that holds it, at that member's own base, answered as that node answered; `501` when no registry is configured |
| `{base}/v1/definition/` and below | routed to the one node `openEHR-federation-endpoint` names, never merged; without the header a `400`; a template upload naming `*` or several endpoints fanned out to each when `federation.fan_out_template_upload` is set; stored-query definitions held at the gateway when `[stored_queries]` is set, and distributed to the members a `PUT` names when `federation.fan_out_stored_queries` is set beside it; without `[stored_queries]`, routed like every other definition request; `501` when no registry is configured |
| `{base}/v1/demographic/` and below | `501`, never federated; when `federation.demographic_endpoint` is set, routed to that endpoint, with or without `openEHR-federation-endpoint` naming it; a header naming another endpoint is a `400` |
| any other path under `{base}/v1/` | `501` |
| any other path | `404` |

Every response carries an `x-request-id`: the client's value when it is short
printable ASCII, the gateway's own id otherwise.

The metrics are not on this surface: they have a listener of their own, off
by default ([Metrics](metrics.md)).

## The replica name

Each gateway process keeps its integrity incidents, its learned routing
table and its dependency states to itself, so every report of the
[operator surface](authentication.md#the-operator-surface) and the
[export of the identity state](identity-state-export.md) names the process
that produced it. `[server] replica` sets the name: 1 to 64 ASCII letters,
digits, `.`, `_` and `-`. Any other value is refused at load with
`server.replica is no replica name`. Unset, the name is the `HOSTNAME` the
process runs with, which Kubernetes sets to the pod's own name, and
`unnamed` when `HOSTNAME` is unset or is no such name. A start is never
refused for its host name.

Give each replica a name you can tell apart, or leave it to the pod name.
The name is not a secret and names no patient, and it never reaches a
node.

## TLS on the listeners

The client listener and the admin listener each run in one of two modes.

- **Plain HTTP**, the default. Use it behind a reverse proxy, a load
  balancer or a service mesh that terminates TLS and reaches the gateway
  over a network you trust, such as the loopback interface or a pod's own
  network namespace. Bearer tokens and patient identifiers cross the hop
  from the proxy to the gateway in the clear, so pick this mode only where
  that hop cannot be read.
- **TLS**, with `[server.tls]` or `[metrics.tls]`. Use it when the hop from
  the proxy to the gateway crosses a network others share, or when your
  policy forbids a cleartext hop inside the network (NEN 7510 is one such
  policy). Set `client_ca_file` as well to admit only the proxy: a client
  that presents no certificate that CA signed fails its handshake and reaches
  nothing.

```toml
[server.tls]
certificate_file = "/run/secrets/ferrofed/listener.crt"
key_file = "/run/secrets/ferrofed/listener.key"
client_ca_file = "/run/secrets/ferrofed/proxy-ca.pem"
healthcheck_identity_file = "/run/secrets/ferrofed/healthcheck.pem"
```

- `certificate_file` holds the chain the listener presents, its own
  certificate first, and `key_file` its private key, both PEM.
- `client_ca_file`, optional, holds the CA certificates a client
  certificate must chain to. Set, every client must present one.
- `healthcheck_identity_file`, `[server.tls]` only, holds a client
  certificate chain and its key, PEM, which `ferrofed healthcheck` presents
  to a listener that requires one. It is refused anywhere else.

The listener negotiates TLS 1.3 (RFC 8446) or TLS 1.2 and nothing older,
and picks TLS 1.3 whenever the client offers it, as BCP 195 asks (RFC 9325
§3.1.1). Its TLS 1.2 cipher suites are the four ECDHE AES-GCM suites RFC 9325
§4.2 recommends, each with forward secrecy. It offers HTTP/1.1 in ALPN. A
client that does not finish its handshake within 10 seconds is dropped.

`config check` reads every file, and refuses, with exit code 78 and the key
named, a missing certificate or key, a file that holds no PEM, a key that
does not match its certificate, and a client CA that is no usable trust
anchor. It never prints the content of a file.

To renew the certificate, write the new files over the old ones and send
`SIGHUP`, as for [a registry reload](registry.md). Every handshake after the
signal presents the new certificate and checks clients against the new CA
file; connections already open keep the certificate they began with. Files
that do not form a valid pair refuse the whole reload, logged with their
table, and the running certificate, registry and `[auth]` stay. Where no
signal reaches the process, `POST /admin/reload` on the admin listener runs
the same reload. A changed path in `[server.tls]` or
`[metrics.tls]` takes a restart.

`ferrofed healthcheck` follows `[server.tls]`: it asks over `https` on
loopback and accepts exactly the certificate `certificate_file` holds. If
the files were replaced and the gateway has not been sent `SIGHUP`, it
reports so and exits `1`.

## What the log records

One line per request: the method, the matched route, the status, the latency,
the gateway's request id (`request_id`) and whether the client sent its own
(`client_named`). A façade query carries the patient identifier, so the line
never carries a request body, the AQL text, a header value, the request path,
or a query value other than the ITS-REST paging parameters `offset` and
`fetch`, and those only when they are digits. The route is a path template,
never the path the client sent: a request the gateway routes or answers
under `{base}/v1/` is logged under the template of the ITS-REST operation it
addresses, so a routed composition read is logged as
`/v1/ehr/{ehr_id}/composition/{uid_based_id}`, with the configured base path
in front of it once, and its `ehr_id` and version uid stay out of the line.
An `OPTIONS` request is logged under the template of the resource it
describes. A path that names no route, and any other method the ITS-REST
operation does not declare, is logged as `<unmatched>`. The client's own
`x-request-id` is a header value too, and it is never logged, so a request the client named is found in the log by its time, route
and status, and in a node's log by the `request_id` of that line. A request
the gateway answers before its handler finishes has its line too, with the
status it answered: `500` for a handler panic, `408` past the request
timeout, `413` over the body ceiling. A handler panic is also logged under
the gateway's id, without its message, which could quote a value the handler
held. Every panic in `ferrofed serve`, inside a request or not, writes one
more line, "a thread panicked", with the source location and, inside a
request, the gateway's `request_id`. The panic message is never written:
the gateway replaces Rust's default panic hook, which prints it to stderr,
so a panic writes nothing to stderr. A federated query the gateway fails with a `500` also logs "the
federated query failed" with its error code and the same `request_id` as its
request line. A fan-out template upload, a stored-query distribution or
repair, and a drift check whose task for one member panics fail the same
way, with a `500`, and log that the fan-out could not tell what became of
every member.
