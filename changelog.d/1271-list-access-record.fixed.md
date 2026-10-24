- A document list on the FHIR face that lists the gateway's summary writes
  one access record even when it asked no member, naming the members the
  patient resolved at and the `ehr_id` at each, since the list tells the
  caller that they hold the patient's record (Regulation (EU) 2025/327
  Annex II 3.2). Before, it wrote none (#1271).
