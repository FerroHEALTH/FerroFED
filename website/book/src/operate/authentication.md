<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Client authentication

Every client authenticates to the gateway (§13.1, N25). A request to the
ITS-REST surface under `{base}/v1/`, and `OPTIONS {base}/`, carries an access
token the gateway verifies before it reads anything else of the request. A
request that fails is answered `401`, `403` or `503`, and no node, no
cross-reference service and no store is asked anything. There is no
unauthenticated mode. This page covers how you configure the issuers your
clients get their tokens from, what each operation requires, the purpose of
use, the edge mode for a deployment that authenticates at a proxy, and what
each node is told about the caller. FerroFED's answers to the five questions §13.4 asks
every deployment, and the template for yours, are on
[The §13.4 deployment decisions](deployment-decisions.md).

The health family (`GET {base}/health`, `/health/readiness`,
`/health/dependencies`) and `GET {base}/` stay open: they describe the
process and the product, never a patient or a member. A path outside the
base, or under it but outside `{base}/v1/` and the routes above, is a `404`.

## The token

A client sends `Authorization: Bearer <token>` (RFC 6750 §2.1), once. The
token is an RFC 9068 access token, verified as follows:

- its JOSE header names the type `at+jwt` (RFC 9068 §4) and one of the
  algorithms ES256, ES384, PS256 and RS256. A token signed with `none` or an
  HMAC algorithm is refused (RFC 8725 §3.1, §3.2);
- its `iss` names an issuer on the trust list, `[[auth.issuer]]`;
- the issuer's key set verifies the signature, under the key its `kid`
  names. A token with no `kid` is verified only when the set holds one key;
- `aud` names this gateway, `auth.audience`;
- `exp` and `nbf` hold, within `auth.clock_skew_s`;
- it carries every claim RFC 9068 §2.2 requires: `iss`, `exp`, `aud`,
  `sub`, `client_id`, `iat` and `jti`.

An issuer can be verified by RFC 7662 introspection instead. The gateway then
asks the issuer's endpoint about each token, authenticating with its own
client credentials, and admits the token when the answer is `active`, names
this gateway in `aud`, and has an `exp` that has not passed. A token that is
no JWS goes to the one issuer the deployment introspects.

