- The node profile checks of `conformance run --node-profile` send each
  `ehr_id`-scoped query as a fixed AQL template with the `ehr_id` as the
  value of its `$ehr_id` parameter in `query_parameters`, so a node must
  bind ITS-REST query parameters, in a `WHERE` clause and in an `EHR`
  predicate, to pass them (#739). The scenario queries of
  `conformance run` are built and printed through `openehr-query`, so a
  value always reaches the gateway as an escaped AQL literal. A new CI
  guard, `scripts/checks/aql-splice.sh`, refuses AQL built by string
  formatting or concatenation outside the tests.
