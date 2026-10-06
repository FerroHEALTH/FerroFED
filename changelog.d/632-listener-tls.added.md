- TLS on the gateway's listeners (#632). `[server.tls]` serves HTTPS with a
  certificate chain and key from `certificate_file` and `key_file`, and
  `[metrics.tls]` does the same for the admin listener. An optional
  `client_ca_file` requires every client to present a certificate that CA
  signed, so only the proxy in front of the gateway is admitted. The
  listener negotiates TLS 1.3 or TLS 1.2 and nothing older, prefers TLS 1.3,
  and offers TLS 1.2 only with the four suites RFC 9325 §4.2 recommends
  (BCP 195). `SIGHUP` reads the files again, so a renewed certificate is
  presented from the next handshake with no restart; files that do not form
  a valid pair are refused and the running certificate stays. `ferrofed
  healthcheck` follows `[server.tls]`, accepting exactly the certificate the
  file holds and presenting `healthcheck_identity_file` where a client
  certificate is required, and says when the file was replaced without a
  `SIGHUP`. `config check` reads every file and refuses a missing, unreadable
  or mismatched one naming its key, never its content. Unset, the listeners
  speak plain HTTP as before, for a proxy that terminates TLS; the
  configuration page says which mode to use when. `url_scheme` on the HTTP
  server metrics reads `https` on a TLS listener.
