- Every query the gateway sends a node now carries an explicit `fetch`, so no
  node's own default page can cut its share unseen: the `LIMIT` the query
  carries, or for a query with no `LIMIT` the new `federation.max_node_rows`
  plus one (9999 rows by default, so a node is asked for 10000, the page FerroEHR
  serves by default; declared in `OPTIONS {base}/` as
  `paging.max_node_rows`). A node that answers more rows than that is
  `node-error` and its rows are never merged: the query fails `424` under
  all-or-nothing, and a `partial` answer is incomplete and names the node
  (§11.4, §11.5). The ITS-REST `offset` and `fetch` members now select a
  window over the AQL result (the skip is the AQL `OFFSET` plus `offset`, the
  count the smaller of `fetch` and the rows the AQL `LIMIT` leaves), paged
  under the declared `OFFSET` strategy. The code `paging-conflict`, a `400`
  for a member that differed from its clause, is retired: no request is
  refused with it any more, and the error page lists it as retired (#868).
