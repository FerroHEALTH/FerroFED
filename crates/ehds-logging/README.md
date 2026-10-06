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

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
