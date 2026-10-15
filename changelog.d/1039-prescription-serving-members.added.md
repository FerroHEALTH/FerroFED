- The members that serve electronic prescriptions and dispensations on the
  FHIR face (#1039, Regulation (EU) 2025/327 Art 14(1)(b) and (c)):
  `[[fhir.serve]]` declares, per category, the members that serve it on
  their own `hl7-fhir-rest` endpoint in the HL7 Europe MPD 1.0.0 profiles,
  and `config check` names them. A category no member is declared for is
  answered as not served on `GET {fhir-base}/MedicationRequest`,
  `GET {fhir-base}/MedicationRequest/{id}` and
  `POST {fhir-base}/MedicationDispense`: `404` with an `OperationOutcome`
  of the issue type `not-supported`, never an empty list, and no member is
  asked. `GET {fhir-base}/metadata` lists `MedicationRequest` and
  `MedicationDispense` only for a declared category. The start is refused
  for a declared member the registry does not hold, one with no endpoint of
  its own serving the category, a native endpoint of either category that
  leaves out its MPD profile, and a face with no resolver.
