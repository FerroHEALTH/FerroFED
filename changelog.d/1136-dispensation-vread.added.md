- The version read of a dispensation and a prescription on the FHIR face
  (#1136, FHIR R4 `http.html` §3.1.0.3):
  `GET {fhir-base}/MedicationDispense/{id}/_history/{vid}` and
  `GET {fhir-base}/MedicationRequest/{id}/_history/{vid}` read as the read
  does, at the member the `id` names alone, with its scope, `ETag`,
  `Last-Modified` and access record, and answer the resource when the
  member's `meta.versionId` is `{vid}`, `404` otherwise. The `Location` a
  dispensation's receipt answers now reads back what it named, carries the
  member's version only when that version is a FHIR `id`, and the
  capability statement declares `vread` for both resources.
