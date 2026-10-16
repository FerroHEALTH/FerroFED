- The read of one prescription on the FHIR face answers an `ETag` (#1118,
  FHIR R4 `http.html` §3.1.0.2): when the member's `MedicationRequest`
  carries `meta.versionId`, `GET {fhir-base}/MedicationRequest/{id}`
  answers `ETag: W/"<versionId>"`. A prescription with no `versionId` is
  answered with no `ETag`, and so is one whose `versionId` is not a valid
  FHIR `id`, which the gateway logs by its shape alone, never by its value.
