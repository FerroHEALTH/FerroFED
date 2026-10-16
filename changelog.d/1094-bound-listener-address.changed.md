- `ferrofed serve` logs the address each listener bound on its `listening` and
  `serving the admin listener` lines, so a listener configured on port 0 names
  the port the system gave it, and `server.listen` and `metrics.listen` may
  both name port 0 on one address, since each bind takes a port of its own.
