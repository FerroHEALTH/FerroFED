<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Client authentication

Every client authenticates to the gateway (§13.1, N25). A request to the
ITS-REST surface under `{base}/v1/`, and `OPTIONS {base}/`, carries an access
token the gateway verifies before it reads anything else of the request. A
request that fails is answered `401`, `403` or `503`, and no node, no
cross-reference service and no store is asked anything. There is no
unauthenticated mode. This page covers how you configure the issuers your
clients get their tokens from, what each operation requires, the purpose of
use, and the edge mode for a deployment that authenticates at a proxy. What
each node is told about the caller is on
[What a node is told about the caller](caller-token.md). FerroFED's answers to the five questions §13.4 asks
every deployment, and the template for yours, are on
[The §13.4 deployment decisions](deployment-decisions.md).

The health family (`GET {base}/health`, `/health/readiness`) and
`GET {base}/` stay open. They name no patient and no member, and
`GET {base}/` names the product version. Restrict the root at the proxy if
your network should not learn the version
([Hardening](hardening.md#network-placement)). The dependency report,
`GET {base}/operator/dependencies`, names every member endpoint by its id
with the state the gateway last observed of it, so it sits on the
[operator surface](#the-operator-surface), behind the operator scope. A
path outside the base, or under it but outside `{base}/v1/` and the routes
above, is a `404`.

## The token

A client sends `Authorization: Bearer <token>` (RFC 6750 §2.1), once. The
gateway checks it through `ferrohealth-smart`, the SMART and OAuth 2.0
resource-server crate every FerroHEALTH product runs on, which checks tokens
and never issues them. The token is a JWT access token, verified as follows:

- the bearer value is a `b64token` (RFC 6750 §2.1);
- its `iss` names an `[[auth.issuer]]` the gateway trusts, whose key set
  verifies the signature under the key its `kid` names. A token with no
  `kid` is verified only when the set holds one candidate key;
- its JOSE header names an algorithm the issuer's `algorithms` admits, by
  default RS256, RS384, PS256, PS384, ES256 and ES384. A token signed with
  `none` or an HMAC algorithm is refused, and so is an HMAC algorithm in
  `algorithms` at load (RFC 8725 §3.1, §3.2);
- under the default `profile = "rfc9068"`, its header names the type
  `at+jwt` (RFC 9068 §4) and it carries every claim RFC 9068 §2.2 requires:
  `iss`, `exp`, `aud`, `sub`, `client_id`, `iat` and `jti`. A `sub` that is
  empty or blank names no caller and is refused;
- `aud` names one of the issuer's `audience`;
- `exp`, `nbf` and `iat` hold within the issuer's `clock_skew_seconds`;
- the token names a caller the issuer's `kind` admits
  ([Who the caller is](#who-the-caller-is)).

Every token that fails a check gets the same `401` with
`WWW-Authenticate: Bearer realm="ferrofed", error="invalid_token"`, and no
`error_description` (RFC 9068 §4, RFC 6750 §3.1): a refusal that named the
failed check would teach a forger which claim to fix next. Which check failed
reaches the security log alone, as `check` on the `caller-refused` event. A
request with no credential, or with a credential in another scheme, gets
`401` with the challenge and no error code (RFC 6750 §3.1). Two
`Authorization` fields are a `400` (`credential-malformed`) with
`error="invalid_request"`.

An issuer can be verified by RFC 7662 introspection instead, with
`mode = "introspection"` and an `[auth.issuer.introspection]` table. The
gateway reads the issuer's metadata when a token first needs it, then asks
the issuer's endpoint about each token, authenticating with its own client
credentials, and admits the token when the answer is `active`, names one of
the issuer's `audience` in `aud`, and has an `exp` that has not passed. A
token that is no JWS goes to the one issuer the deployment introspects.

The gateway never forwards the client's token to a node. Its audience is the
gateway, and one token would unlock every member that trusts its issuer (RFC
9700 §2.3). Toward each node the gateway sends that endpoint's own onward
credentials ([Onward credentials](onward-credentials.md)), and the caller's
identity in a token of its own
([below](#what-a-node-is-told-about-the-caller)).

### Who the caller is

Every `[[auth.issuer]]` declares its `kind`, whose tokens it signs, and
`ferrohealth-smart` classifies each token it verifies under it:

- `persons`: the professionals of your deployment, signing in through your
  own clients. A token whose `sub` is its `client_id` is a system acting
  without a user context; any other token is admitted only through a client
  the issuer lists in `first_party_clients`, or a delegate it lists in
  `delegates`, and an RFC 8693 `act` claim only when its actor is a listed
  delegate;
- `patients`: patients, each token carrying `patient/` scopes and no `user/`
  or `system/` scope. An issuer bound to a member with
  `[trust.issuer.patient]` is usually one;
- `machines`: systems under the client credentials grant alone;
- `federation`: another party's issuer, with its `[[auth.federation]]`
  table.

A token that matches no step for its issuer's kind is refused as an invalid
token. The gateway counts a system caller as no natural person
([Professionals and assurance](#professionals-and-assurance)). The gateway
admits no HTTP Basic caller: a machine, such as a scraper of the admin
listener or a PMIR Registry, authenticates with a client-credentials token
(RFC 6749 §4.4). Any `[[auth.basic.users]]` entry refuses the start, naming
the user; one that lists a `system/` scope is refused by `ferrohealth-smart`
first (GHSA-jhg2-jwjp-hq74).

## Configuration

Two sections configure client authentication. `[auth]` is the section of
`ferrohealth-smart`, the same in every FerroHEALTH product: one
`[[auth.issuer]]` table per issuer, with its kind, its profile, its
audiences, its token rules and its assurance table, and the product scope.
`[trust]` is FerroFED's own policy over the callers `[auth]` verifies: what
each issuer's tokens are honoured for, the edge mode and the purpose-of-use
rule.

```toml
[auth]
product_scope = "__ferrofed.operator"

[[auth.issuer]]
issuer = "https://idp.example.org"
kind = "persons"
audience = ["https://gateway.example.org/fed"]
first_party_clients = ["example-clinical-app"]

[[trust.issuer]]
issuer = "https://idp.example.org"
backend_clients = ["example-reporting-service"]
demographic_clients = []
```

| Key | Default | Meaning |
|---|---|---|
| `auth.issuer[].issuer` | none | The issuer identifier its tokens carry in `iss`: `https`, or `http` to a loopback host (RFC 8414 §2). An issuer URL that carries a user name or password refuses the start (OpenID Connect Core 1.0 §1.2). Each `auth.issuer[]` key below is the issuer's own; no `[auth]` key stands in for one an issuer leaves out. |
| `auth.issuer[].kind` | none | Whose tokens the issuer signs: `persons`, `patients`, `machines` or `federation` ([Who the caller is](#who-the-caller-is)). Required. |
| `auth.issuer[].profile` | `rfc9068` | The preset the issuer's tokens are read under: `rfc9068`, a standards-only authorization server; `entra`, `keycloak` and `okta`, each that product's token shape; `nl`, the Dutch harmonised authentication of Annex B. Any profile key set on the table, such as `token_types`, `required_claims` or `scope_claim`, is set over the preset. |
| `auth.issuer[].first_party_clients` | `[]` | The `client_id`s of your own applications, the clients a `persons` token of the issuer is admitted through. |
| `auth.issuer[].delegates` | `[]` | The third-party clients, each a `client_id`, and the RFC 8693 actors, each `{ issuer, sub }`, a `persons` token of the issuer is admitted through as a delegated person. |
| `auth.issuer[].audience` | the public URL | The audiences, of which every token of the issuer names one in `aud` (RFC 9068 §4). Unset, the `gateway.public_url` as written; an issuer without an audience refuses the start. |
| `auth.issuer[].algorithms` | six | The signature algorithms admitted: RS256, RS384, PS256, PS384, ES256 and ES384. An HMAC algorithm refuses the start. |
| `auth.issuer[].clock_skew_seconds` | `60` | The leeway on `exp`, `nbf` and `iat`, at most `300` (RFC 7519 §4.1.4). |
| `auth.issuer[].jwks_max_age_seconds` | `600` | How long a key set is used before it is read again. |
| `auth.issuer[].jwks_refresh_interval_seconds` | `60` | The least time between two reads of one key set, after an unknown `kid` or a failed read. |
| `auth.issuer[].jwks_file` | none | A JWK Set file handed over out of band, read in place of the `jwks_uri` the issuer's metadata names. A file holding a symmetric key or private key material refuses the start, naming the key's index and `kid` and never a value. |
| `auth.issuer[].timeout_ms` | `10000` | How long one call to the authorization server may take: its metadata, its key set, or an introspection. |
| `auth.issuer[].role_claims` | `["roles", "groups", "entitlements"]` | The claims `ferrohealth-smart` reads a token's roles from, each a dotted path. The gateway grants nothing by these roles: the professional's roles it conveys come from the IUA `subject_role`. |
| `auth.issuer[].mode` | `jwt` | `jwt`: the token is a JWT checked against the key set. `introspection`: the token is checked at the issuer's RFC 7662 endpoint, under `[auth.issuer.introspection]`. One issuer at most is introspected. |
| `auth.issuer[].introspection.endpoint` | the metadata's | The introspection endpoint, in place of the one the issuer's metadata names. |
| `auth.issuer[].introspection.client` | none | The gateway's client authentication at the endpoint: `client_id` with `client_secret` or `client_secret_file`, or a `private_key_jwt` key, as an `[<lane>.credentials.oauth]` table takes them. |
| `auth.ca_file` | none | A PEM file of trust roots added to the platform's for every issuer's metadata, key set and introspection, so an issuer behind a private certificate authority is reached without changing the trust of the whole process. A file that cannot be read, or holds no certificate, refuses the start. |
| `auth.realm` | `ferrofed` | The realm every `WWW-Authenticate` challenge names. |
| `auth.product_scope` | none | The scope that admits a caller to the read-only [operator surface](#the-operator-surface) and to the write actions of the [admin listener](metrics.md#who-the-admin-listener-serves): a string starting with `__` or a full URI you control (SMART App Launch 2.2.0, Scopes and Launch Context, Extensibility). Without it, no caller reaches either. |
| `auth.basic.users[]` | none | Refused: the gateway admits no Basic caller. A machine authenticates with a client-credentials token of an `[[auth.issuer]]`, named in `metrics.scrape_clients` or `pmir.feed_clients`. |
| `auth.issuer[].assurance.claim` | `acr` | The claim, a dotted path, that carries the authentication assurance of this issuer's tokens ([The assurance level](#the-assurance-level)). |
| `auth.issuer[].assurance.levels` | `{}` | The level, `low`, `substantial` or `high`, each claim value stands for. |
| `auth.issuer[].assurance.minimum` | none | The least level a patient-data request needs; some value must reach it. |
| `auth.dpop.external_url` | none | The gateway's base URL as its clients address it; setting `[auth.dpop]` admits `DPoP` requests ([`DPoP`](#dpop)), and a proof's `htu` is compared with this URL, the request path appended. |
| `auth.dpop.max_age_seconds`, `.clock_skew_seconds`, `.algorithms` | `60`, `10`, the six asymmetric algorithms | How long after its `iat` a proof is accepted, at most 300; how far an `iat` may lie ahead, at most 300; and the algorithms a proof may be signed with. |
| `trust.mode` | `token` | `token`: the client presents a bearer token. `edge`: a proxy authenticates the client ([The edge mode](#the-edge-mode)). |
| `trust.purpose_of_use.required` | `true` | Whether a token must declare a purpose of use ([Purpose of use](#purpose-of-use)). |
| `trust.issuer[].issuer` | none | The issuer this entry's policy is for, which an `[[auth.issuer]]` table trusts. |
| `trust.issuer[].backend_clients` | `[]` | The `client_id`s whose `system/aql-*` grant is honoured. |
| `trust.issuer[].demographic_clients` | `[]` | The `client_id`s admitted to the DEMOGRAPHIC API. |
| `trust.issuer[].patient.endpoint` | none | The registry endpoint id of the one member whose platform issues this issuer's patient tokens; setting `[trust.issuer.patient]` is the opt-in that honours its `patient/` grants ([Patient grants](patient-grants.md)). |
| `trust.issuer[].patient.ehr_id_system` | none | The identifier system under which the cross-reference service knows that member's `ehr_id`s. |
| `trust.issuer[].patient.act_is_representative` | `false` | Whether the RFC 8693 `act` claim of this issuer's patient tokens names a proxy or a legal representative who acts for the patient (Regulation (EU) 2025/327 Art 4(2)), so a document such a token inserts names that representative ([Patient grants](patient-grants.md)). |
| `trust.issuer[].requester` | none | Refused: who asks for the data is read from the `nl.professional_number`, `nl.role`, `nl.ura` and `nl.organisation_type` fields of the issuer's `[[auth.federation]]` table, which the [Mitz consent pre-filter](consent.md#dutch-consent-nl_gfmitz) asks about. |
| `trust.issuer[].client_tokens_act_for_professional` | none | Refused: a token `ferrohealth-smart` classifies as a system's reaches no patient data ([Professionals and assurance](#professionals-and-assurance)). |
| `trust.issuer[].professional_issuing_authority` | none | The string claim of this issuer's tokens that carries the name of the agency that issued the professional's `national_provider_identifier` (Implementing Regulation (EU) 2026/2099 Annex Table 1 `issuing_authority_name`), which each node is told. IHE IUA defines no claim for it. Without it, none is read or conveyed. |
| `trust.issuer[].assurance` | none | Refused: set the issuer's `[auth.issuer.assurance]` table instead. |
| `federation.contact_point[].issuer` | none | A national contact point: an `[[auth.issuer]]` of `kind = "federation"` whose `[[auth.federation]]` table sets `contact_point = true` ([National contact points](#national-contact-points)). Every such issuer needs one `[[federation.contact_point]]`, and one naming any other issuer is refused. |
| `federation.contact_point[].correlation_header` | none | The request header the contact point's connector sends its correlation identifier in, recorded in the access record. A header that carries a credential (`Authorization`, `Proxy-Authorization`, `Cookie`, `DPoP`) is refused. |
| `federation.contact_point[].additional_countries` | `[]` | The ISO 3166-1 alpha-2 codes the contact point's `[[auth.federation]] countries` may name, in addition to the 27 EU Member States. Use it for a third country whose contact point the Commission connected to MyHealth@EU (Regulation (EU) 2025/327 Art 24(3)). Neither act names the EEA States, so add one here when your deployment serves it. A code that is not two upper-case letters, or that is already a Member State, is refused. |
| `trust.edge.header` | none | The header the edge's assertion travels in, with `trust.mode = "edge"`. |

`[auth] enabled = false` is refused: the gateway has no unauthenticated mode.
So are `auth.scopeless_subjects`, `auth.capabilities` and
`auth.scopes_supported`, which the gateway reads nowhere: it serves no SMART
discovery document and grants nothing without a scope. A key `[auth]` held
for every issuer before `ferrohealth-smart` 0.9 (`audience`, `algorithms`,
`clock_skew_seconds`, `jwks_max_age_seconds`, `jwks_refresh_interval_seconds`,
`jwks_file`, `timeout_ms`, `mode`, `role_claims`) is refused at the section,
naming the `[[auth.issuer]]` table it moved to, and so is an issuer written
as `[auth] issuer = "…"`. A `[[trust.issuer]]` entry names an
`[[auth.issuer]]`. A gateway that federates and lists no
issuer refuses to start, with exit code `78`, because it could admit no one.
Each bad value refuses to start under its key; a value `ferrohealth-smart`
refuses is named under `auth` with the crate's rule.

A [reload](registry.md#reloading-the-registry), on `SIGHUP` or
`POST /admin/reload`, applies a changed `[auth]` and `[trust]` to every
request that arrives after it, on the client face and the admin listener,
and reads each issuer's metadata and key set again; a changed `audience`
of an issuer the gateway already trusts waits for a restart. A reload whose `[auth]` does not load, or trusts no
issuer of a federating gateway, is refused and the running issuers stay.

### The key set

The gateway reads an issuer's metadata at its RFC 8414 location, with
OpenID Connect Discovery 1.0 as the fallback, and the key set its
`jwks_uri` names, when a token of that issuer first needs them; with the
issuer's `jwks_file` it reads the file and no metadata. The set is read again
once it is older than `jwks_max_age_seconds`. A token naming a key the set
does not hold makes the gateway read the set once more, so a key the issuer
rotated in is picked up; that read happens at most once per
`jwks_refresh_interval_seconds`, so tokens naming unknown keys cannot make
the gateway flood the issuer. Metadata or a key set that cannot be read is a
`503` (`authentication-unavailable`), never a pass, and a failed read is not
repeated inside the refresh interval. A key declared for another use or
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
| The DEMOGRAPHIC API under `{base}/v1/demographic/` | a client listed in `demographic_clients`, its token no [patient grant](patient-grants.md) |
| The ADMIN API under `{base}/v1/admin/` | refused to every caller (`operation-refused`) |
| `GET {base}/operator/incidents`, `/operator/creating-systems`, `/operator/stored-queries`, `/operator/stored-queries/{name}`, `/operator/dependencies` and `/operator/deployment` | a verified token carrying `auth.product_scope`, no purpose of use |
| A write action on the admin listener, such as `POST /admin/stored-queries/{name}/{version}/distribute` or `POST /admin/ehr-ids/{ehr_id}/clear`, and the export `POST /admin/identity-state/export` | a verified token carrying `auth.product_scope`, no purpose of use ([Metrics](metrics.md#who-the-admin-listener-serves)) |
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
  ([Patient grants](patient-grants.md)).
- `system/aql-*` "would grant access to all registered and ad-hoc AQL queries
  system-wide" (master08), so it counts only for a client listed in
  `backend_clients`.
- A scope the grammar reads as anything but a resource scope, `openid` or
  `launch/patient` for example, grants nothing.

The node behind the gateway still makes its own access decision (§13.2,
N26).

## The operator surface

Six read-only routes on the client listener give the
[operator console](operator-console.md) and your monitoring what no other
surface carries:

| Route | Answers |
|---|---|
| `GET {base}/operator/incidents` | how many integrity incidents of each kind the gateway emitted since it started, and the last 20 of each kind |
| `GET {base}/operator/creating-systems` | the `creating_system_id` routing table: each member's own `system_id`, each `[[creating_system]]` mapping, and each learned or withdrawn mapping |
| `GET {base}/operator/stored-queries` | every stored-query version the gateway holds, with its AQL and whether it is read-only |
| `GET {base}/operator/stored-queries/{name}` | every version of the one stored query whose qualified name is `{name}`, such as `org.example::patient_compositions`, lowest first; a `404` (`stored-query-unknown`) when the gateway holds none, and a `400` (`query-name-invalid`) for a name ITS-REST does not admit |
| `GET {base}/operator/dependencies` | the state the gateway last observed of each member endpoint and service ([Health probes](health.md#the-dependency-report)) |
| `GET {base}/operator/deployment` | how the process is deployed: its profile, the product version and the replica |

The incident report is `{"counts": {...}, "recent": [...], "cleared":
[...]}`: `counts` names every incident kind with how many the gateway
emitted since it started, and `recent` lists the kept incidents oldest
first, each with its `kind`, its RFC 3339 `at`, its `description`, and the
`creating_system_id`, `ehr_id`, `detection`, `endpoints` and `nodes` it is
about. The gateway keeps the last 20 of each of the five kinds, so the
report is one page of at most 100 incidents and takes no paging parameters.
`cleared` lists the last 20 collisions you cleared on this replica, oldest
first, each with its `at`, its `ehr_id` (a bare UUID, else `null`) and the
member node ids as `claimants`; a clear removes no incident
([Clearing a held collision](registry.md#clearing-a-held-collision)).

The routing table and both stored-query routes answer one page, as
`{"items": [...], "offset": 0, "total": 2}`, where `total` counts every row
and `items` holds at most `limit` of them from `offset`. Both query
parameters are optional: `offset` defaults to `0` and `limit` to `100`, its
most. A `limit` of `0` or over `100` is a `400` (`parameter-invalid`). A
stored query is the ITS-REST `StoredQuery`, `name`, `type`, `version`,
`saved` and `q`, with one member beside them, `read_only`. It is `true` when
the gateway takes no `PUT` under that name: the name sits in the gateway's
own namespace `eu.ferrofed.eehrxf`, or the registry reads the read-only
definition files (`backend = "files"`). The embedded and PostgreSQL stores
take new versions, so a deployment's own query there is `false`. The bodies are the types of `ferrofed_operator_api::operator`,
which the operator console reads as well, and the stored query is the
`openehr-its` `StoredQuery`, so the gateway and the console share one
definition of each.

The deployment report is one object:

```json
{
  "mode": "development",
  "product": "FerroFED",
  "version": "0.0.11",
  "replica": "ferrofed-7d9c4b5f6-x2k8q"
}
```

`mode` is the `profile` the process started with, `production` or
`development`, and a reload never changes it
([Configuration](configuration.md#the-startup-banner)). The gateway knows no
other profile, so the report names no other. `product` and `version` are
those of `GET {base}/`.
The report carries no setting value, URL, credential or patient
identifier.

Each gateway process keeps its own incidents, learned routing table and
dependency states, so every answer of the five routes names the replica
that gave it: in the `ferrofed-replica` response header, on a `400` as
well, and in a `replica` member of each report, such as `"replica":
"ferrofed-7d9c4b5f6-x2k8q"`. The name is `[server] replica`, or the host
name the process runs with, which Kubernetes sets to the pod's name, or
`unnamed` without either ([Configuration](configuration.md#the-replica-name)).
Behind a load balancer each request reaches one replica, so read a report
as that replica's view; to see every replica, ask each at its own address.

A caller reaches them only with a token whose `scope` holds
`auth.product_scope`, as one whole scope token, from any trusted issuer:

```toml
[auth]
product_scope = "__ferrofed.operator"
```

The product scope is a string starting with `__` or a full URI you control,
the two forms SMART App Launch 2.2.0 (Scopes and Launch Context,
Extensibility) leaves to a server's own scopes; another refuses the start. A
token without it is a `403` (`scope-insufficient`), and so is a token
whose scope only contains it, starts with it or differs from it in case.
A gateway that names no `product_scope` admits no operator at all. No purpose of use is
asked, because the routes answer no clinical data: routing ids, counts and
stored definitions only. An incident names an `ehr_id` only when it is a
bare UUID, and a stored definition names no patient, because the gateway
refuses one that does before it is held (§5.4.1, N33). The routes are not
part of the ITS-REST surface. No specification governs them: they are
FerroFED's own design.

## Purpose of use

A token declares why its data is requested (§13.4: a deployment must not
rely on a node inferring it from the query). The gateway reads it from the
IHE IUA extension, `extensions.ihe_iua.purpose_of_use`, an array of FHIR
`Coding` (ITI TF-2 3.71.4.2.2.1.1), or from RFC 9396
`authorization_details[].purpose_of_use`, written `system|code` (Annex B
§B.4a.3). A token that declares none is a `403`
(`purpose-of-use-required`) on every route that reaches a node. A deployment
relaxes the rule with `trust.purpose_of_use.required = false`, and records why
in its [§13.4 decisions](deployment-decisions.md#3-purpose-of-use). The
organisation the caller acts for is read from
`extensions.ihe_iua.subject_organization_id`. The IUA `person_id` claim, a
patient identifier, is never read.

### Emergency purposes

A caller asserts an access in the vital interests of the patient, which
Regulation (EU) 2025/327 Art 11(5) lets reach data the patient restricted,
by declaring an emergency purpose of use in its token. The gateway never
infers one. It conveys every declared purpose to each node unchanged, and
the node decides what to release (§13, N26); the gateway sends the same
request it would send without it and never overrides a node's refusal.

You name the purposes that mark an access as an emergency access in the
access log, `[[access_log.emergency_purpose]]`, each a `code` and its
`system`, matched exactly ([Emergency access](access-log.md#emergency-access)).
The HL7 v3 `ActReason` codes a deployment maps are `BTG`, "break the
glass", whose definition "may include override of subject of care consent
directive restricting access", and, where your national rules call for it,
`ETREAT`, "Emergency Treatment"
(<https://terminology.hl7.org/CodeSystem-v3-ActReason.html>). Ask your
issuers which they put in `purpose_of_use`, and declare those:

```toml
[[access_log.emergency_purpose]]
system = "http://terminology.hl7.org/CodeSystem/v3-ActReason"
code = "BTG"
```

## Professionals and assurance

Regulation (EU) 2025/327 asks an EHR system "designed to be used by health
professionals" to "provide reliable mechanisms for the identification and
authentication of health professionals" (Annex II 3.1). Its Art 12 admits to
a health professional access service only professionals holding electronic
identification means recognised under Regulation (EU) No 910/2014 or
compliant with the Art 36 common specifications. For a cross-border exchange,
Implementing Regulation (EU) 2026/2099 Art 6(3) has the entity a Member State
lists authenticate the professional at assurance level "substantial", and at
level "high" from 26 March 2032. The gateway verifies the token; your issuer
authenticates the person. Two rules connect the two, and both hold on every
request that reaches patient data: a query, the EHR API and the DEMOGRAPHIC
API. A definition request, `OPTIONS` and the operator surface reach none, so
neither rule applies to them.

### A natural person

A token names no natural person when `ferrohealth-smart` classifies it as a
system's under its issuer's profile, as a token whose `sub` is its
`client_id` is ("If known, unique identifier of the user; the client\_id
otherwise", IHE IUA ITI TF-2 3.71.4.2.2.1), or when only `system/` scopes
cover the operation, which SMART on openEHR grants "to backend applications
acting without a user context" (master08). Such a token is refused patient
data with `401 natural-person-required`, whatever professional it names.

A client application that acts for a professional signs that professional
in at its issuer, so the token names the person and the client both: the
issuer's profile classifies it as a delegated person, and it reaches patient
data as the person does.

### The assurance level

You declare per issuer, in the `[auth.issuer.assurance]` table of
`ferrohealth-smart`, which claim carries the assurance of its tokens, which
values stand for which level, and the least level patient data needs:

```toml
[[auth.issuer]]
issuer = "https://idp.example.org"
kind = "persons"
# ...

[auth.issuer.assurance]
claim = "acr"
minimum = "substantial"

[auth.issuer.assurance.levels]
"urn:example:loa:substantial" = "substantial"
"urn:example:loa:high" = "high"
```

The levels are the three of Regulation (EU) No 910/2014 Art 8(2): `low`,
`substantial` and `high`. The claim defaults to `acr`, which an access token
may carry (RFC 9068 §2.2.1); the gateway reads it from the token, or from
the introspection answer, as one string. No specification the gateway binds
says which `acr` values stand for which level, so you list the values your
issuer writes. A token that carries no value, a value listed at no level, or
a level below `minimum` is refused patient data with
`401 authentication-assurance-insufficient`, its challenge
`error="insufficient_user_authentication"` (RFC 9470 §3), so a client knows to
authenticate its user again.

An issuer without `[auth.issuer.assurance]` has no level read or required,
and `config check` names each such issuer in a note. Set it for every issuer
whose tokens reach patient data. Where the gateway serves a cross-border
exchange, set `minimum = "substantial"`, and `"high"` from 26 March 2032. A
national contact point's tokens carry the level it asserts for a
professional another Member State authenticated: the gateway cannot check
that Member State's means, and Implementing Regulation (EU) 2026/2099
Art 6(2) gives the check to the requesting Member State's entity. The least
level applies to such an issuer as to any other.

Who acts, the professional's identification and the level reached travel to
each node in the caller's conveyance
([below](#what-a-node-is-told-about-the-caller)), and in nothing else the
gateway sends. Which claims carry them and the refusal are FerroFED's own
design; the texts above set the obligation and the levels.

## National contact points

A national contact point for digital health relays the request of a health
professional of another Member State (Regulation (EU) 2025/327 Art 23).
Implementing Regulation (EU) 2026/2099 gives the identification,
authentication and authorisation of that professional to the entity their
Member State lists (Art 6(1), (2)), and has the contact point of the
professional's Member State communicate the professional's and the
provider's identification data, the attributes of its Annex Tables 1 and 2
(Art 7). You declare the issuer of the contact point's tokens as a
`federation` issuer whose `[[auth.federation]]` table sets
`contact_point = true`, and name it in a `[[federation.contact_point]]`:

```toml
[[auth.issuer]]
issuer = "https://ncp.example.org"
kind = "federation"
sender_constraint = "required"

[[auth.federation]]
issuer = "https://ncp.example.org"
contact_point = true
mapping = "ehdsi"
countries = ["LU"]
fields."professional.country_code" = [{ constant = "LU" }]
fields."professional.issuing_authority_name" = [{ constant = "Example HP Register" }]
fields."provider.issuing_authority_name" = [{ constant = "Example HCP Register" }]

[[federation.contact_point]]
issuer = "https://ncp.example.org"
correlation_header = "ncp-correlation-id"
```

A contact point needs FerroFED's own store, [`[store]`](store.md): a
configuration that declares one and sets no `[store]` refuses to start,
naming `federation.contact_point[0]` and `[store]`. The gateway refuses a
contact point with no `[[federation.contact_point]]`, a
`[[federation.contact_point]]` that names no contact point, a `countries`
code that is neither a Member State nor one of its `additional_countries`,
and an `[[auth.federation]]` table that names no source of
`provider.issuing_authority_name`.

**Who authenticates the professional.** The contact point's side does. The
gateway verifies the contact point's token, as it verifies every caller's,
and reads the professional and the provider from it as the contact point
asserts them; it never authenticates the professional, and cannot check the
other Member State's identification means. Federation Tier §13.4 lets the
source "rely on the requester's assertion", and the node is told the
attributes marked as the contact point's assertion. An assurance level the
token states is read through `[auth.issuer.assurance]` as for any issuer
([The assurance level](#the-assurance-level)).

**The token.** The contact point's authorization server writes what it read
from the validated HP Identity Assertion and treatment relationship
confirmation under the token's `ehdsi` claim, each attribute at its field
name, and binds the token to the connector's client certificate
(`cnf."x5t#S256"`, RFC 8705 §3), which the connector presents on its mutual
TLS connection to the gateway. `ferrohealth-smart` holds the token to the
MyHealth@EU contact-point profile, what a conforming assertion carries
(eHDSI SAML Profile 10.0.0 §2.1, §2.3): the professional's identifier, the
assertion's `NameID`, with its format at `assertion.name_id_format`, one of
the three §2.1 allows, the one-string name, the role with `on-behalf-of` for
an ISCO `44` role, the authentication class, the provider's
`organization-id`, the facility type, the home community, the point of
care, a purpose of use of `TREATMENT` or `EMERGENCY`, and the assertion and
TRC ids and windows. The gateway checks no field itself. The country and
the issuing authorities are no part of the assertion, so you configure them
as constants of the contact point. A token that falls short is refused
`403 contact-point-attributes-required` on every route, and no node is
asked anything; a token not bound to the connection's certificate is
refused `401`. The contact point's connector acts for the professional the
token names, which `ferrohealth-smart` classifies as an external person. The
IUA `person_id`, a patient identifier, is never read (§5.4.1, N33): the
patient is named in the request, as for every caller.

**What a node is told.** A node is told the professional by their
identifier and by no name or address (GDPR Art 5(1)(c)), and a node that
needs a display name shows the identifier. The access record keeps the
name the identity assertion states, which Implementing Regulation (EU)
2026/2099 Annex Table 1 makes mandatory ([The access
log](access-log.md#a-request-a-national-contact-point-relays)).

**Consent exclusions stay hidden.** Every request of a contact point is
served with `disclose = false`, whatever `[federation.consent] disclose`
says ([Withholding consent exclusions](consent-exclusions.md)): the
provider it relays is a healthcare provider to whom a restriction "shall
not be visible" (Regulation (EU) 2025/327 Art 8, Art 11(5)). The setting can
only tighten: a deployment that withholds exclusions from its own callers
withholds them from the contact point too. The gateway applies no category
rule of the Member State of treatment; release stays with the node, which
receives the foreign professional in the conveyance (N26, N27).

**What the record and the node receive.** The access record names the
foreign professional by their identifier and the provider, with their
country and issuing authorities, marked as asserted by the contact point
([Audit](access-log.md#a-request-a-national-contact-point-relays)), and each node
is told them in the `national_contact_point` claim of the conveyance
([below](#what-a-node-is-told-about-the-caller)). A correlation identifier
the connector sends in `correlation_header` is recorded with the request,
so the record can be joined with the contact point's own exchange log; it
reaches no node. A value sent twice, empty, longer than 128 bytes or with a
byte outside visible ASCII is refused `400 correlation-invalid`.

`OPTIONS {base}/` declares every contact point under
`federation.national_contact_point`: `disclose: false`, `professional:
"asserted-by-contact-point"`, `purpose_of_use: "required"`, the emergency
access limit in effect as `emergency` with its `threshold` and `window`,
and per issuer the claim each field is read from, by its field name, and
the correlation header. The eHDSI profile defines no token carrier for its
attributes, so the claim names, the member and the header are FerroFED's
own design.

## DPoP

A client may bind its token to a key and prove it holds the key on every
request with a `DPoP` proof (RFC 9449), presenting the token in the `DPoP`
scheme. With `[auth.dpop]` the gateway admits such a request: it checks the
proof against the request's method and URL (§4.3) and the token's
`cnf.jkt` against the proof's key (§6), and spends the proof's `jti` so a
second use is refused (§11.1). Without the table no `DPoP` request is
admitted. A national contact point's tokens must be sender-constrained, by
`DPoP` or by the client certificate of the connection (RFC 8705 §3).

```toml
[auth.dpop]
external_url = "https://gateway.example.org"
```

RFC 9449 §11.1 notes that single use "may not always be feasible in
practice, e.g., when multiple servers behind a single endpoint have no
shared state". With [FerroFED's own store](store.md) set, every replica
records each accepted proof in `ferrofed.dpop_proof`, by the SHA-256 of its
`jti`, in the one statement that also checks it, so of two replicas sent one
proof at once only one admits it. With the store down, a `DPoP` request is
refused `503`, never checked in memory instead. A gateway with no store
records proofs in its own memory, which holds within one replica alone, so
run several replicas with `DPoP` only beside the store. A contact point
needs the store in any case. A proof that does not hold, or was sent before,
is refused `401` with the challenge `DPoP error="invalid_dpop_proof"`, and a
token that does not hold in the `DPoP` scheme with `DPoP
error="invalid_token"` (§7.1). The store and the single-statement check are
FerroFED's own design.

## The edge mode

A deployment that authenticates its clients at a proxy configures the edge
mode explicitly; it is never a default. The proxy authenticates the client by
whatever means it has, then signs an assertion of who it authenticated, an
RFC 9068 access token whose `aud` is this gateway, and sends it in the header
`trust.edge.header`. The gateway verifies the assertion against the edge's key
set exactly as it verifies a token, reads the caller from it, and logs the
security event `edge-identity-asserted` with the edge's issuer and two
references, `subject_ref` and `client_ref`. Each reference is an
HMAC-SHA256 of the subject or the client under a key the gateway draws at
start: one caller's events carry the same references for the life of the
process, and the log names no caller. A bearer token in `Authorization`
admits no one in this mode.

```toml
[auth]

[[auth.issuer]]
issuer = "https://edge.example.org"
kind = "persons"
audience = ["https://gateway.example.org/fed"]
first_party_clients = ["example-clinical-app"]

[trust]
mode = "edge"

[trust.edge]
header = "ferrofed-edge-assertion"
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
| `400` | `credential-malformed` | two `Authorization` fields, or one that is no credential; the challenge is `error="invalid_request"` (RFC 6750 §3.1) |
| `401` | `unauthenticated` | no token, or one the gateway does not accept; the challenge carries `error="invalid_token"` for a token that does not hold and no error code for a missing one, and never names the check that failed; in the `DPoP` scheme, `error="invalid_dpop_proof"` for a proof that does not hold or was sent before (RFC 9449 §7.1) |
| `403` | `scope-insufficient` | no granted scope covers the operation, or the client is not admitted to the DEMOGRAPHIC API |
| `403` | `purpose-of-use-required` | the token declares no purpose of use |
| `401` | `natural-person-required` | the request reaches patient data and the token names no natural person, and its issuer does not declare client tokens acting for the professional the token names |
| `401` | `authentication-assurance-insufficient` | the request reaches patient data and the token states no assurance at the least level its issuer requires |
| `403` | `contact-point-attributes-required` | a national contact point's token falls short of the MyHealth@EU contact-point profile ([National contact points](#national-contact-points)) |
| `400` | `correlation-invalid` | a national contact point's correlation header is sent twice, empty, longer than 128 bytes, or not visible ASCII |
| `403` | `operation-refused` | the ADMIN API, refused to every caller |
| `403` | `patient-context-missing` | only a bound issuer's `patient/` scope covers the operation, and the token carries no `ehrId` |
| `403` | `patient-confinement` | the request reaches beyond the patient a `patient/` grant is confined to, or a patient grant addresses the DEMOGRAPHIC API |
| `424` | `patient-context-unavailable` | the patient of a `patient/` grant cannot be resolved at every member |
| `503` | `authentication-unavailable` | the issuer's metadata, key set or introspection endpoint cannot be had |

A `403` names no scope, in the challenge or the body (RFC 6750 §3.1 makes
the `scope` attribute optional). Every refusal is logged under the
`ferrofed::security` target as `caller-refused`, with its reason, the check
that refused a credential and the gateway's request id, never the
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

The header each node receives with the caller's identity, its claims, and
how a node verifies it are on
[What a node is told about the caller](caller-token.md).
