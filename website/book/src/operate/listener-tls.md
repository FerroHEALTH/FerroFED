<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# TLS on the listeners

**Audience:** operators who terminate TLS at the gateway, on its client listener or its admin listener. [The public address](public-address.md) covers a proxy in front of it.

## TLS on the listeners

The client listener and the admin listener each run in one of two modes.

- **Plain HTTP**, the default. Use it behind a reverse proxy, a load
  balancer or a service mesh that terminates TLS and reaches the gateway
  over a network you trust, such as the loopback interface or a pod's own
  network namespace. Bearer tokens and patient identifiers cross the hop
  from the proxy to the gateway in the clear, so pick this mode only where
  that hop cannot be read.
- **TLS**, with `[server.tls]` or `[metrics.tls]`. Use it when the hop from
  the proxy to the gateway crosses a network others share, or when your
  policy forbids a cleartext hop inside the network (NEN 7510 is one such
  policy). Set `client_auth = "required"` with a `client_ca_file` to admit
  only the proxy: a client that presents no certificate that CA signed fails
  its handshake and reaches nothing.

```toml
[server.tls]
certificate_file = "/run/secrets/ferrofed/listener.crt"
key_file = "/run/secrets/ferrofed/listener.key"
client_auth = "required"
client_ca_file = "/run/secrets/ferrofed/proxy-ca.pem"
healthcheck_identity_file = "/run/secrets/ferrofed/healthcheck.pem"
```

- `certificate_file` holds the chain the listener presents, its own
  certificate first, and `key_file` its private key, both PEM.
- `client_auth` says whether the listener asks a client certificate:
  `none`, the default, asks none; `optional` verifies one a client presents;
  `required` refuses a client that presents none.
- `client_ca_file` holds the CA certificates a client certificate must chain
  to. It is required with `optional` or `required`, and refused with `none`,
  where it would do nothing (a client sends a certificate only when the
  listener asks for one, RFC 8446 §4.3.2).
- `min_version` is the oldest TLS version the listener speaks: `"1.3"`, the
  default, or `"1.2"` to keep a TLS 1.2 client. `"1.1"` and `"1.0"` are
  refused (RFC 8996).
- `reload_interval`, optional, reads the files again on that period, beside
  `SIGHUP` and an applied configuration revision.
- `healthcheck_identity_file`, `[server.tls]` only, holds a client
  certificate chain and its key, PEM, which `ferrofed healthcheck` presents
  to a listener that requires one. It is refused anywhere else.

The listener speaks TLS 1.3 (RFC 8446) alone by default. With `min_version
= "1.2"` it speaks TLS 1.2 too, and picks TLS 1.3 whenever the client offers
it, as BCP 195 asks (RFC 9325 §3.1.1); its TLS 1.2 cipher suites are then the
four ECDHE AES-GCM suites RFC 9325 §4.2 recommends, each with forward
secrecy. It offers HTTP/2 and HTTP/1.1 in ALPN. A client that does not
finish its handshake within 10 seconds is dropped, and one that does not
send a request head within `server.header_read_timeout` (30 seconds by
default, counted from the head's first byte) is disconnected. A kept-alive
HTTP/1.1 connection that sends no next request within
`server.idle_timeout` (120 seconds by default) is closed with a clean end
of stream; keep it longer than the idle timeout of the clients and proxies in
front, so they close an idle connection first.

`config check` reads every file, and refuses, with exit code 78 and the key
named, a missing certificate or key, a file that holds no PEM, a key that
does not match its certificate, and a client CA that is no usable trust
anchor. It never prints the content of a file.

To renew the certificate, write the new files over the old ones and send
`SIGHUP`, as for [a registry reload](registry.md). Every handshake after the
signal presents the new certificate and checks clients against the new CA
file; connections already open keep the certificate they began with. Files
that do not form a valid pair refuse the whole reload, logged with their
table, and the running certificate, registry and `[auth]` stay. Where no
signal reaches the process, `POST /admin/reload` on the admin listener runs
the same reload. A changed path in `[server.tls]` or
`[metrics.tls]` takes a restart.

`ferrofed healthcheck` follows `[server.tls]`: it asks over `https` on
loopback and accepts exactly the certificate `certificate_file` holds. If
the files were replaced and the gateway has not been sent `SIGHUP`, it
reports so and exits `1`.
