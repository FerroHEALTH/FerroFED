- The search of a patient's prescriptions on the FHIR face (#1040,
  Regulation (EU) 2025/327 Art 14(1)(b)):
  `GET {fhir-base}/MedicationRequest?patient.identifier=system|value` asks
  every member declared for `Electronic-Prescriptions` that holds the
  patient at its own endpoint, by its own `ehr_id` alone, following the
  member's `next` links under its own base. The answer is one `searchset`
  with each member's `MedicationRequest` as the member sent it except its
  `id` and its entry's `fullUrl`, which carry the face's locator, the `id`
  the read takes (#1041); one `Provenance` naming the member, and a
  `warning` when two members hold a prescription under the same
  identifier. `_count` pages the answer through the gateway's own `-offset`
  parameter. A silent member, and a member the cross-reference could not
  answer for (`not-resolved`), fails the search under all-or-nothing and is
  named under `partial`; an unknown patient and a patient a consent
  decision hides get the same empty `searchset`. The search takes the
  `composition-` search scope over every template and is recorded under
  `Electronic-Prescriptions`.
