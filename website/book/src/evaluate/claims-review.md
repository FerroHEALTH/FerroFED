<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Claims review

FerroFED's manufacturer reviews what its public texts claim, once per
release, against two provisions of Regulation (EU) 2025/327 on the European
Health Data Space. This page is the method, the record of each review, and
the assessment of the features that could burden access to health data.
Every quotation is from the Official Journal text, vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice.

## What the Regulation asks

Art 28 forbids, "in the information sheet, instructions for use or other
information accompanying EHR systems, and in the advertising of EHR systems",
any text or sign that may mislead the professional user "with regard to their
intended purpose, interoperability and security" by:

- "(a) ascribing functions and properties to the EHR system which it does
  not have;"
- "(b) failing to inform the professional user of likely limitations related
  to interoperability or security features of the EHR system in relation to
  its intended purpose;"
- "(c) suggesting uses for the EHR system other than those stated to form
  part of the intended purpose in the technical documentation."

Annex II, point 2.5: "The harmonised software components of an EHR system
shall not include features that prohibit, restrict or place an undue burden
on authorised access, personal electronic health data sharing or use of
personal electronic health data for permitted purposes." Art 7(1) gives a
person the right to have their data transmitted to another healthcare
provider "without hindrance from the healthcare provider or from the
manufacturers of the systems used by that healthcare provider".

## How a review is done

The review reads every public text: `README.md`, `SUPPORT.md`, `SECURITY.md`,
`llms.txt` and `CITATION.cff`; every page of this book; the landing page at
<https://ferrofed.eu>; the README and the manifest description of each crate;
and the title, description and labels of both container images. Each
statement about FerroFED's intended purpose, interoperability or security is
held to three questions, one per point of Art 28:

1. Does a release ship it, and does a test hold it? A statement that
   ascribes more is rewritten, (a).
