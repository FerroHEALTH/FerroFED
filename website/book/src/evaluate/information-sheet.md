<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Information sheet

Regulation (EU) 2025/327 on the European Health Data Space (the EHDS
Regulation) has every EHR system "accompanied by an information sheet that
includes concise, complete, correct and clear information that is relevant,
accessible and comprehensible to professional users" (Art 38(1)), free of
charge for the user (Art 30(1)(d)). This page is that sheet for FerroFED.
Each section answers one point of Art 38(2). The
[instructions for use](../operate/instructions-for-use.md) are the other
document Art 30(1)(d) asks for.

Art 38(3) lets a manufacturer enter the same information into the EU
database of Art 49 in place of supplying the sheet. The data that database
holds is set by a delegated act under Art 49(4) that is not adopted, so the
sheet is supplied here. Every quotation is from the Official Journal text,
vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).

## (a) The manufacturer

Art 38(2)(a): "the identity, registered trade name or registered trademark,
and contact details of the manufacturer and, where applicable, of its
authorised representative".

| | |
|---|---|
| Manufacturer | Cadasto B.V. |
| Postal address | Comeniusstraat 2d, 1817 MS Alkmaar, The Netherlands |
| Single point of contact | [info@cadasto.com](mailto:info@cadasto.com) |
| Website | <https://www.cadasto.com/contact/> |
| Authorised representative | none: Art 31(1) asks for one only of a manufacturer "established outside of the Union" |

