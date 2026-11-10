<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Conformity record

Art 26(1) of Regulation (EU) 2025/327 on the European Health Data Space
(the EHDS Regulation): "EHR systems shall be placed on the market or put
into service only if they comply with the provisions laid down in this
Chapter". This page is FerroFED's record of where it stands against each
obligation of Chapter III that Articles 25 to 30, 36 to 41 and 49 place on
the manufacturer, and against each item of the EU declaration of conformity
that Annex IV lists. For every one it names the evidence in this repository,
the issue that closes any gap, and the date Art 105 applies it from.

This record is no EU declaration of conformity and states no conformity. No
declaration has been issued for any release, no CE marking has been affixed,
FerroFED is not registered in the EU database, and no assessment by an
external body stands behind this record. A row marked met says that the repository holds what the
provision asks for what the release does. Issuing a declaration, affixing
the marking and registering are legal acts of the manufacturer, Cadasto
B.V., and [What the manufacturer must decide or
do](#what-the-manufacturer-must-decide-or-do) lists them.

Two other records sit beside this one. The [Annex II
checklist](annex-ii.md) holds each essential requirement, and the [Annex III
index](technical-documentation.md#the-annex-iii-index) each element of the
technical documentation; a row here points at them where a provision rests on
them. The record is kept as
[`conformance/chapter-iii.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/chapter-iii.tsv),
and `scripts/checks/chapter-iii.sh` fails a change that quotes a phrase the
provision's own text in the vendored Regulation does not hold, names a page
heading or a test that does not exist, leaves a gap without its issue, gives
a row a date Art 105 does not, or lets the tables below differ from the
file. CI runs it on every change.

Every quotation of the Regulation is from the Official Journal text,
vendored at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
This page is not legal advice.

## The dates it applies from

Art 105 sets them. The Regulation "shall apply from 26 March 2027". "Articles
25, 26, 27, 47, 48 and 49 shall apply as follows": "from 26 March 2029" to
EHR systems intended by the manufacturer to process the categories of Art
14(1), points (a), (b) and (c), and "from 26 March 2031" to those intended to
process points (d), (e) and (f). "Chapter III shall apply to EHR systems put
into service in the Union referred to in Article 26(2) from 26 March 2031."
The implementing acts of "Article 36(1) shall apply from the dates referred
to in the third paragraph" of Art 105, by category in the same way.

FerroFED's intended purpose covers all six categories of Art 14(1), so both
category dates apply to it: 26 March 2029 for patient summaries,
prescriptions and dispensations, and 26 March 2031 for medical test results,
discharge reports and imaging. The column "Applies from" gives each row its
date by that reading. Every Article Art 105 does not name applies from
26 March 2027, but each one attaches to an EHR system placed on the market
or put into service, which Art 26(1) governs from the category dates; the
[regulatory status](regulatory-status.md#the-obligations-and-their-dates)
page says why counsel should confirm that reading.

## How to read it

| Status | Meaning |
|---|---|
| met | the repository holds the record, the procedure or the code the provision asks for, for what this release does |
| open | work is still to be done in this repository or a sibling's; the issue holds it |
| awaits-act | the provision rests on an act or a facility of the Commission that does not exist yet: an implementing act, a delegated act, the common specifications, the testing environment, the declaration template or the EU database |
| manufacturer | only Cadasto B.V. can do it, as a decision or a legal act; the repository has prepared what it can |
| not-applicable | the provision does not bind FerroFED; the note says why |

A provision with more than one phrase to answer has one row per phrase. The
second table is the evidence index of the EU declaration of conformity: for
each item of Annex IV, where the declaration's generator takes the value from
([The EU declaration of
conformity](technical-documentation.md#the-eu-declaration-of-conformity)).

## The record

<!-- chapter-iii:begin -->
<!-- Rendered from conformance/chapter-iii.tsv by scripts/conformance/chapter-iii.sh --write. Do not edit between the markers. -->

| Status | Rows |
|---|---|
| met | 29 |
| open | 7 |
| awaits-act | 8 |
| manufacturer | 14 |
| not-applicable | 4 |
| all | 62 |

### The obligations

| Provision | Requirement | Applies from | Status | Evidence | Issue | Note |
|---|---|---|---|---|---|---|
| Art 25(1) | "EHR systems shall include a European interoperability software component for EHR systems and a European logging software component for EHR systems" | 26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f) | open | [Changes to the harmonised components](technical-documentation.md#changes-to-the-harmonised-components); `app/ferrofed-engine/tests/it/architecture.rs` (`the_two_harmonised_components_are_independent_of_each_other`); [What is missing](annex-ii.md#what-is-missing) | [#1794](https://github.com/FerroHEALTH/FerroFED/issues/1794), [FerroHEALTH/FerroBRIDGE#1827](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1827) | Both components ship in every release from 0.0.10. The interoperability component serves patient summaries, prescriptions, dispensations and laboratory, discharge and imaging reports, and receives no imaging report yet; the Annex II checklist lists what else is missing. |
| Art 25(2) | "This Chapter shall not apply to general purpose software used in a healthcare environment" | 26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f) | not-applicable | [The general-purpose exclusion](regulatory-status.md#the-general-purpose-exclusion) | - | The exclusion does not cover FerroFED: it is classified as an EHR system, so Chapter III binds it. |
| Art 26(1) | "EHR systems shall be placed on the market or put into service only if they comply with the provisions laid down in this Chapter" | 26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f) | open | [The obligations and their dates](regulatory-status.md#the-obligations-and-their-dates); [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) | The condition of every other row: before a release is placed on the market or put into service as an EHR system, no row of this record may stay open or wait on an act or on the manufacturer. |
| Art 26(2) | "shall be considered as having been put into service" | 26 March 2031 | not-applicable | [A deployment a health institution runs for itself](regulatory-status.md#a-deployment-a-health-institution-runs-for-itself) | - | A definition that makes a health institution that builds FerroFED for itself, or an operator that hosts it as a service, the one putting it into service; Art 105 applies Chapter III to such systems from 26 March 2031. |
| Art 28(a) | "ascribing functions and properties to the EHR system which it does not have" | 26 March 2027 | met | [How a review is done](claims-review.md#how-a-review-is-done); [The record](claims-review.md#the-record); [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | - | Every release's public texts are reviewed against Art 28 before the tag, and the findings are recorded. |
| Art 28(b) | "failing to inform the professional user of likely limitations related to interoperability or security features" | 26 March 2027 | met | [Limitations](../operate/instructions-for-use.md#limitations); [Limitations](what-ferrofed-claims.md#limitations); [The record](claims-review.md#the-record) | - | - |
| Art 28(c) | "suggesting uses for the EHR system other than those stated to form part of the intended purpose in the technical documentation" | 26 March 2027 | met | [Intended purpose](regulatory-status.md#intended-purpose); [Not claimed](what-ferrofed-claims.md#not-claimed); [The record](claims-review.md#the-record) | - | - |
| Art 30(1)(a) | "are in conformity with the essential requirements laid down in Annex II and with the common specifications in accordance with Article 36" | 26 March 2027 | open | [What is missing](annex-ii.md#what-is-missing); [`conformance/annex-ii.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/annex-ii.tsv) | [#1405](https://github.com/FerroHEALTH/FerroFED/issues/1405), [#1406](https://github.com/FerroHEALTH/FerroFED/issues/1406), [#675](https://github.com/FerroHEALTH/FerroFED/issues/675), [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) | Points of Annex II are still partial, each with its issue on the Annex II checklist, and the Art 36(1) common specifications are not adopted (Art 36(1) row). |
| Art 30(1)(b) | "are not adversely affected by other software components of the same EHR system" | 26 March 2027 | met | [Shared hazards](clinical-safety.md#shared-hazards); `app/ferrofed-engine/tests/it/architecture.rs` (`no_crate_but_the_composition_root_links_both_components`); `app/ferrofed-server/tests/it/fhir/independence.rs` (`a_summary_is_recorded_as_the_same_query_over_its_rest_is`); `app/ferrofed-server/tests/it/fhir/independence.rs` (`the_summary_is_the_same_document_whatever_the_access_log_sink_does`) | - | Hazard X-01 of the risk file, controlled at the crate and the server level. |
| Art 30(1)(c) | "draw up the technical documentation of their EHR systems in accordance with Article 37 before placing those EHR systems on the market" | 26 March 2027 | open | [The Annex III index](technical-documentation.md#the-annex-iii-index); [`conformance/technical-documentation.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/technical-documentation.tsv); [`scripts/checks/technical-documentation.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/technical-documentation.sh) | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525), [#675](https://github.com/FerroHEALTH/FerroFED/issues/675), [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The Annex III index is drawn up and held by CI; its items 3, 4 and 6 and the Art 40 result are not complete (the Art 36(1), Art 37(2) and Art 40(3) rows). |
| Art 30(1)(d) | "accompanied, free of charge for the user, by the information sheet provided for in Article 38 and clear and complete instructions for use" | 26 March 2027 | met | [How the sheet is kept](information-sheet.md#how-the-sheet-is-kept); [Instructions for use](../operate/instructions-for-use.md) | - | Both documents are attached to every release from 0.0.10. |
| Art 30(1)(e) | "draw up the EU declaration of conformity in accordance with Article 39" | 26 March 2027 | manufacturer | [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity); [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940), [#675](https://github.com/FerroHEALTH/FerroFED/issues/675) | The generator and its release-lane wiring exist; no declaration is issued (issue = false). Issuing one is Cadasto B.V.'s act, and Annex IV item 4 needs the Art 40 result. |
| Art 30(1)(f) | "affix the CE marking of conformity in accordance with Article 41" | 26 March 2027 | manufacturer | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Written as step 8 of the release process. No release carries the marking, and it follows an issued declaration. |
| Art 30(1)(g) | "indicate the name, registered trade name or registered trade mark, the postal address, and the website, email address or other digital contact details" | 26 March 2027 | met | [The manufacturer](post-market.md#the-manufacturer); [(a) The manufacturer](information-sheet.md#a-the-manufacturer); `app/ferrofed-server/tests/it/options.rs` (`the_manufacturer_is_named_beside_federation_and_endpoints`); `app/ferrofed-operator-api/tests/it/manufacturer.rs` (`the_manufacturer_is_the_licensor_with_its_address_and_contact`) | - | The running gateway names the manufacturer and its single point of contact. |
| Art 30(1)(h) | "comply with the registration obligations in Article 49" | 26 March 2027 | awaits-act | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) | See the Art 49(2) row. |
| Art 30(1)(i) | "take without undue delay any necessary corrective action in respect of their EHR systems" | 26 March 2027 | met | [Corrective action, recall and withdrawal](post-market.md#corrective-action-recall-and-withdrawal); [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), A non-conforming version; [`scripts/release/withdraw.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/withdraw.sh) | - | A written procedure with its withdrawal script; it runs when a non-conformity is found. |
| Art 30(1)(j) | "inform the distributors of their EHR systems and, where applicable, the authorised representative, importers and users of the non-conformity" | 26 March 2027 | met | [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), A non-conforming version; [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), The register of economic operators | - | - |
| Art 30(1)(k) | "of any mandatory preventive maintenance of the EHR systems and its frequency" | 26 March 2027 | met | [Maintenance and its frequency](../operate/instructions-for-use.md#maintenance-and-its-frequency) | - | - |
| Art 30(1)(l) | "provide, in an official language of the Member State concerned, market surveillance authorities in that Member State with all the information and documentation necessary" | 26 March 2027 | met | [Cooperation with the authorities](post-market.md#cooperation-with-the-authorities); [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), A request from an authority; [Language and translation on request](technical-documentation.md#language-and-translation-on-request) | - | A written procedure; the documentation is in English and translated on request. |
| Art 30(1)(m) | "cooperate with market surveillance authorities, at their request" | 26 March 2027 | met | [Cooperation with the authorities](post-market.md#cooperation-with-the-authorities); [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), A request from an authority | - | - |
| Art 30(1)(n) | "establish channels of complaint and keep distributors informed thereof" | 26 March 2027 | met | [Making a complaint](post-market.md#making-a-complaint); [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), The channels | - | - |
| Art 30(1)(o) | "keep a register of complaints and a register of non-conforming EHR systems" | 26 March 2027 | met | [The registers](post-market.md#the-registers); [`docs/registers/complaints.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/registers/complaints.tsv); [`docs/registers/non-conforming-versions.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/registers/non-conforming-versions.tsv) | - | - |
| Art 30(2) | "ensure that procedures are in place to ensure that the design, development and deployment of the harmonised software components" | 26 March 2027 | met | [Changes to the harmonised components](technical-documentation.md#changes-to-the-harmonised-components); [Review and change](clinical-safety.md#review-and-change); [`scripts/checks/technical-documentation.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/technical-documentation.sh) | - | A release that changes a harmonised component is held until its clinical safety review and its change row exist. Checking against the Art 36 common specifications waits on their adoption (Art 36(1) row). |
| Art 30(3) | "keep the technical documentation referred to in Article 37 and the EU declaration of conformity referred to in Article 39 for 10 years" | 26 March 2027 | manufacturer | [Keeping it for ten years](technical-documentation.md#keeping-it-for-ten-years); [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), After the tag | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) | The tag ruleset and immutable releases keep the repository's part. The archive copy outside GitHub at each release is Cadasto B.V.'s to make and keep, and this repository holds no record that it was made. |
| Art 30(3) | "make available the source code or the programming logic included in the technical documentation, upon a reasoned request" | 26 March 2027 | met | [Keeping it for ten years](technical-documentation.md#keeping-it-for-ten-years) | - | The source of every release is public at its tag. |
| Art 30(4) | "A manufacturer of EHR systems established outside the Union" | 26 March 2027 | not-applicable | [(a) The manufacturer](information-sheet.md#a-the-manufacturer) | - | Cadasto B.V. is established in the Netherlands, in the Union. |
| Art 30(5) | "upon a reasoned request from a market surveillance authority, provide it with all the information and documentation" | 26 March 2027 | met | [Cooperation with the authorities](post-market.md#cooperation-with-the-authorities); [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md), A request from an authority | - | - |
| Art 36(1) | "adopt common specifications in respect of the essential requirements laid down in Annex II" | 26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f) | awaits-act | [The obligations and their dates](regulatory-status.md#the-obligations-and-their-dates); [`scripts/checks/ehds-acts.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/ehds-acts.sh) | [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) | Not adopted; the Commission's initiative 18579 was at the planning stage when ehds-acts last read it. When it is adopted, Annex II is checked again against it, and Annex III item 3 and Annex IV item 6 name it. |
| Art 37(1) | "draw up technical documentation before the EHR system is placed on the market or put into service, and shall keep that documentation up to date" | 26 March 2027 | met | [The Annex III index](technical-documentation.md#the-annex-iii-index); [Changes to the harmonised components](technical-documentation.md#changes-to-the-harmonised-components); [`scripts/checks/technical-documentation.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/technical-documentation.sh) | - | Drawn up and kept up to date by CI and the release gate; its content is incomplete (Art 37(2) rows). |
| Art 37(2) | "shall demonstrate that the EHR system complies with the essential requirements laid down in Annex II" | 26 March 2027 | open | [The Annex III index](technical-documentation.md#the-annex-iii-index); [What is missing](annex-ii.md#what-is-missing) | [#1405](https://github.com/FerroHEALTH/FerroFED/issues/1405), [#1406](https://github.com/FerroHEALTH/FerroFED/issues/1406), [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) | Annex III item 4 stays partial while an Annex II point is partial. |
| Art 37(2) | "a reference to the results obtained from a European digital testing environment referred to in Article 40" | 26 March 2027 | awaits-act | [The European digital testing environment](technical-documentation.md#the-european-digital-testing-environment) | [#675](https://github.com/FerroHEALTH/FerroFED/issues/675) | See the Art 40(3) row. |
| Art 37(3) | "drawn up in an official language of the Member State concerned or a language that is easily understandable in that Member State" | 26 March 2027 | met | [Language and translation on request](technical-documentation.md#language-and-translation-on-request) | - | Written in English, with a translation of the relevant parts on a reasoned request. |
| Art 37(4) | "within 30 days of the date of the request" | 26 March 2027 | met | [Language and translation on request](technical-documentation.md#language-and-translation-on-request) | - | A written procedure with the deadline and a record of each request. |
| Art 38(1) | "EHR systems shall be accompanied by an information sheet" | 26 March 2027 | met | [How the sheet is kept](information-sheet.md#how-the-sheet-is-kept) | - | - |
| Art 38(2)(a) | "the identity, registered trade name or registered trademark, and contact details of the manufacturer" | 26 March 2027 | met | [(a) The manufacturer](information-sheet.md#a-the-manufacturer) | - | - |
| Art 38(2)(b) | "the name and version of the EHR system and date of its release" | 26 March 2027 | met | [(b) Name, version and date of release](information-sheet.md#b-name-version-and-date-of-release) | - | - |
| Art 38(2)(c) | "the intended purpose of the EHR system" | 26 March 2027 | met | [(c) Intended purpose](information-sheet.md#c-intended-purpose) | - | - |
| Art 38(2)(d) | "the categories of electronic health data that the EHR system has been designed to process" | 26 March 2027 | met | [(d) The categories of data](information-sheet.md#d-the-categories-of-data) | - | - |
| Art 38(2)(e) | "the standards, formats and specifications supported by the EHR system and versions of those standards, formats and specifications" | 26 March 2027 | met | [(e) Standards, formats and specifications](information-sheet.md#e-standards-formats-and-specifications) | - | - |
| Art 38(3) | "enter the information referred to in paragraph 2 of this Article into the EU database" | 26 March 2027 | not-applicable | [Information sheet](information-sheet.md) | - | An alternative FerroFED does not use: the sheet is attached to every release. |
| Art 39(1) | "shall state that the manufacturer of an EHR system has demonstrated that the essential requirements laid down in Annex II have been fulfilled" | 26 March 2027 | open | [What is missing](annex-ii.md#what-is-missing); [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity) | [#1405](https://github.com/FerroHEALTH/FerroFED/issues/1405), [#1406](https://github.com/FerroHEALTH/FerroFED/issues/1406), [#675](https://github.com/FerroHEALTH/FerroFED/issues/675), [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) | A declaration cannot state this while an Annex II point is partial. |
| Art 39(2) | "a single EU declaration of conformity shall be drawn up in respect of all Union legal acts applicable to the EHR system" | 26 March 2027 | manufacturer | [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml); [The Cyber Resilience Act](regulatory-status.md#the-cyber-resilience-act) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Which other Union acts the declaration covers (other_union_acts) is Cadasto B.V.'s to state; Regulation (EU) 2024/2847 is the one the repository already treats as applying. |
| Art 39(3) | "shall contain the information set out in Annex IV" | 26 March 2027 | manufacturer | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The generator writes every Annex IV item in its order (the Annex IV rows); the values of items 4, 5 and 7 are the manufacturer's. |
| Art 39(3) | "translated into one or more official Union languages determined by the Member States" | 26 March 2027 | manufacturer | [Decisions open to the manufacturer](technical-documentation.md#decisions-open-to-the-manufacturer) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The declaration is drawn up in English; the translations are not decided. |
| Art 39(4) | "made accessible online for the expected lifetime of the EHR system" | 26 March 2027 | manufacturer | [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity); [Decisions open to the manufacturer](technical-documentation.md#decisions-open-to-the-manufacturer) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The releases page serves an attached declaration while the repository does; where it is published again if that stops within ten years is not decided. |
| Art 39(5) | "the manufacturer shall assume responsibility for the compliance of the harmonised software components" | 26 March 2027 | manufacturer | [Decisions open to the manufacturer](technical-documentation.md#decisions-open-to-the-manufacturer); [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Only Cadasto B.V. sets issue = true; no release step sets it. |
| Art 39(6) | "The Commission shall publish a standard uniform template for the EU declaration of conformity" | 26 March 2027 | awaits-act | [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity); [`scripts/checks/ehds-acts.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/ehds-acts.sh) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Not published; the generator keeps the order of Annex IV, and ehds-acts watches Art 39(6). |
| Art 40(3) | "manufacturers shall use the digital testing environments referred to in paragraphs 1 and 2 of this Article" | 26 March 2027 | awaits-act | [The European digital testing environment](technical-documentation.md#the-european-digital-testing-environment); [`scripts/checks/ehds-acts.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/ehds-acts.sh) | [#675](https://github.com/FerroHEALTH/FerroFED/issues/675) | No testing environment exists to use: the Art 40(4) implementing act is not adopted, and the Commission's initiative 17513 was at the planning stage when ehds-acts last read it. |
| Art 41(1) | "affixed visibly, legibly and indelibly to the accompanying documents of the EHR system" | 26 March 2027 | manufacturer | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Planned on the information sheet, the instructions for use and the declaration, once a declaration is issued. |
| Art 41(2) | "affixed before placing the EHR system on the market" | 26 March 2027 | manufacturer | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | No release has been placed on the market with the marking. |
| Art 41(3) | "subject to the general principles set out in Article 30 of Regulation (EC) No 765/2008" | 26 March 2027 | manufacturer | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | Regulation (EC) No 765/2008 is not vendored here, so this record does not restate its Art 30; the release process cites its Art 30(1). |
| Art 49(2) | "enter the required data as referred to in paragraph 4 of this Article into the EU database" | 26 March 2029 for (a) to (c), 26 March 2031 for (d) to (f) | awaits-act | [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md), Before the tag; [`scripts/checks/ehds-acts.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/checks/ehds-acts.sh) | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) | The Commission has not established the database (Art 49(1)) and the Art 49(4) delegated act listing the data is not adopted; ehds-acts watches Art 49(4). The entry is then Cadasto B.V.'s, with the Art 40 result. |

### The declaration item by item

| Provision | Requirement | Applies from | Status | Evidence | Issue | Note |
|---|---|---|---|---|---|---|
| Annex IV 1 | "The name of the EHR system, version and any additional unambiguous reference allowing identification of the EHR system" | 26 March 2027 | met | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity) | - | Read from CITATION.cff, the tag, its commit and the pins of the two components. |
| Annex IV 2 | "Name and address of the manufacturer or, where applicable, its authorised representative" | 26 March 2027 | met | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [(a) The manufacturer](information-sheet.md#a-the-manufacturer) | - | - |
| Annex IV 3 | "A statement that the EU declaration of conformity is issued under the sole responsibility of the manufacturer" | 26 March 2027 | met | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh) | - | The generator writes the statement as Annex IV words it. |
| Annex IV 4 | "A statement that the EHR system in question is in conformity with the provisions laid down in Chapter III" | 26 March 2027 | open | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity) | [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) | The statement can be made only once no row of this record is open or waiting; a draft makes no such statement. |
| Annex IV 4 | "with any other relevant Union law that provides for the issuing of an EU declaration of conformity" | 26 March 2027 | manufacturer | [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The value other_union_acts (Art 39(2) row). |
| Annex IV 4 | "complemented by the result from the testing environment mentioned in Article 40" | 26 March 2027 | awaits-act | [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml); [The European digital testing environment](technical-documentation.md#the-european-digital-testing-environment) | [#675](https://github.com/FerroHEALTH/FerroFED/issues/675) | The value testing_environment_result (Art 40(3) row). |
| Annex IV 5 | "References to any relevant harmonised standards used and in relation to which conformity is declared" | 26 March 2027 | manufacturer | [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The value harmonised_standards; this record names none. |
| Annex IV 6 | "References to any common specifications used and in relation to which conformity is declared" | 26 March 2027 | awaits-act | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh); [The Annex III index](technical-documentation.md#the-annex-iii-index) | [#799](https://github.com/FerroHEALTH/FerroFED/issues/799) | None is adopted (Art 36(1) row). |
| Annex IV 7 | "Place and date of issue of the declaration, signature plus name and function of the person who signed" | 26 March 2027 | manufacturer | [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml); [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh) | [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) | The date is the release date in CHANGELOG.md; the place, the signatory, the signature and on whose behalf are the manufacturer's. |
| Annex IV 8 | "Where applicable, additional information" | 26 March 2027 | met | [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh) | - | The technical documentation at the tag, the attached information sheet and instructions for use, and the standards table. |

<!-- chapter-iii:end -->

## What the manufacturer must decide or do

Each row marked manufacturer above is one of these, and the repository
cannot complete any of them:

- **Art 30(1)(e), Art 39(5) and Annex IV 7: issue the declaration.** Set
  `issue = true` in
  [`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml)
  only once no row of this record is open or awaiting an act, and fill the
  place of issue, the signatory's name and function, the signature and on
  whose behalf it is signed. By drawing it up the manufacturer "shall assume
  responsibility" (Art 39(5)).
- **Art 39(2) and Annex IV 4: name the other Union acts.** Decide which
  other acts that provide for a declaration the single declaration covers
  (`other_union_acts`). The repository already treats Regulation (EU)
  2024/2847, the Cyber Resilience Act, as applying to FerroFED
  ([The Cyber Resilience Act](regulatory-status.md#the-cyber-resilience-act)).
- **Annex IV 5: name the harmonised standards,** or state that none is used
  (`harmonised_standards`).
- **Art 39(3): fill the Annex IV values and choose the translations.**
  Decide into which official Union languages the declaration is translated,
  for the Member States where FerroFED is made available.
- **Art 39(4): choose where the declaration stays online** for at least ten
  years if the repository stops serving a release.
- **Art 30(1)(f), Art 41(1), Art 41(2) and Art 41(3): affix the CE
  marking** on the accompanying documents before a release is placed on the
  market, after its declaration is issued, as step 8 of
  [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md)
  describes. Art 41(3) applies the general principles of Art 30 of
  Regulation (EC) No 765/2008, which is not vendored here; have it read
  before the first marking.
- **Art 30(3): keep the archive copy** of each release outside GitHub for
  ten years, as [Keeping it for ten
  years](technical-documentation.md#keeping-it-for-ten-years) describes, and
  keep a record that it was made.
- **Art 49(2): register in the EU database** once the Commission
  establishes it and the Art 49(4) delegated act lists the data, with the
  result of the testing environment.
- **Confirm the reading of the dates.** The application of Articles 28, 30
  and 35 to 41 from 26 March 2027, against Art 26(1) from the category
  dates, is a reading counsel should confirm.

## What waits on the Commission

These acts and facilities do not exist yet, and the rows that rest on them
stay awaits-act until they do. The weekly `ehds-acts` job of
`pin-freshness.yml` (`scripts/checks/ehds-acts.sh`) reads EUR-Lex and the
Commission's "Have your say" register and files an issue when one is
adopted.

- **The common specifications for the essential requirements** (Art
  36(1)), due "By 26 March 2027": Annex II is checked again against them,
  and Annex III item 3 and Annex IV item 6 name them
  ([#799](https://github.com/FerroHEALTH/FerroFED/issues/799)).
- **The common specifications for the European digital testing
  environment** (Art 40(4)), and the environment itself (Art 40(1)): the
  result Art 37(2), Art 40(3), Art 49(2) and Annex IV item 4 ask for
  ([#675](https://github.com/FerroHEALTH/FerroFED/issues/675)).
- **The standard uniform template for the declaration** (Art 39(6)): until
  it is published, the generator writes the items in the order of Annex IV.
- **The EU database and the list of data to enter** (Art 49(1), Art 49(4)).
- **The European electronic health record exchange format** (Art 15(1)):
  the Annex II interoperability points are checked again against it
  ([FerroHEALTH/FerroBRIDGE#1327](https://github.com/FerroHEALTH/FerroBRIDGE/issues/1327)).
