- The operator console's stored queries are listed by namespace, each version a
  chip and the gateway's own namespace marked read-only, with no AQL in the
  list. A query's detail at `/stored-queries/{name}` shows its versions with a
  switch between them, when each was stored, and its AQL highlighted, and
  "Open in query console" chooses the query there by its name and version
  alone. The self-description shows what `OPTIONS {base}/` declares as cards,
  with the manufacturer it names, and the whole document behind a disclosure
  (#1077).
