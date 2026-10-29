- The client listener serves through the stack of `ferrohealth-server`
  0.3.10: the request id, the request log, the client address, the panic
  catch, the request timeout, the body limit, the overload bound and the
  caller rate are the crate's, with the gateway's error bodies, and the
  FHIR face keeps its own base beside `server.base_path`. An answer is now
  compressed with gzip or brotli for a client that accepts it, once it is
  1024 bytes or longer: `server.compression = false` turns it off and
  `server.compression_min_bytes` sets the size.
- A `500 internal` for a request whose handler panicked, a `503 overloaded`
  and a `429 rate-limited` name the gateway's own request id in the body's
  `request_id`, where they named the client's `X-Request-Id`; the response
  header still echoes the client's id.
