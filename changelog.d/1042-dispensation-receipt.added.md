- The receipt of a dispensation on the FHIR face (#1042, Regulation (EU)
  2025/327 Art 14(1)(c)): `POST {fhir-base}/MedicationDispense` takes one
  `MedicationDispense`, held to the HL7 Europe MPD 1.0.0 profile before any
  member is asked, and writes it once to the one member that owns the
  prescription it names, its body as it was sent. The gateway resolves the
  patient from `subject`, asks every member holding the patient for its
  prescriptions by that member's own `ehr_id`, and matches the
  prescription's business identifier itself, so the identifier never
  reaches a member. A prescription named by the face's
  `MedicationRequest/{id}` alone is refused `422`, since the member
  receives the body unchanged and can link it only by the identifier. A
  written dispensation answers `201` with a `Location` naming it by the
  face's id; no owner answers `404` as an unknown patient does; two members
  holding the prescription answer `409` and raise the new
  `PrescriptionIdentifierCollision` integrity incident; the owner's refusal
  answers `409` and its silence `504`, the outcome unknown and the write
  never sent again. The receipt takes the `composition-` create scope over
  every template, is recorded under `Electronic-Dispensations` and
  `Electronic-Prescriptions`, and is counted by outcome in
  `ferrofed_fhir_dispensations_total`. Serving `Electronic-Dispensations`
  now needs `fhir.receive_namespaces`, the incident report keeps the last
  20 incidents of each kind, and the capability statement declares exactly
  the interactions the face serves (#1110).