The gateway never forwards the client's token to a node. Its audience is the
gateway, and one token would unlock every member that trusts its issuer (RFC
9700 §2.3). Toward each node the gateway sends that endpoint's own onward
credentials ([Configuration](configuration.md#the-file)), and the caller's
identity in a token of its own
([below](#what-a-node-is-told-about-the-caller)).

## Configuration

```toml
[auth]
audience = "https://gateway.example.org/fed"
clock_skew_s = 60

[[auth.issuer]]
issuer = "https://idp.example.org"
jwks_uri = "https://idp.example.org/jwks"
backend_clients = ["example-reporting-service"]
demographic_clients = []
```

| Key | Default | Meaning |
|---|---|---|
| `auth.mode` | `token` | `token`: the client presents a bearer token. `edge`: a proxy authenticates the client ([The edge mode](#the-edge-mode)). |
| `auth.audience` | none | The audience every token names in `aud`. Required once an issuer is listed. |
| `auth.clock_skew_s` | `60` | The leeway on `exp` and `nbf`, at most `300` (RFC 7519 §4.1.4). |
| `auth.key_set_max_age_s` | `600` | How long a key set is used before it is fetched or read again. |
| `auth.key_set_refetch_s` | `30` | The least time between two fetches of one key set. |
| `auth.fetch_timeout_ms` | `5000` | How long a key set fetch or an introspection call may take. |
| `auth.purpose_of_use.required` | `true` | Whether a token must declare a purpose of use ([Purpose of use](#purpose-of-use)). |
| `auth.issuer[].issuer` | none | The issuer identifier its tokens carry in `iss`. |
| `auth.issuer[].jwks_uri` | none | The URL of its JWK Set: `https`, or `http` to a loopback host, under every profile ([What must travel encrypted](configuration.md#what-must-travel-encrypted)). |
| `auth.issuer[].jwks_file` | none | A file holding its JWK Set, for keys handed over out of band. |
| `auth.issuer[].jwks` | none | Its JWK Set itself, as JSON text. |
| `auth.issuer[].introspection_endpoint` | none | Its RFC 7662 introspection endpoint, with `client_id` and `client_secret` or `client_secret_file`: `https`, or `http` to a loopback host, under every profile. |
| `auth.issuer[].backend_clients` | `[]` | The `client_id`s whose `system/aql-*` grant is honoured. |
| `auth.issuer[].demographic_clients` | `[]` | The `client_id`s admitted to the DEMOGRAPHIC API. |
| `auth.issuer[].patient.endpoint` | none | The registry endpoint id of the one member whose platform issues this issuer's patient tokens; setting `[auth.issuer.patient]` is the opt-in that honours its `patient/` grants ([Patient grants](#patient-grants)). |
| `auth.issuer[].patient.ehr_id_system` | none | The identifier system under which the cross-reference service knows that member's `ehr_id`s. |
| `auth.edge.header` | none | The header the edge's assertion travels in, with `mode = "edge"`. |

An issuer names exactly one of `jwks_uri`, `jwks_file`, `jwks` and
`introspection_endpoint`, and one issuer at most is introspected. A gateway
that federates and lists no issuer refuses to start, with exit code `78`,
because it could admit no one. Each bad value refuses to start under its key.

### The key set

The gateway reads an issuer's key set when a token first needs it, and again
once the set is older than `key_set_max_age_s`. A token naming a key the set
does not hold makes the gateway fetch the set once more, so a key the issuer
rotated in is picked up; that refetch happens at most once per
`key_set_refetch_s`, so tokens naming unknown keys cannot make the gateway
flood the issuer. A key set that cannot be fetched or read is a `503`
(`authentication-unavailable`), never a pass, and a failed attempt is not
repeated inside the refetch interval. A key declared for another use or
another algorithm, or a symmetric key, verifies nothing.

## Scopes per route

The token's `scope` claim is read in the SMART on openEHR grammar (ITS-REST
SMART on openEHR, master08 §Resource Scopes), with the `openehr-sdt` crate's
parser. The grammar has three resource families, `template-`, `composition-`
and `aql-`, each with the CRUDS permissions `c`, `r`, `u`, `d` and `s`. An
operation needs a granted scope of its family, in the `user/` or `system/`
compartment, whose permissions hold the operation's and whose pattern covers
the resource. Where the gateway cannot see which resource a request
addresses, only `*` or `**` covers it.

| Route | Requires |
|---|---|
| `POST` and `GET {base}/v1/query/aql` | `aql-*` with `s` |
| `POST` and `GET {base}/v1/query/{name}[/{version}]` | `aql-` with `s`, its pattern covering `{name}` |
| `GET {base}/v1/definition/query/{name}[/{version}]` | `aql-` with `r`, its pattern covering `{name}` |
| `PUT {base}/v1/definition/query/{name}[/{version}]` | `aql-` with `c`, its pattern covering `{name}` |
| `GET {base}/v1/definition/template/…` (a list) | `template-*` with `r` |
| `GET {base}/v1/definition/template/…/{template_id}…` | `template-` with `r`, its pattern covering `{template_id}` |
| `POST {base}/v1/definition/template/…` (an upload) | `template-*` with `c` |
| A COMPOSITION under `{base}/v1/ehr/{ehr_id}/…`, its versions and tags | `composition-*` with the operation's permission |
| The EHR, its `EHR_STATUS`, `DIRECTORY` and `CONTRIBUTION`s, and `GET {base}/v1/ehr?subject_id=…` | `composition-*` with the operation's permission |
| The DEMOGRAPHIC API under `{base}/v1/demographic/` | a client listed in `demographic_clients`, its token no [patient grant](#patient-grants) |
| The ADMIN API under `{base}/v1/admin/` | refused to every caller (`operation-refused`) |
| `OPTIONS {base}/` and `OPTIONS` on any path under `{base}/v1/` | a verified token, no scope, no purpose of use |
| A path or method ITS-REST does not define under `{base}/v1/` | a verified token, then `501` |

The table above is the gateway's own; the specification and the SMART on
openEHR grammar govern what each scope means, and the following choices are
FerroFED's design:

- The EHR's other resources have no SMART family. The gateway holds them to
  the `composition-` family over every template, the broadest grant over an
  EHR's content.
- The DEMOGRAPHIC API has no SMART family either, so a scope cannot grant it,
  and the gateway admits only the clients you list.
- A `patient/` grant is confined to the patient of the token's launch context
  (master07 §Context Selection), which SMART on openEHR gives as an `ehrId`
  at one platform. An `ehrId` names no namespace and means nothing outside
  the CDR that issued it, so a bare match could admit another patient's EHR
  at another node (§12.5, §12.5.2). A `patient/` grant therefore admits
  nothing at the gateway, unless you bind its issuer to one member
  ([Patient grants](#patient-grants)).
- `system/aql-*` "would grant access to all registered and ad-hoc AQL queries
  system-wide" (master08), so it counts only for a client listed in
  `backend_clients`.
- A scope the grammar reads as anything but a resource scope, `openid` or
  `launch/patient` for example, grants nothing.

The node behind the gateway still makes its own access decision (§13.2,
N26).

## Patient grants

A patient-facing app holds a token whose `patient/` scopes are confined to
one patient, named by the token's `ehrId` claim (SMART on openEHR master04
§Capabilities, master07 §Context Selection). That `ehrId` is an `ehr_id` at
the platform that issued the token, and at no other member. You opt in per
issuer by naming that platform's member and the identifier system under
which your cross-reference service knows its `ehr_id`s:

```toml
[[auth.issuer]]
issuer = "https://patient-portal.example.org"
jwks_uri = "https://patient-portal.example.org/jwks"

[auth.issuer.patient]
endpoint = "node-a-pub"
ehr_id_system = "urn:oid:2.999.9.1"
```

`config check` and the start refuse a binding whose endpoint the registry
lacks, a binding without a registry, and a binding without a cross-reference
service (`[dev]` or `[pixm]`), each naming its key. Without the section, a
`patient/` grant of that issuer admits nothing, as above.

With the opt-in, a `patient/` scope of that issuer counts on an EHR's data
alone: the `composition-` family, and an `aql-` search. When only
`patient/` scopes cover an operation, the gateway confines the request to
the token's patient:

1. It reads the `ehrId` claim from the access token, or from the
   introspection answer. A token without one, or with one that is no
   `HIER_OBJECT_ID`, is `403` (`patient-context-missing`).
2. It resolves the pair (`ehr_id_system`, `ehrId`) through the
   cross-reference service at every member (§5.2), giving the patient's own
   `{node, ehr_id}` pairs. The bound member keeps the token's `ehrId`. A
   cross-reference that cannot answer, or that places the patient under
   another `ehr_id` at the bound member, is `424`
   (`patient-context-unavailable`).
3. It admits the request only when every `{node, ehr_id}` pair it would
   send to is one of those. Everything else is `403`
   (`patient-confinement`) with nothing sent: a query for another patient,
   a query that names no patient, an `ehr_id` that is not the patient's at
   the member it would go to, a read by subject of another patient, the
   creation of an EHR and a definition request. An `ehr_id` the patient's
   own pairs do not place is never looked up in the gateway's index or
   probed for at the members, and a refused read by subject leaves no
   resolution binding for the caller and no entry in the index.
4. A query must read the patient's EHR alone: every class in its `FROM`
   is contained, conjunctively, under the one `EHR` it is scoped to
   (`FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o`). A class
   beside that `EHR` under `AND` or `OR`, a second `EHR`, or an `EHR`
   under `NOT CONTAINS` would let a node answer other patients' rows, so
   such a query is `403` (`patient-confinement`) before anything is looked
   up (§7.1).
5. A query or a read by subject that names a patient is first resolved
   at the bound member alone. When the patient there is not the token's
   own `ehrId`, the request is `403` (`patient-confinement`) before any
   localizer (XCPD or the NVI), consent pre-filter or other member is
   asked about it, so no ITI-55 exchange or audit event is made for that
   patient.

A token whose resource scopes are all `patient/` never reaches the
DEMOGRAPHIC API, whether or not its issuer is bound and even when its client
is listed in `demographic_clients`: a patient scope reaches its patient's
own EHR alone (master08 §Resource Scopes). The gate refuses it `403`
(`patient-confinement`) before anything is resolved.

The gateway never compares the bare `ehrId` with an `ehr_id` at another
member, because one `ehr_id` can name another patient's EHR there (§12.5.2).
Each node is told the patient's own `ehr_id` at that node in the `ehrId`
claim of the caller's token, with only the `patient/` scopes that cover the
operation in `scope`, so the node can enforce the grant as well (N26;
[below](#what-a-node-is-told-about-the-caller)). The outbound gate holds
that claim to the `ehr_id` the request to that node is composed for: the
one its query is scoped to, or the one its path names. A request whose
token would tell the node another `ehr_id`, or that names none, is never
sent (§5.4.1, N33). No specification defines a patient grant across nodes,
so this opt-in is FerroFED's own design.

The residual risk is the cross-reference service. The confinement is only
as correct as its link between the `ehrId` at the bound member and the
patient's `ehr_id` at every other member: a wrong link admits the wrong
EHR at that member. Bind an issuer only to the member whose platform issued
its tokens, and only when your cross-reference service holds that member's
`ehr_id`s as identifiers under `ehr_id_system`. Record the choice in your
[§13.4 decisions](deployment-decisions.md#5-what-the-technique-does-not-cover).

## Purpose of use

A token declares why its data is requested (§13.4: a deployment must not
rely on a node inferring it from the query). The gateway reads it from the
IHE IUA extension, `extensions.ihe_iua.purpose_of_use`, an array of FHIR
`Coding` (ITI TF-2 3.71.4.2.2.1.1), or from RFC 9396
`authorization_details[].purpose_of_use`, written `system|code` (Annex B
§B.4a.3). A token that declares none is a `403`
(`purpose-of-use-required`) on every route that reaches a node. A deployment
relaxes the rule with `auth.purpose_of_use.required = false`, and records why
in its [§13.4 decisions](deployment-decisions.md#3-purpose-of-use). The
organisation the caller acts for is read from
`extensions.ihe_iua.subject_organization_id`. The IUA `person_id` claim, a
patient identifier, is never read.

## The edge mode

A deployment that authenticates its clients at a proxy configures the edge
mode explicitly; it is never a default. The proxy authenticates the client by
whatever means it has, then signs an assertion of who it authenticated, an
RFC 9068 access token whose `aud` is this gateway, and sends it in the header
`auth.edge.header`. The gateway verifies the assertion against the edge's key
set exactly as it verifies a token, reads the caller from it, and logs the
identity the edge asserted, its issuer, subject and client, as the security
event `edge-identity-asserted`. A bearer token in `Authorization` admits no
one in this mode.

```toml
[auth]
mode = "edge"
audience = "https://gateway.example.org/fed"

[auth.edge]
header = "ferrofed-edge-assertion"

[[auth.issuer]]
issuer = "https://edge.example.org"
jwks_uri = "https://edge.example.org/jwks"
```

The edge mode takes a signed assertion, never a header trusted for the
address it came from. A forwarded header "cannot be relied upon to be
correct, as it may be modified ... by every node on the way to the server",
and trusting proxies by address leaves it open to anyone "with access to the
network" between them (RFC 7239 §8.1). A signature holds wherever the request
travelled.

## Failure statuses

| Status | Code | When |
|---|---|---|
| `401` | `unauthenticated` | no token, or one the gateway does not accept; the `WWW-Authenticate` challenge names the reason in `error_description` |
| `403` | `scope-insufficient` | no granted scope covers the operation, or the client is not admitted to the DEMOGRAPHIC API |
| `403` | `purpose-of-use-required` | the token declares no purpose of use |
| `403` | `operation-refused` | the ADMIN API, refused to every caller |
| `403` | `patient-context-missing` | only a bound issuer's `patient/` scope covers the operation, and the token carries no `ehrId` |
| `403` | `patient-confinement` | the request reaches beyond the patient a `patient/` grant is confined to, or a patient grant addresses the DEMOGRAPHIC API |
| `424` | `patient-context-unavailable` | the patient of a `patient/` grant cannot be resolved at every member |
| `503` | `authentication-unavailable` | the issuer's key set or introspection endpoint cannot be had |

Every refusal is logged under the `ferrofed::security` target as
`caller-refused`, with its reason and the gateway's request id, never the
token ([Errors and status codes](../integrate/errors.md)).

## The quickstart issuer

The compose quickstart trusts one development issuer.
`scripts/quickstart/token.sh` generates its RSA key pair with `openssl` on
its first run, writes the key set to `docker/quickstart/issuer/jwks.json`,
which the gateway reads, and prints a token valid for an hour, with every
scope in the `user/` compartment and the purpose of use `TREAT`. The private
key stays in that directory, which git ignores. It is a development key and
not for anything real.

```sh
curl -s http://127.0.0.1:8080/v1/query/aql \
  -H "Authorization: Bearer $(scripts/quickstart/token.sh)" \
  -H 'Content-Type: application/json' -d '{"q": "…"}'
```

## What a node is told about the caller

The gateway authenticates to each node as itself, with that endpoint's own
credentials ([OAuth 2.0 to a node](configuration.md#oauth-20-to-a-node)),
and tells the node who asks in a header of its own (§13.1, N24, N25, §12.4).
Where a node's authorization server exchanges tokens (RFC 8693), the gateway
also asks it for a token per caller, with the caller's verified token as the
subject and only the caller's scopes that cover the operation; the caller's
token reaches that authorization server alone, never the node, and a caller
the edge mode asserted has no token to exchange, so that node is
`node-error` with nothing sent
([A token per caller](configuration.md#a-token-per-caller-token-exchange)).
Every request it sends a node carries `openEHR-federation-client`: the
federated query, every routed read and write, a definition request, the
ask-all probe and the read of an EHR by subject. The value is a compact JWS
the gateway signs for that one node with the current key of `[signing]`,
the key whose public half it publishes at `{base}/.well-known/jwks.json` and
declares as `federation.auth.jwks_uri` in `OPTIONS {base}/`. Its JOSE header
names `alg` `ES384`, the key's `kid` and `typ`
`openehr-federation-client+jwt`. Its claims:

| Claim | Value |
|---|---|
| `iss` | the gateway: the `client_id` of the endpoint's `oauth2` grant, the `iss` of its client assertions there, or else `federation.id` |
| `aud` | the node's endpoint id in the registry |
| `iat`, `exp` | seconds since the epoch; `exp` is 60 seconds after `iat` |
| `jti` | a fresh version 4 UUID per token |
| `sub` | the caller's `sub`, as the gate verified it |
| `iss_upstream` | the issuer that vouched for the caller |
| `verified_by` | `signature`, `introspection`, or `edge` for an identity the edge asserted |
| `subject_organization_id` | the caller's organisation, when its token names one (IHE IUA) |
| `purpose_of_use` | each purpose of use the token declares, as `{"system", "code"}` (IHE IUA, HL7 v3 `PurposeOfUse`) |
| `scope` | the caller's scopes as granted; under a [patient grant](#patient-grants), only the `patient/` scopes that cover the operation |
| `ehrId` | under a [patient grant](#patient-grants) only: the patient's own `ehr_id` at this node (SMART on openEHR master04 §Capabilities) |

The token never carries the caller's own token, its `client_id`, or a
patient identifier: an IUA `person_id` is never read (N33). The outbound
gate reads every caller claim as it reads the rest of a request, and a
request whose caller claims would carry the identifier its query was
resolved on is refused with nothing sent, logged as a security event and
answered `500` (§5.4.1). Under client credentials the node's grant to the
gateway is a `system/` scope, so the caller's narrower scopes travel in
`scope` for the node to apply (N26).

A request that reaches the dispatcher without a verified caller is the
gateway's own failure: it is answered `500` and no node is sent anything.
The gateway's own requests for its operator, the
[admission check](admission.md) and the redistribution of a held stored
query on the admin listener, carry the header with the gateway as `sub` and
no caller claim.

`[signing]` is therefore required whenever a registry is configured, by
`registry.document` or by `[registry.mcsd]`: a federating gateway without a
signing key refuses to start, naming the key
([Signing keys](configuration.md#signing-keys-and-the-jwk-set)). The
specification leaves end-user conveyance open (§13.1), so the header and its
claims are FerroFED's own design.

### Verifying it at the node

A node that admits FerroFED reads the header before it decides what to
release, and audits who asked:

1. At admission, record the gateway's `jwks_uri` (its `OPTIONS {base}/`
   declares it as `federation.auth.jwks_uri`) and the `iss` it will see: the
   `client_id` the node's authorization server registered for the gateway,
   or the federation id where the gateway sends no OAuth 2.0 grant.
2. Read exactly one `openEHR-federation-client` value; refuse a request with
   none or with more than one.
3. Check the JOSE header: `typ` is `openehr-federation-client+jwt` and `alg`
   is `ES384`. Refuse any other algorithm, `none` included (RFC 8725 §3.1).
4. Verify the signature with the key of the gateway's JWK Set whose `kid`
   the header names. Fetch the set again when the `kid` is unknown: during a
   rotation the set publishes the current and the previous key.
5. Check `iss` against step 1, `aud` against the node's own endpoint id, and
   `exp`, allowing a few seconds of clock skew (RFC 7519 §4.1.4).
6. Where the gateway authenticates with an OAuth 2.0 grant, check that `iss`
   equals the `client_id` of the access token on the same request.
7. Apply `scope` and `purpose_of_use` to what you release, confine a
   `patient/` scope to the EHR the `ehrId` claim names, and record `sub`,
   `iss_upstream`, `verified_by` and `subject_organization_id` in your audit
   trail. Consent stays your own check (§13.2, N27).
