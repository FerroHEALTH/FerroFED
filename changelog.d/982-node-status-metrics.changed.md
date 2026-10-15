- `ferrofed_node_requests_total` reads the `outcome` of a routed request and
  of an ask-all probe with the rule the access record names a routed
  request's origin by (Federation Tier §11.1, N16): a node's `4xx` answer is
  now `node-error` where it was `active`, so the counter and the record never
  read one answer apart, and a new `class` label, `member` or
  `client-error`, tells a member's failure from a client's mistake it
  answered correctly. A probed member's `404`, which says it holds no such
  EHR, stays `active`, and a request answered with a `DPoP` nonce challenge
  no proof could answer is now counted as `node-error`. The dashboard's
  member request panel splits by `class` (#982).
