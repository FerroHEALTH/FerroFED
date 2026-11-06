- On the FHIR face, a national contact point's patient-data request is
  held to the patient of its treatment relationship confirmation. The
  patient travels in the token's `ehdsi.patient.id`, as `system|value`. A token without it is `403` `trc-missing`. Another patient,
  a resource of another patient, and a patient no member holds get one
  answer: `404` `not-found` with `trc-patient-mismatch` and the service's
  MyHealth@EU `*_NOT_FOUND` code. A mismatch is logged as a
  `trc-patient-mismatch` security event that names the professional and
  both patients by pseudonym only (eHDSI XCA Profile 8.0.1 §4.2.1, §4.2.2)
  (#1622).
