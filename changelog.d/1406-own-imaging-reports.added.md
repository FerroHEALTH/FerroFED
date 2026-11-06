- The document list names the imaging documents of a member that serves
  them itself as an HL7 Europe EU Health Data API Document Access Provider
  (an `hl7-fhir-rest` endpoint declared for `Medical-Imaging`): the member
  is asked by its own `ehr_id` alone with `Patient?identifier=` and
  `DocumentReference?patient=&type=`, the type naming the codes of
  `eehrxf-document-type-medical-imaging-vs` (`85430-7`, `18748-4`) the
  search admits. Each of its `DocumentReference`s is listed as it holds it,
  with each attachment URL readdressed to a `Binary/{token}` URL the gateway
  signs, and the read answers the member's `Binary` unchanged. A member that
  does not answer fails the list under all-or-nothing, and each list and
  read is recorded under `Medical-Imaging`.
