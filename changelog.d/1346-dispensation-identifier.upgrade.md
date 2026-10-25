- A dispensation sent to `POST {fhir-base}/MedicationDispense` must carry
  its own business identifier, an `identifier` with a `system` and a
  `value`; one without it is refused `422` `required`, naming
  `MedicationDispense.identifier`. Have the pharmacy systems and the
  national connector that send dispensations through the gateway set it
  (#1346).
