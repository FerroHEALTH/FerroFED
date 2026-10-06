- `eehrxf` 0.0.4: the patient summary crosswalk (#777). Under the
  `patient-summary` feature, each row of `crosswalk::patient_summary` keys on
  an Xt-EHR `EHDSPatientSummary` element path and names the eHealth Network
  element ids of the patient summary guideline Release 3.4, the producer
  obligation, the element or slice of the HL7 Europe Patient Summary
  composition that carries it, and the profiles a FHIRconnect context may map
  to in order to feed it. `Crosswalk::check` fails when an element a
  producer `SHALL:able-to-populate` has no row, when a row names a slice the
  pinned profile lacks, or when a required EPS section slice is uncovered.
  The medical alert (A.2.1.2) and the functional status (A.2.3.4) are listed
  as gaps for the clinical safety review. `dataset::ResourceProfile` reads a
  FHIR resource profile and its slices from a package, and each element now
  carries its target profiles.
