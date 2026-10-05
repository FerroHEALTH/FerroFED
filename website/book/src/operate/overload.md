<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Overload protection

One federated query becomes a request to every member it asks, so a busy
gateway passes its load on to every CDR behind it. Three limits keep that in
bounds:

| Limit | Key | Default | Past it |
|---|---|---|---|
| Requests the gateway serves at once | `server.max_concurrent_requests` | `512` | `503 overloaded`, with `Retry-After` |
| Requests one verified caller sends | `[server.caller_rate]` | off | `429 rate-limited`, with `Retry-After` |
| Requests the gateway sends one member endpoint at once | `federation.max_in_flight_per_node` | `64` | the request waits; past its per-node deadline the member is `time-out` |

`serve`, `config check` and a reload refuse a zero in any of them, naming the
key. A reload that changes one logs it as needing a restart, like the rest
of `[server]` and `[federation]`.

```toml
[server]
max_concurrent_requests = 512   # one more at once answers 503 overloaded
overload_retry_after_s = 1      # the Retry-After of that 503, in seconds

[server.caller_rate]            # unset, no caller is rate limited
requests_per_second = 10        # sustained, per caller
burst = 20                      # at once, after a quiet spell

[federation]
max_in_flight_per_node = 64     # per member endpoint, never shared between endpoints
```

## The concurrency limit

A request that arrives while the gateway serves `max_concurrent_requests`
others is answered `503` with the code `overloaded` and a `Retry-After` of
`overload_retry_after_s` seconds (RFC 9110 §15.6.4, §10.2.3). The gateway
refuses it before it reads anything else of it: the caller is not verified,
and no resolver, localizer or member is asked. The health family,
`{base}/health`, `{base}/health/readiness` and `{base}/health/dependencies`,
is never refused, so an orchestrator's liveness probe does not restart a
gateway for being busy.

Each replica counts its own requests. Size the limit from what the members
can take: with `n` members asked by each query, `max_concurrent_requests`
queries put up to `n` times as many requests on the members together.

## The per-caller rate

With `[server.caller_rate]` set, each caller has a bucket of `burst`
requests that refills at `requests_per_second`. A request past an empty
bucket is answered `429` with the code `rate-limited` and a `Retry-After` of
the whole seconds until the bucket holds one again (RFC 6585 §4).

The caller is the one [client authentication](authentication.md) verified:
its token's issuer and `client_id`. The gateway never reads a forwarded
address such as `X-Forwarded-For`, so a client cannot spread its requests
over addresses it names, and the clients behind one proxy are not counted
as one. A request outside client authentication, such as a health probe or
`GET {base}/.well-known/jwks.json`, has no caller and is never rate limited.
Each replica holds its own buckets, so a caller balanced over `r` replicas
gets up to `r` times the rate. The gateway tracks at most 10 000 callers at
once and forgets those whose bucket has refilled.

## The per-member cap

The gateway sends at most `max_in_flight_per_node` requests to one member
endpoint at once, whatever sent them: federated queries, routed reads and
writes, the ask-all probe, template uploads and stored-query distribution.
A request past the cap waits for a slot until its per-node deadline
(`federation.per_node_timeout_ms`, or the shorter budget a client asked for
with `Prefer: wait`). One still waiting then was abandoned at the per-node
timeout, so the member is `time-out` in `meta.federation.endpoints[]` with
an `error` saying the cap was full and nothing was sent (§11.1, §11.5,
N38). Under the all-or-nothing default that fails the query with `504`, as
any `time-out` does; under `partial` the query answers with the other
members' rows. A routed request answers `504` with `node-timeout`. No member
is marked down for it, since the gateway learned nothing of the member.

One slow member therefore holds at most its own slots: requests to every
other member go out at once. Each replica has its own caps, and a registry
reload starts the new registry's caps while requests on the previous one
finish under theirs.

## Counting the refusals

Every refusal is counted in `ferrofed_overload_refusals_total` by `limit`
(`concurrency`, `caller-rate`, `node-in-flight`), the last with the
`endpoint` ([Metrics](metrics.md)). A `503` or `429` refusal is also logged at
`WARN` with its limit and no value of the request; a capped member is
reported in the answer's `meta.federation`. The shipped alert rules page on
`concurrency` refusals and open a ticket on the other two
([Dashboard and alert rules](metrics.md#dashboard-and-alert-rules)).
