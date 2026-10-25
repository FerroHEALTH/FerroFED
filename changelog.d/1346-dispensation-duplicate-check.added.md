- A dispensation received on the FHIR face is checked against the owner's
  dispensations before it is written: the gateway reads the patient's
  dispensations at the owner's own endpoint, by its `ehr_id` alone, matches
  the received one's business identifier at the gateway, and refuses one the
  owner already holds with `409` `duplicate`, naming it, with nothing
  written. An owner that does not answer that read refuses the dispensation.
  A dispensation a client sends again after a time-out is never written
  twice (hazard I-12, #1346).
