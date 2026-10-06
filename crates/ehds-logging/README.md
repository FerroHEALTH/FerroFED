<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ehds-logging

The European logging software component of an EHR system under Regulation
(EU) 2025/327 (the European Health Data Space), in Rust: the record of every
access to personal electronic health data that Annex II 3.2 asks for, and
what classifies it.

- `record`: the access record, with (a) the provider and (b) the natural
  person who accessed the data, (c) the categories, (d) the time and (e) the
  origins, beside the data subject and the purpose of use.
- `category`: the six priority categories of Art 14(1) and the national
  categories a deployment declares.
- `map` and `classify`: a deployment's map from openEHR template ids and
  archetype ids to categories, and the classification of one access by the
  model ids of what it delivered, read or wrote, or by the ids its request
  constrains its data to. An access the map cannot classify is marked
  unclassified with its ids as evidence; it is never refused for it.
- `retention`: how long a record is kept, in whole years per category and
  per origin, never under the three years from each date of access of Art
  9(2): the longest its categories and origins call for, and the longest
  declared anywhere for an unclassified access. The record states the
  period, the first date it may be deleted on and what called for it, so the
  store that holds it can apply it.
- `sink`: where records go, and the error a sink returns when it cannot
  store one.
- `balp` (feature `balp`): the record written as an IHE BALP `AuditEvent`
  through the audit types of `ihe-iti`, and a sink over an `ihe-iti` audit
  recorder, such as one that spools records for an ATNA Audit Record
  Repository.

The crate ships no category map: the deployment authors it. It depends on no
interoperability component and on no application.

## The categories on the wire

Each category is a coded value, a system and a code in it, as a FHIR R4
`Coding` is. The six priority categories carry the codes of HL7 Europe's
`EEHRxFDocumentPriorityCategoryCS` (`hl7.fhir.eu.health-data-api`
1.0.0-ballot), in the system
`http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs`,
version `1.0.0-ballot`, compared case-sensitively:

| Art 14(1) | Code |
|---|---|
| (a) patient summaries | `Patient-Summaries` |
| (b) electronic prescriptions | `Electronic-Prescriptions` |
| (c) electronic dispensations | `Electronic-Dispensations` |
| (d) medical imaging studies and related imaging reports | `Medical-Imaging` |
| (e) medical test results, including laboratory and other diagnostic results and related reports | `Laboratory-Reports` |
| (f) discharge reports | `Discharge-Reports` |

A national category (Art 14(1) third subparagraph) is its Member State's
code in the absolute URI of that state's code system, held to the FHIR
`code` rules. Where one string carries a category, as a BALP `detail` value
does, it is `<system>|<code>`, the FHIR search token form, and the record
names each code system's version once in `ehds-category-version`. "No
category" and "unclassified" are states of the record, never categories,
and the reasons an access is unclassified are a closed set:
`unmapped`, `named-nothing`, `unbound`, `operation-not-read`,
`format-not-read`, `body-not-read` and `no-object-returned`.

The integration tests pin this wire form for every product that writes the
same record (`tests/it/map.rs`, `tests/it/category.rs`, `tests/it/balp.rs`),
and hold the six codes to the vendored code system.

## API stability

The crate is on the 0.0.x line, so any release may change its API. Its
public API exposes these types of other crates, each a dependency a
consumer must use at a compatible version:

- `jiff` 0.2 (`Timestamp` on the record, `Date` on its retention), pre-1.0;
- `secrecy` 0.10 (`SecretString` on a patient identifier and on the query
  text), pre-1.0;
- `async-trait` 0.1, whose expansion is the signature of `AccessSink`,
  pre-1.0, kept because a trait with an `async fn` is not object-safe
  without it;
- `serde` 1 (`Deserialize` on `Declared`) and `url` 2 (`Url` under
  `balp`), both stable;
- `ihe-iti` (`AuditRecorder` and `Exchange` under `balp`), the sibling
  crate published from the same repository on its own 0.0.x line.

A version of this crate that moves one of these to a new major version
changes its own version.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
