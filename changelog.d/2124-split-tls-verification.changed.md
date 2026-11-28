- Every outbound TLS client verifies a chain by its root. A chain to a root
  of a table's `trust_roots_file` is verified on every operating system,
  ML-DSA certificates included (RFC 9881), and opens a connection only once
  a CRL or OCSP check cleared the endpoint: the gateway clears each node,
  the ITI-20 syslog repository, the OTLP collector and every other endpoint
  it reaches under such roots when it builds the client and every four
  minutes after, and a connection fails closed while no revocation source
  answers. The new `trust_roots_without_revocation_file` key of every
  outbound `tls` table names roots whose PKI publishes no revocation
  service: a chain to one of them is trusted with no check, each connection
  over one is logged, and the startup banner names each such table and its
  roots on a `No revocation` line. A chain to any other root goes to the
  platform's verifier as before. The post-quantum inventory scores the
  certificate rows by the same split: 16 of 29 uses are safe.
