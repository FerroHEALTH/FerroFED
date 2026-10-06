- The access record names only whose data the access reached (#756). A
  federated query's record names an `ehr_id` only at a member the query was
  sent to, at the endpoint it was sent through. A routed request's record
  reads the path `ehr_id` and the subject of `GET {base}/v1/ehr` with the
  parsers the route is served by, so a `subject_id` holding a `+` is
  recorded as the patient the gateway resolved, where it was recorded with
  a space. A routed answer whose `ehr_id` or subject the record cannot read
  is withheld with `503 access-unrecorded`.
