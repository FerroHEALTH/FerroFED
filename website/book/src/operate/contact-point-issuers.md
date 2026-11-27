<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# National contact point issuers

**Audience:** operators who connect a national contact point. This page configures the issuer of a contact point's tokens: an `[auth.issuer.<name>]` of `kind = "federation"` and its `[[federation.contact_point]]`. [A national contact point](../integrate/national-contact-point.md) describes what the connector calls, and [Client authentication](authentication.md) every other issuer.

## National contact points

A national contact point for digital health relays the request of a health
professional of another Member State (Regulation (EU) 2025/327 Art 23).
Implementing Regulation (EU) 2026/2099 gives the identification,
authentication and authorisation of that professional to the entity their
Member State lists (Art 6(1), (2)), and has the contact point of the
professional's Member State communicate the professional's and the
provider's identification data, the attributes of its Annex Tables 1 and 2
(Art 7). You declare the issuer of the contact point's tokens as a
`federation` issuer whose `[auth.issuer.<name>.federation]` table sets
`contact_point = true`, and name it in a `[[federation.contact_point]]`:

```toml
[auth.issuer.ncp]
issuer = "https://ncp.example.org"
kind = "federation"
sender_constraint = "required"

[auth.issuer.ncp.federation]
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
and an `[auth.issuer.<name>.federation]` table that names no source of
`provider.issuing_authority_name`.

**Who authenticates the professional.** The contact point's side does. The
gateway verifies the contact point's token, as it verifies every caller's,
and reads the professional and the provider from it as the contact point
asserts them; it never authenticates the professional, and cannot check the
other Member State's identification means. Federation Tier §13.4 lets the
source "rely on the requester's assertion", and the node is told the
attributes marked as the contact point's assertion. An assurance level the
token states is read through `[auth.issuer.<name>.assurance]` as for any issuer
([The assurance level](authentication.md#the-assurance-level)).

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
([below](authentication.md#what-a-node-is-told-about-the-caller)). A correlation identifier
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
