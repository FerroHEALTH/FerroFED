- The gateway bounds the load it takes on and passes on (#631). Past
  `server.max_concurrent_requests` (512 by default) a request is answered
  `503 overloaded` with `Retry-After` (`server.overload_retry_after_s`)
  before it reaches client authentication or a member; the health family is
  never refused. The optional `[server.caller_rate]` gives each verified
  caller, by issuer and `client_id` and never by a forwarded address, a
  bucket of `burst` requests refilled at `requests_per_second`; past it the
  answer is `429 rate-limited` with `Retry-After`. At most
  `federation.max_in_flight_per_node` requests (64 by default) go to one
  member endpoint at once; one past it waits for a slot until its per-node
  deadline and is then `time-out` with nothing sent (§11.5, N38), so one
  slow member holds no other member's requests. A zero in any of them is
  refused by `config check`, and `ferrofed_overload_refusals_total` counts
  every refusal by its limit. The errors page lists `overloaded` and
  `rate-limited`.
