- The read of a received dispensation on the FHIR face (#1129, Regulation
  (EU) 2025/327 Art 14(1)(c)): `GET {fhir-base}/MedicationDispense/{id}`
  reads a dispensation by the `id` its receipt's `Location` named, at the
  member that `id` names alone, by the member's own `ehr_id` and its own
  `id`, with the read rules, the `ETag` and the `Last-Modified` of a
  prescription read. It takes the `composition-` read scope, is recorded
  under `Electronic-Dispensations`, and the capability statement declares
  it beside the `create`. A test now holds every interaction and operation
  the capability statement declares to a route that answers it.
