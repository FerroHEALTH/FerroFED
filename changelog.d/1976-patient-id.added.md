- The patient lookup gives each patient a stable id, the same in every
  answer, derived from the identifier with HMAC-SHA-256 under a key from
  `fhir.id_key`. With `[store]`, the identifier it stands for is kept sealed
  for `fhir.patient_id_lifetime`, and the document list answers
  `patient=Patient/{id}`, the MyHealth@EU national contact point API's
  `patient` parameter. An id the store no longer holds answers an empty
  `searchset` with an `outcome` entry asking for the lookup again
  ([Looking a patient up](https://ferrofed.eu/docs/integrate/patient-summary.html#looking-a-patient-up),
  #1976).
