- The configuration page of the operator console edits a console member's
  FHIR endpoints (`hl7-fhir-rest`, the exchange-format endpoints) beside its
  Query API endpoints: the member form shows each with its `endpoint_id`,
  base URL, `ehr_id` identifier system, categories, profiles and status, and
  a save adds, edits or removes one as the form shows them, merged onto the
  held endpoint of the same `endpoint_id`. Before, a FHIR endpoint was kept
  on save and changed only through the admin API.
