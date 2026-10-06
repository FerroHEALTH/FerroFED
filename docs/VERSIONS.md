<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Pinned version matrix

This file is the single source of truth for every version pin in FerroFED.
When it and a file that repeats a pin disagree, that is drift. Fix the
disagreement; never let either side silently win.
`scripts/checks/versions.sh` enforces the cross-file agreement it can reach,
and skips loudly for any file it compares that is absent.

No specification governs this file; it is FerroFED's own design.

## Specifications

The ground for each pin is the pin table in `docs/architecture.md`, the
architecture of record, which records why the value is what it is. The guard
compares the first token of each `Pin` cell below with the first token of the
same row there.

| Item | Pin | Repeated in |
|---|---|---|
| Federation Tier with AQL | 0.9.0 | `docs/architecture.md`, the `FEDERATION_SPEC` constant of `openehr-federation`, the source of the `spec_version` an `OPTIONS {base}/` body carries |
| openEHR ITS-REST | 1.1.0 | `docs/architecture.md`, the `ITS_REST` constant of `ferrofed-engine` |
| openEHR AQL | 1.1.0 | `docs/architecture.md`, the `AQL` constant of `openehr-federation` |

The federation specification is a release candidate circulated for comment by
the openEHR Federation Working Group. Its 1.0 release replaces this row and the
two corpus rows below in one change: re-pin, re-run both vendor scripts, and
diff the trees. The pinned commit is past the `0.9.0` git tag: it carries the
SEC review amendments of 2026-09-28 (the `meta.federation` nesting, the
all-or-nothing default, the `node-error` status) while the document still
declares `spec-version: '0.9.0'`.

The federation specification binds ITS-REST by name, Release-1.1.0, so the
ITS-REST row follows it rather than the latest ITS-REST development line.

## Bindings (decided, vendored with their first consumer)