The running gateway names the same details in `GET {base}/`,
`OPTIONS {base}/`, its startup banner and `ferrofed --version`
([Complaints and incidents](post-market.md#the-manufacturer)).

## (b) Name, version and date of release

Art 38(2)(b): "the name and version of the EHR system and date of its
release".

The EHR system is **FerroFED**. This sheet describes FerroFED 0.0.9,
released on 2026-10-05. It ships as these forms:

- the gateway image `ghcr.io/ferrohealth/ferrofed`, for Linux on `x86_64`
  and `aarch64`;
- the gateway binary `ferrofed`, for Linux on `x86_64` and `aarch64`, on
  glibc and on musl;
- the operator console image `ghcr.io/ferrohealth/ferrofed-viewer`;
- the source code, under the Business Source License 1.1
  ([Licensing](licensing.md)).

Each release lists its changes in the
[changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md),
and `ferrofed --version` prints the version a binary is. The book on the
main branch also documents changes merged since the release above; the
changelog's `[Unreleased]` section lists them, and the next release carries
them. From FerroFED 0.0.10 on, the sheet of a release is this page at that
release's tag.

| Release | Date |
|---|---|
| 0.0.9 | 2026-10-05 |
| 0.0.8 | 2026-10-04 |
| 0.0.7 | 2026-10-03 |
| 0.0.6 | 2026-10-03 |
| 0.0.3 | 2026-10-02 |
| 0.0.1 | 2026-10-01 |
| 0.0.1-rc.1, a pre-release | 2026-10-01 |

## (c) Intended purpose

Art 38(2)(c): "the intended purpose of the EHR system".

FerroFED is a federation gateway intended by its manufacturer to be used by
healthcare providers when providing patient care. A clinician's application
sends it an ordinary AQL query about a patient. FerroFED resolves the
patient to a record at each openEHR clinical data repository (CDR) of a
federation, asks each CDR for its part of the answer, and returns one answer
that names the CDR each row came from. It routes the follow-up reads and
writes of that answer to the CDR that holds the record. It holds no clinical
data of its own.

The users are healthcare providers and their health professionals, through
the client applications they use in patient care, and the operator who
configures a deployment. FerroFED offers patients no access service of its
own. The full statement, and the classification as an EHR system under Art
2(2)(k), are on [Regulatory status](regulatory-status.md#intended-purpose).

## (d) The categories of data

Art 38(2)(d): "the categories of electronic health data that the EHR system
has been designed to process".

FerroFED selects no data by category: a query reaches whatever the member
CDRs hold. It is designed to intermediate personal electronic health data of
all six priority categories of Art 14(1):

| Art 14(1) | Category | Code in the access record |
|---|---|---|
| (a) | patient summaries | `Patient-Summaries` |
| (b) | electronic prescriptions | `Electronic-Prescriptions` |
| (c) | electronic dispensations | `Electronic-Dispensations` |
| (d) | medical imaging studies and related imaging reports | `Medical-Imaging` |
| (e) | medical test results, including laboratory and other diagnostic results and related reports | `Laboratory-Reports` |
| (f) | discharge reports | `Discharge-Reports` |

The codes are those of HL7 Europe's `EEHRxFDocumentPriorityCategoryCS`
(`hl7.fhir.eu.health-data-api` 1.0.0-ballot), and the record writes each
with that system. The categories a deployment declares under national law
are recorded as well, each in the system its Member State defines
([Categories](../operate/audit.md#categories)).

## (e) Standards, formats and specifications

Art 38(2)(e): "the standards, formats and specifications supported by the
EHR system and versions of those standards, formats and specifications".

`scripts/checks/versions.sh` holds every row of this table to the pin
matrix,
[`docs/VERSIONS.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/VERSIONS.md),
so a version that moves there fails CI until this sheet moves with it. Each
Item names its row in the matrix exactly.

| Item | Pin | What FerroFED does with it |
|---|---|---|
| Product version | 0.0.9 | the FerroFED version this sheet describes |
| Federation Tier with AQL | 0.9.0 | the federation it implements: the façade, the rewrite per node, the merged answer with `meta.federation`, follow-up routing; a release candidate |
| openEHR ITS-REST | 1.1.0 | the API a client calls and the API it calls at each member |
| openEHR AQL | 1.1.0 | the query language a client sends and each member answers |
| openEHR Reference Model specification source | tag Release-1.1.0 | the identifiers and versioning model routing reads, RM 1.1.0 |
| IHE PIXm FHIR package | `ihe.iti.pixm`, version 3.1.0 | identity resolution, ITI-83 |
| IHE PDQm FHIR package | `ihe.iti.pdqm`, version 3.2.0 | the demographics step ahead of resolution, ITI-78 and ITI-119 |
| IHE PMIR FHIR package | `ihe.iti.pmir`, version 1.6.0 | the identity feed, ITI-93 and ITI-94 |
| IHE mCSD FHIR package | `ihe.iti.mcsd`, version 4.0.0 | the registry read from a care services directory, ITI-90 and ITI-91 |
| IHE IUA supplement | tag 2.5 | the claims of a caller's access token, ITI-71 and ITI-72 |
| IHE ITI-20 Record Audit Event | Revision 20.2 | the audit trail sent to an Audit Record Repository |
| IHE RESTful ATNA supplement | Rev. 3.6 | the FHIR Feed of ITI-20 the audit and access records travel by |
| IHE BALP FHIR package | `ihe.iti.balp`, version 1.1.4 | the `AuditEvent` patterns of the audit trail and of the access record of Annex II, point 3.2 |
| Netherlands Generic Functions IG source | tag v0.3.0, version 0.3.0 | the Dutch binding: NVI localization, Mitz consent, LRZa addressing, Annex B of the Federation Tier |
| Nuts specifications | commit 7c0de53 | the Nuts grant of the Dutch binding, RFC003 and RFC021 |

Beside them:

- **IHE XCPD** (ITI-55), IHE ITI Technical Framework Volume 2 Revision 20.1,
  for localizing an undirected query.
- **FHIR** R4 (4.0.1), the version every IHE and Dutch profile above
  is written for.
- **OAuth 2.0 and its extensions**, by RFC: 6749, 7523 (client assertions),
  7662 (introspection), 8414 (server metadata), 8693 (token exchange), 8705
  (mutual TLS), 9068 (access tokens), 9396 (authorization details) and 9449
  (DPoP); and the OpenID FAPI 2.0 Security Profile, Final, for the FAPI 2.0
  grant of the Dutch binding.
- **SMART on openEHR**, the scope grammar of ITS-REST 1.1.0, for the scopes
  each route requires.

The European electronic health record exchange format of Art 15 is not
supported by this release. Its implementing act is not adopted, the
interoperability component that will produce the format is a library the
gateway does not serve, and no receive path exists
([Regulatory status](regulatory-status.md#what-is-built-and-what-is-planned)).
The instructions for use list this and the other
[limitations](../operate/instructions-for-use.md#limitations).

## Support period

Art 38(2) asks for no support period. Regulation (EU) 2024/2847, the Cyber
Resilience Act, does, and it reaches every FerroFED release as a product
Cadasto B.V. places on the market
([Regulatory status](regulatory-status.md#the-cyber-resilience-act)). Its
Art 13(19) has "the end date of the support period ..., including at least
the month and the year", specified "at the time of purchase", and its Annex
II, point 7, puts "the end-date of the support period" in the information
to the user.

The manufacturer gives every release placed on the market from 11 December
2027 a support period of at least five years, the minimum of Art 13(8),
with its end date published.
[`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)
says which releases receive security fixes. Stating the support period and
the end date of each release there, and on this sheet, is planned
([#763](https://github.com/FerroHEALTH/FerroFED/issues/763)).

## Accessible formats

Recital 37 asks for the sheet and the instructions "including in accessible
formats for persons with disabilities". This sheet is HTML text with headings
and tables with header rows; no information on it is carried by an image or
by colour alone. Its source is plain Markdown in the repository,
[`website/book/src/evaluate/information-sheet.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/website/book/src/evaluate/information-sheet.md),
which a screen reader or a braille display reads as text. The book's print
page renders every page as one document for printing or saving. Ask the
single point of contact above for another format.

## How the sheet is kept

Every release cut updates this sheet in its version-bump pull request
([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
Before the tag): the version, the date of release, the release table and the
standards. `scripts/checks/versions.sh` fails while the product version or a
pin here differs from the pin matrix, or while the date of release differs
from the release's heading in `CHANGELOG.md`.
