- The read of one prescription on the FHIR face (#1041, Regulation (EU)
  2025/327 Art 14(1)(b)): `GET {fhir-base}/MedicationRequest/{id}` reads
  a prescription by the `id` the search now gives it, which names the
  member's own endpoint, its `ehr_id` and its own `id` of the prescription,
  so the gateway holds no state between the two. The member the `id` names
  is asked alone, by its `ehr_id` and its own `id` (`_id`), and its
  `MedicationRequest` is answered as it sent it, with the `id` read and the
  member named in `openEHR-federation-endpoint`. An `id` with no owner
  answers `404` with one text whatever the reason; an `ehr_id` two members
  claim answers `409` and raises the integrity incident; a member answering
  two prescriptions for one `id` answers `409`; a caller confined to one
  patient reads that patient's prescriptions alone. The read takes the
  `composition-` read scope over every template and is recorded under
  `Electronic-Prescriptions`. The search names each prescription by the
  same `id`, and its `fullUrl` by the read; a prescription whose parts do
  not fit an `id` is listed as its member sent it, with a `warning` naming
  the member and quoting no identifier.
