- The operator console answers `413 Payload Too Large` to a request body
  sent chunked that grows past its limit, as it answers one whose
  `Content-Length` declares it. Before, it answered `500`.
