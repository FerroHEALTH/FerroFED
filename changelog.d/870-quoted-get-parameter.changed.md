- A double-quoted query parameter on `GET {base}/v1/query/aql` and
  `GET {base}/v1/query/{qualified_query_name}[/{version}]` binds the string
  it names, quotes removed and escapes decoded, as the same string does in a
  `POST` body: `name="123"` binds `123`, where the quotes were kept before.
  A patient identifier bound that way, quoted, inside text, namespace
  qualified or spelled in JSON escapes, is still refused
  `400 identifier-elsewhere` before any member is asked, and one bound
  quoted as the subject now resolves as the bare identifier does.
