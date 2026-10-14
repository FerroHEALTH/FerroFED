- Every stored-query answer that carries `Location` names the stored
  query's own URL, `{base}/v1/definition/query/{name}/{version}` (ITS-REST
  `Location_Query`). With `server.public_url` set it is that absolute URL
  on every route. Without it, the `PUT` answers keep a reference relative to
  the request URL, and the operator's redistribution on the admin listener,
  whose bare version resolved to `…/{version}/{version}`, carries no
  `Location`, because no reference from the admin listener reaches the
  client listener (#899).
