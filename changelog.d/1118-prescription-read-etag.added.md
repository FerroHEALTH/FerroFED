- The read of one prescription on the FHIR face answers an `ETag` and a
  `Last-Modified` (#1118, FHIR R4 `http.html` §3.1.0.2): when the member's
  `MedicationRequest` carries `meta.versionId`,
  `GET {fhir-base}/MedicationRequest/{id}` answers `ETag: W/"<versionId>"`,
  and when it carries `meta.lastUpdated`, it answers `Last-Modified` with
  that instant as an RFC 9110 HTTP-date in GMT. A prescription with no
  `versionId` or no `lastUpdated` is answered without that header, and so
  is one whose `versionId` is not a valid FHIR `id` or whose `lastUpdated`
  is not a valid FHIR `instant`, which the gateway logs by its shape alone,
  never by its value.
