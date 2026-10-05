- The query console of the operator console (#277), at `/query`. An AQL
  query, or a stored query by name and optional version, runs through the
  gateway's `POST {base}/v1/query/aql` or `/v1/query/{name}` as the
  signed-in operator, with the targeting, dedup and best-effort headers the
  gateway's self-description offers. The answer shows the rows, every
  endpoint's status, latency and error from `meta.federation`, and says in
  words when `complete` is false; a `504` or `424` all-or-nothing failure
  shows its diagnostic envelope, and a refusal its status and stable code.
  A patient is named through a parameter in the request body, and nothing
  entered in the console reaches a URL, the browser history or a log.
