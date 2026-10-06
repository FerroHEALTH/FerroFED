- `ferrofed admission check --read-only` no longer prints or keeps the
  `ehr_id`s of the existing EHRs it reads. Those are real patients'
  pseudonymous identifiers at a production node, so the report now names each
  EHR by its row and keeps only how many it read; a run with writes still
  names the synthetic EHRs it created.
