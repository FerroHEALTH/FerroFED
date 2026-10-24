- A read of a prescription or a dispensation by its locator id on the FHIR
  face is held to the consent pre-filter as the search is: the gateway names
  the patient behind the id's `ehr_id` in the `[access_log]
  patient_namespaces`, and a member the pre-filter denies is asked nothing
  and answers as an id the face never wrote, with no trace of the
  restriction. Before, the read went to that member (#1270).
