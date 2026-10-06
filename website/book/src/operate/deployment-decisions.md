<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The §13.4 deployment decisions

The specification fixes no authentication solution. It fixes five questions
a deployment must answer and record instead
([§13.4](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-deployment-decisions),
[N25](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/requirements.html#n25),
[CP-39](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/conformance.html#cp-39)).
CP-39 is an operator point: the specification scores it against your
deployment, not against the gateway. This page gives FerroFED's own answer
to each question, as the gateway behaves with its default configuration,
names the configuration that changes each answer, and ends with a
[template](#the-operators-template) for the parts only your deployment can
answer. Copy the template, fill it in, and publish it with your deployment.

The answers rest on three pieces of the gateway:
[client authentication](authentication.md), OAuth 2.0 from the gateway to
each node ([Configuration](onward-credentials.md#oauth-20-to-a-node)), and the
`openEHR-federation-client` token that tells each node who asks
([What a node is told about the caller](authentication.md#what-a-node-is-told-about-the-caller)).

## 1. Which identity is verified across the trust boundary

([§13.4 authn-which-identity](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-which-identity))

There are two boundaries, and a different identity crosses each.

**From the caller to the gateway.** The gateway verifies an RFC 9068 access
token: its issuer is on your trust list, its signature verifies against that
issuer's key set (or the issuer's RFC 7662 introspection endpoint calls it
active), and its `aud` names the gateway. What is verified is the issuer's
statement: the caller's `sub`, the `client_id`, and the organisation the IHE
IUA `subject_organization_id` names. The gateway checks the issuer's
signature over that organisation identifier. It does not look it up in an
organisation register: whether the issuer may speak for that organisation is
the trust you place in the issuer when you list it.

**From the gateway to each node.** The gateway authenticates as itself. With
an `oauth2` section for the endpoint, it signs an RFC 7523 client assertion
naming the `client_id` the node's authorization server registered for it,
and the node's authorization server verifies it against the gateway's
published JWK Set and its own client registry. With `client_auth =
"tls_client_auth"` or `"self_signed_tls_client_auth"`, the gateway's TLS
client certificate is that identity instead: the node's authorization
server matches the certificate to the client it registered for `client_id`
(RFC 8705 §2). Without a grant, it sends the bearer token or the user and
password configured for that endpoint, or, with TLS material alone, only
presents its certificate. Every request also carries the
`openEHR-federation-client` token, signed with the `[signing]` key, whose
`iss` the node can check against that same `client_id`.

**What changes the answer:** `[[auth.issuer]]` (the trust list, by key set or
introspection), `auth.audience`, `auth.mode`,
`[credentials."<endpoint id>"]` and its `oauth2` section, and `[signing]`.

## 2. Who authenticates the end user, and where that trust stops

([§13.4 authn-end-user](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-end-user))

**The requesting organisation does.** Its authorization server, or the proxy
in the edge mode, authenticates its own clinicians and systems and issues
the token. The gateway never authenticates an end user and never
re-authenticates one: it verifies the token the organisation's authorization
server signed, and that is where its check stops.

**The gateway holds the token to who acts and how they authenticated.** A
request that reaches patient data needs a natural person behind the token,
or a client its issuer declares as acting for the professional the token
names, and, where you declare it per issuer, the least assurance level the
token's `acr` (or the claim you name) must state (Regulation (EU) 2025/327
Annex II 3.1). The issuer's authentication sets that level; the gateway
reads it and refuses a token below it
([Professionals and assurance](authentication.md#professionals-and-assurance)).

**The node relies on the gateway's statement.** It does not re-authenticate
the caller either. It reads who asks from the `openEHR-federation-client`
token: the `sub` and `iss_upstream` the gateway verified, and `verified_by`,
which says `edge` when a proxy asserted the identity rather than an issuer's
token proving it. The caller's own token never reaches a node.

**What changes the answer:** `auth.mode = "edge"` moves end-user
authentication to your proxy, whose signed assertion the gateway then
verifies; `[[auth.issuer]]` decides whose authentication you accept.

## 3. Purpose of use

([§13.4 authn-purpose-of-use](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-purpose-of-use))

The purpose travels in the caller's token, never in the query: the IHE IUA
`extensions.ihe_iua.purpose_of_use` claim, HL7 v3 `PurposeOfUse` codings, or
RFC 9396 `authorization_details[].purpose_of_use`. It is required by
default: a token that declares none is a `403` (`purpose-of-use-required`)
on every route that reaches a node ([Purpose of use](authentication.md#purpose-of-use)).
The gateway relays every purpose the token declares to each node in the
`purpose_of_use` claim of `openEHR-federation-client`, so the node makes its
release and consent decision against it (N26, N27).

A `fapi2` section states a purpose to that node's authorization server as
well: the `purpose_of_use`, and for the query use case the
`subject_organisation_type`, of its configured `authorization_details`
(Annex B §B.4a.3). They are configured per endpoint and are the same for
every request to it, a declaration of the gateway's organisation for the
token, never derived from the caller. The caller's own purpose still
reaches the node in `openEHR-federation-client`
([The FAPI 2.0 grant](onward-credentials.md#the-fapi-20-grant-annex-b-b4a)).

**What changes the answer:** `auth.purpose_of_use.required = false` admits a
token with no purpose. A deployment that sets it records why here, because
its nodes then decide without one.

## 4. What the token is bound to

([§13.4 authn-token-binding](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-token-binding))

The specification asks two separate questions here.

**Bearer or sender-constrained.** Bearer by default at both hops, and
sender-constrained toward a node where you configure it. The caller's token
is a bearer token whose audience is the gateway, and it reaches no node, so
it cannot be replayed at one; under token exchange it reaches the node's
authorization server as the subject of the exchange. The token the gateway
obtains from a node's authorization server is a bearer token, kept until 30
seconds before it expires and audience-restricted when the `oauth2` section
names a `resource` (RFC 8707) or an `audience`, unless the section names a
`dpop_key_file`: the token is then bound to that key with DPoP (RFC 9449),
and a node can refuse it from anyone who does not hold the key
([Tokens bound to a key](onward-credentials.md#tokens-bound-to-a-key-dpop)). The
`openEHR-federation-client` token lives 60 seconds, names one node as its
`aud`, and carries a fresh `jti`, so a node that records `jti` values can
refuse a replay within that window. An `oauth2` or `fapi2` section with
`tls_client_certificate_bound_access_tokens = true` takes only tokens bound
to the gateway's TLS client certificate (RFC 8705 §3), sends them only over
connections that present it, and refuses one whose stated binding names
another certificate
([Mutual TLS to a node](onward-credentials.md#mutual-tls-to-a-node-rfc-8705)).
A `nuts` section binds every token with DPoP, as the Dutch
binding's Nuts track requires
([The Nuts grant](onward-credentials.md#the-nuts-grant-annex-b-b4)), and so
does a `fapi2` section, with DPoP or with the certificate, since the FAPI
2.0 Security Profile issues only sender-constrained tokens
([The FAPI 2.0 grant](onward-credentials.md#the-fapi-20-grant-annex-b-b4a)).

**Transport identity.** Toward the gateway, a transport identity is never
read as an organisation's identity. TLS protects the connection: outside the
development profile, a credential or a patient identifier is sent only over
`https`. A client certificate, where your proxy asks for one, identifies a
connection and not an organisation; the organisation is the one the
verified token names. Toward a node, the gateway's own certificate is read
as its organisation's identity only where you declare it, with
`client_auth = "tls_client_auth"` or `"self_signed_tls_client_auth"` in that
node's grant: the node's authorization server then authenticates the
gateway as the client registered for that certificate (RFC 8705 §2).
Otherwise a certificate the gateway presents to a node, and the XCPD
binding's mutual TLS to a responding gateway, authenticate the connection
alone. Record here, per node, which of the two you chose.

**What changes the answer:** the `resource`, `audience`, `client_auth` and
`tls_client_certificate_bound_access_tokens` keys of an `oauth2` or `fapi2`
section, `client_identity_file` in a node's section, and your reverse
proxy's TLS settings.

## 5. What the technique does not cover

([§13.4 authn-residual-risk](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/security.html#authn-residual-risk))

**Addressed technically, by the gateway:**

- no directly identifying patient identifier reaches a node, in the query,
  the path, a header or the conveyed caller claims (§5.4.1, N33);
- the caller's token is never forwarded, and no node is sent a request
  without its configured onward credential and the caller's identity;
- every caller is verified before anything is read, and every failure to
  verify fails closed;
- the gateway's keys rotate with an overlap window, and each node verifies
  against the published key set;
- consent is never inferred from localization: each node makes its own
  consent decision, whatever a pre-filter answered (§13.2, N27);
- a `patient/` grant admits nothing unless you bind its issuer to one
  member ([Patient grants](authentication.md#patient-grants)). Bound, the
  grant reaches only the patient's own `{node, ehr_id}` pairs, as your
  cross-reference service links them to the token's `ehrId`: a wrong link
  there admits the wrong EHR at that member, so record which issuers you
  bind and why you trust that link. A deployment that instead issues
  `user/` or `system/` scopes to patient users widens what those users can
  reach to everything those scopes cover; record that choice here.

**Addressed by agreement, outside the gateway:**

- admission: who may join, and on what evidence
  ([Admitting a node](admission.md); §12b);
- the trust list of caller issuers, and what each issuer may assert;
- the authentic register of organisation identifiers, and which issuer may
  speak for which organisation;
- audit and supervision while a member participates;
- logging and liability after the fact. The gateway logs each request by
  its own request id, route and status, never a body or a header value
  ([Configuration](configuration.md#request-ids)); each node audits who
  asked from the conveyed token (N24).

**Bearer-token replay** (§13.4 asks a deployment crossing organisational
boundaries to consider it): the caller's token cannot be replayed at a node,
because it never leaves the gateway; a stolen onward token or conveyed token
can be replayed at its own node until it expires. Record whether that is
acceptable in your setting.

**What changes the answer:** a consent pre-filter
([Consent](consent.md)) adds a check before dispatch, never in
place of the node's own; `consent_refusal_codes` in the registry decides
which node refusals are reported `consent-denied`;
`[federation.consent] disclose = false` keeps a member the pre-filter
excludes out of what a client sees, for Regulation (EU) 2025/327 Art 8, and
`OPTIONS {base}/` declares it
([Withholding consent exclusions](consent-exclusions.md)).
No configuration turns off the identifier gate, caller verification, or the
rule that the caller's token stays at the gateway.

### Consent exclusions in the answer

The specification has a member a consent pre-filter excludes reported
`consent-denied` (N27a), which tells the requesting clinician that the
patient restricted access at that member. Regulation (EU) 2025/327 Art 8
says that fact "shall not be visible to healthcare providers". The
specification does not list this among the §13.4 questions, but a
deployment under that regulation has to settle it and record the answer
with the others.

**FerroFED's default** is the specification's: `consent-denied`.
**With `[federation.consent] disclose = false`**, an excluded member is
reported as one the cross-reference does not know the patient at
(`not-resolved`), `meta.federation.complete` stays `false`, a read by subject
only an excluded member could serve answers `404 subject-unavailable`, and
`OPTIONS {base}/` declares `federation.consent.disclose: false`. The
pre-filter metrics still count every exclusion for the operator.

## The operator's template

FerroFED answers the questions above for the gateway. These parts only your
deployment can answer. Copy this, replace every `…`, and publish it with
your deployment's documentation.

```text
§13.4 deployment decisions: <federation id>, <date>, <who decided>

1. Identity across the trust boundary
   Authentic source of organisation identity: …   (for example the URA
     register of CIBG in the Netherlands, Annex B §B.4a)
   Issuers on the trust list, and the organisations each may assert: …
   Each node's authorization server, and the client_id it registered for
     the gateway: …

2. End-user authentication
   Who authenticates the clinicians of each calling organisation: …
   The assurance level each issuer's tokens state, and the least level
     required for patient data ([auth.issuer.assurance]): …
   Issuers whose client tokens act for a named professional, and why: …
   Token mode or edge mode, and for the edge mode which proxy: …
   How each node uses the conveyed caller in its release decision: …

3. Purpose of use
   auth.purpose_of_use.required: true | false, and if false, why: …
   The purpose-of-use codes callers send, and their legal basis: …

4. Token binding
   Bearer at both hops: accepted | not accepted, and why: …
   Transport identity read as organisation identity: never (FerroFED's
     answer); TLS terminated at: …

5. Risk carried by agreement
   Admission requirements and the evidence each member gives: …
   Audit and supervision while a member participates: …
   Logging, retention and liability: …
   Bearer-token replay: acceptable | mitigated by …

Consent exclusions in the answer
   Consent pre-filter: none | nl-gf-mitz, and its service: …
   federation.consent.disclose: true | false, and the legal ground
     (for example Regulation (EU) 2025/327 Art 8): …
```
