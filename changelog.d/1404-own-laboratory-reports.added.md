- The document list names the laboratory reports of a member that serves
  them itself as an HL7 Europe EU Health Data API Document Access Provider
  (an `hl7-fhir-rest` endpoint declared for `Laboratory-Reports`): the
  member is asked by its own `ehr_id` alone with `Patient?identifier=` and
  `DocumentReference?patient=&type=`, and is left out of the laboratory
  report query. Each of its `DocumentReference`s is listed as it holds it,
  with each attachment URL readdressed to a `Binary/{token}` URL the gateway
  signs, and the read answers the member's `Binary` unchanged. A member that
  does not answer fails the list under all-or-nothing, and each list and
  read is recorded under `Laboratory-Reports`.
