- The revocation check of a MyHealth@EU gateway is the family's shared one,
  `ferrohealth-server` 0.5.1 feature `revocation`, with the CRL and OCSP
  rules unchanged. Its TLS 1.2 offers the four ECDHE AES-GCM suites of
  RFC 9325, as the gateway's own listeners do, and a CRL distribution point
  that covers only some revocation reasons is no longer read as a source
  that can clear a certificate (RFC 5280 §6.3.3).
