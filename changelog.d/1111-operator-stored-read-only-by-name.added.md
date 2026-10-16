- The operator surface marks each stored-query version `read_only` when the
  gateway takes no `PUT` under its name (its own `eu.ferrofed.eehrxf`
  namespace, or the read-only definition files), and
  `GET {base}/operator/stored-queries/{name}` answers one stored query's
  versions by its qualified name, a `404` when none is held (#1111).
