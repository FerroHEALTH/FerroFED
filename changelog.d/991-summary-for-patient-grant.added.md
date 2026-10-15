- A person's own patient summary (#991): an electronic health data access
  service (Regulation (EU) 2025/327 Art 3(1), Art 4) asks
  `{fhir-base}/Patient/$summary` with a token confined to the person and
  the scope `patient/aql-eu.ferrofed.eehrxf::*.s`, and gets the summary from
  every member that holds them. A request for another patient is `403`
  (`patient-confinement`) before the PDQm Supplier or any member is asked.
  The access record names the person as the caller and as the data
  subject. A professional's request is unchanged.
