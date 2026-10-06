- The client's address behind a reverse proxy (#722). `server.trusted_proxies`
  lists the proxies, each an IP address or a CIDR block, whose forwarded
  client address the gateway takes, and `server.forwarded_header` names the
  header they write: `forwarded` (RFC 7239), the default, or
  `x-forwarded-for`. A request from a listed proxy is named by the rightmost
  address in that header that is no listed proxy; a request from any other
  peer is named by that peer, whatever headers it sends. The list is empty by
  default, so every request is named by its peer as before. The per-caller
  rate limit keeps keying on the verified caller. The shipped nginx file sets
  `X-Forwarded-For` to the address it received the request from and drops
  `Forwarded`, and the production guide trusts it on loopback.
