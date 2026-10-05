<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Regulatory status

This page states FerroFED's intended purpose and how it is classified under
Regulation (EU) 2025/327 on the European Health Data Space (the EHDS
Regulation). The classification was decided on 2026-10-04
([#519](https://github.com/FerroHEALTH/FerroFED/issues/519)): FerroFED is
built and documented as an EHR system, and its harmonised software components
are to be delivered before the dates the Regulation applies to it.

Every quotation is from the Official Journal text (OJ L, 2025/327, 5.3.2025,
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
  write to the CDR that owns the record. What it stores is the registry of
  members, the `ehr_id` index and the stored-query definitions.
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

## What is built and what is planned

None of the harmonised software components is built yet. Two built features
relate to the essential requirements, and neither has been assessed against
them:

- Every caller is authenticated at the gateway
  ([client authentication](../operate/authentication.md)). Annex II, point
  3.1, asks for "reliable mechanisms for the identification and
  authentication of health professionals".
- Every IHE transaction the gateway makes or receives is audited to an ATNA
  Audit Record Repository ([the audit trail](../operate/audit.md)). That
  record is not the European logging component, whose content Annex II,
  point 3.2, and the implementing acts set.

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

No EU declaration of conformity has been drawn up and no FerroFED release
carries the CE marking.

## Not legal advice

This page is the manufacturer's reading of the Regulation, written to plan
the engineering work. It is not legal advice, and it does not decide any
deployment's obligations. A deployment's counsel should confirm at least:

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
   26(2), and whether making the source available is placing on the market
   (Art 2(1)(d) takes that definition from Regulation (EU) 2019/1020).
6. The dates for the Chapter III articles Art 105 does not name, read
   together with Art 26(1).
7. The national rules that apply beside the Regulation: national
   requirements for EHR systems (Art 42) and the Member State's arrangements
   for connecting healthcare providers to its national contact point (Art
   23(5)).
