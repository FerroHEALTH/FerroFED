<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# eehrxf

The European interoperability software component of an EHR system under
Regulation (EU) 2025/327 (the European Health Data Space), in Rust: the
European electronic health record exchange format of Art 15, as its
published proxies carry it until the Art 15(1) implementing act is adopted.

- `dataset`: the format-neutral dataset model. A `DatasetModel` reads the
  Xt-EHR *EHDS Logical Information Models* package (`xtehr.eu.ehds.models`,
  a FHIR package archive) as data: every logical model keyed on its canonical
  URL, every element on its element path
  (`EHDSPatientSummary.allergiesAndIntolerances`), and every obligations
  profile with what a producer and a consumer must do with each element. No
  element is a Rust type of its own.
- `category`: one Cargo feature per priority category of Art 14(1)
  (`patient-summary`, `prescription`, `dispensation`, `imaging`,
  `laboratory`, `discharge`), each naming the logical model and the
  obligations profile of its dataset.
- `crosswalk`: per category, the rows from the Xt-EHR element paths to the
  eHealth Network element ids and to the elements and slices of the HL7
  Europe profile, with the producer obligation and the profiles a
  FHIRconnect context may map to in order to feed each. `Crosswalk::check`
  holds the rows to the packages they name. The patient summary carries one
  (`crosswalk::patient_summary`, feature `patient-summary`).
- `fhir-r4`: the FHIR R4 serialisation of the exchange format. It compiles
  the R4 resources and no openEHR crate.
- `mapping` (feature `openehr`, which turns on `fhir-r4`): FHIRconnect 1.0.0
  context mappings compiled once against an openEHR operational template and
  run in process, through FerroBRIDGE's `fhirconnect` engine, over a
  canonical-JSON composition. The answer is a FHIR R4 `Bundle` of the mapped
  resources with one `Provenance` covering them.

The crate authors no mapping language and no profile mapping: the mapping
files are its input. It depends on no logging component and on no
application, so the two harmonised software components stay independent of
each other (Art 2(2)(n), (o)).

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
