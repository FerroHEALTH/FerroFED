- The access record of a query over many patients' data (Regulation (EU)
  2025/327 Art 9(1)): when the query projects the `ehr_id`, the record names
  each `ehr_id` its delivered rows hold, at the member that answered it, and
  the patient behind it, up to `[access_log] max_named_ehrs` (50 by
  default); otherwise it carries an `ehds-population` entity stating the
  query's scope and why no `ehr_id` is named, beside the members it asked.
  The query each member receives is unchanged (#819).
