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
  holds the rows to the packages they name, and `Category::crosswalks`
  names a category's crosswalks, one per dataset. The patient summary
  (`crosswalk::patient_summary`, feature `patient-summary`), the electronic
  prescription (`crosswalk::prescription`, feature `prescription`), the
  electronic dispensation (`crosswalk::dispensation`, feature
  `dispensation`), the imaging report and the imaging study
  (`crosswalk::imaging`, feature `imaging`, against the HL7 Europe Imaging
  Report 1.0.0-ballot composition with its `DiagnosticReport`, and its
  `ImagingStudy`), the laboratory report (`crosswalk::laboratory`, feature
  `laboratory`) and the discharge report (`crosswalk::discharge`, feature
  `discharge`) carry one, the two medication ones against the HL7 Europe MPD
  1.0.0 `MedicationRequest-eu-mpd` and `MedicationDispense-eu-mpd` profiles,
  the laboratory report against the HL7 Europe Laboratory Report 2.0.0
  `Composition-eu-lab` with its `DiagnosticReport-eu-lab` companion
  (`Crosswalk::check_with`), and the discharge report against the HL7
  Europe Hospital Discharge Report 1.0.0 `composition-eu-hdr` profile.
- `fhir-r4`: the FHIR R4 serialisation of the exchange format. It compiles
  the R4 resources and no openEHR crate.
- `mapping` (feature `openehr`, which turns on `fhir-r4`): FHIRconnect 1.0.0
  context mappings compiled once against an openEHR operational template and
  run in process, through FerroBRIDGE's `fhirconnect` engine, over a
  canonical-JSON composition. The answer is a FHIR R4 `Bundle` of the mapped
  resources with one `Provenance` covering them.
- `medication` (feature `fhir-r4` with `prescription` or `dispensation`):
  the electronic prescription and dispensation datasets as Rust records.
  `Prescription::read` reads a member's `MedicationRequest`s, one per item,
  and `Dispense::read` its `MedicationDispense`, each strictly against FHIR
  R4 and with a typed refusal for an element the dataset requires and the
  resource lacks; `Prescription::requests` and `Dispense::resource` write
  them back in the MPD form. `Mpd` checks a resource, and each `Medication`
  it contains, against the vendored MPD profiles by the rules of the
  `receive` check. The number of dispensations an item has left travels in
  the extension
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` on
  `dispenseRequest`, written only when a value is given; its FHIR R4
  `StructureDefinition` is `REMAINING_DISPENSATIONS_DEFINITION`, which the
  documentation site serves at that URL.
- `receive` (feature `fhir-r4`): a document received in the exchange format
  (Annex II 2.2 and 2.3). `ReceivedDocument::read` decodes it against FHIR R4,
  holds it to the document rules of the R4 `Bundle` (`bdl-9`, `bdl-10`,
  `bdl-11`), resolves its subject to its `Patient` entry and keeps its text
  byte for byte. `ReceivedDocument::check` walks a `Bundle` and a
  `Composition` profile read from the vendored package over it:
  cardinalities, `fixed[x]` and `pattern[x]` values, and slices, with every
  constraint it cannot evaluate listed. `ReceivedDocument::category` names
  the priority category whose document profile fixes a `Composition.type`
  coding of the document, and `Category::profiles` the profiles a document
  of it is held to. With `openehr`,
  `Mapping::to_openehr` maps the document into one canonical composition
  through FHIRconnect and keeps the document in the composition's
  `FEEDER_AUDIT.original_content`; `ReceivedComposition::model` is the
  composition as the RM model holds it.
- `terminology` (feature `fhir-r4`): the terminology rules. With `openehr`,
  `transcode` turns an openEHR text item into FHIR codings and narrative: a
  code in a preferred system as recorded, a `TERM_MAPPING` of match `=` as a
  second coding, every other match in the narrative, an archetype-local code
  displayed with its term from the operational template (`local`), and any
  other code in no preferred system under eHN PS Art 10(7). `system` writes
  each coding's system as a FHIR URI: a local code under its archetype's
  code system, the openEHR names `SNOMED-CT`, `LOINC` and `ICD10` as their
  URIs, and a code of a terminology with no known URI as no coding, in the
  narrative and reported as unmapped. `ehn` holds the
  eHN Patient Summary §4 preferred systems, the default set. `Expansions` reads the value set
  expansions of a package, and `bindings::check_required` holds the
  `required` bindings of a profile to them, naming every binding it cannot
  evaluate. `Terminology` is the optional FHIR terminology server
  (`$translate`, taking only `equivalent` and `equal`, `$lookup`,
  `$validate-code`) behind the `TerminologyServer` seam; with none
  configured every operation fails closed.

The crate authors no mapping language and no profile mapping: the mapping
files are its input. It depends on no logging component and on no
application, so the two harmonised software components stay independent of
each other (Art 2(2)(n), (o)).

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
