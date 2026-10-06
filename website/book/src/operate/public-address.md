<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The public address

Clients, the members' authorization servers and the identity services reach
the gateway at a public address, usually through a reverse proxy that
terminates TLS. This page covers the two settings that address takes: the
public base URL, which several keys repeat, and the proxies whose forwarded
client address the gateway takes.

## The public base URL

Three keys name the URL clients and services reach the gateway at:
`auth.audience`, the JWK Set's `signing.jwks_uri` and the PMIR feed's
`pmir.callback_url`. Name it once in `server.public_url`, with the scheme,
the host, any port and the [base path](configuration.md#the-base-path), and
leave the three out:

```toml
[server]
base_path = "/fed"
public_url = "https://gateway.example.org/fed"
```

Each of the three that is not set then takes its value from it:

| Key | Value |
|---|---|
| `auth.audience` | `server.public_url` as written |
| `signing.jwks_uri` | `{public_url}/.well-known/jwks.json` |
| `pmir.callback_url` | `{public_url}` followed by `pmir.path`, `/pmir/feed` by default |

The gateway refuses to start, naming the key, when `server.public_url` is
no `http` or `https` URL, carries a user name, a password, a query or a
fragment, or names a path other than `server.base_path` (a trailing `/`
aside). A `signing.jwks_uri` or `pmir.callback_url` you still set must name
the route the gateway serves under `server.public_url`, or `config check`
refuses it and names the key and the URL it expects. A copy that misses the
base path is then found before a node fails to fetch the JWK Set or the
Registry sends the feed nowhere. An `auth.audience` you set is taken as
written: it is the gateway's identifier at its issuers, which need not be
its URL. Without `server.public_url`, each key is read as written and
nothing is compared.

## Behind a reverse proxy

The gateway names each request by the address it came from, which the
access log of [#623](https://github.com/FerroHEALTH/FerroFED/issues/623)
records ([The audit trail](audit.md)). That is the address of the
connection's peer, so behind a reverse proxy it is the proxy's. The
client's own address reaches the gateway only in a header the proxy writes,
which any client can write as well, so the gateway reads it only from a
proxy you list (RFC 7239 §8.1):

```toml
[server]
trusted_proxies = ["127.0.0.1", "10.20.0.0/16"]
forwarded_header = "x-forwarded-for"
```

- `trusted_proxies` lists each proxy as an IP address or a CIDR block. Empty,
  the default, the gateway names every request by its peer and reads no
  forwarded header.
- `forwarded_header` is the header the proxies write: `forwarded`, the
  default, reads the `for=` parameters of `Forwarded` (RFC 7239);
  `x-forwarded-for` reads the comma-separated list of `X-Forwarded-For`.

A request from a listed proxy is named by the rightmost address in that
header that is no listed proxy. Each proxy appends the address it received
the request from, so anything to the left of the last address a listed
proxy wrote may have come from the client. A hop that names no address,
such as `unknown`, stops the walk at the last address a listed proxy gave.
A request from any other peer is named by that peer, whatever headers it
sends.

Have the proxy at the edge set the header rather than append to it, as
`deploy/nginx/ferrofed.conf` does with
`proxy_set_header X-Forwarded-For $remote_addr`, and drop the header it
does not write. The per-caller rate limit keys on the verified caller,
never on an address, so a forwarded address changes no limit
([Overload protection](overload.md)).
