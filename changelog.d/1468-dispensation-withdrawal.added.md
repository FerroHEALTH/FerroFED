- The FHIR face withdraws a dispensation it received: `PUT
  {fhir-base}/MedicationDispense/{id}` with `status` `entered-in-error`, by
  the id the receipt's `Location` named, is held to
  `MedicationDispense-eu-mpd`, read at the member that id names, and sent
  there once as an update by the member's own id, its body unchanged but for
  its top-level `id` (FHIR R4 `http.html` §3.1.0.4). A body whose `id` is
  not the request's is `400`, any other `status` `422`, the member's refusal
  `409` and its silence `504`; the withdrawal is recorded as an update under
  `Electronic-Dispensations`, and `metadata` declares `update` for
  `MedicationDispense`. MyHealth@EU 07.04 is met (#1468).
