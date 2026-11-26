- The FHIR face writes the vaccination history MyHealth@EU Requirements
  Catalogue v10.0.0 §1.24 asks of the contact point: with a
  `[fhir.vaccination]` template, the document list names one
  `bundle-euvac-myhealtheu` per attested immunisation list per member,
  typed LOINC `11369-6`, whose legal authenticator is the person who
  attested the record's version. A record no person attested, or a patient
  with no name or birth date, is listed unavailable with the reason
  ([The vaccination history over FHIR](https://ferrofed.eu/docs/integrate/vaccination-history.html),
  #1980).
