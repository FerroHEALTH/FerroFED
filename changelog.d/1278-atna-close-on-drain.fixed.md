- An ITI-20 syslog record the gateway wrote just before it ended its
  connection to the Audit Record Repository could be lost: the connection
  was dropped with the repository's TLS session tickets unread, so the
  socket was reset. The gateway now stops every audit trail on its `SIGTERM`
  drain, and each forwarder closes its connection with a TLS `close_notify`
  first (RFC 5425 §4.4), as it now does before it reconnects. `ihe-atna`
  and `ehds-logging` move to 0.2.6 and `ihe-iti` to 0.0.50 (#1278).
