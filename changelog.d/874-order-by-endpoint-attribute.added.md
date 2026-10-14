- A directed query can order on an ENDPOINT attribute through the
  directive's variable, `ORDER BY p/organisation DESC` alone or among other
  keys (#874). The gateway takes the key out of every node query, orders
  the merged rows on the value it holds per endpoint in the place the client
  wrote it, and breaks a tie on the endpoint id and then the uid; each node
  keeps the other keys and the `LIMIT` (§8.1, §9.3, §11.6.1). Under
  `DISTINCT` the attribute must also be selected, and under
  version-identity dedup with a `LIMIT` the key is refused `400` with the
  new code `endpoint-order-dedup` (§10.2). The variable anywhere else is
  still refused with `endpoint-variable`.
