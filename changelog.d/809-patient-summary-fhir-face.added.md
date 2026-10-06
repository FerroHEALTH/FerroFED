- The FHIR R4 face of the European exchange format (Regulation (EU)
  2025/327 Annex II 2.1, #809): with `[fhir]` set, `GET` and
  `POST {fhir-base}/Patient/$summary` answer the patient summary as an HL7
  Europe Patient Summary document `Bundle`, and `GET {fhir-base}/metadata`
  the face's `CapabilityStatement`. The face sits on its own base beside
  `{base}`, so no ITS-REST path changes. A summary runs the gateway's own
  eleven section queries over every member that holds the patient, with or
  without `[stored_queries]`, resolving the patient once and sending no
  patient identifier to a node, and maps each composition through the
  FHIRconnect mappings `[[fhir.mapping]]` names. Each member whose data a
  section holds is named as its author with one `Provenance` per mapped
  composition; a silent member fails the summary under all-or-nothing and is
  named in every section under partial; what no mapping covers is counted,
  never dropped. Every request passes client authentication, a summary takes
  the `aql-` search scope of each section query, every summary that reached
  a member writes one access record with the `patient-summary` category, and
  every error is an `OperationOutcome`. The book's new page "The patient
  summary over FHIR" describes the request, the document and the
  configuration.
- `eehrxf` 0.0.7: the `document` module writes the EPS document `Bundle`
  from the resources and sections a caller gives, naming every entry under a
  base and keeping two runs over one composition apart; `Mapping` names its
  compiled contexts and runs under a FHIRconnect call context (#809).
- `ehds-logging` 0.0.9: `Evidence::constructed` classifies an access that
  serves one category by construction, such as a patient summary, beside
  what its data show (#809).