The specification names the IHE ITI profiles as its proposed binding (Annex
A) without naming their versions, and the Dutch Generic Functions IG as a
regional alternative (Annex B), which it names at `fhir.nl.gf#0.3.0`. The
bindings and their versions are decided (`docs/architecture.md` §6, decision
A18): PIXm 3.1.0, mCSD 4.0.0 and PMIR 1.6.0 (CC-BY-4.0) and `fhir.nl.gf`
0.3.0 (EUPL-1.2), each vendored under `docs/specs/` and moved into the corpus
table below by the issue that first reads it (#42, #74 and #86, #147, #87).
XCPD has no FHIR package; its adapter (#85) binds the ITI Technical Framework
revision below, cited and not vendored: IHE International licenses its own
text for reproduction (General Introduction ch. 9), but the ITI-55 pages
reproduce HL7 v3 tables whose rights HL7 reserves. The client is held to the
revision by synthetic fixtures shaped after its examples
(`crates/ihe-iti/tests/fixtures/xcpd/`). IUA, which client authentication
reads for the ITI-71 token claims and the ITI-72 bearer presentation, is
vendored: its supplement is published in IHE's own repository under
CC-BY-4.0, General Introduction ch. 9 grants reproduction of IHE's own text,
and it reproduces no base-standard table. PDQm is not
used by the gateway; its ITI-78 client is a capability of `crates/ihe-iti`
for other callers (#119), vendored with it. The versions are what the FHIR
package registry listed as latest on 2026-10-01.

The FHIR profiles audit each transaction as a FHIR `AuditEvent` built on the
IHE Basic Audit Log Patterns (BALP), which travel to the Audit Record
Repository over the ATX: FHIR Feed Option of ITI-20 the RESTful ATNA
supplement defines (#486). BALP is vendored as a FHIR package, each profile's
audit profiles with its own package, and the ATNA text (the ITI-20 page of
Volume 2 and the RESTful ATNA supplement, IHE International's own text under
General Introduction ch. 9, which refers to DICOM PS3.15 by link) by the
sha256 of each file, since neither carries a revision in its URL. BALP 1.1.4
is what the FHIR package registry listed as latest on 2026-10-04.

| Binding | Package | Latest on 2026-10-01 |
|---|---|---|
| PIXm (ITI-83, ITI-104) | `ihe.iti.pixm` | 3.1.0, vendored by #42 (the corpus table below) |
| PDQm (ITI-78, ITI-119) | `ihe.iti.pdqm` | 3.2.0, vendored by #119 (the corpus table below) |
| PMIR (ITI-93, ITI-94) | `ihe.iti.pmir` | 1.6.0, vendored by #147 (the corpus table below) |
| mCSD (ITI-90, ITI-91) | `ihe.iti.mcsd` | 4.0.0, vendored by #86 (the corpus table below) |
| XCPD (ITI-55) | the IHE ITI Technical Framework, no FHIR package | Vol 2 Rev 20.1 (2024-12-12, Final Text) |
| ATNA (ITI-20, syslog with the DICOM message) | the IHE ITI Technical Framework, with DICOM PS3.15 Annex A.5 (NEMA, reproduction by permission only, cited and not vendored) and RFC 5424 and RFC 5425 | Vol 2 Rev 20.2 §3.20 (2025-11-11, Final Text), vendored by #486 (the corpus table below) |
| ATNA (ITI-20, the ATX: FHIR Feed Option) | the IHE ITI Technical Framework Supplement *Add RESTful ATNA*, no FHIR package | Rev 3.6 (2025-11-25, Trial Implementation), vendored by #486 (the corpus table below) |
| BALP (the `AuditEvent` patterns) | `ihe.iti.balp` | 1.1.4, vendored by #486 (the corpus table below) |
| IUA (ITI-71, ITI-72) | the IHE ITI Technical Framework Supplement, no FHIR package | Rev 2.5 (2026-06-18, Trial Implementation), vendored by #414 (the corpus table below) |
| Netherlands Generic Functions | `fhir.nl.gf` | 0.3.0, as Annex B names it; released as a git tag with no package on the registry, so #87 vendors its source (the corpus table below) |
| Mitz closed authorization question (Annex B §B.6) | VZVZ architecture documents, no package and no licence stated | Implementatiehandleiding Open en gesloten autorisatievraag 3.8.2 (2024-05-27), pinned and not redistributed by #475 (the corpus table below) |

## Corpora and machine-readable inputs

A corpus is pinned by commit or immutable tag, or, for a FHIR package, by its
version and the sha256 of the registry tarball, or, for a document published
at a fixed URL (an RFC, a dated W3C or OpenID publication), by that URL and the
sha256 of its bytes; never by a moving tag or a `latest` URL, and vendored by a
committed `scripts/vendor/*.sh` with a `PROVENANCE.md`
(`.claude/rules/vendored-inputs.md`). A document whose licence does not
reach every reader of a public repository (the OpenID Foundation's, limited
to developing and implementing the specification) is pinned the same way,
fetched under the ignored `.vendor-cache/`, and only its `PROVENANCE.md` is
committed. Each script below reads
its pin from this table, and `scripts/checks/versions.sh` reads each vendored
`PROVENANCE.md` back and fails when it names a different commit or tag.
`scripts/checks/pin-freshness.sh` reads every corpus row of this file, in
every table below, against its upstream, or lists its vendor script with the
reason no reader is needed; its self-test fails on a row with neither.

| Item | Pin | Repeated in |
|---|---|---|
| Federation Tier with AQL specification | `syntaric/openehr-federation-spec` commit `7162d0c760d23105d62a743bf0ad1073c45fdb85` | `scripts/vendor/federation-spec.sh`, `docs/specs/federation-spec/PROVENANCE.md` |
| Federation Tier reference implementation | `syntaric/openehr-federation-ref` commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5` | `scripts/vendor/federation-ref.sh`, `docs/specs/federation-ref/PROVENANCE.md` |
| openEHR ITS-REST OpenAPI | `openEHR/specifications-ITS-REST` tag `Release-1.1.0`, all seven API modules, the Query validation document and the SMART on openEHR source (`docs/smart_app_launch/`, DEVELOPMENT status in this release) | `scripts/vendor/its-rest.sh`, `docs/specs/its-rest/PROVENANCE.md` |
| openEHR AQL specification source | `openEHR/specifications-QUERY` tag `Release-1.1.0`, the AQL and AQL examples documents and the grammar | `scripts/vendor/aql.sh`, `docs/specs/aql/PROVENANCE.md` |
| openEHR Reference Model specification source | `openEHR/specifications-RM` tag `Release-1.1.0`, the AsciiDoc sources of the RM 1.1.0 specifications and the class definitions they include | `scripts/vendor/openehr-rm.sh`, `docs/specs/openehr-rm/PROVENANCE.md` |
| openEHR BASE specification source | `openEHR/specifications-BASE` tag `Release-1.1.0`, the release paired with the RM `Release-1.1.0` (decided 2026-10-06): the AsciiDoc sources of the BASE 1.1.0 specifications and the class definitions they include | `scripts/vendor/openehr-base.sh`, `docs/specs/openehr-base/PROVENANCE.md` |
| IHE PIXm FHIR package | `ihe.iti.pixm` version `3.1.0` from `packages.fhir.org`, tarball sha256 `19e2e8eaf3030ac7b4d809c5e1eeb8face02c8635318aeb6d35bc2bb889de0d0`, the ITI-83 artefacts | `scripts/vendor/ihe-pixm.sh`, `docs/specs/ihe-pixm/PROVENANCE.md` |
| IHE PDQm FHIR package | `ihe.iti.pdqm` version `3.2.0` from `packages.fhir.org`, tarball sha256 `61e09fbee991ff7c131b6ba5474921001e07782209961f3e85cee5f3ebcaedc2`, the ITI-78 and ITI-119 artefacts | `scripts/vendor/ihe-pdqm.sh`, `docs/specs/ihe-pdqm/PROVENANCE.md` |
| IHE mCSD FHIR package | `ihe.iti.mcsd` version `4.0.0` from `packages.fhir.org`, tarball sha256 `933a143d7bb14c66731a32f52a084c6cb92476aca1b917db77a4640f8a5290ad`, the ITI-90 and ITI-91 artefacts | `scripts/vendor/ihe-mcsd.sh`, `docs/specs/ihe-mcsd/PROVENANCE.md` |
| IHE PMIR FHIR package | `ihe.iti.pmir` version `1.6.0` from `packages.fhir.org`, tarball sha256 `ec9d25fc64ac2f3087f921c14c0da56afc7e794caa80298db3f130bc0a40fe70`, the ITI-93 and ITI-94 artefacts | `scripts/vendor/ihe-pmir.sh`, `docs/specs/ihe-pmir/PROVENANCE.md` |
| IHE IUA supplement | `IHE/ITI.IUA` tag `2.5`, the Revision 2.5 Trial Implementation supplement text (ITI-71, ITI-72, ITI-102, ITI-103) and its figures | `scripts/vendor/ihe-iua.sh`, `docs/specs/ihe-iua/PROVENANCE.md` |
| IHE BALP FHIR package | `ihe.iti.balp` version `1.1.4` from `packages.fhir.org`, tarball sha256 `be46dda3088ee9d486d7163458dc5a4d7192e8a1c587fd47e2beb70e4bcefa90`, the whole package but its manifest | `scripts/vendor/ihe-balp.sh`, `docs/specs/ihe-balp/PROVENANCE.md` |
| IHE PIXm narrative pages | the IG of package `ihe.iti.pixm` 3.1.0 at `profiles.ihe.net/ITI/PIXm/3.1.0/`, pin-set digest `b3cfa964a0c8813c068d0193bc35f5e606cead23e845a0de0925f041fe9ad2a1`, the Volume 1 (1:41), ITI-83 and ITI-104 pages | `scripts/vendor/ihe-iti-pages.sh`, `docs/specs/ihe-pixm-pages/PROVENANCE.md` |
| IHE PDQm narrative pages | the IG of package `ihe.iti.pdqm` 3.2.0 at `profiles.ihe.net/ITI/PDQm/3.2.0/`, pin-set digest `a3271aa6b45fad9ceb2da373c6d4cab0eb7649e06fcb278431f967bde976e0b6`, the Volume 1 (1:38), ITI-78 and ITI-119 pages | `scripts/vendor/ihe-iti-pages.sh`, `docs/specs/ihe-pdqm-pages/PROVENANCE.md` |
| IHE PMIR narrative pages | the IG of package `ihe.iti.pmir` 1.6.0 at `profiles.ihe.net/ITI/PMIR/1.6.0/`, pin-set digest `ba4747caf6acc383df4ffb041b253272ad58ead796345fb78dc38740f58c4b09`, the Volume 1 (1:49), ITI-93 and ITI-94 pages | `scripts/vendor/ihe-iti-pages.sh`, `docs/specs/ihe-pmir-pages/PROVENANCE.md` |
| IHE mCSD narrative pages | the IG of package `ihe.iti.mcsd` 4.0.0 at `profiles.ihe.net/ITI/mCSD/4.0.0/`, pin-set digest `982bda79f08b954960230d43f546ba15321a393236da2bcf4a5cbecfddfadacf`, the Volume 1 (1:46), ITI-90 and ITI-91 pages | `scripts/vendor/ihe-iti-pages.sh`, `docs/specs/ihe-mcsd-pages/PROVENANCE.md` |
| IHE BALP narrative pages | the IG of package `ihe.iti.balp` 1.1.4 at `profiles.ihe.net/ITI/BALP/1.1.4/`, pin-set digest `f682913c6c5d5d2599c1dcb3c41a22373240b126a2051dab6b58ca8d56d1524d`, the Volume 1 (1:52) and Volume 3 (3:5.7) pages | `scripts/vendor/ihe-iti-pages.sh`, `docs/specs/ihe-balp-pages/PROVENANCE.md` |
| IHE ITI-20 Record Audit Event | `profiles.ihe.net/ITI/TF/Volume2/ITI-20.html`, Revision 20.2, page sha256 `881c7d6423fdf5ecaf4f9f50f8d25be61c3ed8ef97c87eff9591bd7fdf51570d` and its figure `media/Figure_3.20.4-1.png` sha256 `7aba1a2437e3492202460e150a6dda85b8a1886035bd2aa8daa4c892a579b734` | `scripts/vendor/ihe-atna.sh`, `docs/specs/ihe-atna/PROVENANCE.md` |
| IHE RESTful ATNA supplement | `IHE_ITI_Suppl_RESTful-ATNA.pdf` from `www.ihe.net`, Rev. 3.6, sha256 `d8451a4a0d951662b6a04b745084c33afff6196db5647f2cf79d9149dfa7265a` | `scripts/vendor/ihe-atna.sh`, `docs/specs/ihe-atna/PROVENANCE.md` |
| Netherlands Generic Functions IG source | `nuts-foundation/nl-generic-functions-ig` tag `v0.3.0`, commit `5367430787042c218996f11570f904bd3cd37a83`, the source of package `fhir.nl.gf` version `0.3.0`: the localization, consent, care services, identification and authentication pages (with the six GFI transactions) and the FSH profiles, capability statements and examples | `scripts/vendor/nl-gf.sh`, `docs/specs/nl-gf/PROVENANCE.md` |
| Nuts specifications | `nuts-foundation/nuts-specification` commit `7c0de533b8cce537812b6647542e5719c93433a8`, RFC003 (OAuth2 Authorization), RFC021 (VP Token Grant Type) and RFC022 (Discovery Service) | `scripts/vendor/nuts-rfc.sh`, `docs/specs/nuts-rfc/PROVENANCE.md` |
| IETF RFC 6749 | `https://www.rfc-editor.org/rfc/rfc6749.txt` sha256 `f204fc8661d6c92d2ec6e0b54808f961a9ad26e792f57f312d9528335519bd71`, The OAuth 2.0 Authorization Framework | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 7519 | `https://www.rfc-editor.org/rfc/rfc7519.txt` sha256 `fecd930e9ccf2276b95c0017c6c4ff5d09352e4bc3c7629946447894e0f97248`, JSON Web Token (JWT) | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 7521 | `https://www.rfc-editor.org/rfc/rfc7521.txt` sha256 `d5d97b3e691c9bbc495c277cc2cd79316b82486991468fc346a96ab59ba4b3c8`, Assertion Framework for OAuth 2.0 Client Authentication and Authorization Grants | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 7523 | `https://www.rfc-editor.org/rfc/rfc7523.txt` sha256 `ae24f77a8fc4338903c805c6ace38def1f23d40194aea87b123b13c5b3d2d915`, JWT Profile for OAuth 2.0 Client Authentication and Authorization Grants | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 7662 | `https://www.rfc-editor.org/rfc/rfc7662.txt` sha256 `2b7d688cb849f093e860557ac97e6cddac2556d69a4386561b22cdf97bf13657`, OAuth 2.0 Token Introspection | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 8414 | `https://www.rfc-editor.org/rfc/rfc8414.txt` sha256 `16c816e4e0fdbffb7e910ff3017867bf39debe9cb7f52f5cbc508a052ed660e8`, OAuth 2.0 Authorization Server Metadata | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 8705 | `https://www.rfc-editor.org/rfc/rfc8705.txt` sha256 `6e45a1ee94c6a6a177a4cb077fe67d74bd70c1218c9362f2db2f153239612414`, OAuth 2.0 Mutual-TLS Client Authentication and Certificate-Bound Access Tokens | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9126 | `https://www.rfc-editor.org/rfc/rfc9126.txt` sha256 `a79d0e30fcc24a22b79c8e18aa82362f6e63a7b8a5d58b480e746360e97388db`, OAuth 2.0 Pushed Authorization Requests | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9396 | `https://www.rfc-editor.org/rfc/rfc9396.txt` sha256 `d6a8f032d8a585daae1c33a8c7b6e539d199f886ec8cc1c7898436f7f2eed29c`, OAuth 2.0 Rich Authorization Requests | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9449 | `https://www.rfc-editor.org/rfc/rfc9449.txt` sha256 `3842c58e1f6043389416023b9bb8d765048266024982fbbd90640e05943f4e13`, OAuth 2.0 Demonstrating Proof of Possession (DPoP) | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| W3C Verifiable Credentials Data Model 1.1 | `https://www.w3.org/TR/2022/REC-vc-data-model-20220303/` sha256 `d7c796e1f7e9a233d9037eec03e9d9fe7d69220a71639b07f5038684949a3091`, W3C Recommendation 2022-03-03 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| W3C Decentralized Identifiers 1.0 | `https://www.w3.org/TR/2022/REC-did-core-20220719/` sha256 `5e44345740d9bfaa852d3b66c57e98c9beb6c5bf6083b0126dd5daac377b9993`, W3C Recommendation 2022-07-19 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| W3C DID Resolution 1.0 | `https://www.w3.org/TR/2026/CRD-did-resolution-1.0-20261001/` sha256 `2b194843d37f5b7f01a4f5464f64d672dc4be43dafe5f2f49114c6070819586e`, W3C Candidate Recommendation Draft 2026-10-01 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| W3C Bitstring Status List 1.0 | `https://www.w3.org/TR/2025/REC-vc-bitstring-status-list-20250515/` sha256 `3cdf3358a09f3b02f2a97e5dac13cf2b63115c2db1260bf7b7c236d8702924ec`, W3C Recommendation 2025-05-15 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| did:web Method Specification | `w3c-ccg/did-method-web` commit `ea423c114e6f2537498ee6f94e8d794c64f60c18`, the Credentials Community Group report and its licence | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| DIF Presentation Exchange 2.0.0 | `decentralized-identity/presentation-exchange` commit `7cbe949c95fe1e19413b24c9b49dc34f76f5d76a`, the v2.0.0 specification text and JSON Schemas | `scripts/vendor/dif-pe.sh`, `docs/specs/dif-pe/PROVENANCE.md` |
| DIF Claim Format Registry | `decentralized-identity/claim-format-registry` commit `4a15817a7717efdda29912a1eef6e59135d8d02a`, the claim format designation schemas Presentation Exchange 2.0.0 references | `scripts/vendor/dif-pe.sh`, `docs/specs/dif-pe/PROVENANCE.md` |
| OpenID FAPI 2.0 Security Profile | `https://openid.net/specs/fapi-security-profile-2_0-final.html` sha256 `26a49ad19b1f2b19ecc1cd9b825b4d5012f5e03a5dc8cb0dbd26462d39da465c`, Final; pinned and fetched to `.vendor-cache/openid/`, never committed (its licence is limited to developing and implementing the specification) | `scripts/vendor/openid.sh`, `docs/specs/openid/PROVENANCE.md` |
| OpenID for Verifiable Credential Issuance 1.0 | `https://openid.net/specs/openid-4-verifiable-credential-issuance-1_0-final.html` sha256 `f123c3178cacd27688b15b762098a045e9eb35eccfe2f5f18a357c3815e06ba7`, Final; pinned and fetched as the row above | `scripts/vendor/openid.sh`, `docs/specs/openid/PROVENANCE.md` |
| OpenID for Verifiable Presentations draft 18 | `https://openid.net/specs/openid-4-verifiable-presentations-1_0-18.html` sha256 `48d539e12e6b75235d7b673b0ee1b3a6589f6013c665bbf00f3f2933d0ec0dcd`, the draft Nuts RFC021 cites; pinned and fetched as the rows above | `scripts/vendor/openid.sh`, `docs/specs/openid/PROVENANCE.md` |
| Mitz closed authorization question | pin-set digest `1f0196ab826e5093f6f7b8cbb935b106c22e19b6f9b246d4cce8c157602a14cd` over the VZVZ Confluence page `828314367` (space `MA11`, "Bijlage Architectuurdocumenten"), attachment version `1` of each document: `VZVZ_Mitz_Implementatiehandleiding_OpenGesloten_v3.8.2.pdf` sha256 `a5ce8f0d7eba8969a395a8560cf69f9e359f9da4c76145adc0748b9907ac0cf3`, `VZVZ_Mitz_PvE_AMC_Aansluiting_Mitz-connector_v3.8.1.ad1.pdf` sha256 `b1f18b48969475ce969472299179b37663cdcf67b067e9275532e0fa35bb59cb`, `VZVZ_Mitz_Implementatiehandleiding_Berichtauthenticatie_v3.8.1.ad1.pdf` sha256 `9659bdcd20a4deebc699357a455aaca7b3643c78a53b078329edcb899ef02f01`; no licence is stated, so the documents are pinned and fetched into the git-ignored `.vendor-cache/mitz/`, never committed | `scripts/vendor/mitz.sh`, `docs/specs/mitz/PROVENANCE.md` |

### Country research corpora (#488)

The national sources of the country research on #488 have no git commit to
pin: statutes, specification pages, PDFs, FHIR packages and Confluence pages.
Each artefact is pinned in its country script by the URL it is fetched from
and the sha256 of its bytes (`scripts/vendor/lib/pinned.sh`), and each row
below carries the pin-set digest, the sha256 over the sorted
`mode  file  url  sha256` lines of that corpus's pins. The script fails when
its pins and this digest disagree, and when an upstream hash moves.
`PINNED_DIGESTS_ONLY=1 scripts/vendor/<country>.sh` prints the digests for a
re-pin. An artefact whose licence does not allow redistribution, or is
unclear, is fetched into the git-ignored `.vendor-cache/` and only its
provenance is committed. Live pages (the NSPOP, Inera and NHN pages) move
with every edit, so their scripts fail until the pins are renewed. The
Commission's reuse terms are pinned as Decision 2011/833/EU at its Cellar
manifestation, which does not move, and the legal notice page, whose bytes
change with every render, is cited and not pinned.

| Item | Pin | Repeated in |
|---|---|---|
| German ePA für alle (gematik) | ePA specification pages and the ePA-Basic OpenAPI documents, pin-set digest `ec39f57443d348a3e61330a380644c13c5ba5804e11f2a416ba0429ba031114a` | `scripts/vendor/de.sh`, `docs/specs/de-gematik-epa/PROVENANCE.md` |
| German VZD FHIR-Directory (gematik) | the VZD specification page, package `de.gematik.fhir.directory` 1.3.0 and its source licence, pin-set digest `099fb9c34136d4c7de5b9b95f07e6efda0c9b667cb19e17075d431e7b65fb562` | `scripts/vendor/de.sh`, `docs/specs/de-gematik-vzd/PROVENANCE.md` |
| German ZETA (gematik) | gemSpec_ZETA 1.3.2, pin-set digest `ff1ccf6ae282fe84c0127141f423dbd7ae434337d4f27bc6fafc64a830c39c6a` | `scripts/vendor/de.sh`, `docs/specs/de-gematik-zeta/PROVENANCE.md` |
| German base profiles (HL7 Deutschland) | package `de.basisprofil.r4` 1.6.0, pin-set digest `9f7dfc9072bfec1942a16b073691433575c6312cb980d576c6f7ccb41fc2d0d6` | `scripts/vendor/de.sh`, `docs/specs/de-hl7-basisprofil/PROVENANCE.md` |
| German ISiK (gematik) | package `de.gematik.isik` 6.0.0, pin-set digest `6331afd5ea3c218941d399fcb8b11fd3541c54f678953eecaf59dd6febb509f0` | `scripts/vendor/de.sh`, `docs/specs/de-gematik-isik/PROVENANCE.md` |
| German MII consent module | package `de.medizininformatikinitiative.kerndatensatz.consent` 2026.0.0, pin-set digest `b3b8f3c69d0f3c2441e7fa5627b074d2281739346e01be5f4f800dd090eacb0b` | `scripts/vendor/de.sh`, `docs/specs/de-mii-consent/PROVENANCE.md` |
| German SGB V | §§290, 339 and 342 SGB V, pin-set digest `4012d33a8ab6a4555f954e831517e0f0e2108068bf8cfab539bb2b8c39215e4f` | `scripts/vendor/de.sh`, `docs/specs/de-sgb5/PROVENANCE.md` |
| Austrian GTelG 2012 | the consolidated GTelG 2012 of 2026-10-04, pin-set digest `92c3913a8772e92243c3ee54dd192750b97e8e918a89fe6a4b1c2e8085b0f786` | `scripts/vendor/at.sh`, `docs/specs/at-gtelg/PROVENANCE.md` |
| Austrian ELGA Berechtigungssystem | six BeS v5.5 developer pages, pin-set digest `5dff95e1dd642758ced2ab24e964c1899fcde35e4e32444a74e4c2e8006d3f63` | `scripts/vendor/at.sh`, `docs/specs/at-elga-bes/PROVENANCE.md` |
| Austrian ELGA overview | the ELGA technical overview and the Digital Health Standards Catalogue Austria 2026, pin-set digest `08326503145045a59936fd3b2780d9a570de4fc087676603f044e54572db4374` | `scripts/vendor/at.sh`, `docs/specs/at-elga/PROVENANCE.md` |
| Austrian core profiles (HL7 Austria) | package `hl7.at.fhir.core.r4` 2.0.0, pin-set digest `0668e925bc8fb66f24fce08690f220c023e211cf882d2d0756f0447260cb3929` | `scripts/vendor/at.sh`, `docs/specs/at-hl7-core/PROVENANCE.md` |
| Swiss EPR legislation (Fedlex) | EPDG, EPDV, EPDV-EDI with its annexes, and the EGDG draft, pin-set digest `384f0bbb5b958da909b54b835faf1535b2d0a141a05bc4a92d324914ff943910` | `scripts/vendor/ch.sh`, `docs/specs/ch-fedlex-epr/PROVENANCE.md` |
| Swiss CH EPR FHIR package | package `ch.fhir.ig.ch-epr-fhir` 5.0.0, pin-set digest `75f8cce821998e823bd9a9fecac8848ca661d862d2576b1488e6399a9309eed5` | `scripts/vendor/ch.sh`, `docs/specs/ch-epr-fhir/PROVENANCE.md` |
| Swiss EPR central services interface pack | `Central-Services_20260601_PROD.zip`, pin-set digest `6f6eb0c51aa0843a4332e915b6f9e0020ab36d6d6dfa245eb3ff9de9a1020ed0` | `scripts/vendor/ch.sh`, `docs/specs/ch-ehs-central-services/PROVENANCE.md` |
| IHE PIXm FHIR package, Swiss pin | package `ihe.iti.pixm` 3.0.4, pin-set digest `68b36cfa85cc04551e30b3628ce16218c1b980495c06bb835f34b0ccc706063a` | `scripts/vendor/ch.sh`, `docs/specs/ihe-pixm-ch/PROVENANCE.md` |
| IHE PDQm FHIR package, Swiss pin | package `ihe.iti.pdqm` 3.1.0, pin-set digest `ae4fb56c9eba92fcdf5637c86135618e37b60642c99013a8ca06ba747c9c4e73` | `scripts/vendor/ch.sh`, `docs/specs/ihe-pdqm-ch/PROVENANCE.md` |
| IHE IUA supplement, Swiss pin | `IHE/ITI.IUA` Revision 2.3, pin-set digest `2a5f13a87ccf307fecda8c48e77a61fd09c440089b2172be0edea912dea85439` | `scripts/vendor/ch.sh`, `docs/specs/ihe-iua-ch/PROVENANCE.md` |
| EU EHDS Regulation and eHealth Network guidelines | Regulation (EU) 2025/327, Implementing Regulations (EU) 2026/2083 and 2026/2099, Recommendation (EU) 2019/243, two eHealth Network guidelines and Commission Decision 2011/833/EU on the reuse of Commission documents, pin-set digest `4af818fc47ead2a2a5120225d68a338ba068bed71c46b4e65b064491ceddf148` | `scripts/vendor/eu.sh`, `docs/specs/eu-ehds/PROVENANCE.md` |
| MyHealth@EU NCPeH API and OpenNCP | package `myhealth.eu.fhir.ncp-api` 9.1.0, two guide pages and OpenNCP v10.1.0, pin-set digest `491dc60ee8b1bf8510758e0d4a62a8578c728f27c8b519a56ba40548f72c0c56` | `scripts/vendor/eu.sh`, `docs/specs/ehdsi/PROVENANCE.md` |
| IHE ITI Technical Framework Volume 1 pages | ITI TF Revision 20.2 chapters 13, 18 and 27, pin-set digest `838b2f672e0bc34841d7fe297fd561c6f49c42fdd12b5a15119eb10a7234aeb7` | `scripts/vendor/ihe-iti-tf.sh`, `docs/specs/ihe-iti-tf/PROVENANCE.md` |
| Belgian eHealth platform documents | ten cookbooks, two Swagger documents and the re-use conditions, pin-set digest `62377196eaf498ce49beca04948308cc78d02a718b5ecd1471514c1567b2c025` | `scripts/vendor/be.sh`, `docs/specs/be-ehealth/PROVENANCE.md` |
| Belgian core profiles (HL7 Belgium) | package `hl7.fhir.be.core` 2.2.0, pin-set digest `ad5ae8d7d42c01757df5c7ed3a879fd69151e19a90b17636ad1c6fba8f6b3ca7` | `scripts/vendor/be.sh`, `docs/specs/be-fhir/PROVENANCE.md` |
| French ANS publications | FR Core, the Annuaire Santé, Pro Santé Connectée transport security, PDSm and PDSm for DMP, pin-set digest `26094f61487a2a7e02d324a0ae7890f38f09bc4be4b7bfb90a266e6489c40eb0` | `scripts/vendor/fr.sh`, `docs/specs/fr-ans/PROVENANCE.md` |
| Danish NSP documentation (NSPOP) | five NSPOP Confluence pages, pin-set digest `bfcb5dec956a2ce7a255d38a7667efa01ceb884c00fcacf273260e1deceb4fdc` | `scripts/vendor/dk.sh`, `docs/specs/dk-nsp/PROVENANCE.md` |
| Swedish RIV-TA and Inera documentation | four RIV-TA service contracts, Basic Profile 2.1 and the engagement index FAQ, pin-set digest `32e7de7b0fced3297889794a12b7a5f2026199254935135b95339d4f2e286cf8` | `scripts/vendor/se.sh`, `docs/specs/se-inera/PROVENANCE.md` |
| Norwegian NHN developer portal | ten Pasientens journaldokumenter, HelseID and document-sharing pages, pin-set digest `3eef682e5633ba560c8cbe2caee315501204cba9935a9432fdb2a4308e64364b` | `scripts/vendor/no.sh`, `docs/specs/no-nhn/PROVENANCE.md` |
| Finnish Kanta documents and packages | three Kanta documents and two Kanta FHIR packages, pin-set digest `8709b4a22c5b8ce006490664a80c681dded2bd587811bde2afc77ddd1c6cb50e` | `scripts/vendor/fi.sh`, `docs/specs/fi-kanta/PROVENANCE.md` |
| Finnish base profiles (HL7 Finland) | package `hl7.fhir.fi.base` 2.0.0, pin-set digest `b8a3c0782939e16f1dcfbf2b4be037abcccdb26be448fca4c818efb79f70eef9` | `scripts/vendor/fi.sh`, `docs/specs/fi-hl7/PROVENANCE.md` |

### Exchange-format proxy packages (#683)

The published proxies for the European electronic health record exchange
format of Regulation (EU) 2025/327 Article 15(1), until its implementing act
is adopted: the Xt-EHR EHDS Logical Information Models and the HL7 Europe
FHIR guides, with the HL7 Europe Extensions, the International Patient
Summary and the IHE Pharmacy Medication Prescription and Dispense profile at
the versions those guides depend on. Each is the registry tarball
of `packages.fhir.org`, pinned in `scripts/vendor/eehrxf.sh` by URL and
sha256 through `scripts/vendor/lib/pinned.sh` and committed whole, and each
row carries the pin-set digest first and the tarball sha256 after it.
`scripts/checks/pin-freshness.sh` reads each package's registry entry.

| Item | Pin | Repeated in |
|---|---|---|
| Xt-EHR EHDS Logical Information Models | package `xtehr.eu.ehds.models` 1.0.0, pin-set digest `796cc0974133058ba0b34d9b678baf272eb89b1ceeddc6930e6d66936a38c953`, tarball sha256 `a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-xtehr-models/PROVENANCE.md` |
| HL7 Europe Base and Core | package `hl7.fhir.eu.base` 2.0.1, pin-set digest `b84f7ba0799903e6bf4d2bf0ecaeeaefd0f71510238c7e30d63244ff6c603c67`, tarball sha256 `3fb23b64d70656ea809e1ad66d84d5d008400e18424266debf176cb46fba0b4a` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-base/PROVENANCE.md` |
| HL7 Europe Patient Summary | package `hl7.fhir.eu.eps` 1.0.0-ballot, pin-set digest `ba59282865c06a3819a25d92b4110f5d6c84b2df2b0f4e0ae34092a66659f038`, tarball sha256 `e0fff1fb20d3daf75609259faa6b860131ebf4fc8890bb68f0da882307afc33c` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-eps/PROVENANCE.md` |
| HL7 Europe Medication Prescription and Dispense | package `hl7.fhir.eu.mpd` 1.0.0, pin-set digest `4c4ea5572812dbacfc87573af4165742284a0825b0a200fa606df07393ea9758`, tarball sha256 `f1bc1084efc93e16ae4b45c1bb71340385d314a72031949c4a7e0d97aea5fd6b` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-mpd/PROVENANCE.md` |
| HL7 Europe Laboratory Report | package `hl7.fhir.eu.laboratory` 2.0.0, pin-set digest `716111c5e8d1a60490dd0ac34f5e98534fd83cce8055218e05f7d675778af682`, tarball sha256 `097aa45c6efcdbc3f25ead8b7d448fee5011b5e7ecf264aa400c5e2c69c5c488` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-laboratory/PROVENANCE.md` |
| HL7 Europe Extensions | package `hl7.fhir.eu.extensions.r4` 1.3.1 and 1.3.0, pin-set digest `6a5701133f97d68913f2b007f01a9b654b339cff1f8340493b5f2b615b6be24d`, tarball sha256 `37f3ee7ae7a2312e71a4e855e6c36f45d2bc3b13f995bcf4a9fb499ca014cff6` (1.3.1, which `hl7.fhir.eu.base` 2.0.1 depends on) and `8510e930856961d550c04d43f243ef0bdcb27089431fe183ab11c7f805f0c2f1` (1.3.0, which `hl7.fhir.eu.eps` 1.0.0-ballot and `hl7.fhir.eu.laboratory` 2.0.0 depend on) | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-extensions/PROVENANCE.md` |
| HL7 International Patient Summary | package `hl7.fhir.uv.ips` 2.0.0, pin-set digest `b26ce0cf1012b4a97f3678efa9ece6198b4b353f85ec0090a1de057643b31c5a`, tarball sha256 `b3964eba08ee699bc121b905c4290641e54dd34f2cf3b5cd3edeb08a40a66979`, the version `hl7.fhir.eu.eps` 1.0.0-ballot depends on | `scripts/vendor/eehrxf.sh`, `docs/specs/hl7-ips/PROVENANCE.md` |
| IHE Pharmacy Medication Prescription and Dispense | package `ihe.pharm.mpd.r4` 1.0.0-comment-2, pin-set digest `12effdefce443cb7b61ff9b7f39a4431b54d9267becdeef7d3505770ddc8398e`, tarball sha256 `05eda0d1871d2980cb13023d18a5a9618c9945c3a85cf721c14c5e989e0d3776`, the version `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.eps` 1.0.0-ballot and `hl7.fhir.eu.mpd` 1.0.0 depend on | `scripts/vendor/eehrxf.sh`, `docs/specs/ihe-pharm-mpd/PROVENANCE.md` |

## openEHR model crates (crates.io)

The openEHR surface comes from the published `openehr-*` crates, consumed by
version like any other dependency (`docs/architecture.md` §2). FerroEHR
releases them as one lockstep family, so the five rows below are one group:
they move together, and `scripts/checks/versions.sh` fails when one member
moves alone, here or in the root `Cargo.toml` `[workspace.dependencies]`. The
pin is the latest version on crates.io, 0.0.84 since 2026-10-05.
`openehr-sdt` (the SMART on openEHR scope grammar) joined the group at the
family pin with its default features off, so only the grammar is compiled:
the onward grant (#81) writes and checks the scope it requests with it, and client
authentication (#80) reads every caller's scopes with it.

| Item | Pin | Repeated in |
|---|---|---|
| `openehr-query` | 0.0.84 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-its` | 0.0.84 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-base` | 0.0.84 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-rm` | 0.0.84 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-sdt` | 0.0.84 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |

**0.0.74 is the lockstep release of the whole `openehr-*` family that carries
the federation gaps FerroFED raised, FerroEHR #3505 to #3514 (the AST visitor,
spans, parameter binding, the `FROM ENDPOINT` directive, the parser fix, the
router builder, the operation matcher with `forward`, the credentials provider
and per-call options; `docs/architecture.md` §2), published on 2026-10-01.
0.0.75 adds the crates' `repository` field. 0.0.76, published on 2026-10-02,
carries FerroEHR #3526: every generated ITS-REST type whose schema is open
keeps the extra members in an `additional_properties` map, so the open `Error`
carries the gateway's `code` and `request_id` (#57). 0.0.77, published on
2026-10-02, carries FerroEHR #3529, which classifies every AQL function call
as a built-in function of AQL or another name, and FerroEHR #3531, which
builds every `ReqwestTransport` client with redirects switched off (#195).
0.0.78, published on 2026-10-02, carries FerroEHR #3535: the
`Authorization` value of a `Credentials` is public, checked against RFC 7617
§2 for basic credentials and the `b64token` of RFC 6750 §2.1 for a bearer
token, so configuration load checks exactly what the node client sends
(#237). 0.0.79, published on 2026-10-02, carries FerroEHR #3537: the
`openehr-rm` attribute model holds the BASE primitives, the `Ordered` marker
and the reference targets of `OBJECT_REF` attributes, with the
`is_primitive` and `conforms_to_ordered` lookups the rewrite uses to decide
which paths a node can order under `DISTINCT` (#234). 0.0.80, published on
2026-10-03, carries FerroEHR #3539 (the openEHR identifier class of each path
parameter, #291), #3540 (a public decoder from a request to each operation's
params, #287, #292), #3541 (a reader for a Simplified Formats CONTRIBUTION,
#297) and #3543 (every operation's request-body media types, #298). FerroEHR
#3542, the canonical XML CONTRIBUTION reader, stays open upstream (#308).
0.0.81, published on 2026-10-03, carries FerroEHR #3548 (the `201_EHR` body of
`ehr_create` typed), #3551 (the request-body media-type picker made public)
and #3552 (`ROUTE_REQUEST_MEDIA` agreeing with each operation's `Content-Type`
parameter); #329 drops the three workarounds they replace.
0.0.82, published on 2026-10-04, carries FerroEHR #3557 and #3558. The first
is a `Display` for `openehr_sdt::smart_scopes::SmartScope` and its parts that
prints the canonical form of the SMART on openEHR grammar, with
`SmartScope::format_all` as the printer that matches `parse_all`. The onward
grant (#81) and the token exchange's covering scope (#439) now send that
form (#456). The second is a DPoP credential on the generated client
(RFC 9449): `Credentials::Dpop` writes the `DPoP` scheme, and
`Client::with_dpop_prover` takes a `DpopProver` that signs each request's
proof and answers a `use_dpop_nonce` challenge with one re-send. The node
clients prove their requests through it, so the `Transport` decorator that
rewrote the scheme is gone (#448).
0.0.83, published on 2026-10-05, carries FerroEHR #3560: the client's
`ClientError::DeadlineElapsed` gains a `sent` field, true when an earlier
send of the call went out, such as the first send of a request a node
answered with a `use_dpop_nonce` challenge. The node clients read a deadline
before the nonce re-send as the node's time-out from that field, where they
inferred it from what the call's prover saw (#470, #566). On that pin the
prover's record still read a `DpopProof` failure of the re-send, which
carried no such field.
0.0.84, published on 2026-10-05, carries FerroEHR #3565: `ClientError::DpopProof`
gains the same `sent` field, true when the proof for a retry or for the nonce
re-send failed after an earlier send went out. The node clients read both
errors from the client, and the per-call prover record is gone (#574).

## FHIR model crate (crates.io)

The FHIR model of the IHE bindings comes from the published `fhir-types`
crate (`docs/architecture.md` §6, decision A16), compiled only in
`crates/ihe-iti` with its `pixm` and `pdqm` features, so the gateway core
never builds it. `scripts/checks/versions.sh` fails when this row and the root
`Cargo.toml` `[workspace.dependencies]` disagree.

| Item | Pin | Repeated in |
|---|---|---|
| `fhir-types` | 0.1.107 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |

The `pixm` feature takes `r4` and `terminology`: that root set holds every
type ITI-83 reads (`Parameters`, `OperationOutcome`, `Identifier`, `Reference`,
`Bundle`). The `pdqm` feature takes `r4` and `resources`, every R4 resource,
because ITI-78 answers with `Patient` resources, which only `resources`
carries (#119); the mCSD directory (#86) reads `Organization` and `Endpoint`
from the same set.

## Metrics crates (crates.io)

The metrics surface (#281) is one OpenTelemetry `MeterProvider` read by the
Prometheus pull reader and the optional OTLP push, the stack FerroEHR runs.
The five `opentelemetry` crates are released in lockstep, so their rows are
one group: they move together, and `scripts/checks/versions.sh` fails when
one member moves alone, here or in the root `Cargo.toml`
`[workspace.dependencies]`. `prometheus` is the registry and text encoder
the pull reader renders through, on its own release line, checked against
the root `Cargo.toml` alone. The trace export (#353) bridges the gateway's
`tracing` spans to the same OpenTelemetry stack through
`tracing-opentelemetry`, whose release line is paired with the
`opentelemetry` one (0.34 with 0.33) and is checked against the root
`Cargo.toml` alone too. The testkit's in-process OTLP collector (#437)
answers the OTLP trace service of `opentelemetry-proto`, a member of the
group, on a `tonic` gRPC server, whose release line is the one
`opentelemetry-otlp` sends with, checked against the root `Cargo.toml`
alone; both are test-only. Every version was the latest on crates.io on
2026-10-03, and `tracing-opentelemetry`, `opentelemetry-proto` and `tonic`
on 2026-10-04.

| Item | Pin | Repeated in |
|---|---|---|
| `opentelemetry` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry_sdk` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry-prometheus` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry-otlp` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry-proto` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `prometheus` | 0.14.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tracing-opentelemetry` | 0.34.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tonic` | 0.14.6 | the root `Cargo.toml` `[workspace.dependencies]` |

## Operator console crates and build tools

The operator console, `app/ferrofed-viewer` (#275), is a Leptos app rendered
on the server and hydrated in the browser, the shape of FerroEHR's viewer.
Each Leptos crate is on a release line of its own and is held to the root
`Cargo.toml` alone. cargo-leptos builds the site bundle, and it runs the
wasm-bindgen CLI it finds on `PATH`, which refuses a bundle the
`wasm-bindgen` crate of another version wrote, so the CLI row is also held to
the `wasm-bindgen` requirement of the root `Cargo.toml` and the version
`Cargo.lock` locks. Both tools are fetched by `taiki-e/install-action`, which
verifies the upstream release checksum, at the release whose manifest names
these versions. Every version was the latest on crates.io on 2026-10-05.
`scripts/checks/versions.sh` holds every row.

| Item | Pin | Repeated in |
|---|---|---|
| `leptos` | 0.8.21 | the root `Cargo.toml` `[workspace.dependencies]` |
| `leptos_axum` | 0.8.10 | the root `Cargo.toml` `[workspace.dependencies]` |
| `leptos_meta` | 0.8.7 | the root `Cargo.toml` `[workspace.dependencies]` |
| `leptos_router` | 0.8.16 | the root `Cargo.toml` `[workspace.dependencies]` |
| `cargo-leptos` | 0.3.11 | `.github/workflows/ci.yml`, `.github/workflows/release-viewer.yml` |
| `wasm-bindgen` | 0.2.129 | the CLI in `.github/workflows/ci.yml` and `.github/workflows/release-viewer.yml`, the crate in the root `Cargo.toml` `[workspace.dependencies]` and `Cargo.lock` |

The browser journeys (#608) drive the console in headless Chrome over
WebDriver, through chromedriver, with the `thirtyfour` client, a test-only
dependency of the testkit. Chrome and chromedriver come as one Chrome for
Testing release, which `browser-actions/setup-chrome` installs in the
`journeys (browser)` job of `ci.yml`, so the browser and its driver always
match. `thirtyfour` was the latest on crates.io on 2026-10-05, and the Chrome
for Testing release was the newest stable one. `scripts/checks/versions.sh`
holds both rows, and the weekly `scripts/checks/pin-freshness.sh` reports a
newer stable Chrome for Testing release.

| Item | Pin | Repeated in |
|---|---|---|
| `thirtyfour` | 0.37.5 | the root `Cargo.toml` `[workspace.dependencies]` |
| Chrome for Testing (Chrome and chromedriver) | 154.0.8037.92 | the `chrome-version` of `.github/workflows/ci.yml` |

## Language and runtime

`rust-toolchain.toml` carries the toolchain, and the root `Cargo.toml` carries
the edition, the resolver and the MSRV. The release lane builds every
published binary on this toolchain, with no cache.

| Item | Pin | Repeated in |
|---|---|---|
| Rust toolchain | 1.98.1 | `rust-toolchain.toml` `channel` (stable) |
| Edition | 2024 | root `Cargo.toml` `[workspace.package]` `edition` |
| Cargo resolver | 3 | root `Cargo.toml` `[workspace]` `resolver` |
| MSRV | 1.98 | root `Cargo.toml` `[workspace.package]` `rust-version` |

The deliverable is a server binary, so the MSRV tracks the pinned stable
toolchain.

## Databases

A single gateway needs no database (`docs/architecture.md` §8). PostgreSQL is
used only as the optional backend of the stored-query store when several
gateway replicas run. Every PostgreSQL FerroFED itself tests against or
documents is the latest release line. A member node in the test harness runs
its product's documented database image, which is part of the product under
test (§13): FerroEHR's, and for the node profile's second product EHRbase's,
built on PostgreSQL 16.2 (#549). The stored-query store's end-to-end tests run
on FerroEHR's image, built on `postgres:18.6`, one database per use (#268).
The one other PostgreSQL image pinned is the database of the harness PIX
Manager, SanteMPI, below (#622).

| Item | Pin | Repeated in |
|---|---|---|
| PostgreSQL | 18 | the stored-query `postgres` backend's tests, on the FerroEHR node database image below; the configuration page of the book |

## Container images

The gateway image builds on distroless static, and the quickstart runs four
member CDRs beside it, four FerroEHR instances on the same pin, each with its
own `system_id` and its own database on one FerroEHR PostgreSQL container
(`docs/architecture.md` §13, decisions A44 and A47). Every
image is pinned by tag and by the digest of its image index, resolved on
2026-10-01. The two FerroEHR images were resolved on 2026-10-05 from FerroEHR
v4.3.3, which publishes them under `ghcr.io/ferrohealth` and makes AQL
honour `EHR_ACCESS` on every query form (FerroEHR #3562, #566).
`scripts/checks/versions.sh` holds every row equal to the file that repeats
it.

| Item | Pin | Repeated in |
|---|---|---|
| Container base image | `gcr.io/distroless/static-debian13:nonroot@sha256:e2e927ec666bae08560abb3c55d0659eceabb657f56b6782ab500a9fc7f555e3` | `docker/Dockerfile` `FROM`, `docker/viewer/Dockerfile` `FROM` |
| FerroEHR node image | `ghcr.io/ferrohealth/ferroehr:4.3.3@sha256:1a5580b510dca1e49418e4c83431d19b92656d06ea0961d28d7df03d518b941f` | `compose.yaml`, the `FERROEHR` constant in `tools/ferrofed-testkit/src/containers.rs` |
| FerroEHR node database image | `ghcr.io/ferrohealth/ferroehr-postgres:4.3.3@sha256:b84808bf7321390491c5ba2e74676a8a36657fb9d00b1645006818ccb9a2beaa` | `compose.yaml`, the `FERROEHR_POSTGRES` constant in `tools/ferrofed-testkit/src/containers.rs` |
| EHRbase node image | `ehrbase/ehrbase:2.36.0@sha256:c8e642264b73637e0576ec01b5c73f5dc9be6f34eb3644f0ced890c5f916640a` | the `EHRBASE` constant in `tools/ferrofed-testkit/src/containers.rs` |
| EHRbase node database image | `ehrbase/ehrbase-v2-postgres:16.2@sha256:abe14e8f9ba33cabc9946c6c17c5aa95b64b35387f266cd20a894149203196d7` | the `EHRBASE_POSTGRES` constant in `tools/ferrofed-testkit/src/containers.rs` |
| SanteMPI PIX Manager image | `santesuite/santedb-mpi:2.5.12@sha256:608484de046a932ec2f92e9991a32507fc8ec89d53d7639cbab886a63dbf6207` | the `SANTEMPI` constant in `tools/ferrofed-testkit/src/containers.rs` |
| SanteMPI database image | `postgres:15.19@sha256:724292da1f2e50bdccfc3302ce75bbba7f4a6076701b588cc795fcac65683550` | the `SANTEMPI_POSTGRES` constant in `tools/ferrofed-testkit/src/containers.rs` |
| Reference implementation build image | `maven:3.9.16-eclipse-temurin-21@sha256:99e61abcff91a9b1333463bd8451fb18495d6eba9250ac66a338b518f8278320` | the `MAVEN` constant in `tools/ferrofed-testkit/src/containers.rs` |
| Reference implementation runtime image | `eclipse-temurin:21.0.12.1_1-jre-noble@sha256:000fd431958bc81a24abe1e8e5f0f0fd3ae365a594bd50aadb20696805f9408c` | the `TEMURIN_JRE` constant in `tools/ferrofed-testkit/src/containers.rs` |
| nginx reverse proxy image | `nginx:1.30.5-alpine@sha256:0985e772fb9f729e6fa0980da05fca5d9c468e870eed43071545afa9d2e27d94` | read from this row by `scripts/checks/production-guide.sh`, which runs `deploy/nginx/ferrofed.conf` in it |

The quickstart's gateway image, `ghcr.io/ferrohealth/ferrofed`, carries the
product version below as its tag default, and the guard holds the two equal.

The end-to-end lane starts the same two node images through the testkit
harness, behind the `FERROFED_E2E` gate (`docs/ci-cd.md`): each is a
`PinnedImage` constant in `tools/ferrofed-testkit/src/containers.rs`, and the
guard holds every constant equal to its row here, so the quickstart and the
test suite always run the same nodes. The weekly
`scripts/checks/pin-freshness.sh` reports a newer stable tag of any
`PinnedImage` constant.

The node profile also runs against a second CDR product, EHRbase (#549), so a
finding can be told apart as the product's or the check's. Its rows pin the
current stable release, 2.36.0, and the database image EHRbase's own compose
file documents beside it, each resolved on 2026-10-05 from the registry by
the digest of its image index. EHRbase is published under the Apache License
2.0, and both images are public and need no credential to pull. No specification governs which products the
harness runs: our own design.

The identity binding also runs against a deployable PIX Manager, SanteMPI
(#622), which answers PIXm ITI-83 and takes the PMIR ITI-93 feed. Its rows
pin the newest stable image SanteSuite publishes, 2.5.12 of 2023-07-04,
which is built for `linux/amd64` alone and is a single manifest rather than
an image index, so the digest is the manifest's. SanteSuite's compose file
names the official `postgres` image untagged; the row pins the newest patch
of 15, the major release current when 2.5.12 was published. Both were
resolved on 2026-10-05 from Docker Hub. SanteDB and SanteMPI are published
under the Apache License 2.0, and both images are public.

The production guide ships an nginx configuration, `deploy/nginx/ferrofed.conf`
(#624), and `scripts/checks/production-guide.sh` runs it in front of the
gateway in the image above: nginx 1.30.5 on Alpine, the release the
`stable-alpine` tag named when it was resolved, on 2026-10-06, from Docker
Hub by the digest of its image index. The guard reads the image from this
row, so the row is its one pin. nginx is published under the 2-clause BSD
licence, and the image is public.

The differential run (#94) builds the Federation Tier reference
implementation from the vendored source at its pinned commit, with the Maven
manifest fetched from that commit and held to the sha256 its `PROVENANCE.md`
records, in the build image and on the runtime image above. The testkit's
`REFERENCE_COMMIT` and `POM_SHA256` constants
(`tools/ferrofed-testkit/src/reference.rs`) repeat the reference
implementation row and the provenance, and the guard holds them equal.

## Product and citation version

The product version is the workspace `version` in the root `Cargo.toml`, which
every member inherits. The milestone line is 0.0.x, starting at v0.0.1. v0.0.1 is
the first release (the `v0.0.1-rc.1` pre-release rehearsed the lane, #14);
each cut moves this row and every file that repeats it in one pull request.

| Item | Pin | Repeated in |
|---|---|---|
| Product version | 0.0.9 | `CITATION.cff` `version`, the root `Cargo.toml` `[workspace.package]` `version`, the `compose.yaml` gateway image tag default |

`CITATION.cff` tracks this row exactly, and the guard compares the two, and
the root `Cargo.toml` `[workspace.package]` `version` with both.

## Documentation toolchain

The site is an mdBook rendered by `.github/workflows/docs.yml`. Every tool it
installs is pinned here and repeated in the composite action that installs
them, so the book renders the same way in CI as it does on a laptop.

| Item | Pin | Repeated in |
|---|---|---|
| mdBook | 0.5.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-version` |
| mdbook-toc | 0.15.4 | `.github/actions/docs-toolchain/action.yml` `mdbook-toc-version` |
| mdbook-mermaid | 0.17.1 | `.github/actions/docs-toolchain/action.yml` `mdbook-mermaid-version` |

## Licence

| Item | Pin | Repeated in |
|---|---|---|
| Project licence | BUSL-1.1 | `LICENSE`, `NOTICE`, the SPDX header of every first-party file, later the `license` field of every own `Cargo.toml`, the container `image.licenses` label, the README badge |

`LICENSE` names Apache License 2.0 as a licence of its own, as the Change
License four years after each version. `scripts/checks/versions.sh` fails on an
Apache-2.0 or MIT claim in any first-party file.

Third-party and vendored material keeps its upstream terms, recorded beside the
vendored tree (`.claude/rules/vendored-inputs.md`): the federation
specification is CC0-1.0, its reference implementation Apache-2.0, and the
openEHR specifications carry the licences their `PROVENANCE.md` files quote.

## Rust dependency pins

The root `Cargo.toml` `[workspace.dependencies]` table will be the
authoritative, fully pinned third-party crate set. Beyond the openEHR model
crates above, this file does not duplicate crate versions; on any discrepancy
the manifest wins. A crate joins a member with `dep.workspace = true`.

## CI tool pins

The tier-1 lanes of `.github/workflows/ci.yml` run the analyzers below, each
pinned to an exact version so a CI result matches the local one. `zizmor` and
`shellcheck` are fetched by `taiki-e/install-action`, which verifies the
upstream release checksum; `actionlint`, `hadolint` and `kubeconform` run from
their official container images, pinned by tag and by digest. `kubeconform`
validates the example manifests under `deploy/kubernetes/` against the schemas
of one Kubernetes release, read from `yannh/kubernetes-json-schema` at a pinned
commit, so neither a new schema nor a new release moves the result. `lychee`,
the offline link checker of the `site-links` job, is its upstream release
binary, fetched by version and checked against the SHA-256 its release
publishes, which the job carries beside the version. `promtool`, which the
`observability` job checks the shipped Prometheus rule file with, comes the
same way from the Prometheus release tarball of its version.

| Item | Pin | Repeated in |
|---|---|---|
| `zizmor` | 1.30.1 | `.github/workflows/ci.yml` |
| `actionlint` | 1.7.12 | `.github/workflows/ci.yml` |
| `shellcheck` | 0.11.0 | `.github/workflows/ci.yml` |
| `hadolint` | 2.15.1 | `.github/workflows/ci.yml` |
| `kubeconform` | 0.8.0 | `.github/workflows/ci.yml` |
| `kubeconform schema version` | 1.34.0 | `.github/workflows/ci.yml` |
| `kubernetes-json-schema` | `8df8a883b68a24a104b4a9e43c1288090ae60b3b` | `.github/workflows/ci.yml` |
| `lychee` | 0.24.2 | `.github/workflows/ci.yml` |
| `promtool` | 3.13.4 | `.github/workflows/ci.yml` |

Keep the locally installed versions on these numbers, so a finding costs a
local run rather than a CI round trip (`.claude/rules/ci-cd.md`).

## Release tool pins

The release lane builds and describes every published artifact with the tools
below, each fetched by the digest-pinned `taiki-e/install-action`, which
verifies the upstream release checksum. They decide what a consumer can prove
about a binary, so a floating version here would change the contents of a
release without a reviewed change.

| Item | Pin | Repeated in |
|---|---|---|
| `cargo-auditable` | 0.7.5 | `.github/workflows/release-build.yml`, `.github/workflows/release-viewer.yml` |
| `cargo-cyclonedx` | 0.5.9 | `.github/workflows/release-build.yml` |
| `syft` | 1.51.1 | `.github/workflows/release-build.yml`, `.github/workflows/release-image.yml`, `.github/workflows/release-viewer.yml` |
| `cargo-fuzz` | 0.13.2 | `.github/workflows/fuzz.yml` (the fuzz lane, the one nightly-toolchain job; #134) |

`scripts/checks/versions.sh` reads every `tool:` line of the release and fuzz
workflows back against these rows, so a bump moves one row and the workflows
follow it.

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`). Dependabot bumps them,
and zizmor checks the form.
