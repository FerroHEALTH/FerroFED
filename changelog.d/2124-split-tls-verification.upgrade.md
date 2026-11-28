- A root named in an outbound table's `trust_roots_file` must now have a CRL
  or OCSP responder that answers, or every connection over it fails closed.
  Move the roots of a private PKI that publishes no revocation service to
  the table's new `trust_roots_without_revocation_file`.
