- The access record of a request addressed by `ehr_id`, such as a routed
  read or write or a query scoped to one `ehr_id`, names the patient the
  identity binding holds under it, in each namespace the new `[access_log]
  patient_namespaces` names, so a search of the log by the person's
  identifier (ITI-81 `patient.identifier`) finds it (Regulation (EU)
  2025/327 Art 9(1)). Under PIXm the gateway asks the member's PIX Manager
  with one ITI-83, on behalf of the caller, within the per-node budget.
  Each `ehr` entity says how its patient was looked up in a
  `patient-lookup` detail (`request-named`, `found`, `not-found`,
  `unavailable`, `not-configured`, `unsupported`) and keeps the `ehr_id`;
  the access is recorded and answered whatever the lookup says, and no
  identifier reaches a node or a log line (#796).
- `crates/ehds-logging` 0.0.8: `PatientLookup` and the `patient` of each
  `EhrAt`, every patient found written as a BALP `entity:patient`, and the
  `patient-lookup` detail (#796).
