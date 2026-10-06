<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Regulatory status

This page states FerroFED's intended purpose and how it is classified under
Regulation (EU) 2025/327 on the European Health Data Space (the EHDS
Regulation). The classification was decided on 2026-10-04
([#519](https://github.com/FerroHEALTH/FerroFED/issues/519)): FerroFED is
built and documented as an EHR system, and its harmonised software components
are to be delivered before the dates the Regulation applies to it. The page
also states FerroFED's position under the Cyber Resilience Act, decided on
2026-10-06 ([#654](https://github.com/FerroHEALTH/FerroFED/issues/654)):
each release is a product that Cadasto B.V. places on the market
([The Cyber Resilience Act](#the-cyber-resilience-act)).

Every quotation from the EHDS Regulation is from the Official Journal text
(OJ L, 2025/327, 5.3.2025,
<http://data.europa.eu/eli/reg/2025/327/oj>), vendored verbatim at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice; [the last section](#not-legal-advice) names
what a deployment's counsel should confirm.

## Intended purpose

FerroFED is a federation gateway intended by its manufacturer to be used by
healthcare providers when providing patient care. A clinician's application
sends it an ordinary AQL query about a patient. FerroFED resolves the patient
to a record at each openEHR clinical data repository (CDR) of a federation,
asks each CDR for its part of the answer, and returns one answer that names
the CDR each row came from. It routes the follow-up reads and writes of that
answer to the CDR that holds the record.

- **Users.** Healthcare providers and their health professionals, through the
  client applications they use in patient care. Every caller is
  authenticated at the gateway
  ([client authentication](../operate/authentication.md)). The operator of a
  deployment configures the member CDRs and the identity services. FerroFED
  offers patients no access service of their own, the "electronic health
  data access service" of Art 2(2)(h).
- **Data.** Personal electronic health data, "data concerning health and
  genetic data, processed in an electronic form" (Art 2(2)(a)), held in the
  member CDRs as openEHR records. FerroFED holds no clinical data of its own:
  it passes each CDR's answer through, merges the answers, and forwards a
  write to the CDR that owns the record. What it keeps is the registry of
  members, routing state in memory, the stored-query definitions and the
  IHE audit records waiting for delivery;
  [Data protection](data-protection.md#processing-inventory) lists each.
- **Priority categories.** Art 14(1) lists six: "(a) patient summaries;
  (b) electronic prescriptions; (c) electronic dispensations; (d) medical
  imaging studies and related imaging reports; (e) medical test results,
  including laboratory and other diagnostic results and related reports; and
  (f) discharge reports", with their main characteristics in Annex I.
  FerroFED does not select data by category: a query reaches whatever the
  member CDRs hold. Its intended purpose therefore covers every priority
  category a member CDR holds, all six of Art 14(1)(a) to (f).

## Classification: an EHR system

Art 2(2)(k) defines an EHR system as:

> any system whereby the software, or a combination of the hardware and the
> software of that system, allows personal electronic health data that belong
> to the priority categories of personal electronic health data established
> under this Regulation to be stored, intermediated, exported, imported,
> converted, edited or viewed, and intended by the manufacturer to be used by
> healthcare providers when providing patient care or by patients when
> accessing their electronic health data

FerroFED meets each part of that definition:

- **It is software that allows priority-category data to be intermediated.**
  Answering a clinician's query from many CDRs, and routing a read or a write
  to the CDR that owns the record, is intermediation. The Regulation expects
  an EHR system that intermediates without storing: Annex II, point 2.1,
  begins "Where an EHR system is designed to store or intermediate personal
  electronic health data".
- **The data belong to the priority categories.** Data of each of the six
  categories can be held in an openEHR CDR, and the intended purpose covers
  every category the member CDRs hold.
- **The manufacturer intends it for healthcare providers providing patient
  care.** That is the intended purpose above. Defining the purpose to exclude
  the priority categories would cut FerroFED off from its own use case, so
  the purpose includes them.

### The general-purpose exclusion

Art 25(2) reads: "This Chapter shall not apply to general purpose software
used in a healthcare environment." Recital 38 gives the examples it has in
mind: "text-processing software used for writing reports that would then
become part of written electronic health records, general-purpose middleware,
or database management software that is used as part of data storage
solutions."

FerroFED is middleware, so the recital's second example needs an answer. It
is built for one clinical purpose. It speaks openEHR ITS-REST and AQL to
openEHR CDRs, it resolves the patient through an identity cross-reference,
it rewrites the patient's query for
each CDR, and its intended purpose names healthcare providers providing
patient care. The same recital contrasts general-purpose software with "EHR
systems specifically intended by the manufacturer to be used for processing
one or more specific categories of electronic health data", and FerroFED is
the second kind.

## The obligations and their dates

Art 25(1) sets the core obligation: "EHR systems shall include a European
interoperability software component for EHR systems and a European logging
software component for EHR systems (the 'harmonised software components of
EHR systems')". The interoperability component "provides and receives
personal electronic health data under a priority category for primary use
... in the European electronic health record exchange format" (Art 2(2)(n)).
The logging component "provides logging information related to access by
health professionals or other individuals to priority categories of personal
electronic health data ... in the format defined in point 3.2. of Annex II"
(Art 2(2)(o)).

Art 105 sets the dates. The Regulation "shall apply from 26 March 2027", and
Articles 25, 26, 27, 47, 48 and 49 apply later: "from 26 March 2029 to ...
EHR systems intended by the manufacturer to process" patient summaries,
prescriptions or dispensations, and "from 26 March 2031" to those intended
to process imaging, test results or discharge reports. FerroFED's intended
purpose covers all six categories, so both dates apply to it: 26 March 2029
for patient summaries, prescriptions and dispensations (Art 14(1)(a) to (c)),
and 26 March 2031 for imaging, test results and discharge reports (Art
14(1)(d) to (f)).

| Obligation | Article | Applies to FerroFED from |
|---|---|---|
| Include the two harmonised software components | Art 25(1) | 26 March 2029 for (a) to (c); 26 March 2031 for (d) to (f) |
| Be placed on the market or put into service "only if they comply with the provisions laid down in this Chapter" | Art 26(1) | 26 March 2029 for (a) to (c); 26 March 2031 for (d) to (f) |
| Registration in the EU database before placing on the market or putting into service, with the results of the testing environment | Art 49(2), Art 30(1)(h) | 26 March 2029 for (a) to (c); 26 March 2031 for (d) to (f) |
| Conformity of the harmonised components with the essential requirements of Annex II and the common specifications | Art 30(1)(a), Art 36 | 26 March 2027 (see below) |
| Technical documentation with at least the elements of Annex III and the results of the European digital testing environment | Art 37, Art 40(3), Art 30(1)(c) | 26 March 2027 (see below) |
| An information sheet naming the intended purpose and the data categories, and instructions for use | Art 38, Art 30(1)(d) | 26 March 2027 (see below) |
| The EU declaration of conformity, with the content of Annex IV | Art 39, Art 30(1)(e) | 26 March 2027 (see below) |
| The CE marking of conformity, affixed "before placing the EHR system on the market" | Art 41, Art 30(1)(f) | 26 March 2027 (see below) |
| Corrective action, complaint channels, and the registers of complaints and non-conforming systems | Art 30(1)(i) to (o) | 26 March 2027 (see below) |
| Keeping the technical documentation and the declaration "for 10 years" | Art 30(3) | 26 March 2027 (see below) |
| Reporting a serious incident "not later than three days" after becoming aware of it | Art 44(7) | 26 March 2027 (see below) |
| No claim that ascribes "functions and properties to the EHR system which it does not have" | Art 28 | 26 March 2027 (see below) |

Art 105 names no later date for Articles 28, 30, 36 to 41 and 44, so the
general date of 26 March 2027 applies to them. Each of them attaches to an
EHR system placed on the market or put into service, and Art 26(1), which
makes compliance the condition for either, applies to FerroFED from
26 March 2029 for categories (a) to (c) and from 26 March 2031 for (d) to
(f). That reading is one counsel should confirm.

The technical content comes from implementing acts the Commission has not
yet adopted. The exchange format (Art 15(1)) and the common specifications
for the essential requirements (Art 36(1)) are both due "By 26 March 2027",
and Art 105 applies them from the same 2029 and 2031 dates by category.
[#526](https://github.com/FerroHEALTH/FerroFED/issues/526) tracks them.

Two implementing acts are adopted. Both bind the national contact points
and the cross-border authentication chain, and neither sets the content of
the harmonised components:

| Act | Adopted under | Applies from |
|---|---|---|
| Commission Implementing Regulation (EU) 2026/2083 of 18 September 2026 on MyHealth@EU (OJ L, 21.9.2026) | Art 23(4) and (8) | 26 March 2027 |
| Commission Implementing Regulation (EU) 2026/2099 of 21 September 2026 on the cross-border identification and authentication mechanism (OJ L, 22.9.2026) | Art 16(2) | 26 March 2027, and its Art 3(3) and 5(2) from 26 March 2029 |

Implementing Regulation 2026/2083 has the national contact points exchange
data "in accordance with the requirements catalogue" and "the technical
specifications" the MyHealth@EU steering group approves (its Art 4(1)).
Implementing Regulation 2026/2099 has the entity a Member State lists
authenticate a health professional at eIDAS assurance level "substantial",
and at level "high" from 26 March 2032 (its Art 6(3)). Both are vendored
beside the Regulation, with Commission Recommendation (EU) 2019/243 on a
European Electronic Health Record exchange format, which recital 26 names as
the format's foundation. A weekly check reads EUR-Lex and the Commission's
"Have your say" register for the acts still pending, and files an issue when
one is adopted.

## A deployment a health institution runs for itself

Art 26(2) reads: "EHR systems that are manufactured and used within health
institutions established in the Union, as well as EHR systems offered as a
service ... to a natural or legal person established in the Union, shall be
considered as having been put into service." Putting into service is "the
first use, for its intended purpose, in the Union of an EHR system covered
by this Regulation" (Art 2(2)(l)). Recital 44 explains the intent: a
healthcare provider "developing and using an EHR system 'in-house'" should
"comply with all requirements applicable to manufacturers", after "an
extended transitional period". Art 105 sets that period: "Chapter III shall
apply to EHR systems put into service in the Union referred to in Article
26(2) from 26 March 2031."

For FerroFED this means:

- **A health institution that builds and runs its own FerroFED** may be the
  one that manufactured and used it within the institution. It would then
  carry the manufacturer's obligations for that system, from 26 March 2031.
- **A deployment that changes FerroFED** can make its operator a
  manufacturer whatever the answer to the first point. Art 34 treats a user
  as a manufacturer where it modifies an EHR system "in such a way that
  conformity with the applicable requirements might be affected", or "in
  such a way that it leads to changes in the intended purpose declared by the
  manufacturer". FerroFED's source is open to modification
  ([licensing](licensing.md)), so this applies to any deployment that runs
  changed code.
- **An operator that hosts FerroFED for other organisations** offers an EHR
  system as a service, which Art 26(2) also counts as putting into service.
  Such a hosted service also needs a commercial licence
  ([licensing](licensing.md)).

FerroFED plans its harmonised components for before the dates that apply to
each category (26 March 2029 for (a) to (c)) whichever reading holds, so a
deployment can use them under either. Who carries the
manufacturer's obligations for a given deployment is a question for that
deployment's counsel.

## Cross-border access

Art 11(2): where the Member State of affiliation and the Member State of
treatment differ, "cross-border access to the personal electronic health data
of the natural person under treatment shall be provided through the
cross-border infrastructure referred to in Article 23". That infrastructure
is MyHealth@EU and each Member State's national contact point for digital
health, "an organisational and technical gateway for the provision of
services linked to the cross-border exchange of personal electronic health
data" (Art 23(2)). The exchange between contact points "shall be based on
the European electronic health record exchange format" (Art 23(3)), and
"Member States shall ensure the connection of all healthcare providers to
their national contact points for digital health" (Art 23(5)). Art 105
applies Art 23(2) to (6) from the same 2029 and 2031 dates.

FerroFED does not reach another Member State itself and does not build the
contact point's cross-border protocols. Its route to cross-border care is the
national one: a national contact point asks its national infrastructure for
a patient's documents, and FerroFED can answer that request from federated
openEHR data through its interoperability component. That work is planned in
[#524](https://github.com/FerroHEALTH/FerroFED/issues/524).

The Federation Tier specification does not say whether a federation whose
member CDRs sit in different Member States is in scope. FerroFED neither
detects nor refuses one; whether such a deployment is lawful, and for which
data, is the deployment's to answer.

## The Cyber Resilience Act

Regulation (EU) 2024/2847, the Cyber Resilience Act (CRA), "applies to
products with digital elements made available on the market" whose intended
purpose includes a data connection to a device or network (Art 2(1)).
FerroFED is software whose purpose is that connection. The quotations in this
section are from the Official Journal text (OJ L, 2024/2847, 20.11.2024),
vendored with Regulation (EU) 2019/1020 at
[`docs/specs/eu-cra/`](https://github.com/FerroHEALTH/FerroFED/tree/main/docs/specs/eu-cra),
and Art 104 of the EHDS Regulation amends three of its articles.

### Why the CRA reaches a release

- **A commercial activity.** Art 3(22) defines making available on the
  market as "the supply of a product with digital elements for distribution
  or use on the Union market in the course of a commercial activity, whether
  in return for payment or free of charge". Recital 15 names "an intention
  to monetise" as one mark of a commercial activity. FerroFED's licence is
  free for non-production use and for non-commercial production use, and
  sells a licence for any other production use ([Licensing](licensing.md)).
  A release is therefore supplied in the course of a commercial activity,
  a free download included.
- **Not free and open-source software.** Art 3(48) asks for "a free and
  open-source licence which provides for all rights to make it freely
  accessible, usable, modifiable and redistributable". The Business Source
  License 1.1 withholds production use outside its Additional Use Grant, so
  FerroFED is not free and open-source software within the CRA, and the
  open-source software steward of Art 3(14) and Art 24 does not apply to it.
- **The same test under the EHDS.** EHDS Art 2(1)(d) takes "placing on the
  market", "manufacturer" and "economic operator" from Regulation (EU)
  2019/1020, whose Art 3(1) has the same words: "in the course of a
  commercial activity, whether in return for payment or free of charge".

### What the manufacturer does

- **One product per release.** Every tagged release, meaning the source tag,
  the binary tarballs, the gateway image and the console image of one
  version, is one product with digital elements. Cadasto B.V. places it on
  the market as its manufacturer (Art 3(13)). The `main` branch is
  development code and is not supplied for use.
- **Reporting applies now.** Art 14 has the manufacturer notify an actively
  exploited vulnerability and a severe incident, with an early warning
  within 24 hours. Art 71(2) applies Art 14 "from 11 September 2026", and
  Art 69(3) applies it to "all products with digital elements ... placed on
  the market before 11 December 2027", the v0.0.x releases among them. The
  written reporting procedure is planned
  ([#762](https://github.com/FerroHEALTH/FerroFED/issues/762)); until it
  lands, report a vulnerability as
  [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#reporting-a-vulnerability)
  says.
- **The rest from 11 December 2027.** Art 71(2) applies the rest of the CRA
  from 11 December 2027. Each release placed on the market from that date
  meets the essential requirements of Annex I and has the support period
  below. Art 69(2) holds a product placed before that date to those
  requirements only once it is "subject to a substantial modification".
- **One file, one declaration, one CE marking.** EHDS Art 104 inserts CRA
  Art 32(5a): the manufacturer of an EHR system shows conformity with CRA
  Annex I "using the relevant conformity assessment procedure provided for
  in Chapter III" of the EHDS Regulation. It also replaces CRA Art 31(3), so
  "a single set of technical documentation shall be drawn up" for both acts.
  CRA Art 28(3) and EHDS Art 39(2) each require a single EU declaration of
  conformity. A release placed from 11 December 2027 therefore carries one
  technical documentation set, one declaration and one CE marking for both
  acts, all planned under
  [#525](https://github.com/FerroHEALTH/FerroFED/issues/525).
- **The economic operators supplied.** EHDS Art 35 has an economic operator
  identify, for 10 years, "any economic operator to which they have
  supplied an EHR system". CRA Art 23(1)(b) asks the same "where available";
  EHDS Art 35 has no such qualifier. The manufacturer's answer is a
  register, kept for 10 years, of every economic operator it supplies under
  a contract: commercial licensees, resellers, integrators and hosts. The
  public release is not gated, and it is supplied for non-production use
  and for non-commercial production use under the licence. If counsel holds
  that an anonymous production user is an economic operator, for example
  one that puts FerroFED into service under EHDS Art 26(2), downloads move
  behind a registration.

A FerroFED that someone hosts as a service, or that a health institution
builds and runs for itself, is outside the CRA for that service: EHDS
recital 112 says that "EHR systems offered through the SaaS licensing and
delivery model do not fall within the scope of" the CRA, nor do "EHR systems
that are developed and used in-house". EHDS Art 26(2) still counts both as
put into service ([A deployment a health institution runs for
itself](#a-deployment-a-health-institution-runs-for-itself)). The release
that was downloaded to run it stays a CRA product.

### The support period

Art 13(8) has the manufacturer handle a product's vulnerabilities for its
support period, and "the support period shall be at least five years". Art
13(19) has the end date of that period, "including at least the month and
the year", specified "at the time of purchase", and Annex II, point 7, puts
"the end-date of the support period" in the information to the user.
The manufacturer gives every release placed on the market from 11 December
2027 a support period of at least five years, with its end date published.
[`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)
says which releases receive security fixes; stating the support period and
the end date of each release there is planned
([#763](https://github.com/FerroHEALTH/FerroFED/issues/763)), and so is a
notice to users when a release reaches its end date
([#782](https://github.com/FerroHEALTH/FerroFED/issues/782)).

### Already in place

None of these is a conformity claim; each answers a CRA requirement in
part:

- Each release attaches a CycloneDX SBOM of the source dependency graph and
  an SPDX SBOM of the shipped binary, with signed provenance (Annex I, Part
  II, points 1 and 7).
- Vulnerabilities are reported privately, with an acknowledgement within
  seven days (Annex I, Part II, points 5 and 6; Art 13(17)).
- The running system names the manufacturer, its postal address and its
  single point of contact (Art 13(16)).
- A published release cannot be changed or deleted, so every update stays
  available (Art 13(9)).

## What is built and what is planned

Neither harmonised software component is complete yet. Three built features
relate to the essential requirements, and none has been assessed against
them:

- Every caller is authenticated at the gateway
  ([client authentication](../operate/authentication.md)). Annex II, point
  3.1, asks for "reliable mechanisms for the identification and
  authentication of health professionals". For patient data the gateway
  requires a natural person behind the token, or a client acting for the
  professional the token names, and the assurance level the deployment sets
  per issuer
  ([Professionals and assurance](../operate/authentication.md#professionals-and-assurance)).
- Every access to patient data the gateway intermediates is recorded with
  the verified caller, the patient, each endpoint asked and the categories
  of the data, in the record Annex II, point 3.2, sets for the European
  logging component ([the access log](../operate/audit.md#the-access-log)).
  The library that builds it is `crates/ehds-logging`. The records are
  reviewed at the Audit Record Repository with ITI-81, the connection of
  external software point 3.3 admits, and each states how long it is kept
  by its origins and categories, never under three years (point 3.4, Art
  9(2); [Reading the log](../operate/audit.md#reading-the-log)). Finding
  every access to one person's data by that search
  ([#796](https://github.com/FerroHEALTH/FerroFED/issues/796)) and access
  rights by origin and category
  ([#797](https://github.com/FerroHEALTH/FerroFED/issues/797)) are planned.
- The European interoperability component has a library, `crates/eehrxf`:
  the EHDS dataset model, read from the Xt-EHR logical models, and the
  mapping of an openEHR composition to a FHIR R4 `Bundle` through the
  mapping files you supply. It also reads a received document, checks it
  against its profiles and maps it into one openEHR composition that keeps
  the original document ([Receiving a document](receiving-documents.md)).
  The gateway holds the stored queries that select each patient summary
  section's compositions
  ([the section queries](#the-patient-summarys-section-queries)), and serves
  the patient summary built from them on a FHIR R4 face of its own, the
  interface of Annex II, point 2.1
  ([The patient summary over FHIR](../integrate/patient-summary.md)). The
  document list on that face is planned
  ([#810](https://github.com/FerroHEALTH/FerroFED/issues/810)).

Five of the manufacturer's obligations have a first answer:

- The running system names its manufacturer, with the postal address and the
  single point of contact (Art 30(1)(g)): `GET {base}/`, `OPTIONS {base}/`,
  the startup banner, `ferrofed --version`, the operator console's footer and
  the image labels.
- The complaint channels, the two registers, corrective action and the
  serious-incident report (Art 30(1)(i) to (o), Art 44(7)) are written
  procedures ([Complaints and incidents](post-market.md)).
- Every public text is reviewed against Art 28 at each release, and the
  completeness default, the budgets and the consent pre-filter are assessed
  against Annex II, point 2.5 ([Claims review](claims-review.md)).
- The [information sheet](information-sheet.md) gives every item of Art
  38(2), held to the pin matrix by a guard, and the
  [instructions for use](../operate/instructions-for-use.md) cover
  installation, the configuration of a deployment in the EU, maintenance
  and the limitations (Art 30(1)(d), Art 32(2)(d), Annex III, point 1(j)).
- The [clinical safety risk file](clinical-safety.md) names each hazard of
  the two harmonised components with its control and the test that holds
  it, for Annex II, point 1.1.

The planned work is filed under
[#519](https://github.com/FerroHEALTH/FerroFED/issues/519), in the v0.0.10
milestone, which is due before the Regulation applies on 26 March 2027:

- [#521](https://github.com/FerroHEALTH/FerroFED/issues/521): the European
  logging software component.
- [#522](https://github.com/FerroHEALTH/FerroFED/issues/522): the European
  interoperability software component for the patient summary, built from
  federated openEHR data.
- [#523](https://github.com/FerroHEALTH/FerroFED/issues/523): the
  interoperability component for electronic prescriptions and dispensations.
- [#524](https://github.com/FerroHEALTH/FerroFED/issues/524): serving a
  national contact point for digital health.
- [#525](https://github.com/FerroHEALTH/FerroFED/issues/525): the Annex II
  checklist, the technical documentation, the EU declaration of conformity,
  the CE marking and the registration.
- [#526](https://github.com/FerroHEALTH/FerroFED/issues/526): tracking and
  vendoring the implementing acts the components depend on.
- [#762](https://github.com/FerroHEALTH/FerroFED/issues/762): the written
  procedure for the reports of CRA Art 14.
- [#763](https://github.com/FerroHEALTH/FerroFED/issues/763): the support
  period and its end date for each release.

No EU declaration of conformity has been drawn up and no FerroFED release
carries the CE marking.

### The patient summary crosswalk

The eHealth Network *Guidelines on Patient Summary* (Release 3.4, §4), the
Xt-EHR `EHDSPatientSummary` logical model and the HL7 Europe Patient Summary
composition (`hl7.fhir.eu.eps` 1.0.0-ballot) describe one dataset three
ways. `eehrxf` holds the crosswalk between them as data, keyed on the Xt-EHR
element path. Each row names:

- the eHealth Network element ids, such as `A.2.1.1` for allergies;
- the obligation the Xt-EHR obligations profile puts on a producer;
- the element or slice of the EPS composition that carries it, such as
  `Composition.section:sectionAllergies.entry:allergyOrIntolerance`;
- the profiles a FHIRconnect context may map to in order to feed it. The
  header, each section's narrative, its empty reason and its note have none,
  because the document is assembled around them.

Every section of the model has a row, and so does every header element a
producer must be able to populate. A test fails when an element a producer
`SHALL:able-to-populate` has no row, when a row names a slice the pinned EPS
profile lacks, or when one of the five required EPS sections (problems,
allergies, medications, procedures, devices) is uncovered. Two EPS sections
have no counterpart in the model and no row: vital signs and the general
patient history. The presented form, a rendering of the whole summary, has
no element in the EPS composition.

Two elements are open in the [clinical safety risk file](clinical-safety.md)
(hazard I-03), because no openEHR content is named to feed them yet:

- the medical alert (A.2.1.2), which a producer must be able to populate;
- the functional status (A.2.3.4).

### The patient summary's section queries

Each section openEHR content feeds has a stored query the gateway holds
itself, read-only, under the namespace `eu.ferrofed.eehrxf` at the one
version `1.0.0` (§12.7, N44). The crate `app/ferrofed-eehrxf`, the
federation half of the interoperability component, builds them; `eehrxf`
links nothing of the gateway. The queries are:

- `eu.ferrofed.eehrxf::patient-summary-allergies-and-intolerances`
- `eu.ferrofed.eehrxf::patient-summary-problems`
- `eu.ferrofed.eehrxf::patient-summary-medication-summary`
- `eu.ferrofed.eehrxf::patient-summary-medical-devices-and-implants`
- `eu.ferrofed.eehrxf::patient-summary-procedures`
- `eu.ferrofed.eehrxf::patient-summary-immunisations`
- `eu.ferrofed.eehrxf::patient-summary-social-history`
- `eu.ferrofed.eehrxf::patient-summary-pregnancy-history`
- `eu.ferrofed.eehrxf::patient-summary-advance-directives`
- `eu.ferrofed.eehrxf::patient-summary-observation-results`
- `eu.ferrofed.eehrxf::patient-summary-care-plans`

Each one selects, from every member, the whole compositions that contain one
of the archetypes the openEHR International Patient Summary template names
for its section, each composition once with its uid and its template id,
because a FHIRconnect mapping is chosen per template. The patient is the
`$patient` and `$namespace` parameters alone, so the query passes the
stored-query admission and the gateway's rewrite asks each member by its own
`ehr_id`. The section-to-archetype table, and the four sections no query
feeds, are in the [clinical safety risk file](clinical-safety.md#the-section-queries).
Vital signs, which the EPS composition carries in a section of its own, feed
the Xt-EHR observation results. Running a query by name and the reserved
namespace are described under
[the gateway's own queries](../integrate/stored-queries.md#the-gateways-own-queries).
The FHIR face of Annex II 2.1 assembles the patient summary from their
answers ([The patient summary over FHIR](../integrate/patient-summary.md),
[#809](https://github.com/FerroHEALTH/FerroFED/issues/809)); the document
list is planned ([#810](https://github.com/FerroHEALTH/FerroFED/issues/810)).

## Not legal advice

This page is the manufacturer's reading of the EHDS Regulation and the CRA,
written to plan the engineering work. It is not legal advice, and it does
not decide any deployment's obligations. The manufacturer's own counsel is
asked to confirm, now, that a release is supplied in the course of a
commercial activity, that Cadasto B.V. is its manufacturer, and that CRA Art
14 applies to the v0.0.x releases; the rest of the CRA position is put to
counsel before the first release placed on the market from 11 December
2027. A deployment's counsel should confirm at least:

1. The intended-purpose statement and the classification as an EHR system
   under Art 2(2)(k), including the answer to recital 38's
   "general-purpose middleware" example.
2. That the intended purpose covers all six priority categories, given that
   FerroFED passes through whatever the member CDRs hold.
3. Whether the deployment is an EHR system "manufactured and used within
   health institutions" (Art 26(2)), and who then carries the manufacturer's
   obligations of Art 30.
4. Whether the deployment changes FerroFED in a way that makes its operator a
   manufacturer under Art 34.
5. Whether hosting FerroFED for others is offering it as a service under Art
   26(2). The manufacturer reads each tagged release as placed on the market
   by Cadasto B.V. ([The Cyber Resilience Act](#the-cyber-resilience-act);
   Art 2(1)(d) takes the definition from Regulation (EU) 2019/1020). What
   remains for a deployment is whether it is an economic operator that
   Cadasto B.V. supplied, which EHDS Art 35 has the manufacturer identify:
   2019/1020 Art 3(13) counts whoever puts a product into service under the
   applicable legislation, and EHDS Art 26(2) counts a hosted or in-house
   system as put into service.
6. The dates for the Chapter III articles Art 105 does not name, read
   together with Art 26(1).
7. The national rules that apply beside the Regulation: national
   requirements for EHR systems (Art 42) and the Member State's arrangements
   for connecting healthcare providers to its national contact point (Art
   23(5)).
