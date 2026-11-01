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
| Federation Tier with AQL | 1.0.0 | `docs/architecture.md`, the `FEDERATION_SPEC` constant of `openehr-federation`, the source of the `spec_version` an `OPTIONS {base}/` body carries |
| openEHR ITS-REST | 1.1.0 | `docs/architecture.md`, the `ITS_REST` constant of `ferrofed-engine` |
| openEHR AQL | 1.1.0 | `docs/architecture.md`, the `AQL` constant of `openehr-federation` |

The federation specification is pinned at its 1.0.0 release, approved by the
openEHR SEC on 2026-10-12 and tagged `1.0.0` upstream. The corpus row below
pins that tag and the commit it resolves to, and the vendor script fails when
the tag has moved. Against the `0.9.0` pin it replaced, the release changes
the declared version, the `$id` of both schemas, and the
`federation.spec_version` an `OPTIONS {base}/` body reports, now `"1.0"`
(§7a.2), and it drops the change record page; no N, CP or section number
moved. The reference implementation row below stays at commit `92aff3c`: the
reference implementation has no release that matches 1.0.0, and its newest
tag is `0.9.0`.

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
(`tests/fixtures/xcpd/` of `ihe-iti`, in FerroHEALTH/FerroPIX). IUA, which client authentication
reads for the ITI-71 token claims and the ITI-72 bearer presentation, is
vendored: its supplement is published in IHE's own repository under
CC-BY-4.0, General Introduction ch. 9 grants reproduction of IHE's own text,
and it reproduces no base-standard table. PDQm is not
used by the gateway; its ITI-78 client is a capability of `ihe-iti`
for other callers (#119), and FerroHEALTH/FerroPIX vendors the package with
it (#367). The versions are what the FHIR
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
| Federation Tier with AQL specification | `syntaric/openehr-federation-spec` tag `1.0.0`, commit `7dcf0f44ac122fe947b1e5ceba6bcbbb7721917d` | `scripts/vendor/federation-spec.sh`, `docs/specs/federation-spec/PROVENANCE.md` |
| Federation Tier reference implementation | `syntaric/openehr-federation-ref` commit `92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5` | `scripts/vendor/federation-ref.sh`, `docs/specs/federation-ref/PROVENANCE.md` |
| FerroPULSE report envelope v2 | `FerroHEALTH/FerroPULSE` commit `26957066ad7a7dcd5fc38947b6baa8df0c08609d` path `schemas/report-envelope.v2.json` sha256 `09048000b428cbb010d165b52a4579f60672c471f167a3b58410226dddfec9c2`, with `docs/report-v2.md` and the examples under `schemas/examples/report-envelope.v2/` | `scripts/vendor/ferropulse.sh`, `docs/specs/ferropulse/PROVENANCE.md` |
| openEHR ITS-REST OpenAPI | `openEHR/specifications-ITS-REST` tag `Release-1.1.0`, all seven API modules, the Query validation document and the SMART on openEHR source (`docs/smart_app_launch/`, DEVELOPMENT status in this release) | `scripts/vendor/its-rest.sh`, `docs/specs/its-rest/PROVENANCE.md` |
| openEHR AQL specification source | `openEHR/specifications-QUERY` tag `Release-1.1.0`, the AQL and AQL examples documents and the grammar | `scripts/vendor/aql.sh`, `docs/specs/aql/PROVENANCE.md` |
| openEHR Reference Model specification source | `openEHR/specifications-RM` tag `Release-1.1.0`, the AsciiDoc sources of the RM 1.1.0 specifications and the class definitions they include | `scripts/vendor/openehr-rm.sh`, `docs/specs/openehr-rm/PROVENANCE.md` |
| openEHR BASE specification source | `openEHR/specifications-BASE` tag `Release-1.1.0`, the release paired with the RM `Release-1.1.0` (decided 2026-10-06): the AsciiDoc sources of the BASE 1.1.0 specifications and the class definitions they include | `scripts/vendor/openehr-base.sh`, `docs/specs/openehr-base/PROVENANCE.md` |
| IHE PIXm FHIR package | `ihe.iti.pixm` version `3.1.0` from `packages.fhir.org`, tarball sha256 `19e2e8eaf3030ac7b4d809c5e1eeb8face02c8635318aeb6d35bc2bb889de0d0`, the ITI-83 artefacts | `scripts/vendor/ihe-pixm.sh`, `docs/specs/ihe-pixm/PROVENANCE.md` |
| IHE PDQm FHIR package | `ihe.iti.pdqm` version `3.2.0` from `packages.fhir.org`, tarball sha256 `61e09fbee991ff7c131b6ba5474921001e07782209961f3e85cee5f3ebcaedc2`, the ITI-78 and ITI-119 artefacts | the `ihe-iti` crate; FerroHEALTH/FerroPIX vendors the package by `scripts/vendor/ihe-pdqm.sh` (#367) |
| IHE mCSD FHIR package | `ihe.iti.mcsd` version `4.0.0` from `packages.fhir.org`, tarball sha256 `933a143d7bb14c66731a32f52a084c6cb92476aca1b917db77a4640f8a5290ad`, the ITI-90 and ITI-91 artefacts | `scripts/vendor/ihe-mcsd.sh`, `docs/specs/ihe-mcsd/PROVENANCE.md` |
| IHE PMIR FHIR package | `ihe.iti.pmir` version `1.6.0` from `packages.fhir.org`, tarball sha256 `ec9d25fc64ac2f3087f921c14c0da56afc7e794caa80298db3f130bc0a40fe70`, the ITI-93 and ITI-94 artefacts | `scripts/vendor/ihe-pmir.sh`, `docs/specs/ihe-pmir/PROVENANCE.md` |
| IHE IUA supplement | `IHE/ITI.IUA` tag `2.5`, the Revision 2.5 Trial Implementation supplement text (ITI-71, ITI-72, ITI-102, ITI-103) and its figures | `scripts/vendor/ihe-iua.sh`, `docs/specs/ihe-iua/PROVENANCE.md` |
| IHE BALP FHIR package | `ihe.iti.balp` version `1.1.4` from `packages.fhir.org`, tarball sha256 `be46dda3088ee9d486d7163458dc5a4d7192e8a1c587fd47e2beb70e4bcefa90`, the whole package but its manifest | the `ihe-iti` crate (`balp`) and `ihe-atna`; FerroHEALTH/FerroPIX and FerroHEALTH/FerroAUDIT vendor the package by `scripts/vendor/ihe-balp.sh` (#367) |
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
| IETF RFC 8693 | `https://www.rfc-editor.org/rfc/rfc8693.txt` sha256 `358d658014738bf4dc56c8aae2094ce716ede23b6d5a80488a7dc8e8d72da609`, OAuth 2.0 Token Exchange | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 8705 | `https://www.rfc-editor.org/rfc/rfc8705.txt` sha256 `6e45a1ee94c6a6a177a4cb077fe67d74bd70c1218c9362f2db2f153239612414`, OAuth 2.0 Mutual-TLS Client Authentication and Certificate-Bound Access Tokens | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9126 | `https://www.rfc-editor.org/rfc/rfc9126.txt` sha256 `a79d0e30fcc24a22b79c8e18aa82362f6e63a7b8a5d58b480e746360e97388db`, OAuth 2.0 Pushed Authorization Requests | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9396 | `https://www.rfc-editor.org/rfc/rfc9396.txt` sha256 `d6a8f032d8a585daae1c33a8c7b6e539d199f886ec8cc1c7898436f7f2eed29c`, OAuth 2.0 Rich Authorization Requests | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| IETF RFC 9449 | `https://www.rfc-editor.org/rfc/rfc9449.txt` sha256 `3842c58e1f6043389416023b9bb8d765048266024982fbbd90640e05943f4e13`, OAuth 2.0 Demonstrating Proof of Possession (DPoP) | `scripts/vendor/ietf-oauth.sh`, `docs/specs/ietf-oauth/PROVENANCE.md` |
| W3C Verifiable Credentials Data Model 1.1 | `https://www.w3.org/TR/2022/REC-vc-data-model-20220303/` sha256 `d7c796e1f7e9a233d9037eec03e9d9fe7d69220a71639b07f5038684949a3091`, W3C Recommendation 2022-03-03 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| W3C Decentralized Identifiers 1.0 | `https://www.w3.org/TR/2022/REC-did-core-20220719/` sha256 `5e44345740d9bfaa852d3b66c57e98c9beb6c5bf6083b0126dd5daac377b9993`, W3C Recommendation 2022-07-19 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
| W3C DID Resolution 1.0 | `https://www.w3.org/TR/2026/CRD-did-resolution-1.0-20261026/` sha256 `2ca67fcccc42028c8f4dbf67560517d2c3218b09232cdd24f587e0456b647351`, W3C Candidate Recommendation Draft 2026-10-26 | `scripts/vendor/w3c-did-vc.sh`, `docs/specs/w3c-did-vc/PROVENANCE.md` |
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
| EU EHDS Regulation and eHealth Network guidelines | Regulation (EU) 2025/327, Implementing Regulations (EU) 2026/2083 and 2026/2099, Recommendation (EU) 2019/243, the eHealth Network Patient Summary, ePrescription, Laboratory Results, Hospital Discharge Report, Medical imaging studies and reports and General Guidelines and Commission Decision 2011/833/EU on the reuse of Commission documents, all committed, with the eHDSI interoperability specifications and the MyHealth@EU requirements catalogue recorded for manual retrieval (EU Login), pin-set digest `e03a49fe736e3b5209af1a9534792fc849d9a489046cb47e6cf086c9a3478ba2` | `scripts/vendor/eu.sh`, `docs/specs/eu-ehds/PROVENANCE.md` |
| EU Cyber Resilience Act and market surveillance acts | Regulation (EU) 2024/2847 with its three English corrigenda, Implementing Regulation (EU) 2025/2392, Delegated Regulations (EU) 2025/1535 and 2026/881, Regulation (EU) 2019/1020 and Regulation (EC) No 765/2008, pin-set digest `c5641e7937d0a0a9638af933a62143b1a9807fa002a1b3b337632e527e4f655f` | `scripts/vendor/eu.sh`, `docs/specs/eu-cra/PROVENANCE.md` |
| MyHealth@EU NCPeH API and OpenNCP | pin-set digest `653ef3161823f668756ad2b5783d29e2329926aeeac4734dee8dd9a841031598`, package `myhealth.eu.fhir.ncp-api` 9.1.1-ci (the current build at `build-fhir.ehdsi.eu`, 2026-09-08) and three of its rendered pages (home, Patient Identification, Document Retrieval), committed; the `fhir.ehdsi.eu` site index and OpenNCP v10.1.0, cache only | `scripts/vendor/eu.sh`, `docs/specs/ehdsi/PROVENANCE.md` |
| IHE ITI Technical Framework Volume 1 pages | ITI TF Revision 20.2 chapters 13, 18 and 27, pin-set digest `838b2f672e0bc34841d7fe297fd561c6f49c42fdd12b5a15119eb10a7234aeb7` | `scripts/vendor/ihe-iti-tf.sh`, `docs/specs/ihe-iti-tf/PROVENANCE.md` |
| IHE ITI Technical Framework Volume 2 pages | the ITI-38 page (Revision 20.2), committed, and the ITI-55 page (Revision 20.1), cache only, pin-set digest `84c3ff8cfad148b5c066ae364d1f8a3044c6210ed806e9b87f363d8dda2d5546` | `scripts/vendor/ihe-iti-tf.sh`, `docs/specs/ihe-iti-tf-vol2/PROVENANCE.md` |
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
the versions those guides depend on, and the IHE MHD profile the MyHealth@EU
NCPeH API depends on. Each is the registry tarball
of `packages.fhir.org` (the HL7 Europe Imaging Report 1.0.0-alpha preview is
HL7 Europe's own, which the registry does not list), pinned in `scripts/vendor/eehrxf.sh` by URL and
sha256 through `scripts/vendor/lib/pinned.sh` and committed whole, and each
row carries the pin-set digest first and the tarball sha256 after it.
`scripts/checks/pin-freshness.sh` reads each package's registry entry.

| Item | Pin | Repeated in |
|---|---|---|
| Xt-EHR EHDS Logical Information Models | package `xtehr.eu.ehds.models` 1.0.0 and 0.3.0 (the version the MyHealth@EU guides depend on), pin-set digest `7d4ef5d38be992ad0d72849688f235e7621f50eb0e9821bc39ab368538e046b4`, 1.0.0 tarball sha256 `a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-xtehr-models/PROVENANCE.md` |
| HL7 Europe Base and Core | package `hl7.fhir.eu.base` 2.0.1, pin-set digest `b84f7ba0799903e6bf4d2bf0ecaeeaefd0f71510238c7e30d63244ff6c603c67`, tarball sha256 `3fb23b64d70656ea809e1ad66d84d5d008400e18424266debf176cb46fba0b4a` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-base/PROVENANCE.md` |
| HL7 Europe Patient Summary | package `hl7.fhir.eu.eps` 1.0.0-ballot, pin-set digest `ba59282865c06a3819a25d92b4110f5d6c84b2df2b0f4e0ae34092a66659f038`, tarball sha256 `e0fff1fb20d3daf75609259faa6b860131ebf4fc8890bb68f0da882307afc33c` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-eps/PROVENANCE.md` |
| HL7 Europe Medication Prescription and Dispense | package `hl7.fhir.eu.mpd` 1.0.0, pin-set digest `4c4ea5572812dbacfc87573af4165742284a0825b0a200fa606df07393ea9758`, tarball sha256 `f1bc1084efc93e16ae4b45c1bb71340385d314a72031949c4a7e0d97aea5fd6b` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-mpd/PROVENANCE.md` |
| HL7 Europe Laboratory Report | package `hl7.fhir.eu.laboratory` 2.1.0 and 2.0.0 (the version the MyHealth@EU guides and the interoperability component read), pin-set digest `ee502f9693d6df00d1b52ef7e0548eafd3512d9698d05469681d5e5635573e02`, 2.1.0 tarball sha256 `87019c396f4cff934ec5230ce1f97d76b79b40591be08161e694ad661129379c` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-laboratory/PROVENANCE.md` |
| HL7 Europe Extensions | package `hl7.fhir.eu.extensions.r4` 1.3.1 and 1.3.0, and `hl7.fhir.eu.extensions` 0.1.0 (the package name before `.r4`, which MyHealth@EU Hospital Discharge Report 9.1.1 depends on), pin-set digest `19839fe8077fe6c794fb035ab52447003fcd3c6996b354fe893bf82588b3658c`, tarball sha256 `37f3ee7ae7a2312e71a4e855e6c36f45d2bc3b13f995bcf4a9fb499ca014cff6` (1.3.1, which `hl7.fhir.eu.base` 2.0.1 depends on) and `8510e930856961d550c04d43f243ef0bdcb27089431fe183ab11c7f805f0c2f1` (1.3.0, which `hl7.fhir.eu.eps` 1.0.0-ballot and `hl7.fhir.eu.laboratory` 2.0.0 depend on) | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-extensions/PROVENANCE.md` |
| HL7 International Patient Summary | package `hl7.fhir.uv.ips` 2.0.1 and 2.0.0, pin-set digest `6c8531fdbe8adbc888a8a4f33dcbe9619b8e42f47e1c7186f698cc11eb389055`, tarball sha256 `7183242b70fb2a9058aa3701fb607517a3c2fd0e3100d1d8c538d744c2adf799` (2.0.1, which `hl7.fhir.eu.laboratory` 2.1.0 and `hl7.fhir.eu.hdr` 1.0.0 depend on) and `b3964eba08ee699bc121b905c4290641e54dd34f2cf3b5cd3edeb08a40a66979` (2.0.0, which `hl7.fhir.eu.eps` 1.0.0-ballot depends on) | `scripts/vendor/eehrxf.sh`, `docs/specs/hl7-ips/PROVENANCE.md` |
| IHE Pharmacy Medication Prescription and Dispense | package `ihe.pharm.mpd.r4` 1.0.0-comment-2, pin-set digest `12effdefce443cb7b61ff9b7f39a4431b54d9267becdeef7d3505770ddc8398e`, tarball sha256 `05eda0d1871d2980cb13023d18a5a9618c9945c3a85cf721c14c5e989e0d3776`, the version `hl7.fhir.eu.base` 2.0.1, `hl7.fhir.eu.eps` 1.0.0-ballot and `hl7.fhir.eu.mpd` 1.0.0 depend on | `scripts/vendor/eehrxf.sh`, `docs/specs/ihe-pharm-mpd/PROVENANCE.md` |
| HL7 Europe EU Health Data API | package `hl7.fhir.eu.health-data-api` 1.0.0-ballot, pin-set digest `253ebbce321fc294ae582c3c3abb67274ad53de6578c3eb3360e1566697691ac`, tarball sha256 `5204af60bf7f74f301a627e641a34683d392014f53d5df8ba3651c964e92977b`, whose `EEHRxFDocumentPriorityCategoryCS` codes the priority categories the access record writes | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-health-data-api/PROVENANCE.md`, `tools/ferrofed-testkit/src/health_data_api.rs` |
| HL7 Europe Imaging Report | package `hl7.fhir.eu.imaging` 1.0.0-ballot and 1.0.0-alpha (the preview at `https://hl7.eu/fhir/imaging/xtehr/package.tgz` that MyHealth@EU Medical Imaging Report 9.1.1 depends on), pin-set digest `61b070162691604c38ec749df275cffd12b5636fb1f22cf478f0eedb6500595d`, 1.0.0-ballot tarball sha256 `bb6012bd7a019ac82f5e939dd6e892e6db6b15fd2931dcf7cc445c1ad92c3d70`, 1.0.0-alpha tarball sha256 `bbc369b9aadabd96b223f4944643a9adff0c40479c61970153661a50bba73e89` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-imaging/PROVENANCE.md` |
| HL7 Europe Hospital Discharge Report | package `hl7.fhir.eu.hdr` 1.0.0 and 0.1.0-ballot, pin-set digest `00575e0cccddc95fb3af7edf5c1d8fa33cc86dcc3bab87874eecebdda54bc168`, 1.0.0 tarball sha256 `eedf9af9f66b68cd66f7d6c678a36dfba86b0920df4ef15e8d2da900c4134879` | `scripts/vendor/eehrxf.sh`, `docs/specs/eu-hl7-hdr/PROVENANCE.md` |
| MyHealth@EU Master Value Sets Catalogue | package `myhealth.eu.fhir.mvc-package` 9.1.0, pin-set digest `06f68a58f7382db396f029b1c0a420fcac6cbf53d8a948df47594937b0886749`, tarball sha256 `3f99350f36a2a1ea17b7fde37c39ff766bd5c3bc9a29c6183af1acfd07b78ce1` | `scripts/vendor/eehrxf.sh`, `docs/specs/ehdsi-mvc/PROVENANCE.md` |
| FHIR package closure of the Patient Summary | the 15 registry tarballs the committed HL7 Europe manifests name and the repository does not commit (`hl7.fhir.r4.core` 4.0.1, `hl7.fhir.eu.base` 2.0.0, `hl7.fhir.uv.extensions.r4` 5.2.0 and 5.3.0, `hl7.fhir.uv.xver-r5.r4` 0.1.0, `hl7.terminology.r4` 5.0.0, 6.2.0, 6.3.0, 6.5.0, 7.1.0 and 7.3.0, `hl7.fhir.uv.ipa` 1.1.0, `hl7.fhir.uv.smart-app-launch` 2.0.0, `hl7.fhir.uv.ips` 1.1.0, `fhir.dicom` 2022.4.20221006), cache only for their size, which FerroBRIDGE's validator holds a received document to with the committed packages (#808); pin-set digest `4da6e21751ced98a8fb1879aafd6b2b37b975921338adffc4d6d3465088db485`, R4 core sha256 `ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44` | `scripts/vendor/fhir-packages.sh`, `docs/specs/fhir-packages/PROVENANCE.md`, `tools/ferrofed-testkit/src/fhir_packages.rs` |
| IHE MHD FHIR package | package `ihe.iti.mhd` 4.2.2, pin-set digest `6f75d10023f323696b2ed5219ca1e2db57df4b41a6efede3caf69139f3ffdd70`, tarball sha256 `824061df179f6ade65c29bd79dfc35f00075748fe13054c8505a6e6f3d1cd578`, the version `myhealth.eu.fhir.ncp-api` 9.1.1-ci depends on | `scripts/vendor/eehrxf.sh`, `docs/specs/ihe-mhd/PROVENANCE.md` |
| FHIR package closure of the MyHealth@EU guides | the 3 registry tarballs the manifests of the MyHealth@EU guides, `hl7.fhir.eu.laboratory` 2.1.0 and `hl7.fhir.eu.hdr` 1.0.0 name beside the Patient Summary's closure and the committed packages (`hl7.terminology.r4` 7.2.0 and 7.4.0, `hl7.fhir.uv.xver-r5.r4` 0.0.1-snapshot-2), cache only for their size; pin-set digest `40c0a9bdd1b3da6c3061474c0ef3c0ef1e387ef2f8c74ce1fce99786cf7b3e32` | `scripts/vendor/fhir-packages.sh`, `docs/specs/fhir-packages-myhealth-eu/PROVENANCE.md` |

### MyHealth@EU, eHDSI and Xt-EHR material (#1290)

The public material of the MyHealth@EU programme and of the Xt-EHR joint
action that the decisions on the national contact point, the exchange format
and the priority categories read. The MyHealth@EU FHIR guides and the eHDSI
material are pinned in `scripts/vendor/myhealth-eu.sh`, the Xt-EHR
deliverables and metadata guide in `scripts/vendor/xt-ehr.sh`, each by URL
and sha256 through `scripts/vendor/lib/pinned.sh`. A guide on the FHIR
package registry is its registry tarball; a guide only its publisher's site
serves is pinned at that site's package URL, which serves the current build.
Each row carries the pin-set digest before any tarball sha256.
`scripts/checks/pin-freshness.sh` reads each package's registry entry or the
version its build URL serves, and each `code.europa.eu` repository's branch
head or tags. The NCPeH API guide is the `MyHealth@EU NCPeH API and OpenNCP`
row above, the Master Value Sets Catalogue and the Xt-EHR logical models
are rows of the proxy table.

| Item | Pin | Repeated in |
|---|---|---|
| MyHealth@EU Core | package `myhealth.eu.fhir.core` 1.0.0, pin-set digest `1c03b4287f36f09497741fe240d335dd6bd1413ec4e27743427379d02b798263`, tarball sha256 `d40b9979dd971a76fc39452aee4091a5f79a9d46192ece7c28e70b4d429a9270` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-core/PROVENANCE.md` |
| MyHealth@EU Patient Summary | package `myhealth.eu.fhir.eps` 1.0.0-poc and its rendered home page (CC-BY-4.0), pin-set digest `0dc5b4f7bc3bc4770c7e2673420eeb945366524a2324abd205c16a851c457c85`, tarball sha256 `34c49509890f6b41f5ffcb788f04cbeefb5379ba92c2a1f486f4295ee58fdb62` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-eps/PROVENANCE.md` |
| MyHealth@EU ePrescription and eDispensation | package `myhealth.eu.fhir.epd` 1.0.0-poc and its rendered home page (CC-BY-4.0), pin-set digest `27781c1e57be144cd78fccd659f8f27eead8e70c29149dbdde0b9774a2de95d4`, tarball sha256 `5e9e01171e70cb62d523c64c7e47238584bdde3c43875dd09fd2dbb5eaed1d5e` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-epd/PROVENANCE.md` |
| MyHealth@EU Laboratory Report | package `myhealth.eu.fhir.laboratory` 9.1.1, pin-set digest `95a9a0bc8c4a7979588cbf1198fd052057473c651d94422801cea45c6c3f2494`, tarball sha256 `3e3bd638231997fd99799a40a7d8934541ca5ba65304f3a4b3886a7e675b0b56` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-laboratory/PROVENANCE.md` |
| MyHealth@EU Hospital Discharge Report | pin-set digest `e59077df1d1e1a7ab9f0be1f01aa5437840e6915d645c2fa1de6c9bbecc05a15`, package `myhealth.eu.fhir.hdr` 9.1.1 (the build `fhir.ehdsi.eu` serves, 2026-05-05), tarball sha256 `38eb62092c18aa269c72cdceac01d706b293a5e5389186c682c50467bf84b38c` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-hdr/PROVENANCE.md` |
| MyHealth@EU Medical Imaging Report | pin-set digest `28478270f5ca1df8358480a8b18a004e141f5ed1d710975b82306e2e2c25b6d2`, package `myhealth.eu.fhir.imaging` 9.1.1 (the build `fhir.ehdsi.eu` serves, 2026-05-05), tarball sha256 `286eebc30bc4be9c95b2478152e1dce95a6f23bc31f42f4587fd846ae04a982d` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-imaging/PROVENANCE.md` |
| MyHealth@EU Vaccination | package `myhealth.eu.fhir.euvac` 10.0.0-preview (CC-BY-4.0), pin-set digest `c4ad39ad2bdbcd4f4061ca49be293dee7a5064403758d031d7d075d5f2d605e5`, tarball sha256 `96bf16bc13cf01b1c9a7642aef20704cb210c4900d83ae4ec6511081535fe3f6` | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-euvac/PROVENANCE.md` |
| eHDSI CDA reference test documents | pin-set digest `5f358affe656663017a0f806d54791dce590497aa3db325429cbfb956b2e9147`, `code.europa.eu/ehdsi/ehdsi-general-repository` branch `main` commit `e521266708aac6e47a1e78f243c178bc0eb7d3b2`, cache only (no licence statement) | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-cda-test-documents/PROVENANCE.md` |
| OpenNCP Transformation Manager resources | pin-set digest `5de8e98013948ea4433573a33c99e2b069129fe4d1e6bbcbe5d27d88c518526a`, `code.europa.eu/ehdsi/ehealth` tag `v10.1.0`, commit `bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f`: the CDA Schematrons and the eHDSI coded element list, committed (Apache-2.0) | `scripts/vendor/myhealth-eu.sh`, `docs/specs/ehdsi-openncp-tm/PROVENANCE.md` |
| Xt-EHR deliverables | every deliverable on `xt-ehr.eu/deliverables/` (D1.1 to D9.3) with the T5.2 and T5.3 outcomes, cache only (no licence statement, the site reserves all rights), among them D5.1, D5.2, D6.1, D6.2, D7.1 to D7.3 and D8.1, the closest published text of the Article 15(1) specifications while no draft act is published; pin-set digest `c1201635d3c9a535301b5fc317276178b031217406813e5601fdd4142f7a774a`, D6.1 sha256 `65da226ed74e549e4e3d3611b9a458382a147f8059bd7903b6e1d2bc6fe0374c` | `scripts/vendor/xt-ehr.sh`, `docs/specs/xt-ehr-deliverables/PROVENANCE.md` |
| Xt-EHR Metadata Foundation | pin-set digest `a029ec92d1c9994de051cf23183620fd712c92a662c3e33e615d13de1b2adb52`, package `hl7.eu.fhir.xt-ehr.metadata` 0.1.0 (the continuous build at `xt-ehr.github.io`, 2025-09-10), tarball sha256 `50066947a06a59f819b44024650a2ebada71dea40479c239db770b0893713a4a` | `scripts/vendor/xt-ehr.sh`, `docs/specs/eu-xtehr-metadata/PROVENANCE.md` |

### FHIR R4 core specification (#1053, #1072, #1040)

The pages of FHIR R4 4.0.1 that govern a definition FerroFED publishes:
Extensibility, Defining Extensions, the StructureDefinition, CodeSystem and
NamingSystem resources, Using Codes in Resources (4.1), Search (3.1.1),
the RESTful API (3.1.0) and the licence page, each pinned in
`scripts/vendor/fhir-r4.sh` by its versioned URL and sha256 through
`scripts/vendor/lib/pinned.sh`. Every `ferrofed.eu` FHIR definition is tested
against them, and the prescription search is read against the Search and
RESTful API pages.

| Item | Pin | Repeated in |
|---|---|---|
| FHIR R4 core specification pages | `hl7.org/fhir/R4/` 4.0.1, pin-set digest `c16ba48646a27bf3fdb93cf44393879ac77841a2e900af03a46bbc1ebbd3aa64`, the Extensibility (2.5.0), Defining Extensions (2.5.1), StructureDefinition, CodeSystem, NamingSystem, Using Codes in Resources (4.1), Search (3.1.1), RESTful API (3.1.0) and License and Legal Terms (2.20) pages | `scripts/vendor/fhir-r4.sh`, `docs/specs/fhir-r4/PROVENANCE.md` |

### openEHR clinical models (#776, #980, #1043)

The openEHR International Patient Summary template of the openEHR
international Clinical Knowledge Manager, whose sections name the archetypes
the patient summary's stored section queries select compositions by, and the
medication order and medication management archetypes an openEHR mapping of
prescriptions and dispensations reads, with the authorisation, order summary
and supply amount clusters the order's slots admit (#523). CKM publishes no git commit or
release feed for a template or an archetype, so each artefact is
pinned by its REST URL and sha256 through `scripts/vendor/lib/pinned.sh` in
`scripts/vendor/openehr-ckm.sh` and committed verbatim under CC BY-SA, the
licence the openEHR Foundation gives the clinical models CKM hosts. Each
archetype states its own version in its `licence` field: 4.0 International
for `openEHR-EHR-INSTRUCTION.medication_order.v3`,
`openEHR-EHR-ACTION.medication.v1` and
`openEHR-EHR-CLUSTER.medication_supply_amount.v0`, and 3.0 for
`openEHR-EHR-CLUSTER.medication_authorisation.v0` and
`openEHR-EHR-CLUSTER.medication_order_summary.v0`.

| Item | Pin | Repeated in |
|---|---|---|
| openEHR CKM International Patient Summary template | template `937fca6c-ec24-4c0f-8986-623843b6ebca`, CKM cid `1013.26.376` asset version 1 (DRAFT), pin-set digest `ac31d1254c60a728686eb84e9dd78084de8b05ea9082342c7ed5404b7843f736`, OET sha256 `e293bc003fc787e95318be0e18f32ae7ca8903ef8475ed7644b86258a59e5a34` | `scripts/vendor/openehr-ckm.sh`, `docs/specs/openehr-ckm-ips/PROVENANCE.md` |
| openEHR CKM laboratory test result template | template `0ce44071-d6e5-4fa2-9a41-046f9b4aea66`, CKM cid `1013.26.408` asset version 3 (DRAFT), pin-set digest `7ae45e1e0bbc1d7e9c3f6c76c5667519229207d694bcb0c28039f6ea9a761f94`, OET sha256 `3811e80486c9fa526881e1ec891ba320b92547794ffec5447618ee9cb0216ec3` | `scripts/vendor/openehr-ckm.sh`, `docs/specs/openehr-ckm-laboratory/PROVENANCE.md` |
| openEHR CKM medication archetypes | `openEHR-EHR-INSTRUCTION.medication_order.v3` CKM cid `1013.1.5946` revision 3.2.1 asset version 7 and `openEHR-EHR-ACTION.medication.v1` CKM cid `1013.1.123` revision 1.6.0 asset version 44 (both PUBLISHED), `openEHR-EHR-CLUSTER.medication_authorisation.v0` CKM cid `1013.1.2300` asset version 3, `openEHR-EHR-CLUSTER.medication_order_summary.v0` CKM cid `1013.1.2306` asset version 6 and `openEHR-EHR-CLUSTER.medication_supply_amount.v0` CKM cid `1013.1.2453` asset version 5 (all three DRAFT, revision 0.0.1-alpha), pin-set digest `d6435a63b0d59b1557a939c30080ea0c212c2e058480c74cead2b5855528a655`, ADL sha256 `206c2a07fa1a7197ebc9ed7a29a418ecfc9cb6b63fa2aaf811e780ee69df9e9b` and `7e2484d4aec5ad8c5ceb5edb466a82b43b45396fbe3abaa4ee4a298b8e6437bc` | `scripts/vendor/openehr-ckm.sh`, `docs/specs/openehr-ckm-medication/PROVENANCE.md` |

## openEHR model crates (crates.io)

The openEHR surface comes from the published `openehr-*` crates, consumed by
version like any other dependency (`docs/architecture.md` §2). FerroEHR
releases them as one lockstep family, so the five rows below are one group:
they move together, and `scripts/checks/versions.sh` fails when one member
moves alone, here or in the root `Cargo.toml` `[workspace.dependencies]`. The
pin is 0.0.109 since 2026-10-28, the line the published `eehrxf`,
`fhirconnect` and `openehr-mapping-core` require exactly, so the lock holds
one copy of each member (#1181, #813).
`openehr-sdt` (the SMART on openEHR scope grammar) joined the group at the
family pin with its default features off, so only the grammar is compiled:
the onward grant (#81) writes and checks the scope it requests with it, and client
authentication (#80) reads every caller's scopes with it.

| Item | Pin | Repeated in |
|---|---|---|
| `openehr-query` | 0.0.109 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-its` | 0.0.109 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-base` | 0.0.109 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-rm` | 0.0.109 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |
| `openehr-sdt` | 0.0.109 | the root `Cargo.toml` `[workspace.dependencies]`, the `OPENEHR_FAMILY` constant of `ferrofed-server` |

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
0.0.86, published on 2026-10-06, changes no code: FerroEHR names Cadasto B.V.
as the Licensor and copyright holder in the packaged licence files (FerroEHR
#3567, #3572), and 0.0.85 was never published.
0.0.87, published on 2026-10-15, carries FerroEHR #3600, #3781 and #3782.
The first is `ReqwestTransport::with_max_body`, a bound on the bytes the
client reads of one answer, failing with `ClientError::AnswerTooLarge`; the
node clients use it in place of the local transport that bounded the body
(#714). The second reads a quoted value of a GET query parameter as a JSON
string, quotes removed (#870). The third is
`IdentifiedPath::column_path_with_variable`, the column path that keeps the
AQL variable, which `columns[].path` now carries (#877).
0.0.89, published on 2026-10-19, and 0.0.90, published on 2026-10-20, change
nothing the gateway reads: they add the PARTY builder, the caller's
`rm_version` and party validation to `openehr-sdt` (FerroEHR #3809, #3810,
#3832). 0.0.109 is taken with the published interoperability component
(#813): `eehrxf` 0.1.128 and `fhirconnect` 0.1.204 require it, and a 0.0.x
caret requirement admits one version.
`ferrohealth-smart` 0.8.5 and later carry the SMART on openEHR scope grammar
as their own module and no longer depend on `openehr-sdt`, so the lock holds
one `openehr-sdt` (#1390).

## FHIR model crate (crates.io)

The FHIR model comes from the published `fhir-types` crate
(`docs/architecture.md` §6, decisions A16 and A59), compiled in the binding
crates `ihe-iti` and `nl-generic-functions` and in the interoperability
component `eehrxf` (all from crates.io), never in the engine, so the
gateway core never builds it. `scripts/checks/versions.sh` fails when this row
and the root `Cargo.toml` `[workspace.dependencies]` disagree.

| Item | Pin | Repeated in |
|---|---|---|
| `fhir-types` | 0.1.128 | `docs/architecture.md`, the root `Cargo.toml` `[workspace.dependencies]` |

The `pixm` feature takes `r4` and `terminology`: that root set holds every
type ITI-83 reads (`Parameters`, `OperationOutcome`, `Identifier`, `Reference`,
`Bundle`). The `pdqm` feature takes `r4` and `resources`, every R4 resource,
because ITI-78 answers with `Patient` resources, which only `resources`
carries (#119); the mCSD directory (#86) reads `Organization` and `Endpoint`
from the same set. 0.1.127 is the release the FHIRconnect engine builds on
(#684), so the build carries one `fhir-types`.

## The interoperability component and its engine (crates.io)

The European interoperability component, `eehrxf`, is published from
FerroHEALTH/FerroBRIDGE since the owner decision of 2026-10-28 (#813, decision
A76), and every change to it is made there. Its `openehr` feature (#684,
#730) runs FHIRconnect 1.0.0 in process through FerroBRIDGE's published
`fhirconnect`, with the template index and the mapping-file loader of
`openehr-mapping-core` (decision A61). Each crate below is pinned exactly in
the root `Cargo.toml`, because FerroBRIDGE's crate line moves by patch, at the
version the pinned `eehrxf` requires, and all build on the `openehr-*` line
above. The component's terminology server is asked
through FerroBRIDGE's `ferrobridge-term` (#925), pinned exactly the same way,
over the HTTP client the gateway builds with its credential and TLS material. A
received document is validated in full by FerroBRIDGE's `ferrobridge-validator`
(#808), pinned exactly the same way, on the same `fhir-types` and
`ferrobridge-term` lines.

| Item | Pin | Repeated in |
|---|---|---|
| `eehrxf` | =0.1.128 | the root `Cargo.toml` `[workspace.dependencies]`, the information sheet, the clinical safety risk file |
| `fhirconnect` | =0.1.204 | the root `Cargo.toml` `[workspace.dependencies]` |
| `openehr-mapping-core` | =0.1.135 | the root `Cargo.toml` `[workspace.dependencies]` |
| `ferrobridge-term` | =0.1.141 | the root `Cargo.toml` `[workspace.dependencies]` |
| `ferrobridge-validator` | =0.1.141 | the root `Cargo.toml` `[workspace.dependencies]` |

## Audit crates (crates.io)

The ATNA ITI-20 sender, the BALP `AuditEvent` with its FHIR Feed, and the
European logging component come from the family's audit library,
FerroHEALTH/FerroAUDIT (#1176). Both crates are pinned exactly in the root
`Cargo.toml` and move together, one version line and one release tag. The
IHE and EU corpora FerroAUDIT vendors for them are kept here at the same pins
where FerroFED's own code, tests or guards still read them.

| Item | Pin | Repeated in |
|---|---|---|
| `ihe-atna` | =0.3.2 | the root `Cargo.toml` `[workspace.dependencies]` |
| `ehds-logging` | =0.3.2 | the root `Cargo.toml` `[workspace.dependencies]` |

## Metrics crates (crates.io)

The metrics surface (#281) and the trace export (#353) run on the one
OpenTelemetry provider `ferrohealth-telemetry` builds (#1397), which reads
the metrics with Prometheus and pushes them and the spans over OTLP. The
gateway records its own instruments and carries the trace context of its
node requests through `opentelemetry`, `opentelemetry_sdk` and
`tracing-opentelemetry`. The `opentelemetry` crates are released in
lockstep, so their rows are one group: they move together, and
`scripts/checks/versions.sh` fails when one member moves alone, here or in
the root `Cargo.toml` `[workspace.dependencies]`. `tracing-opentelemetry`'s
release line is paired with the `opentelemetry` one (0.34 with 0.33) and is
checked against the root `Cargo.toml` alone. The testkit's in-process OTLP
collector (#437) answers the OTLP trace service of `opentelemetry-proto`, a
member of the group, on a `tonic` gRPC server, checked against the root
`Cargo.toml` alone; both are test-only. Every version was the latest on
crates.io on 2026-10-03, and `tracing-opentelemetry`, `opentelemetry-proto`
and `tonic` on 2026-10-04.

| Item | Pin | Repeated in |
|---|---|---|
| `opentelemetry` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry_sdk` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `opentelemetry-proto` | 0.33.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tracing-opentelemetry` | 0.34.0 | the root `Cargo.toml` `[workspace.dependencies]` |
| `tonic` | 0.14.6 | the root `Cargo.toml` `[workspace.dependencies]` |

## Operator console crates and build tools

The operator console, `app/ferrofed-console` (#275), draws every page on its
server with FerroUI, the `ferrohealth-ui` crate, and serves the crate's
stylesheet, its FerroFED theme and its script modules from the crate itself
(#1457). It builds with `cargo build` alone: no WebAssembly target, no site
bundle and no build tool of its own. FerroUI is pinned exactly, so the
console moves to a new release deliberately, and the About page names the
version it draws with. `scripts/checks/versions.sh` holds every row.

| Item | Pin | Repeated in |
|---|---|---|
| `ferrohealth-ui` (FerroUI) | 0.6.0 | the root `Cargo.toml` `[workspace.dependencies]` (pinned exactly), `app/ferrofed-console/src/pages/about.rs`, `.claude/rules/console-ui.md` |

The browser journeys (#608) drive the console in headless Chrome over
WebDriver, through chromedriver, with the `thirtyfour` client, a test-only
dependency of the testkit. Chrome and chromedriver come as one Chrome for
Testing release, which `browser-actions/setup-chrome` installs in the
`journeys (browser)` job of `ci.yml`, so the browser and its driver always
match. `thirtyfour` was the latest on crates.io on 2026-10-29, and the Chrome
for Testing release was the newest stable one on 2026-10-14. `scripts/checks/versions.sh`
holds both rows, and the weekly `scripts/checks/pin-freshness.sh` reports a
newer stable Chrome for Testing release.

For a local run, `scripts/release/journey-tools.sh` installs the same
release into `target/journey-tools`, where the journeys look first. It takes
each download URL from the Chrome for Testing JSON API
(`known-good-versions-with-downloads.json`) and checks each zip against the
SHA-256 below before it unpacks it. Chrome for Testing publishes no SHA-256,
so each digest was taken on 2026-10-16 of a download whose MD5 equals the
`x-goog-hash` the storage bucket serves with it.

| Item | Pin | Repeated in |
|---|---|---|
| `thirtyfour` | 0.38.1 | the root `Cargo.toml` `[workspace.dependencies]` |
| Chrome for Testing (Chrome and chromedriver) | 156.0.8078.73 | the `chrome-version` of `.github/workflows/ci.yml`, and `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chrome-linux64.zip` SHA-256 | `c51c24f0126534aaf6d21d1f85b8e243934b3951989dbb59a1f8aab8e32b20c1` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chrome-mac-arm64.zip` SHA-256 | `a46be48ee2e33b3edc1a98a39668b9f0abb8f23f031bc4df195e2388e6de28d1` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chromedriver-linux64.zip` SHA-256 | `7b168a60f0e27f74dd13cc51a66f4f48c5046d1108280e3d77bb5af873c0c8a6` | `scripts/release/journey-tools.sh` |
| Chrome for Testing, `chromedriver-mac-arm64.zip` SHA-256 | `abb23cd13ac8d39201dd7f78a81941c4dbb6af008dc1245dc30ec04e879b3f41` | `scripts/release/journey-tools.sh` |

## Language and runtime

`rust-toolchain.toml` carries the toolchain, and the root `Cargo.toml` carries
the edition, the resolver and the MSRV. The release lane builds every
published binary on this toolchain, with no cache.

| Item | Pin | Repeated in |
|---|---|---|
| Rust toolchain | 1.99.1 | `rust-toolchain.toml` `channel` (stable) |
| Edition | 2024 | root `Cargo.toml` `[workspace.package]` `edition` |
| Cargo resolver | 3 | root `Cargo.toml` `[workspace]` `resolver` |
| MSRV | 1.98 | root `Cargo.toml` `[workspace.package]` `rust-version` |

The toolchain is the newest stable Rust release and the MSRV is the
toolchain minus one minor release, the rule every FerroHEALTH repository
follows. Both move in one pull request when a new stable release ships, and
an MSRV is only ever raised.

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
2026-10-01, and the base image again on 2026-10-22 for the `ca-certificates`
20250419+deb13u1 it ships (Debian DSA-6572-1, #1226). The two FerroEHR images
were resolved on 2026-10-22 from FerroEHR v5.3.2, the release on that same
base. FerroEHR publishes them under `ghcr.io/ferrohealth` from v4.3.3, the
release that makes AQL honour `EHR_ACCESS` on every query form (FerroEHR
#3562, #566).
`scripts/checks/versions.sh` holds every row equal to the file that repeats
it.

| Item | Pin | Repeated in |
|---|---|---|
| Container base image | `gcr.io/distroless/static-debian13:nonroot@sha256:d62067879de890a1def9da0e7766b7d20810a5e5555cef8506b752437d7a3e88` | `docker/Dockerfile` `FROM`, `docker/console/Dockerfile` `FROM` |
| FerroEHR node image | `ghcr.io/ferrohealth/ferroehr:5.5.0@sha256:e1b15849a3792481a7f16680081b1ae6a2d7160d66e5cb1657fcef5cccfed0dc` | `compose.yaml`, the `FERROEHR` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| FerroEHR node database image | `ghcr.io/ferrohealth/ferroehr-postgres:5.5.0@sha256:dfe1d5c1bb95d31d3f6ff00fec03cd76ae20d57d20e6225b31e5bc68adf7fa81` | `compose.yaml`, the `FERROEHR_POSTGRES` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| EHRbase node image | `ehrbase/ehrbase:2.36.0@sha256:c8e642264b73637e0576ec01b5c73f5dc9be6f34eb3644f0ced890c5f916640a` | the `EHRBASE` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| EHRbase node database image | `ehrbase/ehrbase-v2-postgres:16.2@sha256:abe14e8f9ba33cabc9946c6c17c5aa95b64b35387f266cd20a894149203196d7` | the `EHRBASE_POSTGRES` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| FerroPIX PIX Manager image | `ghcr.io/ferrohealth/ferropix:0.0.12@sha256:01adb1e7cc91b46409cc315162200fb99a712a9232508d5c85a228554d22d6d3` | `compose.yaml`, the `FERROPIX` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| FerroPIX database image | `postgres:18.6@sha256:edb0c08dc33befd0262ab3b5cf1c4f3f8cfd8b09e88c05b45adee29d6761ddb6` | `compose.yaml`, the `FERROPIX_POSTGRES` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| Quickstart FerroPIX seed image | `curlimages/curl:8.23.0@sha256:11e64e28c5f58c8d93ee95fcf22ac055f48b0bc440e0ce7d8013bbb5ad6aa8ac` | `compose.yaml` |
| SanteMPI PIX Manager image | `santesuite/santedb-mpi:2.5.12@sha256:608484de046a932ec2f92e9991a32507fc8ec89d53d7639cbab886a63dbf6207` | the `SANTEMPI` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| SanteMPI database image | `postgres:15.19@sha256:724292da1f2e50bdccfc3302ce75bbba7f4a6076701b588cc795fcac65683550` | the `SANTEMPI_POSTGRES` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| Reference implementation build image | `maven:3.10.0-eclipse-temurin-21@sha256:9b4877723dadf350b452dd97d9a6401e7b56f98fa9dfe420c32ad909989c7e4c` | the `MAVEN` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| Reference implementation runtime image | `eclipse-temurin:21.0.12.1_1-jre-noble@sha256:000fd431958bc81a24abe1e8e5f0f0fd3ae365a594bd50aadb20696805f9408c` | the `TEMURIN_JRE` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| Keycloak identity provider image | `quay.io/keycloak/keycloak:26.8.1@sha256:a78d6b79572168cb25886b80a8b5b8d4ed4390743e3eecb810538683e209dd6b` | the `KEYCLOAK` constant in `tools/ferrofed-testkit/src/containers/images.rs` |
| nginx reverse proxy image | `nginx:1.30.5-alpine@sha256:0985e772fb9f729e6fa0980da05fca5d9c468e870eed43071545afa9d2e27d94` | read from this row by `scripts/checks/production-guide.sh`, which runs `deploy/nginx/ferrofed.conf` in it |

The quickstart's gateway image, `ghcr.io/ferrohealth/ferrofed`, carries the
product version below as its tag default, and the guard holds the two equal.

The end-to-end lane starts the same two node images through the testkit
harness, behind the `FERROFED_E2E` gate (`docs/ci-cd.md`): each is a
`PinnedImage` constant in `tools/ferrofed-testkit/src/containers/images.rs`, and the
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

The production guide's Keycloak recipe (#724) runs against Keycloak 26.8.0,
the newest stable release, resolved on 2026-10-06 from Quay by the digest of
its image index. The Keycloak image carries no `awk`, which the recipe pipes
into, so its `kcadm.sh` commands run on the runtime image of the reference
implementation row with the admin CLI copied out of the Keycloak container.
Keycloak is published under the Apache License 2.0, and the image is public.
Quay pages its tag list, and the freshness read follows every page.

The differential run (#94) builds the Federation Tier reference
implementation from the vendored source at its pinned commit, with the Maven
manifest fetched from that commit and held to the sha256 its `PROVENANCE.md`
records, in the build image and on the runtime image above. The build image
is Maven 3.10.0 on Temurin 21, the newest `-eclipse-temurin-21` tag,
resolved on 2026-10-06 from Docker Hub by the digest of its image index;
the reference implementation builds in it. The testkit's
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
| Product version | 0.0.15 | `CITATION.cff` `version`, the root `Cargo.toml` `[workspace.package]` `version`, the `compose.yaml` gateway image tag default |

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
validates the Kubernetes package under `deploy/kubernetes/`, its base and its
example overlay as `kustomize` renders them, against the schemas of one
Kubernetes release, read from `yannh/kubernetes-json-schema` at a pinned
commit, and the ServiceMonitor and HTTPRoute schemas from
`datreeio/CRDs-catalog` at a pinned commit, so neither a new schema nor a new
release moves the result. `kustomize` is its upstream release binary, fetched
by version and checked against the SHA-256 its release publishes. `lychee`,
the offline link checker of the `site-links` job, is its upstream release
binary, fetched by version and checked against the SHA-256 its release
publishes, which the job carries beside the version. `promtool`, which the
`observability` job checks the shipped Prometheus rule file with, comes the
same way from the Prometheus release tarball of its version.
`validator_cli`, HL7's FHIR Validator, which the `fhir-validator` job runs
over the patient summary documents the tests write (#688), is the
`validator_cli.jar` asset of its `hapifhir/org.hl7.fhir.core` release,
checked against the SHA-256 the release publishes, which the job carries
beside the version. The FHIR packages it reads beyond the vendored ones are
pinned by version and SHA-256 in `scripts/checks/fhir-validator.sh`, at the
versions the vendored manifests name.

| Item | Pin | Repeated in |
|---|---|---|
| `zizmor` | 1.31.1 | `.github/workflows/ci.yml` |
| `actionlint` | 1.7.12 | `.github/workflows/ci.yml` |
| `shellcheck` | 0.11.0 | `.github/workflows/ci.yml` |
| `hadolint` | 2.15.1 | `.github/workflows/ci.yml` |
| `kubeconform` | 0.8.0 | `.github/workflows/ci.yml` |
| `kubeconform schema version` | 1.34.0 | `.github/workflows/ci.yml` |
| `kubernetes-json-schema` | `8df8a883b68a24a104b4a9e43c1288090ae60b3b` | `.github/workflows/ci.yml` |
| `CRDs-catalog` | `fd90051867733c60d32d16450556e9cd18459aef` | `.github/workflows/ci.yml` |
| `kustomize` | 5.8.3 | `.github/workflows/ci.yml` |
| `lychee` | 0.24.2 | `.github/workflows/ci.yml` |
| `promtool` | 3.15.1 | `.github/workflows/ci.yml` |
| `validator_cli` | 7.0.3 | `.github/workflows/ci.yml` |

Keep the locally installed versions on these numbers, so a finding costs a
local run rather than a CI round trip (`.claude/rules/ci-cd.md`).

### The clone detector

The clone detector of `scripts/checks/duplication.sh` (#1463) also runs on a
developer machine, so its pins live here and the guard reads them from this
table: the release, the official image of that release by digest, and the
sha256 of each release archive `scripts/checks/duplication.sh --install`
fetches into `target/tools/` for a machine with neither the binary on `PATH`
nor docker or podman. The guard refuses a binary that answers another
version. Bumping jscpd moves every row together, plus a fresh `--update` of
the count when the new release counts differently.

| Tool | Pin | Read by |
|---|---|---|
| `jscpd` | `5.4.2` (release v5.4.2 of github.com/kucherenko/jscpd, 2026-10-20) | `scripts/checks/duplication.sh` |
| `jscpd image` | `ghcr.io/kucherenko/jscpd:5.4.2@sha256:112bc771493f25ec412ebf61bef738dc1fbe80d3793090e6a5eeac827ef0ee79` | `scripts/checks/duplication.sh` (docker, podman) |
| `jscpd darwin-arm64` | `b51d22d7c7610d27d8fd022577021ff3266a10f13295a2e545a8b8e126133a88` (`jscpd-darwin-arm64.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd darwin-x64` | `ae8d24aba2719df7682b1bb2c5021ab096116b8240c45fdae88654fc1ff36b17` (`jscpd-darwin-x64.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd linux-x64` | `5f18a6b70b4210782fc3c401c780ba994a2777249cd44d97d6fa460f6fff4570` (`jscpd-linux-x64-musl.tar.gz`) | `scripts/checks/duplication.sh --install` |
| `jscpd linux-arm64` | `3635d652ae08ae10bfbc6821bfa8ac14080101f317fb5f45f4b590d5d32b9339` (`jscpd-linux-arm64-musl.tar.gz`) | `scripts/checks/duplication.sh --install` |

## Release tool pins

The release lane builds and describes every published artifact with the tools
below, each fetched by the digest-pinned `taiki-e/install-action`, which
verifies the upstream release checksum. They decide what a consumer can prove
about a binary, so a floating version here would change the contents of a
release without a reviewed change.

| Item | Pin | Repeated in |
|---|---|---|
| `cargo-auditable` | 0.7.5 | `.github/workflows/release-build.yml`, `.github/workflows/release-console.yml` |
| `cargo-cyclonedx` | 0.5.9 | `.github/workflows/release-build.yml` |
| `syft` | 1.51.1 | `.github/workflows/release-build.yml`, `.github/workflows/release-image.yml`, `.github/workflows/release-console.yml` |
| `cargo-fuzz` | 0.13.2 | `.github/workflows/fuzz.yml` (the fuzz lane, the one nightly-toolchain job; #134) |
| `trivy` | 0.73.0 | `.github/workflows/release.yml` (the vulnerability record), `.github/workflows/scan-and-tag.yml` (the image gate), `.github/workflows/image-scan.yml` (the weekly scan), each as `TRIVY_VERSION` for the digest-pinned `aquasecurity/setup-trivy` |
| `yq` | v4.53.3 | `.github/workflows/ci.yml` (the `vex` job, as `YQ_VERSION` with its SHA-256) |

`scripts/checks/versions.sh` reads every `tool:` line of the release and fuzz
workflows, and every `TRIVY_VERSION` and `YQ_VERSION`, back against these rows,
so a bump moves one row and the workflows follow it.

## GitHub Actions pins

Every `uses:` in `.github/workflows/**` is pinned to a full commit SHA with a
trailing `# vX.Y.Z` comment (`.claude/rules/ci-cd.md`). Dependabot bumps them,
and zizmor checks the form.
