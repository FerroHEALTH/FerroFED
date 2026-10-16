- The console's stored-query pages mark a version read-only as the gateway's
  operator surface marks it, with no copy of the gateway's reserved-namespace
  rule, and the detail of one stored query reads that name's versions with
  `GET {base}/operator/stored-queries/{name}` instead of every page of the
  listing (#1111).
