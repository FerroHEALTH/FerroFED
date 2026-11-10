<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Onward credentials

How the gateway authenticates to each node it sends a request to (§13.1,
N25). Each endpoint's `[credentials."<id>"]` section names one scheme: a
user and a password ([Configuration](configuration.md#the-file)), or one of
the grants on this page, which obtain a token at the node's authorization
server before the request:

- `oauth`: the client-credentials grant of `ferrohealth-smart`, the table
  every FerroHEALTH product reads for an upstream it calls as itself. It
  binds its tokens to a key or to the TLS client certificate where you ask
  for it, holds to the FAPI 2.0 Security Profile with
  `security_profile = "fapi2"`, and takes an optional `onward` table beside
  it that exchanges each caller's token for one of the node's own
  ([The onward lane of a node](#the-onward-lane-of-a-node));
- `nuts`: the Nuts track of the Dutch Generic Functions (Annex B §B.4).

A section names one of them; two in one section refuse the configuration.
The `oauth2` and `fapi2` tables of earlier releases, and the section's
`mtls_alias_hosts` key, are refused at load, naming the key of the `oauth`
table that replaces each (see the upgrade notes). A node's section may also
name the TLS client certificate the gateway presents to that node and to
its authorization server
([Mutual TLS to a node](#mutual-tls-to-a-node-rfc-8705)). No log line,
error or rendering carries a credential, an assertion, a key, a proof or a
token.

## OAuth 2.0 to a node

An `oauth` section makes the gateway authenticate to that node as itself
(§13.1, N25). Before a request, it asks the node's token endpoint for an
access token with the client-credentials grant (RFC 6749 §4.4), and sends
the token as `Authorization: Bearer` (RFC 6750 §2.1):

```toml
[credentials."cdr-c".oauth]
token_endpoint = "https://auth.cdr-c.example.org/oauth2/token"   # or issuer, read from its RFC 8414 metadata
client_id = "ferrofed-gateway"
client_auth = "private_key_jwt"                 # or tls_client_auth, self_signed_tls_client_auth
private_key_file = "/run/secrets/cdr-c-client.pem"
# assertion_alg = "ES256"                       # by the key type when unset
scope = "system/aql-*.s system/composition-*.cru"
# resource = "https://cdr-c.example.org/openehr"   # RFC 8707
# audience = "cdr-c"
```

- `client_auth = "private_key_jwt"`, the default mechanism of §13.1, signs
  a JWT client assertion (RFC 7523 §2.2) with the key in
  `private_key_file`; the gateway publishes its public half in its JWK Set
  ([Signing keys and the JWK Set](#signing-keys-and-the-jwk-set)), under the
  RFC 7638 thumbprint as its `kid` unless `kid` names another. The
  assertion names `client_id` as its `iss` and `sub`, carries a fresh `jti`
  and lives 300 seconds. `tls_client_auth` and
  `self_signed_tls_client_auth` authenticate by the TLS client certificate
  instead ([Mutual TLS to a node](#mutual-tls-to-a-node-rfc-8705)).
- A node's grant takes no client secret: `client_secret_basic`,
  `client_secret_post`, `client_secret` and `client_secret_file` refuse the
  configuration, naming the key. The gateway authenticates to a node by its
  own key or certificate (§13.1, N25); a client secret is for an identity
  service's grant
  ([Identity resolution](identity.md#a-pix-manager-behind-oauth-20)).
  `client_auth` has no default a node takes, so set it.
- `scope` is space-separated SMART on openEHR scopes, each a resource scope
  of the `system` compartment (`system/aql-*.s`, `system/composition-*.cru`);
  anything else is refused at load. The token request carries it as written
  (RFC 6749 §3.3).
- `token_endpoint` is an `https` URL with no user name, password or
  fragment (RFC 6749 §3.2; RFC 9110 §4.2.4); `http` is accepted only under
  `profile = "development"`
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).
  With `issuer` in its place, the token endpoint is read from the issuer's
  RFC 8414 metadata, and `issuer` is held to the same rule.
- `assertion_audience` is `token_endpoint`, the default, or `issuer`. RFC
  7523 §3 admits either as the assertion's `aud`. An authorization server
  under the FAPI 2.0 Security Profile accepts only its issuer identifier,
  as one string (§5.3.2.1). `issuer` needs the table's `issuer` set, which
  then replaces `token_endpoint`: the two exclude each other.
- `refresh_margin_ms`, 30 seconds by default, is how long before the token
  expires a new one is fetched; `timeout_ms` bounds each call to the
  authorization server.

`ferrohealth-smart` caches the token until `refresh_margin_ms` before the
end of the lifetime its `expires_in` states, or for half that lifetime when
it is shorter than twice the margin, with one token request per endpoint at
a time. A token with no stated lifetime is reused until a node refuses it.
A node that answers `401` drops the cached token, and the next request
obtains a new one. When no token can be obtained, that node is reported
`node-error` and is sent nothing: the gateway never dispatches
unauthenticated. The answer says only
`no onward credential could be obtained, so nothing was sent`, followed by
the token endpoint's RFC 6749 §5.2 `error` code when it refused with a
registered one. The full account, the token endpoint's description
included, goes to the log at `warn` with the endpoint and the request id.
The admission check authenticates the same way.

The caller's own `Authorization` header never reaches a node.

## A token per caller: token exchange

An `onward` table beside the node's `oauth` table gives each verified
caller a token of its own at that node, where the node's authorization
server supports RFC 8693. Set `actor_assertion = true` to name the gateway
as the actor, as the `oauth2` token exchange of earlier releases always
did:

```toml
[credentials."cdr-c".oauth]
token_endpoint = "https://auth.cdr-c.example.org/oauth2/token"
client_id = "ferrofed-gateway"
client_auth = "private_key_jwt"
private_key_file = "/run/secrets/cdr-c-client.pem"
scope = "system/aql-*.s"                         # the gateway's own requests

[credentials."cdr-c".onward]
issuer = "https://auth.cdr-c.example.org"        # the issuer of the exchanged tokens
kind = "persons"
profile = "rfc9068"
token_endpoint = "https://auth.cdr-c.example.org/oauth2/token"
audiences = ["https://cdr-c.example.org/openehr"]
indicators = ["https://cdr-c.example.org/openehr"]   # sent as resource (RFC 8707)
actor_assertion = true

[credentials."cdr-c".onward.keys]
max_onward_lifetime = "1h"
```

For a request on behalf of a caller, the gateway sends the token endpoint
the caller's verified access token as `subject_token`, an assertion of its
own as `actor_token`, with its own `jti`, and its client authentication as
above. The actor assertion is signed with the `oauth` table's
`private_key_file` whatever `client_auth` says, so a grant that
authenticates by its certificate still names itself as the actor. The
exchange names the node with each of `indicators`.

- Where the `onward` table sets no `scope`, the exchange asks for the
  caller's granted scopes that cover the operation, and never the rest
  (N26). A request that no granted scope of the caller covers, such as one
  with no SMART on openEHR family or a demographic request, is never
  exchanged: the node is `node-error` and is sent nothing. Without `scope`
  the authorization server would choose the scope itself (RFC 8693 §2.1).
  A `scope` set on the `onward` table, such as the node scope an Entra
  issuer needs, is asked for as written instead.
- The answer must issue an access token or a JWT (`issued_token_type`,
  RFC 8693 §2.2.1) that `ferrohealth-smart`'s onward check accepts: a JWT
  naming `issuer` as its `iss`, an `aud` inside `audiences`, a lifetime
  within `max_onward_lifetime`, the gateway as its `act` or client, and the
  caller's person. An opaque exchanged token, or an answer that carries a
  refresh token, fails the node with nothing sent.
- A token bound with `DPoP` reaches the node under the `DPoP` scheme, its
  requests proven with the `oauth` table's `dpop_key_file`
  ([Tokens bound to a key](#tokens-bound-to-a-key-dpop)).
- The exchanged token is cached per caller and target until shortly before
  it or the caller's token expires, at most 4096 per endpoint, and dropped
  when the node answers `401`, so the next request exchanges again.
- Only a caller verified by its token's signature or by introspection has a
  token to exchange. A caller the edge mode asserted has none, so that node
  is `node-error` and is sent nothing.
- The gateway keeps the caller's token only while some node has an
  `onward` table, and never writes it to a log, a span, a metric or the
  `openEHR-federation-client` token.
- A caller's token whose text or claims carry the patient identifier the
  query was resolved on is not sent to the token endpoint, and that node is
  `node-error` (§5.4.1, N33).
- The gateway's own requests, the admission check and the redistribution of
  a held stored query, have no caller: they use the `oauth` table's
  client-credentials token, with its `scope`.

The caller's token reaches the node's authorization server, which must
trust the caller's issuer, and never the node itself. Which callers an
`onward` table carries, and the other profiles it runs, follow.

## The onward lane of a node

An `onward` table beside a node's `oauth` table makes every request a
verified caller originates reach that node under a token of the node's own
issuer, obtained for that caller. It is the `[<lane>.credentials.onward]`
table of `ferrohealth-smart`, and the `oauth` table authenticates the gateway
at the onward issuer:

```toml
[credentials."cdr-f".oauth]
issuer = "https://as.cdr-f.example.org"
client_id = "ferrofed-gateway"
client_auth = "private_key_jwt"
private_key_file = "/run/secrets/cdr-f-client.pem"
scope = "system/aql-*.s"            # the gateway's own requests alone

[credentials."cdr-f".onward]
issuer = "https://as.cdr-f.example.org"
kind = "persons"                    # persons, federation or patients
profile = "keycloak"                # rfc9068, entra or keycloak: picks the grant
audiences = ["cdr-f"]               # what an issued token's aud may name
indicators = ["cdr-f"]              # sent as resource or audience, by the profile
# scope = "..."                     # the Entra profile names the node here
# correlation_claim = "txn"         # a chained federation route

[credentials."cdr-f".onward.keys]
max_onward_lifetime = "1h"          # required
# The Keycloak profile only: the issuer's service accounts, whose sub is a
# user's; the profile refuses the table at load until you list them.
app_only = [{ subjects = ["service-account-cdr-f-batch"] }]
```

`ferrohealth-smart` runs the grant the profile picks: RFC 8693 token
exchange with the caller's token as `subject_token` (with `audience` under
the Keycloak profile), or the RFC 7523 JWT bearer grant with the on-behalf-of
parameters under the Entra profile. It checks every issued token before use
(its issuer, its `aud` inside `audiences`, its lifetime against
`max_onward_lifetime`, the gateway as its client or `act`, and the caller's
person), and caches it until shortly before it or the caller's token
expires. A `federation` issuer adds a `[credentials."<id>".onward.federation]`
table whose `exchanges_from` lists the caller issuers it is trusted to map
and whose `person_mapping` names the subject-token claim the issued token's
`professional.hp_identifier` must carry.

Which caller the lane carries depends on `kind`:

| `kind` | Carried | Refused |
|---|---|---|
| `persons` | a person of the node's organisation, acting alone, through a client or for another | everyone else |
| `federation` | a professional of another domestic party, and another domestic party's organisation when the section opts in | everyone else |
| `patients` | the patient, on a token naming the patient context | everyone else |

- A caller the lane does not carry, a system included, fails that node as
  `node-error`, and nothing is sent to the token endpoint or the node.
- A refused exchange fails that node as `node-error`, naming the token
  endpoint's error code. The gateway's own token is never sent in its place,
  and a `401` from the node is never retried under it.
- The gateway's own requests, which no caller originates, use the `oauth`
  table's token.
- A caller the edge mode asserted, or a Basic caller, has no token to
  exchange, so that node is `node-error` and is sent nothing.
- A request with no named person, from another domestic organisation, is
  carried only where the node's agreement declares it: set
  `onward_organisations = true` in the node's `[credentials."<id>"]`
  section, beside an `onward` table of kind `federation`. The issuer
  exchanges the organisation's token for one naming the organisation alone,
  and the node is reached read-only: a request outside the ITS-REST `query`
  group and outside `GET`, `HEAD` and `OPTIONS` is refused
  `403 operation-refused` before any token is asked for. Configuration
  refuses the switch without an `onward` table or on any other `kind`. A
  national contact point is never carried as an organisation: its token
  always names a professional.
- Under a `chained` agreement, set `correlation_claim` to the claim that
  holds the requester's log correlation id. A caller whose verified claims
  do not carry it is refused before the token endpoint is asked. For an
  opaque token those are the claims its issuer's introspection answer
  returned. The issued token must carry the same value, or that node is
  `node-error`.
- With the Keycloak profile, the caller's token must name the gateway's
  client in `aud`: give the inbound client scope an audience mapper that adds
  it, and list the gateway among the node's delegates. Check this before the
  lane carries traffic.

### Who commits through the lane

A node reached through an onward lane authenticates the onward token, so it
derives the committer of every commit from that token, and takes, where
ITS-REST requires a committer, one placeholder naming the token's `iss` and
`sub`, which it recognises and discards (FerroEHR's committer rules):

```json
{"_type": "PARTY_IDENTIFIED", "identifiers": [{"_type": "DV_IDENTIFIER", "issuer": "<token iss>", "id": "<token sub>"}]}
```

- A `CONTRIBUTION` names the placeholder of the onward token the lane
  obtained for the caller in its `audit` and in every version's
  `commit_audit` (ITS-REST 1.1.0 `UpdateAudit`, `UpdateVersion`). The gateway
  writes these members alone and keeps the rest of the body as sent.
- An attestation (`UpdateAttestation`) takes the same placeholder for a
  person exchanged at a `persons` issuer. Through a `federation` or a
  `patients` lane a `CONTRIBUTION` that carries an attestation is refused
  `403 operation-refused` before any token is asked for, because the node
  refuses every attestation by an external person.
- A header commit (`PUT` or `POST` of a composition, an `EHR_STATUS` or a
  directory) is forwarded as the client sent it. The gateway adds no
  committer to its `openehr-audit-details`, and the node derives it.

The configuration refuses an `onward` table without its `oauth` table,
beside a user and password or another grant, on a service's credentials,
and when `ferrohealth-smart` refuses it: no `issuer`, `kind` or `audiences`,
no `max_onward_lifetime`, a `federation` issuer without `exchanges_from` or
`person_mapping`, or a target its grant cannot ask for.

## Tokens bound to a key: DPoP

`dpop_key_file` in a node's `oauth` section binds that node's tokens to a
key of the gateway's (RFC 9449), for a deployment that requires
sender-constrained tokens, the tokens its `onward` table exchanges included. The file holds a P-256 key, which signs ES256, or a P-384 key, which
signs ES384, in PKCS#8 PEM:

```text
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out cdr-c-dpop.pem
```

Every request to that node's URL, and to its token endpoint, then carries a
`DPoP` proof signed with the key: it names the request's method and URL
without query and fragment, a fresh `jti` and the time, and, on a request
that carries the token, the token's SHA-256 in `ath`. The token is sent
under the `DPoP` scheme. The token endpoint must answer `token_type`
`DPoP`; a bearer token is refused and the node is `node-error`. A token
endpoint that answers `400 use_dpop_nonce`, or a node that answers `401`
with a `DPoP` challenge naming `use_dpop_nonce`, is sent the request once
more with the nonce it gave, and every later proof to that server carries
the latest nonce it sent (RFC 9449 §8, §9). Each send to the token endpoint
has `timeout_ms` of its own.
The token endpoint's nonce and the node's are kept apart even when both
live on one host. A token request sent once more carries a newly signed
client assertion, and under token exchange a new actor token, so the
token endpoint never sees a `jti` twice. A node whose deadline passes
before its second send is `time-out`, and it counts as a node that was
asked.
The key is read at start and on each reload; a key that is no P-256 or
P-384 key refuses the configuration, naming the `oauth` section, and a
section with both `dpop_key_file` and `certificate_bound = true` refuses it
too: a token is bound one way.

## Mutual TLS to a node (RFC 8705)

A node's `[credentials."<id>".tls]` table names the TLS material the
gateway reaches that node with, by the keys the identity services take:

| Key | What it is |
|---|---|
| `client_identity_file` | The gateway's client certificate chain and its private key, PEM, the end-entity certificate first. `client_identity` takes the same inline, as a secret. |
| `trust_roots_file` | A PEM bundle of trust roots, beside the platform's, the node and its authorization server are trusted by. |
| `min_version` | The oldest TLS version the gateway speaks to the node and its authorization server: `"1.3"` unless lowered to `"1.2"`. |

The node and its authorization server are reached over one transport built
with that material, so every request to either presents the same
certificate. A node presented a certificate is reached over `https` alone,
under every profile: an `http` node URL refuses to start, and an `http`
token endpoint or issuer refuses the configuration. A section may name the
TLS material alone, with no scheme, when the node authenticates the gateway
by the certificate and asks for no `Authorization` header.

With that material, the section's `oauth` table can authenticate by the
certificate in place of an assertion, and bind its tokens to it. The grant
presents the certificate the `tls` table names, read once: the `oauth`
table names no certificate of its own.

```toml
[credentials."cdr-f".tls]
client_identity_file = "/run/secrets/cdr-f-client.pem"
trust_roots_file = "/etc/ferrofed/cdr-f-roots.pem"

[credentials."cdr-f".oauth]
client_auth = "tls_client_auth"              # or self_signed_tls_client_auth
token_endpoint = "https://auth.cdr-f.example.org/oauth2/token"
client_id = "ferrofed-gateway"
scope = "system/aql-*.s"
certificate_bound = true
```

- `client_auth = "tls_client_auth"` authenticates the gateway by the
  certificate, which the authorization server validates against its PKI and
  matches to the subject it registered for `client_id` (RFC 8705 §2.1);
  `"self_signed_tls_client_auth"` has it matched to the certificate it
  registered instead (§2.2). The token request then carries `client_id` and
  no client assertion. An `onward` table with `actor_assertion = true`
  still names the gateway as the actor, with an assertion of the `oauth`
  table's `private_key_file` (RFC 8693 §2.1).
- `certificate_bound = true` takes only tokens bound to the certificate
  (§3). The token is sent under `Bearer` (RFC 6750), and only over the
  endpoint's transport, which presents the certificate it is bound to.
  Where the token states its binding, as a `cnf` member of the token
  response or the `cnf` claim of a JWT access token, its `x5t#S256` must be
  the SHA-256 thumbprint of the `tls` table's certificate (§3.1, §3.2); a
  token bound to another certificate is never cached or sent, and the node
  is `node-error` with nothing sent. An opaque token states no binding, and
  the node's authorization server and the node check it.
- Either key without `tls.client_identity_file` refuses the configuration,
  and so does an identity file without a certificate and its key, naming
  `credentials."<id>".tls.client_identity`. `client_certificate_file`,
  `client_key_file` and `ca_file` in the `oauth` table of a section that
  names TLS material refuse the configuration, naming the key: one identity
  comes from one place.

The client identity is a secret no rendering shows; the settings and the
start-up log name the endpoints that present a certificate, never the
certificate or its key. A service's
own `credentials` section, under `[[pixm.manager]]` or `[pdqm]` for
example, takes no TLS material: name it on the service's table.

## The Nuts grant (Annex B §B.4)

A `nuts` section makes the gateway authenticate to that node on the Nuts
track of the Dutch Generic Functions, the regional realisation of §13.3
that Annex B §B.4 describes. The gateway is the holder: it presents its own
Verifiable Credentials, signed as a presentation with its `did:web` key,
and the node's authorization server answers with a token bound to the
gateway's `DPoP` key. The wire is Nuts RFC021, the VP Token Grant Type:

1. The gateway reads the authorization server's metadata at the RFC 8414
   well-known URL of `authorization_server`. The metadata must name that
   issuer exactly, a `token_endpoint`, a `presentation_definition_endpoint`
   (RFC021 §5) and `vp_formats` admitting `jwt_vp` with the holder key's
   algorithm (RFC021 §3.1); when it lists `dpop_signing_alg_values_supported`,
   the `DPoP` key's algorithm must be among them. The token and definition
   endpoints must be on the issuer's origin (its scheme, host and port), so
   the credentials go nowhere else, and an answer whose objects repeat a
   name is refused. Write `authorization_server` in its canonical form, a
   lower-case host and no default port, since the metadata must name it as
   the same text.
2. It reads the Presentation Definition for `scope` and maps each
   `[[credential]]` to the input descriptor it names. A credential for a
   descriptor the definition lacks, a descriptor left unanswered when the
   definition has no submission requirements, or a submission requirement
   the credentials do not meet stops the request before any credential is
   sent. The authorization server evaluates each descriptor's constraints
   against the credentials it receives (RFC021 §4.1); the gateway does not.
3. It signs a JWT Verifiable Presentation of every credential (VC Data
   Model 1.1 §6.3.1): `iss` and `sub` the gateway's `did`, `kid` the DID URL
   `kid`, `aud` the issuer, `nbf` now and `exp` five seconds later, and a
   fresh `nonce` and `jti` (RFC021 §4.2). A credential whose `exp` has passed
   is never sent.
4. It posts `grant_type=vp_token-bearer` with the presentation as
   `assertion`, the Presentation Submission, the `scope` and, when set,
   `client_id`, with a `DPoP` proof of `dpop_key_file`'s key. A demanded
   nonce is answered once, with a new presentation, since RFC021 §4.4
   refuses a presentation nonce seen before.
5. It takes the token only when its `token_type` is `DPoP`. The token is
   kept until 30 seconds before it expires and dropped when the node answers
   `401`, and every request to the node carries it
   under the `DPoP` scheme with a proof of the same key
   ([Tokens bound to a key](#tokens-bound-to-a-key-dpop)).

| Key | What it is |
|---|---|
| `authorization_server` | The issuer identifier of the node's authorization server (RFC 8414 §2). `https` outside the development profile. |
| `scope` | The scope the authorization server maps to its Presentation Definition, space-delimited RFC 6749 §3.3 scope tokens. |
| `client_id` | Optional; sent when the authorization server identifies its clients by one (RFC 6749 §3.2.1). |
| `did` | The gateway's `did:web` identifier, the holder of the credentials. |
| `kid` | The DID URL of the holder's key, `<did>#<fragment>`. |
| `key_file` | The holder's key, a P-256 (ES256) or P-384 (ES384) private key in PKCS#8 PEM. |
| `dpop_key_file` | The key the tokens are bound to, as in an `oauth` section. Required: GF-Authentication sender-constrains every token (GFI-005). |
| `[[credential]]` | One per credential: `input_descriptor`, the descriptor it answers, and `file`, the JWT-encoded credential, issued to `did`. |

The gateway serves its DID document, so the authorization server can
resolve the presentation's `kid` and verify it (RFC021 §4.2; GFI-001). The
document sits where the `did:web` method resolves the DID: the path is
`/.well-known/did.json` for a DID with no path, and
`/<segment>/…/did.json` for one with path segments, so
`did:web:gateway.example.org:nuts` is served at `/nuts/did.json`. It is
served at that path whatever the base URL, with no client authentication,
like the JWK Set, as `application/did+ld+json`. Route
`https://<host><path>` of the DID's host to the gateway. The document is
built from the holder keys the `nuts` sections of that `did` name and
nothing else: one `JsonWebKey2020` verification method per key, its `id`
the `kid`, its `publicKeyJwk` the public half of `key_file`'s key,
referenced from `authentication` and `assertionMethod`. A key change in
`key_file` changes the served document on the next reload or restart, with
no file to edit.

Two `nuts` sections may share a `did`. If they name one `kid`, they must
hold one key. A DID whose document path would sit under `{base}/v1/`, or
two DIDs whose documents share one path, refuse the configuration, since
one gateway serves one document per path. A deployment whose DID names
another host still has to serve that host's path from the gateway, through
its reverse proxy. The credentials are issued to the
gateway by their authoritative sources ahead of time (GFI-002); the gateway
reads them from their files at start and on each reload, and checks only
that each is a JWT credential, with a `vc` claim, whose `sub` is `did`.

No log line, error or rendering carries a credential, the presentation, a
key, a proof or the token: a refusal names the authorization server's
`error` code and at most 256 characters of its description. An `oauth`
section and a `nuts` section for the same endpoint refuse the configuration.

### The NVI's Nuts grant

The same table under `[nl_gf.nvi.credentials.nuts]` authenticates the
gateway to the NVI, the Localization Service of
[Dutch localization](localization.md#dutch-localization-nl_gfnvi), as a data
user on GF-Authentication (the IG's Localization page, GFI-004, GFI-005):

```toml
[nl_gf.nvi.credentials.nuts]
authorization_server = "https://nuts.example.org/oauth2/nvi"
scope = "nl-gf-localization"
did = "did:web:gateway.example.org"
kid = "did:web:gateway.example.org#key-1"
key_file = "/run/secrets/nuts-holder.pem"
dpop_key_file = "/run/secrets/nvi-dpop.pem"

[[nl_gf.nvi.credentials.nuts.credential]]
input_descriptor = "organization_credential"
file = "/run/secrets/organization.jwt"
```

The token is obtained and cached as a node's is, each token request bounded
by the localization's `timeout_ms`. Every search carries it under the `DPoP`
scheme with a proof of `dpop_key_file`'s key over the search URL without its
query, so the pseudonym never reaches a proof or the authorization server
(RFC 9449 §4.2, §7.1). A `401` from the NVI drops the token, and a nonce it
demands is answered once (RFC 9449 §9). A grant the authorization server
refuses leaves the localization unavailable, so the query fails closed
(§14.1). The token goes to the NVI alone, never to a node, a log line or an
error. `authorization_server` must be `https` outside development, or boot
is refused, naming its key. The NVI's credentials take no `oauth` grant:
the IG defines none for the Localization Service.

## The FAPI 2.0 grant (Annex B §B.4a)

`security_profile = "fapi2"` holds a node's `oauth` grant to the FAPI 2.0
Security Profile, for a node whose authorization server follows it, as the
BgZ/eOverdracht track of the Dutch binding does (Annex B §B.4a, a VWS memo
at concept v0.9). The track uses OAuth 2.0 client credentials with
`private_key_jwt` client authentication, sender-constrained tokens, and the
healthcare attributes in an RFC 9396 `authorization_details` object of type
`nl-gis-v1` (§B.4a.2, §B.4a.3):

```toml
[credentials."cdr-e".oauth]
issuer = "https://as.cdr-e.example.org"
security_profile = "fapi2"
client_id = "urn:oid:2.16.528.1.1007.3.3.<URA>"
client_auth = "private_key_jwt"
assertion_audience = "issuer"
private_key_file = "/run/secrets/fapi2-client.pem"
dpop_key_file = "/run/secrets/cdr-e-dpop.pem"
scope = "system/aql-*.s"                # optional when authorization_details is set
authorization_details = '''[{"type": "nl-gis-v1",
  "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT",
  "locations": ["https://cdr-e.example.org/openehr"],
  "locations_organization_id": "urn:oid:2.16.528.1.1007.3.3.<URA of the node>"}]'''
```

A token per caller adds an `onward` table beside it, as
[A token per caller](#a-token-per-caller-token-exchange) shows, with the
issuer as its `issuer` and no `token_endpoint`, so the token endpoint is
discovered the same way.

1. At the first request to the node, `ferrohealth-smart` reads the
   authorization server's metadata from `issuer`, once, and keeps it; a
   read that fails is tried again on the next request. The metadata must be
   `application/json`, name that issuer exactly, and name a
   `token_endpoint` on the issuer's origin; an answer whose objects repeat a
   name, or that is longer than 1 MiB, is refused. It must also list:
   - `private_key_jwt` in `token_endpoint_auth_methods_supported`, and the
     key's algorithm in `token_endpoint_auth_signing_alg_values_supported`
     (RFC 8414 §2: an omitted method list means `client_secret_basic`
     alone);
   - `client_credentials` in `grant_types_supported`, and token exchange
     for an `onward` table (an omitted list means `authorization_code` and
     `implicit` alone);
   - every `type` of `authorization_details` in
     `authorization_details_types_supported` (RFC 9396 §10);
   - the `DPoP` key's algorithm in `dpop_signing_alg_values_supported`,
     when the list is there (RFC 9449 §5.1).
2. It asks the token endpoint for a token with the client-credentials
   grant, or the `onward` table exchanges each verified caller's token. The
   client assertion is signed with `private_key_file`'s key, and names the
   issuer, as one string, as its `aud` (FAPI 2.0 §5.3.3.1); the actor token
   of an exchange is signed the same way. The request carries `scope` when
   set and `authorization_details` as written.
3. It takes the token only when its `token_type` is `DPoP` and, under
   `authorization_details`, when the answer states the details it granted
   (RFC 9396 §7). A refusal with `invalid_authorization_details` is
   reported with that code. Every request to the node carries the token
   under the `DPoP` scheme with a proof of `dpop_key_file`'s key
   ([Tokens bound to a key](#tokens-bound-to-a-key-dpop)).

The keys every grant takes, held to the profile:

- `client_auth` is `private_key_jwt`, or `tls_client_auth` or
  `self_signed_tls_client_auth` by the node section's certificate
  ([Mutual TLS to a node](#mutual-tls-to-a-node-rfc-8705)); under
  `private_key_jwt`, `assertion_audience = "issuer"` is required.
- Every key signs PS256, ES256 or EdDSA, the algorithms §5.4.1 admits, so a
  P-384 key cannot sign here: a P-256 key, an RSA key with
  `assertion_alg = "PS256"`, or an Ed25519 key.
- Every token is sender-constrained (§5.3.2.1): `dpop_key_file`, or
  `certificate_bound = true` with the node section's certificate.
- `scope` and `authorization_details` are each optional, and one of them is
  required, so the server never chooses what a token may do (§5.3.3.1:
  least privilege).

The profile admits mutual TLS for both choices (§5.3.2.1). With
`tls_client_auth` or `self_signed_tls_client_auth` the metadata must list
that method in place of `private_key_jwt`, and the token request carries
`client_id` and no assertion. With `certificate_bound = true` the metadata
must state `tls_client_certificate_bound_access_tokens` as `true` (RFC 8705
§3.3), and the token is taken as `Bearer` and held to the certificate. A
grant that uses mutual TLS sends its token requests to the
`token_endpoint` of `mtls_endpoint_aliases` where the metadata names one
(RFC 8705 §5), held to the issuer's origin like the other endpoint.

RFC 8705 §5 lets a server put that alias on another host, as its own
example does with `mtls.example.com`. The gateway sends there only when you
name the host in the `oauth` table:

```toml
[credentials."cdr-e"]
tls = { client_identity_file = "/run/secrets/cdr-e-client.pem" }

[credentials."cdr-e".oauth]
issuer = "https://as.cdr-e.example.org"
security_profile = "fapi2"
client_auth = "tls_client_auth"
certificate_bound = true
mtls_alias_hosts = ["mtls.cdr-e.example.org"]   # or "mtls.cdr-e.example.org:8443"
# ... as above
```

Each entry is a host name, or a host name and a port, in lower case and
without a scheme or a path; port `443` is left out. An alias on a named
host is taken over `https` alone, without userinfo or a fragment. An alias
on any other host is refused before a token request is sent, as the
top-level `token_endpoint` on another origin always is. The key is refused
for a grant without `security_profile = "fapi2"`, and for one that does not
use mutual TLS, since no other grant reads `mtls_endpoint_aliases`. Which
hosts to trust is FerroFED's own design: no specification names them.

The gateway publishes the public half of `private_key_file`'s key in its
JWK Set, beside the `[signing]` keys, and the authorization server
verifies the assertion against it (§5.4.2; Annex B §B.4a.2). To make the
two keys:

```text
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out fapi2-client.pem
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out cdr-e-dpop.pem
```

To rotate the client key, make a new P-256 key, set it as
`private_key_file`, move the old one to `previous_private_key_file`, and
reload or restart. The new key signs every assertion from then on, and the
JWK Set publishes both keys, the new one first. An authorization server
that still holds the JWK Set from before the rotation does not know the
new key, so it refuses the new assertions until it fetches the set again;
the set it fetches then still holds the old key, so an assertion the old
key signed before the rotation verifies too. Once every authorization
server that verifies the grant has fetched the new set, remove
`previous_private_key_file`, or set `previous_key_overlap_ms` to have the
old key withdrawn on its own. `next_private_key_file` publishes a key
ahead of the rotation that never signs. A previous key the profile does
not admit, or that is the current key, refuses the configuration.

```toml
private_key_file = "/run/secrets/fapi2-client-2026-10.pem"
previous_private_key_file = "/run/secrets/fapi2-client.pem"
```

What the gateway does not do on this track:

- **An authorization-endpoint flow.** The authorization code grant needs a
  user agent to redirect, with pushed authorization requests (RFC 9126) and
  PKCE, which FAPI 2.0 requires of those flows (§5.3.2.2, §5.3.3.2). A
  server-to-server gateway has no user agent, and the `oauth` table runs the
  client-credentials grant alone. The client-credentials
  grant is admitted under the profile's general requirements (§5.3.2.1
  Note 2).
- **Purpose of use per caller.** `purpose_of_use` and
  `subject_organisation_type` travel as the configured
  `authorization_details` of the endpoint, the same for every request, as a
  declaration of the gateway's organisation (§B.4a.3). Record that answer
  to §13.4 for the deployment
  ([The §13.4 deployment decisions](deployment-decisions.md)).
- **Verifying the access token.** The issuing organisation signs the access
  token (§B.4a.2); the node verifies it, not the gateway.

## Signing keys and the JWK Set

`[signing]` holds the gateway's signing keys: P-256 or P-384 private keys
in PKCS#8 PEM, each read from a file. The curve decides the algorithm: a
P-256 key signs ES256 and a P-384 key ES384 (RFC 7518 §3.4), and the JWK
Set publishes each key with its `alg`. It is required whenever a registry
is configured, by `registry.document` or by `[registry.mcsd]`: every
request to a node carries the caller's identity in an
`openEHR-federation-client` token signed with the current key
([What a node is told about the caller](caller-token.md);
§13.1, N24), and a federating gateway without the key refuses to start,
naming it. A grant's client assertions are signed with the `oauth` table's
own `private_key_file`, never with `[signing]`. A key on another curve
refuses the configuration, naming the key. To make a key:

```text
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out ferrofed-signing-key.pem
```

Choose P-256 when a node holds to the FAPI 2.0 Security Profile, which
admits PS256, ES256 and EdDSA for a JWT and not ES384 (§5.4.1), so such a
node may refuse an ES384 token. The one key then signs ES256 for every
node:

```text
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out ferrofed-signing-key.pem
```

The gateway serves its public keys as a JWK Set (RFC 7517) at
`GET {base}/.well-known/jwks.json`, with no client authentication, and
declares `jwks_uri` as `federation.auth.jwks_uri` in `OPTIONS {base}/`
(§13.1, N30). Point `jwks_uri` at that route on the gateway's public
address, or at wherever your deployment publishes the keys. Each key's `kid`
is its RFC 7638 thumbprint, so the same key always has the same `kid`. The
set also publishes the `private_key_file` key of every `oauth` grant, with
its previous and next keys, after the `[signing]` keys; a reload that
changes one publishes the new key.

### Rotating the signing key

`[signing]` holds up to three keys, and the JWK Set publishes each with its
own `kid`:

| Key | Signs | Published |
|---|---|---|
| `key_file` | every token | always |
| `previous_key_file` | never | for `rotation_overlap` from the start of the process |
| `next_key_file` | never | always |

A node verifies a token by its `kid` (RFC 7515 §4.1.4) against the JWK Set
it fetched (RFC 7517 §5), and may hold that set for its cache time. A key
must therefore be in the set a node holds before any gateway signs with it,
and stay there until every token it signed has expired. The gateway gives
a token its `[signing]` keys sign at most 300 seconds; the
`openEHR-federation-client` token lives 60.

The procedure assumes that no node caches the JWK Set longer than
`node_jwks_cache` (3600 s by default). Set it to the longest cache time of
your nodes and their authorization servers; `config check` refuses a
`rotation_overlap` shorter than 300 seconds plus `node_jwks_cache`.
`assertion_lifetime_s`, which earlier releases took here, is refused at
load: no `[signing]` key signs a client assertion any more. The three steps hold for one gateway and for any
number of replicas behind one address:

1. **Publish the new key.** Make the new key, copy it to every replica, set
   it as `next_key_file`, and restart the replicas one by one. Each
   restarted replica publishes the current key and the new one, and still
   signs with the current key. When the last replica runs the new
   configuration, wait `node_jwks_cache` more, so every node's cached set
   came from a replica that publishes the new key.
2. **Sign with it.** Set the new key as `key_file`, move the old one to
   `previous_key_file`, remove `next_key_file`, and restart the replicas one
   by one. A restarted replica signs with the new key. During the restart
   every replica publishes both keys: a restarted one as current and
   previous, one not yet restarted as current and next. A replica publishes
   the previous key for `rotation_overlap` from its own start, so finish
   this rolling restart within `rotation_overlap` minus the longest token
   lifetime: 3600 s with the defaults. Raise `rotation_overlap` before
   this step for a slower rollout.
3. **Retire the old key.** Once `rotation_overlap` has passed since the
   last replica restarted, no replica publishes the old key and no token it
   signed is still valid. Remove `previous_key_file` at your next restart.

The new key signs with its own algorithm, so a rotation can also move the
gateway from ES384 to ES256 or back. A next key on another curve than the
current key refuses the configuration, naming `signing.next_key_file`,
unless `next_key_algorithm` names the algorithm you mean it for:

```toml
[signing]
key_file = "/run/secrets/ferrofed-signing-key.pem"            # P-384, signs ES384
next_key_file = "/run/secrets/ferrofed-signing-key-next.pem"  # P-256
next_key_algorithm = "ES256"                                  # the move to ES256 is intended
```

A next key that is already the current or the previous key is refused as
well. A change to `[signing]` takes effect only on a restart; a reload
reports it.

