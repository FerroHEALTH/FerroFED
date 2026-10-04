<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
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
each node ([Configuration](configuration.md#oauth-20-to-a-node)), and the
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
published JWK Set and its own client registry. Without one, it sends the
bearer token or the user and password configured for that endpoint. Either
way, every request also carries the `openEHR-federation-client` token,
signed with the same key, whose `iss` the node can check against that same
`client_id`.

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
([Tokens bound to a key](configuration.md#tokens-bound-to-a-key-dpop)). The
`openEHR-federation-client` token lives 60 seconds, names one node as its
`aud`, and carries a fresh `jti`, so a node that records `jti` values can
refuse a replay within that window. Certificate-bound tokens (RFC 8705) are
not built; the Dutch binding's DPoP profile is planned with
[#88](https://github.com/FerroHEALTH/FerroFED/issues/88).

**Transport identity.** The gateway never reads a transport identity as an
organisation's identity. TLS protects the connection: outside the
development profile, a credential or a patient identifier is sent only over
`https`. A client certificate, where your proxy asks for one, identifies a
connection and not an organisation; the organisation is the one the
verified token names. The XCPD binding's mutual TLS to a responding gateway
authenticates that connection the same way.

**What changes the answer:** the `resource` and `audience` keys of an
`oauth2` section; your reverse proxy's TLS settings.

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
- a `patient/` grant admits nothing, so a patient-facing app cannot use the
  gateway with a patient-confined token. A deployment that instead issues
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
([Consent](identity.md#consent)) adds a check before dispatch, never in
place of the node's own; `consent_refusal_codes` in the registry decides
which node refusals are reported `consent-denied`. No configuration turns
off the identifier gate, caller verification, or the rule that the caller's
token stays at the gateway.

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
```
