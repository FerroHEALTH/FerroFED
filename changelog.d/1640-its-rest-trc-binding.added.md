- On the ITS-REST face, a national contact point's patient-data request is
  held to the patient of its treatment relationship confirmation, as on the
  FHIR face. A token without the `trc_patient` claim is `403` `trc-missing`.
  A query or a read by subject must name the TRC patient, and a request
  under `{base}/v1/ehr/{ehr_id}` or a query scoped to an `ehr_id` must reach
  one of that patient's own `{node, ehr_id}` pairs. Another patient, an EHR
  of another patient, a TRC patient no member holds and one every holder
  restricts all get the answer an unavailable subject gets: `404`
  `subject-unavailable`, with nothing sent to any member. A query that names
  no patient and no EHR, the creation of an EHR and a DEMOGRAPHIC request
  are `403` `trc-unbound`. Each break of the binding is logged as the
  `trc-patient-mismatch` security event the FHIR face records (eHDSI XCA
  Profile 8.0.1 §4.2.1, §4.2.2) (#1640).
