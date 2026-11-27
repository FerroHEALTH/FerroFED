<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Onward credentials of the Dutch binding

**Audience:** operators of a deployment under the Dutch binding (Annex B). This page configures the two grants of Annex B that reach a node or the NVI: the Nuts grant and the FAPI 2.0 grant. [Onward credentials](onward-credentials.md) holds the OAuth 2.0 grants, token exchange, the onward lane, `DPoP`, mutual TLS and the signing keys every deployment uses.

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
   ([Tokens bound to a key](onward-credentials.md#tokens-bound-to-a-key-dpop)).

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

The same table under `[nl_gf.nvi.nuts]` authenticates the
gateway to the NVI, the Localization Service of
[Dutch localization](localization.md#dutch-localization-nl_gfnvi), as a data
user on GF-Authentication (the IG's Localization page, GFI-004, GFI-005):

```toml
[nl_gf.nvi.nuts]
authorization_server = "https://nuts.example.org/oauth2/nvi"
scope = "nl-gf-localization"
did = "did:web:gateway.example.org"
kid = "did:web:gateway.example.org#key-1"
key_file = "/run/secrets/nuts-holder.pem"
dpop_key_file = "/run/secrets/nvi-dpop.pem"

[[nl_gf.nvi.nuts.credential]]
input_descriptor = "organization_credential"
file = "/run/secrets/organization.jwt"
```

The token is obtained and cached as a node's is, each token request bounded
by the localization's `timeout`. Every search carries it under the `DPoP`
scheme with a proof of `dpop_key_file`'s key over the search URL without its
query, so the pseudonym never reaches a proof or the authorization server
(RFC 9449 §4.2, §7.1). The grant rides on the outbound client of
`[nl_gf.nvi.endpoint]` as its authorization, so that table names no
`credentials` beside it. A `401` from the NVI drops the token and the search
is sent once more with a fresh one; a nonce it demands is kept and the search
sent once more with it (RFC 9449 §9). A second `401` fails the search. A grant the authorization server
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
[credentials."cdr-e".credentials.oauth]
issuer = "https://as.cdr-e.example.org"
security_profile = "fapi2"
client_id = "urn:oid:2.16.528.1.1007.3.3.<URA>"
client_auth = "private_key_jwt"
private_key_file = "/run/secrets/fapi2-client.pem"
dpop_key_file = "/run/secrets/cdr-e-dpop.pem"
scope = "system/aql-*.s"                # optional when authorization_details is set
authorization_details = '''[{"type": "nl-gis-v1",
  "purpose_of_use": "http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT",
  "locations": ["https://cdr-e.example.org/openehr"],
  "locations_organization_id": "urn:oid:2.16.528.1.1007.3.3.<URA of the node>"}]'''
```

A token per caller adds an `onward` table beside it, as
[A token per caller](onward-credentials.md#a-token-per-caller-token-exchange) shows, with the
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
   ([Tokens bound to a key](onward-credentials.md#tokens-bound-to-a-key-dpop)).

The keys every grant takes, held to the profile:

- `client_auth` is `private_key_jwt`, or `tls_client_auth` or
  `self_signed_tls_client_auth` by the node section's certificate
  ([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705)); under
  `private_key_jwt`, the assertion names the issuer under `typ`
  `client-authentication+jwt`, which the profile fixes (§5.3.3.1).
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

[credentials."cdr-e".credentials.oauth]
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
`previous_private_key_file`, or set `previous_key_overlap` to have the
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
