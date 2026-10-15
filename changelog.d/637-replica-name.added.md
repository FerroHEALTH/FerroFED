- Every report of the operator surface names the gateway replica that
  produced it, since each replica keeps its own incidents, routing table
  and dependency states (#637). The name is the new `[server] replica`
  setting, else the `HOSTNAME` the process runs with (a Kubernetes pod's
  name), else `unnamed`; a configured name outside 1 to 64 ASCII letters,
  digits, `.`, `_` and `-` is refused at load. Each answer under
  `{base}/operator/` carries it in a `ferrofed-replica` header and each
  report in a `replica` member, and the identity-state export names
  `replica` in place of `scope`. The operator console shows on its
  members, integrity and stored-query views which replica answered; it
  reads one replica at a time, as the book's operator console page says.
