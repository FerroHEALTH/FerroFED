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
| v0.0.11 | 2026-10-16 | see below | 33 to 43, below | the v0.0.11 version bump, except finding 16, which stands |
| v0.0.12 | 2026-10-24 | see below | 44 to 55, below | the v0.0.12 version bump |
| v0.0.13 | 2026-10-25 | see below | 56 to 63, below | the v0.0.13 version bump |
| v0.0.14 | 2026-10-28 | see below | 64 to 69, below | the v0.0.14 version bump |

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

### Findings of 2026-10-16

The review at the v0.0.11 cut. Read in full: `README.md`, `llms.txt`,
`CITATION.cff`, the landing page, the Evaluate pages written by hand, the
instructions for use, the prescriptions page, the Upgrading page, and the
assembled release notes. Every other page of the book, `SUPPORT.md`,
`SECURITY.md` and the README and manifest description of each crate was
searched for "planned", "not yet" and "v0.0.1" next to an issue that is
closed or a release that has shipped.

Against Art 28(a), a function or property FerroFED lacks:

33. The claims page listed electronic prescriptions and dispensations among
    what v0.0.10 shipped. They ship in v0.0.11 ([#1039](https://github.com/FerroHEALTH/FerroFED/issues/1039), [#1040](https://github.com/FerroHEALTH/FerroFED/issues/1040),
    [#1041](https://github.com/FerroHEALTH/FerroFED/issues/1041), [#1042](https://github.com/FerroHEALTH/FerroFED/issues/1042)), and the bullet moved to a new v0.0.11 section.
34. The instructions for use said the record of a query over many patients'
    data names no patient. Since [#819](https://github.com/FerroHEALTH/FerroFED/issues/819) it names the patients behind the
    `ehr_id`s its rows project, up to a bound, and records only its scope
    otherwise. Now said that way.

Against Art 28(b), a limitation left out:

35. The instructions for use and the claims page did not say that a member
    holding its prescriptions only as openEHR data is not served
    ([FerroHEALTH/FerroBRIDGE#1322](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1322)), that the gateway offers no log access interface of its own
    ([#660](https://github.com/FerroHEALTH/FerroFED/issues/660)), or which settings stay in the file under `config.ui =
    "managed"` and that the console reads one replica at a time. All are
    now listed.

Stale or wrong statements, which read as claims about the release a user
runs:

36. `README.md` named v0.0.10 as current and planned the rest of the
    interoperability component with no word of prescriptions. It now names
    v0.0.11, what it adds, and the planned work by issue.
37. The landing page named v0.0.10 as current and v0.0.11 as planned. It
    now names v0.0.11 and has a card for it.
38. `llms.txt` said every priority category other than the patient summary
    is planned. Prescriptions and dispensations are passed through from the
    members that serve them.
39. The claims page planned for v0.0.11 work that shipped in it or moved to
    v0.0.12 and v0.0.13; the regulatory status page put the remaining EHDS
    issues in the v0.0.11 milestone, left prescriptions out of what the
    interoperability component does, and planned the Annex II checklist and
    the technical documentation, which are written. Each now says where the
    work stands.
40. The configuration page planned the export and import of a revision
    ([#579](https://github.com/FerroHEALTH/FerroFED/issues/579)), which ships.
41. The information sheet listed the HL7 Europe MPD package twice in its
    standards table. It is listed once, and the operator console's OpenID
    Connect sign-in is added beside the standards.
42. The release notes described the console's top bar, navigation and
    system theme as the styling change of [#1037](https://github.com/FerroHEALTH/FerroFED/issues/1037) left them; the new shell
    of [#1075](https://github.com/FerroHEALTH/FerroFED/issues/1075) in the same release replaced all three. The entry now
    describes the styling and points at the shell.
43. The release notes named `fhirconnect` 0.1.111 on `openehr-*` 0.0.86 as
    where the mapping engine moves ([#1028](https://github.com/FerroHEALTH/FerroFED/issues/1028)); the release ships the later
    versions of [#714](https://github.com/FerroHEALTH/FerroFED/issues/714). The entry now says so.

Not fixed here: finding 16 stands.

No text suggested a use outside the intended purpose, (c).

### Findings of 2026-10-24

The review at the v0.0.12 cut. Read in full: `README.md`, `llms.txt`,
`CITATION.cff`, the landing page, the Evaluate pages written by hand, the
instructions for use, the Upgrading page, the configuration revisions page
and the assembled release notes. Every other page of the book,
`SUPPORT.md`, `SECURITY.md` and the README of each crate was searched for
"planned", "not yet", "v0.0.1", `ferrofed-viewer`, `FERROFED_VIEWER` and
`bearer_token` next to an issue that is closed, a release that has shipped
or a name the release removes.

Against Art 28(a), a function or property FerroFED lacks:

44. The Upgrading page says a key is never renamed or removed at once and
    the old key is read for at least one release. v0.0.12 moves the issuer
    policy of `[auth]` to `[trust]`, removes every static bearer token and
    renames the console's `FERROFED_VIEWER__` overrides with no release that
    reads the old keys ([#1174](https://github.com/FerroHEALTH/FerroFED/issues/1174), [#1163](https://github.com/FerroHEALTH/FerroFED/issues/1163), [#1219](https://github.com/FerroHEALTH/FerroFED/issues/1219)). The page now says so
    under the rule, and the upgrade notes list each key.
45. The landing page said v0.0.12 brings the national contact point's
    connector. Its end-to-end test waits on a published FerroPIX image and
    moved to v0.0.13 ([#524](https://github.com/FerroHEALTH/FerroFED/issues/524), [#735](https://github.com/FerroHEALTH/FerroFED/issues/735)). The page now names what v0.0.12
    ships, and the claims page plans the connector for v0.0.13.

Against Art 28(b), a limitation left out:

46. The instructions for use and the configuration revisions page left
    `[trust]` out of the bootstrap core that stays in the file under
    `config.ui = "managed"`; the gateway holds it there beside `[auth]`. Both
    pages now list it.

Stale or wrong statements, which read as claims about the release a user
runs:

47. `README.md` named v0.0.11 as current and planned the logging
    component's log access interface ([#660](https://github.com/FerroHEALTH/FerroFED/issues/660)), which is closed: the records
    are read at the Audit Record Repository with ITI-81. It now names
    v0.0.12, what it adds, and plans the Art 36(1) re-check
    ([#799](https://github.com/FerroHEALTH/FerroFED/issues/799)).
48. The landing page named v0.0.11 as current. It now names v0.0.12 and
    has a card for it.
49. `llms.txt` named only the gateway image, called the gateway's outbound
    authentication "a static credential", and planned the logging
    component's review tools. It now names the console image, says HTTP
    Basic, and says where the records are read.
50. The claims page planned the national contact point's connector for
    v0.0.12 and the log access interface of [#660](https://github.com/FerroHEALTH/FerroFED/issues/660) for v0.0.13; the regulatory
    status page put the remaining EHDS issues in the v0.0.12 milestone,
    planned the logging component's review, retention and export, which are
    built, and listed [#526](https://github.com/FerroHEALTH/FerroFED/issues/526), which is closed. Each now says where the work
    stands, and the claims page has a v0.0.12 section.
51. The release notes described the console as each FerroUI step from
    `ferrohealth-ui` 0.1.3 to 0.3.6 left it, so an entry described a
    theme toggle in the top bar that a later entry removes and named paths
    under `app/ferrofed-viewer/`. The console entries now describe the
    console this release ships, on FerroUI 0.3.6.
52. The release notes' upgrade note for the console said the theme cookie
    keeps its name, which holds only from the first FerroUI 0.3 build on
    `main`; from v0.0.11 it moves from `ferrofed_viewer_theme` to
    `ferrofed_console_theme`, so each operator chooses the theme again. The
    note now says so.
53. The release notes' Removed entry for the Tailwind CSS pin named
    `scripts/release/viewer-tools.sh`, which the rename made
    `scripts/release/console-tools.sh`. The entry now names it.
54. The cybersecurity risk assessment said `deny.toml` holds two
    exceptions and that the console's site build runs a CSS tool. It holds
    one, `paste`, whose OpenVEX statement gives the argument, and the site
    build runs only its WebAssembly tools. Point (a) now says so, with the
    release's full scans.
55. The record held finding 16 as standing at the v0.0.10 and v0.0.11
    reviews. The `openehr-federation` README lost the "follows in build
    order" sentence in the v0.0.10 version bump (commit `ef7e3084`), so the
    finding is closed there.

No text suggested a use outside the intended purpose, (c).

### Findings of 2026-10-25

The review at the v0.0.13 cut. Read in full: `README.md`, `llms.txt`,
`CITATION.cff`, the landing page, the Evaluate pages written by hand, the
instructions for use, the Upgrading page and the assembled release notes.
Every other page of the book, `SUPPORT.md`, `SECURITY.md` and the README
of each crate was searched for "planned", "not yet", "plain gRPC" and the
issues the v0.0.13 milestone closed next to a statement that the work is
still to come.

Against Art 28(b), a limitation left out:

56. The instructions for use said a received document is not yet checked
    in full ([#808](https://github.com/FerroHEALTH/FerroFED/issues/808)), which v0.0.13 builds, and the claims page's list of
    limitations said nothing of the received document. Both now name what
    stays: a constraint the validator cannot evaluate is listed as a
    warning, not refused, and a `required` binding the packages cannot
    decide needs the terminology server.

Stale or wrong statements, which read as claims about the release a user
runs:

57. `README.md` named v0.0.12 as current and planned the national contact
    point's connector ([#524](https://github.com/FerroHEALTH/FerroFED/issues/524)), which v0.0.13 serves. It now names
    v0.0.13 and what it adds.
58. The landing page named v0.0.12 as current and said v0.0.13 continues
    the connector. It now names v0.0.13, has a card for it, and plans
    v0.0.14.
59. The claims page planned for v0.0.13 the connector and the operations
    packaging ([#735](https://github.com/FerroHEALTH/FerroFED/issues/735), [#638](https://github.com/FerroHEALTH/FerroFED/issues/638), [#644](https://github.com/FerroHEALTH/FerroFED/issues/644), [#645](https://github.com/FerroHEALTH/FerroFED/issues/645)), which shipped, beside work the
    v0.0.14 milestone holds. It now has a v0.0.13 section and plans
    v0.0.14.
60. The regulatory status page planned the national route in
    [#524](https://github.com/FerroHEALTH/FerroFED/issues/524) and put the remaining EHDS issues in the v0.0.13 milestone,
    and the rights page called the Art 7(2) route planned. Both now say
    the FHIR face serves a national connector, and the remaining issues are
    in the v0.0.14 milestone.
61. The threat model called the OTLP export plain gRPC with
    [#644](https://github.com/FerroHEALTH/FerroFED/issues/644) open, and left egress to the deployment pending
    [#638](https://github.com/FerroHEALTH/FerroFED/issues/638). An `https://` collector is now reached over TLS with mutual
    TLS on request, and the Kubernetes package's `egress` component
    restricts egress; the row and both residual risks now say so.
62. The clinical safety risk file kept I-03 open on [#808](https://github.com/FerroHEALTH/FerroFED/issues/808) and
    [#1213](https://github.com/FerroHEALTH/FerroFED/issues/1213), both closed by v0.0.13. The release's review row names what
    keeps it open: the medical alert and the functional status, which no
    openEHR content feeds yet.
63. `llms.txt` did not name the national connector the FHIR face serves.
    It now does.

No text suggested a use outside the intended purpose, (c).

### Findings of 2026-10-28

The review at the v0.0.14 cut. Read in full: `README.md`, `llms.txt`,
`CITATION.cff`, the landing page, the Evaluate pages written by hand, the
instructions for use, the Upgrading page and the assembled release notes.
Every other page of the book, `SUPPORT.md`, `SECURITY.md` and the README
of each crate was searched for "planned", "v0.0.14", "cross-reference" and
the issues the v0.0.14 milestone closed next to a statement that the work
is still to come.

Stale or wrong statements, which read as claims about the release a user
runs:

64. `README.md` named v0.0.13 as current. It now names v0.0.14.
65. The landing page named v0.0.13 as current and said v0.0.14 brings
    prescriptions from openEHR medication data
    ([FerroHEALTH/FerroBRIDGE#1322](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1322)), which
    this release does not ship. It now names v0.0.14, has a card for what
    it ships, and plans no prescription mapping in FerroFED.
66. The claims page planned for v0.0.14 the other priority categories and
    the prescription mapping, which move to FerroBRIDGE with the
    interoperability component
    ([#813](https://github.com/FerroHEALTH/FerroFED/issues/813)). It now has
    a v0.0.14 section, says where that work is built, and lists the
    re-checks that wait on the EU apart.
67. The regulatory status page put the remaining EHDS issues in the
    v0.0.14 milestone. It now says which wait on an adopted act and that
    the prescription mapping moves to FerroBRIDGE.
68. `llms.txt` planned the further categories without saying where, and
    did not say the gateway keeps no patient index
    ([#1377](https://github.com/FerroHEALTH/FerroFED/issues/1377)). It now
    says both.

Against Art 28(b), a limitation left out:

69. A draft EU declaration of conformity carried the Annex IV point 4
    statement of conformity under a banner that said only "Not issued".
    The draft now says it claims no conformity, that no conformity
    assessment has been carried out and that the Article 40 testing
    environment does not exist yet
    ([#940](https://github.com/FerroHEALTH/FerroFED/issues/940)).

No text suggested a use outside the intended purpose, (c).

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
