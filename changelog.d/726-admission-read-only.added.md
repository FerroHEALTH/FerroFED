- `ferrofed admission check --read-only` checks a member whose governance
  forbids test data in its production CDR (#726, §12b.1, §12b.2, N42a): it
  creates nothing, reads the `ehr_id` and `system_id` of up to `--count`
  existing EHRs through one AQL query, judges `ehr_id` generation and
  `system_id` uniqueness on them, and ends its report with every condition
  a run without writes leaves unproven, the `ehr_id` exchange of §5.5
  (N34) among them. The admission page says when to run it and when to run
  the full check against a staging copy instead.
