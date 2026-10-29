- Every error the gateway composes outside the FHIR face is an RFC 9457
  problem details document, `application/problem+json`, with the status
  unchanged. Its `type` is the code's anchor on the book's Errors and status
  codes page, its `detail` the sentence the ITS-REST `Error` carried as
  `message`, its `instance` the request id as a `urn:uuid:` URI, and `code`
  and `request_id` stay as extension members, so a client that branches on
  `code` keeps working. A client that read `message` reads `detail`. A
  failed fan-out still answers its `RESULT_SET`, a node's own answer on a
  single-node route still passes through as the node sent it, and the FHIR
  face still answers an `OperationOutcome`. The operator console reads the
  new body.
- A `408` past the request timeout and a `413` over the body limit answer an
  `about:blank` problem with the gateway's request id as `instance`, where
  they answered an empty and a plain-text body.
