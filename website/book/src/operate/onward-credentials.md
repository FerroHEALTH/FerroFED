<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Onward credentials

How the gateway authenticates to each node it sends a request to (§13.1,
N25). Each endpoint's `[credentials."<id>"]` section names one scheme: a
bearer token or a user and a password
([Configuration](configuration.md#the-file)), or one of the grants on this
page, which obtain a token at the node's authorization server before the
request:

- `oauth2`: the client-credentials grant or token exchange, authenticated
  by an ES256 or ES384 assertion signed with the `[signing]` key;
- `nuts`: the Nuts track of the Dutch Generic Functions (Annex B §B.4);
- `fapi2`: an authorization server under the FAPI 2.0 Security Profile,
  such as the BgZ/eOverdracht track (Annex B §B.4a).

A section names one of them; two in one section refuse the configuration.
No log line, error or rendering carries a credential, an assertion, a key,
a proof or a token.

## OAuth 2.0 to a node

An `oauth2` section makes the gateway authenticate to that node as itself
(§13.1, N25). Before a request, it asks the node's token endpoint for an
access token with the client-credentials grant (RFC 6749 §4.4). It
authenticates there with a JWT client assertion (RFC 7523 §2.2), signed
with the `[signing]` key, ES256 or ES384 as its curve says. The assertion
names `client_id` as its `iss` and `sub` and the token endpoint as its
`aud`, lives
`assertion_lifetime_s` seconds, and carries a fresh `jti`. The token request
carries `scope` and, when set, `resource` and `audience`. Every key of the
section is required except those two and the assertion audience below:

- `scope` is space-separated SMART on openEHR scopes, each a resource scope
  of the `system` compartment (`system/aql-*.s`, `system/composition-*.cru`);
  anything else is refused at load. The token request carries each scope in
  the canonical form of the grammar, its permissions in `c`, `r`, `u`, `d`,
  `s` order and one space between scopes: `system/aql-*.sr` is requested as
  `system/aql-*.rs`.
- `token_endpoint` is an `https` URL with no user name, password, query or
  fragment; `http` is accepted only under `profile = "development"`
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).
- `assertion_audience` is `token_endpoint`, the default, or `issuer`. RFC
  7523 §3 admits either as the assertion's `aud`. An authorization server
  under the FAPI 2.0 Security Profile accepts only its issuer identifier,
  as one string (§5.3.2.1). With `issuer`, set `issuer` to that identifier,
  an `http` or `https` URL with no query or fragment in its canonical form
  (RFC 8414 §2); `issuer` beside the default refuses the configuration.

```toml
[credentials."cdr-c".oauth2]
# ...
assertion_audience = "issuer"
issuer = "https://auth.cdr-c.example.org"
```

The gateway caches a token until 30 seconds before the end of the lifetime
its `expires_in` states, with one token request per endpoint at a time. A
token with no stated lifetime serves one request. A node that answers `401`
drops the cached token, and the next request obtains a new one. When no
token can be obtained, that node is reported `node-error` and is sent
nothing: the gateway never dispatches unauthenticated. The answer says only
`no onward credential could be obtained, so nothing was sent`, followed by
the token endpoint's RFC 6749 §5.2 `error` code when it refused with a
registered one. The full account, the token endpoint's description
included, goes to the log at `warn` with the endpoint and the request id.
The admission check authenticates the same way.

The caller's own `Authorization` header never reaches a node.

## A token per caller: token exchange

`grant = "token_exchange"` gives each verified caller a token of its own at
that node, where the node's authorization server supports RFC 8693:

```toml
[credentials."cdr-c".oauth2]
grant = "token_exchange"
client_auth = "private_key_jwt"
token_endpoint = "https://auth.cdr-c.example.org/oauth2/token"
client_id = "ferrofed-gateway"
scope = "system/aql-*.s"                         # the gateway's own requests
resource = "https://cdr-c.example.org/openehr"   # required: the node (RFC 8707)
```

For a request on behalf of a caller, the gateway sends the token endpoint
the caller's verified access token as `subject_token`, an assertion of its
own as `actor_token`, with its own `jti`, and the client assertion as
above. It asks for the caller's granted scopes that cover the operation,
and never the rest (N26), each in the canonical form, and names the node
with `resource`. The answer must issue an access token
(`issued_token_type`, RFC 8693 §2.2.1). The token is cached per caller's
token and scope, at most 1024 per endpoint,
until 30 seconds before it expires, and dropped when the node answers
`401`. The cache keys on a SHA-256 of the caller's token, never the token.

- Only a caller verified by its token's signature or by introspection has a
  token to exchange. A caller the edge mode asserted has none, so that node
  is `node-error` and is sent nothing.
- A request that no granted scope of the caller covers, such as one with no
  SMART on openEHR family or a demographic request, is never exchanged: the
  node is `node-error` and is sent nothing. Without `scope` the
  authorization server would choose the scope itself (RFC 8693 §2.1).
- The gateway keeps the caller's token only while some node's grant is
  `token_exchange`, and never writes it to a log, a span, a metric or the
  `openEHR-federation-client` token.
- A caller's token whose text or claims carry the patient identifier the
  query was resolved on is not sent to the token endpoint, and that node is
  `node-error` (§5.4.1, N33).
- The gateway's own requests, the admission check and the redistribution of
  a held stored query, have no caller: they use the client-credentials
  grant at the same token endpoint, with `scope`.

The caller's token reaches the node's authorization server, which must
trust the caller's issuer, and never the node itself.

## Tokens bound to a key: DPoP

`dpop_key_file` in an `oauth2` section binds that node's tokens to a key of
the gateway's (RFC 9449), for a deployment that requires sender-constrained
tokens. The file holds a P-256 key, which signs ES256, or a P-384 key, which
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
more with the nonce it gave, within the request's budget, and every later
proof to that server carries the latest nonce it sent (RFC 9449 §8, §9).
The token endpoint's nonce and the node's are kept apart even when both
live on one host. A token request sent once more carries a newly signed
client assertion, and under token exchange a new actor token, so the
token endpoint never sees a `jti` twice. A node whose deadline passes
before that second send is `time-out`, and it counts as a node that was
asked.
The key is read at start and on each reload; a key that is no P-256 or
P-384 key refuses the configuration, naming `dpop_key_file`.

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
   `401`, as an `oauth2` token is, and every request to the node carries it
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
| `dpop_key_file` | The key the tokens are bound to, as in an `oauth2` section. Required: GF-Authentication sender-constrains every token (GFI-005). |
| `[[credential]]` | One per credential: `input_descriptor`, the descriptor it answers, and `file`, the JWT-encoded credential, issued to `did`. |

The gateway's DID document must publish the holder key under `kid` where the
`did:web` method resolves it (`https://<host>/.well-known/did.json` for a
DID with no path), so the authorization server can verify the presentation
(GFI-001); the gateway does not serve it. The credentials are issued to the
gateway by their authoritative sources ahead of time (GFI-002); the gateway
reads them from their files at start and on each reload, and checks only
that each is a JWT credential, with a `vc` claim, whose `sub` is `did`.

No log line, error or rendering carries a credential, the presentation, a
key, a proof or the token: a refusal names the authorization server's
`error` code and at most 256 characters of its description. An `oauth2`
section and a `nuts` section for the same endpoint refuse the configuration.

## The FAPI 2.0 grant (Annex B §B.4a)

A `fapi2` section makes the gateway authenticate to a node whose
authorization server follows the FAPI 2.0 Security Profile, as the
BgZ/eOverdracht track of the Dutch binding does (Annex B §B.4a, a VWS memo
at concept v0.9). The track uses OAuth 2.0 client credentials with
`private_key_jwt` client authentication, sender-constrained tokens, and the
healthcare attributes in an RFC 9396 `authorization_details` object of type
`nl-gis-v1` (§B.4a.2, §B.4a.3):

```toml
[credentials."cdr-e".fapi2]
issuer = "https://as.cdr-e.example.org"
grant = "client_credentials"            # or token_exchange
client_id = "urn:oid:2.16.528.1.1007.3.3.<URA>"
client_key_file = "/run/secrets/fapi2-client.pem"
dpop_key_file = "/run/secrets/cdr-e-dpop.pem"
scope = "system/aql-*.s"                # optional when authorization_details is set
authorization_details = '''[{"type": "nl-gis-v1",
  "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT",
  "locations": ["https://cdr-e.example.org/openehr"],
  "locations_organization_id": "urn:oid:2.16.528.1.1007.3.3.<URA of the node>"}]'''
# resource = "https://cdr-e.example.org/openehr"   # required by token_exchange
# audience = "cdr-e"                               # optional
```

1. At the first request to the node, the gateway reads the authorization
   server's metadata at the RFC 8414 well-known URL of `issuer`, once, and
   keeps it; a read that fails is tried again on the next request. The
   metadata must name that issuer exactly, with a `token_endpoint` on the
   issuer's origin, and an answer whose objects repeat a name is refused,
   as for the Nuts grant. It must also list:
   - `private_key_jwt` in `token_endpoint_auth_methods_supported`, and
     `ES256` in `token_endpoint_auth_signing_alg_values_supported` (RFC 8414
     §2: an omitted method list means `client_secret_basic` alone);
   - `client_credentials` in `grant_types_supported`, and token exchange
     under `grant = "token_exchange"` (an omitted list means
     `authorization_code` and `implicit` alone);
   - every `type` of `authorization_details` in
     `authorization_details_types_supported` (RFC 9396 §10);
   - `ES256` in `dpop_signing_alg_values_supported`, when the list is there
     (RFC 9449 §5.1).
2. It asks the token endpoint for a token with the client-credentials
   grant, or exchanges each verified caller's token as an `oauth2` grant
   does ([A token per caller](#a-token-per-caller-token-exchange)). The
   client assertion is signed ES256 with `client_key_file`'s key, and names
   the issuer, as one string, as its `aud` (FAPI 2.0 §5.3.3.1); the actor
   token of an exchange is signed the same way. The request carries `scope`
   when set and `authorization_details` as written.
3. It takes the token only when its `token_type` is `DPoP` and, under
   `authorization_details`, when the answer states the details it granted
   (RFC 9396 §7). A refusal with `invalid_authorization_details` is
   reported with that code. Every request to the node carries the token
   under the `DPoP` scheme with a proof of `dpop_key_file`'s key
   ([Tokens bound to a key](#tokens-bound-to-a-key-dpop)).

| Key | What it is |
|---|---|
| `issuer` | The issuer identifier of the node's authorization server (RFC 8414 §2), in its canonical form. `https` outside the development profile. |
| `grant` | `client_credentials` or `token_exchange`. `authorization_code` refuses the configuration. |
| `client_id` | The client the server registered the gateway as, the assertion's `iss` and `sub`. On the §B.4a track, the organisation's URA-based identifier. |
| `client_key_file` | The key every assertion is signed with: a P-256 private key in PKCS#8 PEM. The profile admits PS256, ES256 and EdDSA for a JWT (§5.4.1), so a P-384 key cannot sign here. |
| `previous_client_key_file` | Optional, while the client key is rotated: the previous client key, a P-256 private key in PKCS#8 PEM. It is published beside the current key until you remove it, and it never signs. |
| `dpop_key_file` | A P-256 private key in PKCS#8 PEM. Required: the profile issues only sender-constrained tokens (§5.3.2.1). |
| `scope` | Optional SMART on openEHR `system` scopes, as in an `oauth2` section. |
| `authorization_details` | Optional JSON text: an array of RFC 9396 §2 objects, each with a `type`. It is sent as written. |
| `resource`, `audience` | As in an `oauth2` section; `resource` is required by `token_exchange`. |

`scope` and `authorization_details` are each optional, and one of them is
required, so the server never chooses what a token may do (FAPI 2.0
§5.3.3.1: least privilege). A `fapi2` section needs `[signing]`: the
gateway publishes the public half of `client_key_file`'s key in its JWK Set,
beside the `[signing]` keys, and the authorization server verifies the
assertion against it (§5.4.2; Annex B §B.4a.2), and `assertion_lifetime_s`
sets how long each assertion lives. To make the two keys:

```text
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out fapi2-client.pem
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256 -out cdr-e-dpop.pem
```

To rotate the client key, make a new P-256 key, set it as
`client_key_file`, move the old one to `previous_client_key_file`, and
reload or restart. The new key signs every assertion from then on, and the
JWK Set publishes both keys, the new one first. An authorization server
that still holds the JWK Set from before the rotation does not know the
new key, so it refuses the new assertions until it fetches the set again;
the set it fetches then still holds the old key, so an assertion the old
key signed before the rotation verifies too. Once every authorization
server that verifies the grant has fetched the new set, remove
`previous_client_key_file`. A previous key that is no P-256 key, or that
is the current key, refuses the configuration, naming the key.

```toml
client_key_file = "/run/secrets/fapi2-client-2026-10.pem"
previous_client_key_file = "/run/secrets/fapi2-client.pem"
```

What the gateway does not do on this track:

- **An authorization-endpoint flow.** The authorization code grant needs a
  user agent to redirect, with pushed authorization requests (RFC 9126) and
  PKCE, which FAPI 2.0 requires of those flows (§5.3.2.2, §5.3.3.2). A
  server-to-server gateway has no user agent, so `grant =
  "authorization_code"` refuses the configuration. The client-credentials
  grant is admitted under the profile's general requirements (§5.3.2.1
  Note 2).
- **MTLS client authentication or certificate-bound tokens.** The profile
  admits either in place of `private_key_jwt` and `DPoP` (§5.3.2.1); the
  §B.4a track chooses `private_key_jwt`. Mutual TLS at the transport, which
  §B.4a.2 keeps, is the deployment's network layer.
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
([Client authentication](authentication.md#what-a-node-is-told-about-the-caller);
§13.1, N24), and a federating gateway without the key refuses to start,
naming it. The current key signs every client assertion of an `oauth2`
grant too. A key on another curve refuses the configuration, naming the
key. To make a key:

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
set also publishes the ES256 client key of every `fapi2` section, and its
previous client key while it is rotated, after the `[signing]` keys; a
reload that changes a `fapi2` key publishes the new one.

To rotate, make a new key, set it as `key_file`, move the old one to
`previous_key_file`, and restart. The new key signs from then on, with its
own algorithm, so a rotation also moves the gateway from ES384 to ES256 or
back. The JWK
Set publishes both keys for `rotation_overlap_s` seconds from the start of
the process, then the current key alone. The overlap must be at least the
assertion lifetime plus the time the nodes cache the JWK Set, so a node
still holding the old set, or an assertion the old key signed, finds its
key. Once the window has passed, remove `previous_key_file`. A change to
`[signing]` takes effect only on a restart; a reload reports it.

