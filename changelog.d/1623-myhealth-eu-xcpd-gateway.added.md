- A `[[xcpd.gateway]]` can be marked `myhealth_eu = true`, a MyHealth@EU
  route. Before each discovery, the gateway checks every certificate the
  responding gateway presents against its CRL or OCSP responder. A revoked
  certificate, or one whose status no source gives, fails the discovery
  closed with nothing sent (eHDSI X.509 Certificates Profile 7.0.0 §3.2).
  Boot refuses a marked gateway beside a fixed `assertion` or
  `assertion_file`, and one without `https`, `client_identity_file` or
  `trust_roots_file`, naming the gateway (#1623).
