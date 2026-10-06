<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# A production deployment

This page takes you from nothing to a first federated query over two
openEHR CDRs you already run. It walks the steps in order and links the
reference page for each, so read those for every key and every edge case.
The specification leaves deployment to each federation, so no specification
governs this page: our own design.

Every value below is a placeholder: hosts under `example.org`, identifiers
in the `urn:oid:2.999` example arc. The two members are `cdr-a` and `cdr-b`,
and the gateway's public base URL is `https://gateway.example.org/fed`.

`scripts/checks/production-guide.sh` assembles every TOML block on this
page into one `ferrofed.toml` and one `registry.toml`, runs
`ferrofed config check` over them, and serves them behind the shipped
reverse proxy. Each block opens with a comment naming the file it belongs
in.

## 1. What you need

| What | Why | Reference |
|---|---|---|
| Two openEHR CDRs that serve ITS-REST 1.1.0, ad hoc AQL included | the members; each answers AQL scoped to one `ehr_id` | [What FerroFED runs beside](deployment-shape.md) |
| An identity provider for your callers | every caller presents an RFC 9068 access token; [step 4](#4-client-authentication) configures Keycloak | [Client authentication](authentication.md) |
| A PIX Manager | resolves each patient to the `ehr_id` each member holds | [Identity resolution](identity.md) |
| An Audit Record Repository with the FHIR Feed of ITI-20 | takes the audit and access records | [The audit trail](audit.md) |
| A Linux host with Docker Compose, or a Kubernetes cluster | runs the published image | [The container image](container.md) |
| A DNS name and a TLS certificate for the gateway, and nginx or another reverse proxy | the public address clients, nodes and identity services reach | [step 8](#8-tls-and-the-public-address) |
| Prometheus, optional | scrapes the admin listener | [Metrics](metrics.md) |

Decide three values before you start, because several settings repeat them:

- the public base URL, here `https://gateway.example.org/fed`: its path is
  `server.base_path`, the JWK Set and the PMIR feed route sit under it, and
  `auth.audience` names it;
- the federation id, `federation.id`, which `OPTIONS {base}/` reports;
- each member's node id and endpoint id, which the registry, the PIX
  Manager's domains and the onward credentials all key on.

### Prepare each CDR

Agree each of these with the member's operator before you admit the
member. The first five are the conditions of membership (§12b.1, §12b.2,
N42a); the rest are what the gateway needs to reach the member.

- [ ] **A unique `system_id`.** The CDR stamps one openEHR `system_id` into
  every EHR and version it creates, and no other member uses it (§12b.2).
  Find it by asking the CDR itself, at the member, as the member's
  operator:

  ```sql
  SELECT DISTINCT e/system_id/value FROM EHR e
  ```

  Expect one value. If the CDR reports several, ask its operator which one
  it stamps into new EHRs, and map any other system whose versions it holds
  with a `[[creating_system]]` entry ([The registry](registry.md#the-registry-document)).
- [ ] **`ehr_id`s are version-4 UUIDs**, or come from a scheme with the same
  collision resistance (§12b.2).
- [ ] **No `ehr_id` is reused**, across restores, migrations and test-data
  resets (§12b.2). The evidence is the member's documented procedures.
- [ ] **No foreign `ehr_id` is adopted.** An imported EHR gets a fresh
  `ehr_id` (§12b.2).
- [ ] **Each `ehr_id` reaches the PIX Manager** in the member's `ehr_id`
  domain, with the patient identifier your clients use, from the moment the
  EHR is created, and the EHRs that exist today in a bulk load (§5.5;
  [step 6](#6-identity-resolution)).
- [ ] **ITS-REST is reachable from the gateway** at one base URL per
  endpoint, over `https` whenever the gateway sends it a credential
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).
- [ ] **The member accepts the gateway's credential**: a bearer token, a user
  and a password, or an OAuth 2.0 client registered under the gateway's
  `client_id` with the gateway's JWK Set location
  ([step 5](#5-onward-credentials)).
- [ ] **The member makes its own access decision and enforces consent**
  (§13.2, N26, N27). If it marks a consent refusal with an ITS-REST `Error`
  code, write the code down for the registry's `consent_refusal_codes`
  ([Consent](consent.md)).
- [ ] **The member verifies who asks**, recommended: it reads the
  `openEHR-federation-client` token the gateway signs, with the gateway's
  JWK Set, and records the caller in its own audit trail
  ([Verifying it at the node](authentication.md#verifying-it-at-the-node)).

## 2. Install

The release lane attests every image and every binary. Verify what you
pull before you run it, with the GitHub CLI:

```sh
gh attestation verify oci://ghcr.io/ferrohealth/ferrofed:X.Y.Z \
  --repo FerroHEALTH/FerroFED \
  --signer-workflow FerroHEALTH/FerroFED/.github/workflows/release-image.yml
```

Then pin the image by the digest you verified, `ghcr.io/ferrohealth/ferrofed@sha256:…`;
`docker pull` prints it on its `Digest:` line. For a binary, verify the
tarball against `release-build.yml` instead
([The release binaries](container.md#the-release-binaries)).

Pick one layout. Both mount the configuration at `/etc/ferrofed/`, the
secrets at `/run/secrets/ferrofed/` and a durable volume at
`/var/lib/ferrofed/`, the paths every block on this page uses:

- **One host:** the release's `compose.yaml`, `ferrofed.toml` and
  `registry.toml`
  ([The gateway from a release](container.md#the-gateway-from-a-release)).
  The gateway's port is published on `127.0.0.1:8080`, where the reverse
  proxy of [step 8](#8-tls-and-the-public-address) reaches it.
- **Kubernetes:** the StatefulSet example in `deploy/kubernetes/`
  ([Kubernetes](container.md#kubernetes)). Run the reverse proxy as a
  container in the same pod, or put your ingress in front of the Service
  with TLS on the gateway's listener.

Edit the release's example files as you go through the steps below. The
gateway itself, and the address the proxy passes to:

```toml
# ferrofed.toml
profile = "production"

[server]
listen = "127.0.0.1:8080"
base_path = "/fed"
drain_delay_ms = 5000
shutdown_timeout_ms = 30000

[telemetry]
format = "json"

[federation]
id = "example-federation"
node_selection = "ask-all"
per_node_timeout_ms = 10000
overall_timeout_ms = 25000
```

The image sets `FERROFED__SERVER__LISTEN=0.0.0.0:8080`, which overrides
`listen` inside the container; the published port decides who reaches it.
`drain_delay_ms` keeps the listener open while a load balancer stops
routing to a stopping gateway
([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).

## 3. The registry

The registry document names the members: one `[[organisation]]` per
operator, one `[[node]]` per CDR with its `system_id`, and one
`[[endpoint]]` per ITS-REST base URL
([The registry](registry.md)). Add each endpoint suspended: the gateway
sends a suspended endpoint nothing, and the admission check of
[step 9](#9-check-start-and-admit) still reaches it.

```toml
# registry.toml
[[organisation]]
id = "org-a"
name = "Example organisation A"

[[organisation]]
id = "org-b"
name = "Example organisation B"

[[node]]
id = "cdr-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[node]]
id = "cdr-b"
organisation = "org-b"
system_id = "cdr-b.example.org"

[[endpoint]]
id = "cdr-a-query"
node = "cdr-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
status = "suspended"

[[endpoint]]
id = "cdr-b-query"
node = "cdr-b"
url = "https://cdr-b.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
status = "suspended"
```

```toml
# ferrofed.toml
[registry]
document = "/etc/ferrofed/registry.toml"
format = "toml"
```

The registry refuses a second member with the `system_id` of another, and
reloads on `SIGHUP` without a restart
([Reloading the registry](registry.md#reloading-the-registry)).

## 4. Client authentication

Every request to `{base}/v1/` and `OPTIONS {base}/` carries an access
token the gateway verifies before it reads anything else
([Client authentication](authentication.md)). The token must be an RFC 9068
access token:

- its JOSE header names the type `at+jwt`;
- it carries `iss`, `exp`, `aud`, `sub`, `client_id`, `iat` and `jti`, and
  `aud` names `auth.audience`;
- its `scope` holds the SMART on openEHR scopes of each operation, such as
  `user/aql-*.s` for a federated query
  ([Scopes per route](authentication.md#scopes-per-route));
- it declares a purpose of use, as `extensions.ihe_iua.purpose_of_use`, an
  array of FHIR `Coding`, or as RFC 9396 `authorization_details`
  ([Purpose of use](authentication.md#purpose-of-use)).

### An issuer recipe: Keycloak

Keycloak 26.2 added a client setting that types its access tokens `at+jwt`.
The recipe below was run on 2026-10-06 against Keycloak 26.8.0: the gateway
admitted a user's token and a client-credentials token made this way, and
answered a federated query for each. With the `at+jwt` setting off, it
refused the same user's token as "not typed at+jwt".

Run it with Keycloak's admin CLI, `kcadm.sh`, against your Keycloak's
public address. It creates a realm, one client scope per SMART on openEHR
scope, a clinical application that signs users in, and a reporting service
that uses the client-credentials grant:

```sh
kcadm.sh config credentials --server https://idp.example.org --realm master --user admin
kcadm.sh create realms -s realm=ferrofed -s enabled=true
for scope in 'user/aql-*.s' 'user/composition-*.r' 'system/aql-*.s'; do
  kcadm.sh create client-scopes -r ferrofed -s "name=$scope" -s protocol=openid-connect \
    -s 'attributes."include.in.token.scope"=true' \
    -s 'attributes."display.on.consent.screen"=false'
done
kcadm.sh create clients -r ferrofed -s clientId=example-clinical-app \
  -s publicClient=false -s standardFlowEnabled=true \
  -s 'redirectUris=["https://app.example.org/callback"]' \
  -s 'attributes."access.token.header.type.rfc9068"=true'
kcadm.sh create clients -r ferrofed -s clientId=example-reporting-service \
  -s publicClient=false -s standardFlowEnabled=false -s serviceAccountsEnabled=true \
  -s 'attributes."access.token.header.type.rfc9068"=true'
```

Each client then gets its scopes as default client scopes, loses the
`profile` and `email` scopes, whose personal data the gateway never reads,
and gets three protocol mappers:

```sh
id_of() { kcadm.sh get clients -r ferrofed -q "clientId=$1" --fields id --format csv --noquotes; }
scope_of() {
  kcadm.sh get client-scopes -r ferrofed --fields id,name --format csv --noquotes |
    awk -F, -v n="$1" '$2 == n { print $1 }'
}
app="$(id_of example-clinical-app)"
svc="$(id_of example-reporting-service)"
for scope in 'user/aql-*.s' 'user/composition-*.r'; do
  kcadm.sh update "clients/$app/default-client-scopes/$(scope_of "$scope")" -r ferrofed
done
kcadm.sh update "clients/$svc/default-client-scopes/$(scope_of 'system/aql-*.s')" -r ferrofed
for client in "$app" "$svc"; do
  for scope in profile email; do
    kcadm.sh delete "clients/$client/default-client-scopes/$(scope_of "$scope")" -r ferrofed
  done
  kcadm.sh create "clients/$client/protocol-mappers/models" -r ferrofed -f audience.json
  kcadm.sh create "clients/$client/protocol-mappers/models" -r ferrofed -f purpose-of-use.json
done
kcadm.sh create "clients/$app/protocol-mappers/models" -r ferrofed -f client-id.json
```

`audience.json` puts the gateway's audience in `aud`:

```json
{
  "name": "ferrofed-audience",
  "protocol": "openid-connect",
  "protocolMapper": "oidc-audience-mapper",
  "config": {
    "included.custom.audience": "https://gateway.example.org/fed",
    "access.token.claim": "true",
    "id.token.claim": "false",
    "introspection.token.claim": "true"
  }
}
```

`purpose-of-use.json` declares the purpose of use, here treatment (`TREAT`
of HL7 v3 `ActReason`), in the IHE IUA extension. A hardcoded claim gives
one purpose per client, so a client that requests data for another purpose
gets a client of its own:

```json
{
  "name": "purpose-of-use",
  "protocol": "openid-connect",
  "protocolMapper": "oidc-hardcoded-claim-mapper",
  "config": {
    "claim.name": "extensions.ihe_iua.purpose_of_use",
    "claim.value": "[{\"system\": \"http://terminology.hl7.org/CodeSystem/v3-ActReason\", \"code\": \"TREAT\"}]",
    "jsonType.label": "JSON",
    "access.token.claim": "true",
    "id.token.claim": "false",
    "userinfo.token.claim": "false",
    "introspection.token.claim": "true"
  }
}
```

`client-id.json` adds `client_id` to the user's token. RFC 9068 requires the
claim, and Keycloak 26.8.0 wrote it by itself into the service account's
token and not into the user's:

```json
{
  "name": "client-id",
  "protocol": "openid-connect",
  "protocolMapper": "oidc-hardcoded-claim-mapper",
  "config": {
    "claim.name": "client_id",
    "claim.value": "example-clinical-app",
    "jsonType.label": "String",
    "access.token.claim": "true",
    "id.token.claim": "false",
    "userinfo.token.claim": "false",
    "introspection.token.claim": "true"
  }
}
```

A user who signs in to `example-clinical-app` then gets a token whose
`scope` reads `user/composition-*.r user/aql-*.s`, and the reporting
service a token with `system/aql-*.s`. Keycloak's realm issuer is
`https://idp.example.org/realms/ferrofed`; copy `issuer` and `jwks_uri`
from `https://idp.example.org/realms/ferrofed/.well-known/openid-configuration`,
since the gateway compares `iss` exactly:

```toml
# ferrofed.toml
[auth]
audience = "https://gateway.example.org/fed"
clock_skew_s = 60

[[auth.issuer]]
issuer = "https://idp.example.org/realms/ferrofed"
jwks_uri = "https://idp.example.org/realms/ferrofed/protocol/openid-connect/certs"
backend_clients = ["example-reporting-service"]
```

`system/aql-*` counts only for a client listed in `backend_clients`. An
operator who uses the [operator console](operator-console.md) needs the
issuer's `operator_scope` as well
([The operator surface](authentication.md#the-operator-surface)).

## 5. Onward credentials

The gateway never forwards a caller's token to a node. It authenticates to
each endpoint with that endpoint's own credential, one section per
endpoint id, and signs the caller's identity onto every request with its
own key ([Onward credentials](onward-credentials.md)). Here `cdr-a` takes a
bearer token and `cdr-b` registers the gateway as an OAuth 2.0 client,
authenticated by an assertion the signing key signs:

```toml
# ferrofed.toml
[credentials."cdr-a-query"]
bearer_token_file = "/run/secrets/ferrofed/cdr-a-token"

[credentials."cdr-b-query".oauth2]
grant = "client_credentials"
client_auth = "private_key_jwt"
token_endpoint = "https://auth.cdr-b.example.org/oauth2/token"
client_id = "ferrofed-gateway"
scope = "system/aql-*.s system/composition-*.r"

[signing]
key_file = "/run/secrets/ferrofed/signing-key.pem"
jwks_uri = "https://gateway.example.org/fed/.well-known/jwks.json"
```

Make the signing key, and give every secret file to the gateway's user
alone:

```sh
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out secrets/signing-key.pem
sudo chown -R 65532:65532 secrets && sudo chmod 0400 secrets/*
```

Use `ec_paramgen_curve:P-256` instead when a member's authorization server
holds to the FAPI 2.0 Security Profile
([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)).
`jwks_uri` is the JWK Set route on the public address of
[step 8](#8-tls-and-the-public-address); give it to `cdr-b`'s operator with
the `client_id`, and to every member that verifies the caller token.

## 6. Identity resolution

A patient query names its patient by an identifier and its namespace. The
gateway asks the PIX Manager for the patient's `ehr_id` in each member's
`ehr_id` domain (ITI-83), and sends each member a query scoped to that
`ehr_id` alone (§5.2, N3). Without a cross-reference every patient query is
a `424` ([Choosing one](identity.md#choosing-one)).

The Manager answers only from what its feeds delivered, so it must hold,
for every patient, the identifier clients name and each member's `ehr_id`
in that member's domain. Each member, or the integration engine beside it,
feeds its own domain: ITI-104 or the PMIR ITI-93 feed when it creates an
EHR, and once, a bulk load of the EHRs it already holds
([Keeping the PIX Manager current](identity.md#keeping-the-pix-manager-current)).
Register each domain at the Manager before you add the member.

SanteMPI is the PIX Manager the end-to-end lane verifies against two
members, and the block below is its shape: an OAuth 2.0 client-credentials
grant with the secret in the request body
([A PIX Manager verified with FerroFED: SanteMPI](identity.md#a-pix-manager-verified-with-ferrofed-santempi)).

```toml
# ferrofed.toml
[[pixm.manager]]
url = "https://mpi.example.org/fhir/"

[pixm.manager.members]
"cdr-a" = "urn:oid:2.999.10"
"cdr-b" = "urn:oid:2.999.20"

[pixm.manager.credentials.oauth2]
grant = "client_credentials"
token_endpoint = "https://mpi.example.org/auth/oauth2_token"
client_id = "ferrofed-pix-consumer"
client_auth = "client_secret_post"
client_secret_file = "/run/secrets/ferrofed/mpi-consumer-secret"
scope = "*"
```

Every member of the registry is resolved by exactly one Manager, so a
member missing from `[pixm.manager.members]` refuses the configuration.
When your hospital MPI does not answer ITI-83, run a Manager for the
federation beside it, or let PDQm translate a local identifier first
([When the hospital MPI does not speak PIXm](identity.md#when-the-hospital-mpi-does-not-speak-pixm)).

To hear of identity merges as they happen, subscribe at a PMIR Patient
Identity Registry. It sends each change to the feed route under the base,
which [step 8](#8-tls-and-the-public-address) opens to the Registry alone
([The identity feed](identity.md#the-identity-feed-pmir)):

```toml
# ferrofed.toml
[pmir]
url = "https://pmir.example.org/fhir"
callback_url = "https://gateway.example.org/fed/pmir/feed"
feed_token_file = "/run/secrets/ferrofed/pmir-feed-token"
```

## 7. The access log

Every IHE transaction the gateway makes is audited, and every access to
patient data it intermediates is recorded with the caller it verified
([The audit trail](audit.md),
[#623](https://github.com/FerroHEALTH/FerroFED/issues/623)). Both go to your Audit Record
Repository as FHIR `AuditEvent`s, from a spool on the durable volume:

```toml
# ferrofed.toml
[audit]
destination = "repository"

[audit.repository]
url = "https://arr.example.org/fhir"
hostname = "gateway.example.org"
spool_dir = "/var/lib/ferrofed/audit-feed-spool"
```

Outside the development profile, `config check` refuses this configuration
without `destination`, naming `audit.destination`. With the access log of
#623, a gateway with a registry refuses `log` as well, because the log
target names no caller and no patient.

A record counts as recorded once it is on the spool's disk. Keep the volume
on encrypted storage that outlives the container, one spool per replica
([Losing a spool](container.md#losing-a-spool)).

## 8. TLS and the public address

The listener speaks plain HTTP by default. Choose one of two shapes:

- **A reverse proxy beside the gateway** terminates TLS and reaches the
  gateway over loopback, on the same host or in the same pod. This page
  ships one for nginx.
- **TLS on the listener**, with `[server.tls]`, where the hop from your
  proxy or load balancer to the gateway crosses a network others share. Add
  `client_ca_file` to admit the proxy alone
  ([TLS on the listeners](configuration.md#tls-on-the-listeners)).

`ferrofed.conf` is the nginx configuration for the settings above, and
every release carries it with its checksum. Download it from the release
you run, check it, set your server name and certificate, and reload nginx:

```sh
for f in ferrofed.conf ferrofed.conf.sha256sum; do
  curl -LO "https://github.com/FerroHEALTH/FerroFED/releases/download/vX.Y.Z/$f"
done
sha256sum -c ferrofed.conf.sha256sum && sudo cp ferrofed.conf /etc/nginx/conf.d/
nginx -t && nginx -s reload
```

It passes these routes, each under the base:

| Route | Who reaches it |
|---|---|
| `GET` and `OPTIONS {base}/` | your clients |
| `{base}/v1/` and below | your clients; the gateway authenticates each one |
| `{base}/operator/` and below | the operator console |
| `GET {base}/.well-known/jwks.json` | every member's authorization server, and every member that verifies the caller token |
| `POST {base}/pmir/feed` | the PMIR Patient Identity Registry alone; name its address in the `allow` line |
| `{base}/health` and below | stopped at the proxy with `403`: the dependency report names every member endpoint, so probe the gateway on its own port |
| every other path | `404` at the proxy |

It also:

- passes each request URI as the client sent it, so a version uid's `::`
  and every encoded character reach the gateway unchanged;
- keeps its own timeouts above the gateway's 30-second request timeout and
  its body limit above the gateway's 1 MiB, so a client gets the gateway's
  own answer and error code;
- sets `Strict-Transport-Security`, which the gateway does not send;
- writes an access log line with the gateway's request id in place of the
  request line, because the `GET` form of a query and a read by subject
  carry the patient identifier in the query string. The request id finds
  the gateway's own log line.

`scripts/checks/production-guide.sh` runs the file in front of a gateway
with this page's configuration, and checks each route in the table, the
header and the log line.

Under the Dutch Nuts grant, the gateway also serves its DID document at the
path its `did:web` names, outside the base; pass that path as well
([The Nuts grant](onward-credentials.md#the-nuts-grant-annex-b-b4)).

## 9. Check, start and admit

Check the configuration before every start, with the same image you run.
It resolves the configuration exactly as `serve` would, secret files
included, opens no store and binds no socket:

```sh
docker run --rm -v "$PWD/ferrofed.toml:/etc/ferrofed/ferrofed.toml:ro" \
  -v "$PWD/registry.toml:/etc/ferrofed/registry.toml:ro" \
  -v "$PWD/secrets:/run/secrets/ferrofed:ro" \
  ghcr.io/ferrohealth/ferrofed@sha256:… config check --config /etc/ferrofed/ferrofed.toml
```

A refused file exits `78` with one line naming the key at fault, never a
secret ([Running it](configuration.md#running-it)). Then start the gateway
and ask for its readiness on the host:

```sh
docker compose up --wait
curl http://127.0.0.1:8080/fed/health/readiness
curl http://127.0.0.1:8080/fed/health/dependencies
```

Readiness is `200` once the gateway serves; no member and no identity
service gates it. The dependency report shows the last state the gateway
observed of each member, the resolver and the audit repository
([Health probes](health.md)).

Admit each member before it serves (§12b.1, N42a). The admission check
creates synthetic EHRs at the member and checks each condition a test can
reach, so agree the run with the member's operator first
([Admitting a node](admission.md)):

```sh
docker compose exec ferrofed /usr/local/bin/ferrofed admission check --endpoint cdr-a-query
```

When the member's governance forbids test data in its production CDR, run
the full check against a staging copy of it, and check the production CDR
with `--read-only`, which writes nothing and names every condition it leaves
unproven ([A run without writes](admission.md#a-run-without-writes)).

When both members pass, remove `status = "suspended"` from their endpoints
and reload the registry:

```sh
docker compose kill -s SIGHUP ferrofed
```

The Kubernetes example has no reload path into the pod yet
([#636](https://github.com/FerroHEALTH/FerroFED/issues/636)): change the
ConfigMap and restart the StatefulSet's pods.

## 10. The first federated query

Get a token from the issuer of [step 4](#4-client-authentication). The
reporting service's client-credentials token needs no browser:

```sh
token="$(curl -s https://idp.example.org/realms/ferrofed/protocol/openid-connect/token \
  -d grant_type=client_credentials -d client_id=example-reporting-service \
  -d client_secret="$REPORTING_SERVICE_SECRET" | jq -r .access_token)"
```

Then send one ordinary ITS-REST query for a patient both members know:

```sh
curl -s https://gateway.example.org/fed/v1/query/aql \
  -H "Authorization: Bearer $token" \
  -H 'Content-Type: application/json' -d @- <<'EOF' | jq .meta.federation
{"q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'example-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1'"}
EOF
```

The answer is one ITS-REST `RESULT_SET`, and `meta.federation` says how it
was made ([What a client gets back](../integrate/client-contract.md#what-a-client-gets-back)):

- `complete` is `true` when every member in scope answered.
- `endpoints` has one entry per member. `active` with a `row_count` means
  the member was asked and answered. `not-resolved` means the PIX Manager
  holds no `ehr_id` for the patient there: the member was not asked, and
  `complete` is `false`. `node-error`, `time-out` and `offline` name a
  member that failed, with the reason in `error`.
- The `openEHR-federation-endpoint` and `openEHR-federation-system-id`
  response headers name the members that contributed rows.

A `401` names its reason in the `WWW-Authenticate` challenge, a `403` in
its error code, and a `424` means a member that may hold the patient could
not be resolved ([Errors and status codes](../integrate/errors.md)).

## 11. Operations

Turn on the admin listener, and scrape `GET /metrics` there. It stays on
loopback unless you allow otherwise, and never sits under the base path
([Metrics](metrics.md)):

```toml
# ferrofed.toml
[metrics]
listen = "127.0.0.1:9464"
```

- **Alerts.** Load the shipped dashboard and alert rules, and add an alert
  on the scrape job's `up`
  ([Dashboard and alert rules](metrics.md#dashboard-and-alert-rules)).
- **Logs.** Ship the JSON log to a store with access control and a
  retention period. It carries no patient identifier; an
  `outbound-gate-stopped` event is an incident
  ([Hardening](hardening.md#logs-metrics-and-traces)).
- **Restarts.** On `SIGTERM` readiness turns `503`, the listener stays open
  for `drain_delay_ms`, and the requests in flight get
  `shutdown_timeout_ms`. Keep the runtime's grace period above both
  ([Stopping without dropping a request](health.md#stopping-without-dropping-a-request)).
- **Several replicas.** The replicas share nothing in memory. A follow-up
  write needs the client to name its endpoint, or the balancer to keep a
  client on one replica; the stored-query registry needs PostgreSQL; each
  replica keeps its own audit spool
  ([Running several replicas](deployment-shape.md#running-several-replicas)).
- **Upgrades.** Read the release's upgrade notes, keep a copy of the
  configuration and the image digest you run, verify the new image as in
  [step 2](#2-install), and run its `config check` over your files as in
  [step 9](#9-check-start-and-admit) before you replace the image
  ([Upgrading](upgrading.md#before-you-upgrade)). Going back has its own
  steps for the configuration, the stored-query store and the audit spools
  ([Rollback](rollback.md)).
- **Signing-key rotation.** Rotate in three rolling restarts
  ([Rotating the signing key](onward-credentials.md#rotating-the-signing-key)).
- **Hardening.** Work through the [hardening checklist](hardening.md)
  before the gateway reaches real patient data.