2. Does it leave out a limitation a professional user would need? The
   limitation goes on the [Limitations](what-ferrofed-claims.md#limitations)
   list and next to the statement, (b).
3. Does it suggest a use outside the
   [intended purpose](regulatory-status.md#intended-purpose)? It is
   rewritten, (c).

A review runs before every release cut (`docs/release.md`, Before the tag),
and adds a row to the record below. A finding that is not about a claim, such
as a broken command, is fixed in the same change.

## The record

| Release | Reviewed on | Texts read | Findings | Fixed in |
|---|---|---|---|---|
| v0.0.10 | 2026-10-06 | see below | 16, below | [#674](https://github.com/FerroHEALTH/FerroFED/issues/674), except finding 16 |
| v0.0.10 | 2026-10-15 | see below | 17 to 30, below | the v0.0.10 version bump, except finding 16, which stands |
| v0.0.11 | 2026-10-15 | this page and the clinical safety risk file | 31 and 32, below | [#962](https://github.com/FerroHEALTH/FerroFED/issues/962) |

### Findings of 2026-10-06

Read in full: `README.md`, `SUPPORT.md`, `SECURITY.md`, `llms.txt`,
`CITATION.cff`, the landing page, the book's introduction, the Evaluate
pages written by hand, the self-description section of the client contract,
the README and manifest description of every crate, and the image labels.
Every other page of the book was searched for absolute and security words
(never, always, guarantee, certified, compliant, secure, safe, "never
learns") and each hit read in its paragraph.

Against Art 28(a), a function or property FerroFED lacks:

1. `README.md`, the book's introduction, the landing page and `llms.txt` said
   a client "never learns it was federated". A client reads every member
   with its status in `meta.federation`. Now: a client sends an ordinary
   query with no federation syntax and gets back an ordinary result set
   whose `meta.federation` names each node.
2. `README.md` and the book's introduction said no directly identifying
   identifier "travels to a node". The rule covers every request the gateway
   composes; a body the client writes reaches its node byte for byte (§5.4,
   N22). Now said that way.
3. The landing page's card "No identifier reaches a node", for the same
   reason. Now "No identifier in what the gateway sends a node", with the
   write body named.
4. The landing page credited mutual TLS to a node to v0.0.8. It shipped in
   v0.0.9 (#492).
5. The landing page said the conformance statement "scores every conformance
   point ... against FerroEHR and EHRbase". Some points are deferred with a
   recorded reason, and EHRbase is checked for the node profile only.
6. The landing page's quickstart sent a query with no access token and
   started the gateway with no signing key, so it answered `401`. It now
   writes the key and mints the token as the README does.
7. The README's quickstart sent the body `5820` in place of the query. It
   now reads the query from standard input.

Against Art 28(b), a limitation left out:

8. Nothing listed the limitations a professional user needs before deploying
   FerroFED. The claims page now carries them:
   [Limitations](what-ferrofed-claims.md#limitations).
9. `README.md` claimed ITS-REST 1.1.0 "on both faces" without saying that the
   DEMOGRAPHIC area is not federated and an unserved path answers `501`. Now
   said beside the claim.

Stale or wrong statements, which read as claims about the release a user
runs:

10. `README.md` named v0.0.7 as current and v0.0.8 as unreleased.
11. `llms.txt` named v0.0.8 as on `main`.
12. The claims page listed v0.0.9 as unreleased and its query console as
    planned.
13. The engine's and the console's READMEs listed follow-up forwarding and
    the query console as planned. Both shipped.
14. The client contract said `OPTIONS {base}/` declares no `auth.jwks_uri`
    because the gateway publishes no JWK Set. Every federating gateway
    publishes one and declares it.
15. `CITATION.cff` dated v0.0.9 2026-10-04; its tag and its changelog
    section are of 2026-10-05.

Not fixed here:

16. The `openehr-federation` README says version-identity de-duplication
    "follows in build order"; it shipped in v0.0.6. The text under-claims,
    so Art 28 is not breached. A change to that README is a change to the
    packaged crate and moves its version, which is that crate's own pull
    request.

No text suggested a use outside the intended purpose, (c).

### Findings of 2026-10-15

The review at the v0.0.10 cut. Read in full: `README.md`, `llms.txt`,
`CITATION.cff`, the landing page, the book's introduction, the Evaluate pages
written by hand, the instructions for use, the patient summary page, and the
README and manifest description of `ehds-logging`, `eehrxf` and
`openehr-federation`. Every other page of the book, `SUPPORT.md` and
`SECURITY.md` was searched for "planned" and "yet" next to an issue that is
closed, and for the words of the first review.

Against Art 28(a), a function or property FerroFED lacks: none found.

Against Art 28(b), a limitation left out:

17. The instructions for use did not say that a received document is not
    yet checked in full ([#808](https://github.com/FerroHEALTH/FerroFED/issues/808)), that the terminology rules are not yet
    applied to the summary the gateway writes ([#926](https://github.com/FerroHEALTH/FerroFED/issues/926)), or that behind
    several replicas a follow-up write that names no node can be refused
    ([#641](https://github.com/FerroHEALTH/FerroFED/issues/641)). All three are now listed.

Stale or wrong statements, which read as claims about the release a user
runs:

18. `README.md` described v0.0.10 as being built on `main` and planned the
    reporting procedure and the support period, which ship in it
    ([#762](https://github.com/FerroHEALTH/FerroFED/issues/762), [#763](https://github.com/FerroHEALTH/FerroFED/issues/763)).
19. `llms.txt` said retention by origin and category is planned and the
    gateway does not serve the interoperability component; both ship
    ([#795](https://github.com/FerroHEALTH/FerroFED/issues/795), [#809](https://github.com/FerroHEALTH/FerroFED/issues/809)).
20. The landing page named v0.0.9 as current. It now names v0.0.10 and has
    a card for it.
21. The claims page planned the re-pin to 1.0.0, the reporting procedure
    and the support period for v0.0.10 and listed no v0.0.10 section under
    Shipped.
22. The instructions for use listed the professional's identification and
    the assurance level as missing from the access record ([#742](https://github.com/FerroHEALTH/FerroFED/issues/742)).
23. The regulatory status page planned the Art 14 reporting procedure, the
    support period and the end-of-support notice ([#762](https://github.com/FerroHEALTH/FerroFED/issues/762), [#763](https://github.com/FerroHEALTH/FerroFED/issues/763),
    [#782](https://github.com/FerroHEALTH/FerroFED/issues/782)), and put the remaining EHDS work in the v0.0.10 milestone; it
    is in v0.0.11.
24. The information sheet planned the support period of each release
    ([#763](https://github.com/FerroHEALTH/FerroFED/issues/763)). It now states the end date of 0.0.10.
25. The information sheet's standards table left out the HL7 Europe
    packages, the Xt-EHR models, FHIRconnect and the TLS versions the
    release uses. They are added, held to the pin matrix where it pins
    them.
26. The data-protection page and the threat model said the records are kept
    for no period by origin and category; each record states its period
    ([#795](https://github.com/FerroHEALTH/FerroFED/issues/795)). The data-protection page also planned the serious-incident
    procedure, which is written ([#672](https://github.com/FerroHEALTH/FerroFED/issues/672)).
27. The clinical safety risk file said the gateway does not serve the
    interoperability component, and marked as planned controls that shipped
    ([#742](https://github.com/FerroHEALTH/FerroFED/issues/742), [#722](https://github.com/FerroHEALTH/FerroFED/issues/722), [#663](https://github.com/FerroHEALTH/FerroFED/issues/663), [#687](https://github.com/FerroHEALTH/FerroFED/issues/687), [#688](https://github.com/FerroHEALTH/FerroFED/issues/688), [#734](https://github.com/FerroHEALTH/FerroFED/issues/734),
    [#802](https://github.com/FerroHEALTH/FerroFED/issues/802)). Its review row for 0.0.10 records the update.
28. The patient summary page planned the FHIR Validator lane, which CI runs
    ([#688](https://github.com/FerroHEALTH/FerroFED/issues/688)).
29. This page said the harmonised components are not built; the logging
    component's access record and the interoperability component's FHIR
    face are.
30. The release notes repeated two upgrade notes and two security entries,
    and told an operator to read `GET /health/dependencies`, which v0.0.10
    no longer serves ([#751](https://github.com/FerroHEALTH/FerroFED/issues/751)). The duplicates are merged and the note
    names the operator surface.

Not fixed here: finding 16 stands, and the clinical safety file's X-01
cites server tests planned with [#689](https://github.com/FerroHEALTH/FerroFED/issues/689), which is closed without them.

No text suggested a use outside the intended purpose, (c).

### Findings of 2026-10-15, after the cut

Read against the log work that shipped: this page and the clinical safety
risk file.

Stale or wrong statements, which read as claims about the release a user
runs:

31. This page planned the export of the gateway's state for a replacement
    product (Annex II, point 2.6) with
    [#660](https://github.com/FerroHEALTH/FerroFED/issues/660). It shipped
    with [#798](https://github.com/FerroHEALTH/FerroFED/issues/798).
32. The clinical safety risk file held L-07 open until #660, although the
    review of the records with ITI-81 is built and tested. L-07 is re-rated
    and its review row records it.

## Annex II, point 2.5

Point 2.5 binds the harmonised software components, which are partly built
([Regulatory status](regulatory-status.md#what-is-built-and-what-is-planned)).
The patient summary of the FHIR face runs on the same query path, and the
rest of the interoperability component is planned on it
([#522](https://github.com/FerroHEALTH/FerroFED/issues/522)), so the features
of that path that can stop or delay an answer are assessed here, with Art
7(1), which names the manufacturers of the systems a healthcare provider
uses. The [Annex II checklist](annex-ii.md) takes its reasoning for point
2.5 from this section.

### The completeness default

A federated query is all-or-nothing by default: when a member that was asked
does not answer, the query fails `504` or `424` with no rows, and
`meta.federation` names every member with its status (§11.4, N37). A client
that accepts a partial answer asks for one per request with
`openEHR-federation-completeness: partial`, and gets the rows of the members
that answered, with every missing member named
([Completeness](../operate/queries-and-areas.md#completeness)).

**Assessment: no undue burden.** The default is the one the Federation Tier
specification requires (N37), and it keeps a clinician from reading part of a
record as the whole of it, which Annex II, point 1.1, asks of an EHR system
("does not put at risk patient safety"). Access is not withheld: the partial
answer is one header away, and both answers name what is missing. Two
settings can turn the default into a burden, and the
[instructions for use](../operate/instructions-for-use.md#configuration-for-a-deployment-in-the-eu)
say so:

- `[federation] best_effort = false` withdraws the partial answer, so one
  silent member blocks every answer. Keep the default, `true`.
- A node's consent refusal fails the query `424` until its codes are listed
  in `consent_refusal_codes` ([Consent](../operate/consent.md)). List them for
  every node that refuses on consent.

### The budgets

A member's request has `per_node_timeout_ms` (10 seconds by default) and the
whole fan-out `overall_timeout_ms` (25 seconds); a client may shorten the
wait, never lengthen it
([Timeouts](../operate/queries-and-areas.md#timeouts)). The gateway serves at
most `max_concurrent_requests` at once and answers `503` with `Retry-After`
past it, may rate-limit each caller with `429` and `Retry-After`, and sends a
member at most `max_in_flight_per_node` requests at once
([Overload protection](../operate/overload.md)).

**Assessment: no undue burden.** The budgets bound how long one request or
one slow member can hold the gateway, so one caller cannot take access away
from the others. Every limit is configurable, the time budget is declared in
`OPTIONS {base}/`, the per-caller rate is off by default and the same for
every caller, and a refused request gets a status and a `Retry-After` that
say when to try again. A deployment that sets the limits below what its
members need can delay access; the instructions for use give the
[sizing](../operate/instructions-for-use.md#sizing).

### The consent pre-filter

The pre-filter is optional and off by default. It leaves out a member only
where the consent service denies the request, which carries out a
restriction the person chose (Art 8). When the consent service cannot answer,
every candidate is asked and each node checks consent itself (§13.2.1, N27a)
([Consent](../operate/consent.md)).

**Assessment: no undue burden.** The pre-filter removes access only where the
person restricted it, and its outage never blocks an answer. A deployment in
the EU sets `[federation.consent] disclose = false`, so an exclusion is not
shown to the healthcare provider
([Withholding consent exclusions](../operate/consent-exclusions.md)).

### Art 7(1): transmission without hindrance

FerroFED holds no clinical data of its own: a record stays at the member CDR
that holds it. A write goes, as the client sent it, to the member that
controls the record, or for a new object to the member the client names
(§12.4, N22, N23), and a routed read comes back as the node answered. The
gateway adds no fee, no export limit and no format of its own to either.
Exporting the gateway's own state for a replacement product is Annex II,
point 2.6: an operator exports the routing state the gateway learned with
`POST /admin/identity-state/export`
([Exporting the identity state](../operate/identity-state-export.md)), and
the access records are exported from the Audit Record Repository.
