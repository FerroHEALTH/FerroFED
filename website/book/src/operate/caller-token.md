<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What a node is told about the caller

The gateway authenticates to each node as itself, with that endpoint's own
credentials ([OAuth 2.0 to a node](onward-credentials.md#oauth-20-to-a-node)),
and tells the node who asks in a header of its own (§13.1, N24, N25, §12.4).
Where a node's authorization server exchanges tokens (RFC 8693), the gateway
also asks it for a token per caller, with the caller's verified token as the
subject and only the caller's scopes that cover the operation; the caller's
token reaches that authorization server alone, never the node, and a caller
the edge mode asserted has no token to exchange, so that node is
`node-error` with nothing sent
([A token per caller](onward-credentials.md#a-token-per-caller-token-exchange)).
Every request it sends a node carries `openEHR-federation-client`: the
federated query, every routed read and write, a definition request, the
ask-all probe and the read of an EHR by subject. The value is a compact JWS
the gateway signs for that one node with the current key of `[signing]`,
the key whose public half it publishes at `{base}/.well-known/jwks.json` and
declares as `federation.auth.jwks_uri` in `OPTIONS {base}/`. Its JOSE header
names `alg`, `ES256` for a P-256 key and `ES384` for a P-384 key, the
key's `kid` and `typ` `openehr-federation-client+jwt`. Its claims:

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
| `scope` | the caller's scopes as granted; under a [patient grant](authentication.md#patient-grants), only the `patient/` scopes that cover the operation |
| `ehrId` | under a [patient grant](authentication.md#patient-grants) only: the patient's own `ehr_id` at this node (SMART on openEHR master04 §Capabilities) |
| `acting` | `person` when the caller's `sub` names a natural person, `client` when it names a client application ([Professionals and assurance](authentication.md#professionals-and-assurance)) |
| `subject_name` | the professional's name, when the caller's token states `extensions.ihe_iua.subject_name` (IHE IUA) |
| `national_provider_identifier` | the professional's identifier from their national authority, when the caller's token states it (IHE IUA) |
| `national_provider_identifier_authority` | the name of the agency that issued that identifier, when the caller's issuer sets `professional_issuing_authority` and the token states it (Implementing Regulation (EU) 2026/2099 Annex Table 1 `issuing_authority_name`) |
| `subject_role` | the professional's roles, each as `{"system", "code"}`, when the caller's token states `extensions.ihe_iua.subject_role` (IHE IUA) |
| `assurance_level` | `low`, `substantial` or `high`, the level the caller's authentication reached, when its issuer declares `[trust.issuer.assurance]` and the token states a value it lists |
| `national_contact_point` | for a caller a [national contact point](authentication.md#national-contact-points) vouched for: `health_professional`, every row of Implementing Regulation (EU) 2026/2099 Annex Table 1 under its data identifier, `healthcare_provider`, every row of Table 2, and `asserted_by`, the contact point's issuer, which marks them as its assertion |

With `subject_organization_id`, the provider's identifier, the professional
claims let a node record the professional and the provider of every
registration or update it makes on the caller's behalf, which Regulation (EU)
2025/327 Art 13(4) asks an electronic health record to identify. Declare
`professional_issuing_authority` for an issuer whose tokens state the agency
that issued the identifier.

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
([Signing keys](onward-credentials.md#signing-keys-and-the-jwk-set)). The
specification leaves end-user conveyance open (§13.1), so the header and its
claims are FerroFED's own design.

## Verifying it at the node

A node that admits FerroFED reads the header before it decides what to
release, and audits who asked:

1. At admission, record the gateway's `jwks_uri` (its `OPTIONS {base}/`
   declares it as `federation.auth.jwks_uri`) and the `iss` it will see: the
   `client_id` the node's authorization server registered for the gateway,
   or the federation id where the gateway sends no OAuth 2.0 grant.
2. Read exactly one `openEHR-federation-client` value; refuse a request with
   none or with more than one.
3. Check the JOSE header: `typ` is `openehr-federation-client+jwt` and `alg`
   is the `alg` the gateway's JWK Set publishes for the key the header names,
   `ES256` or `ES384`. Refuse any other algorithm, `none` included (RFC 8725
   §3.1).
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
   trail, with `national_contact_point` where a contact point relays the
   request: its professional and provider are the contact point's
   assertion, which `asserted_by` names. Consent stays your own check
   (§13.2, N27).
