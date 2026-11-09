- Every outbound client reads its TLS material through
  `ferrohealth-server` 0.6.4's `TlsClient`, from a `tls` sub-table that also
  takes `min_version`. Material that holds no certificate and key in PEM is
  refused when the configuration loads, naming
  `<table>.tls.client_identity`, where the federation refused it before.
  The RFC 8705 §3.1 thumbprint a node's grant binds its tokens to is taken
  from the certificate the gateway presents, and a reload compares the
  certificates, keys and roots it read, so a certificate renewed at the
  same path rebuilds the client (#1750).
- The revocation check of a MyHealth@EU route speaks the minimum TLS
  version of `[xcpd.tls]`, where it always admitted TLS 1.2 (#1750).
