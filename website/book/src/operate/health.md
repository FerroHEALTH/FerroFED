<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Health probes

The gateway answers three health routes under its
[base path](configuration.md#the-base-path), and the binary carries a
`healthcheck` command for a runtime that cannot send an HTTP request itself.
No specification governs health probes: our own design.

## The routes

| Route | Answers | Use it as |
|---|---|---|
| `GET {base}/health` | `200` while the process serves; it checks nothing else | liveness |
| `GET {base}/health/readiness` | `200` while the gateway serves and its own subsystems are up; `503` before boot completes and from the moment `SIGTERM` or `SIGINT` arrives | readiness, startup, the image `HEALTHCHECK` |
| `GET {base}/health/dependencies` | always `200`, with the state the gateway last observed of each member endpoint, of the resolver, of the consent pre-filter, of the localizer, of the demographics step, of the mCSD directory, of the audit repository and of the PMIR Patient Identity Registry | monitoring, never a probe |

Readiness reports the gateway's own subsystems by name: the configuration,
the registry and the outbound clients when a registry is configured, and the
stored-query store when one is. Its body names the phase of the process,
`booting`, `serving` or `draining`. On `SIGTERM` readiness turns `503` at
once, while the listener still accepts, so a load balancer stops sending
requests before the gateway stops taking them
([Stopping without dropping a request](#stopping-without-dropping-a-request)).

No member node and no identity source gates readiness. A node outage is
reported per query in `meta.federation` (§11), and a gateway that went unready
with one node would turn one CDR outage into a total outage. Their state is on
`GET {base}/health/dependencies` instead:

```json
{
  "endpoints": { "node-a-query": "up", "node-b-query": "down" },
  "resolver": "up",
  "consent": "down",
  "localizer": "up"
}
```

Each state is the one the last request the gateway made for a client
observed, and it reports the member's reachability and health, never whether
that request was valid:

| State | The last request |
|---|---|
| `up` | got an answer below `500`, a refusal such as `400`, `401`, `404` or `409` included |
| `failing` | got a `5xx` answer |
| `down` | got no answer: the member could not be reached, or did not answer in time |
| `unknown` | none has reached the member since the registry was loaded or reloaded |

The state reads the node's own HTTP status, whatever the call's §11.1 record
in `meta.federation` says: a query member that answered `400` is
`node-error` there and `up` here. The gateway sends no request of its own to
find out, and a request that never left the gateway changes nothing. Every
call that sends a request to a member updates it: a federated query, a
request routed to one node, the ask-all probe, a fan-out template upload, and
a stored-query distribution, repair or drift check. A drift check that finds
a member's copy different or missing records the member `up`, because it
answered. A resolution updates `resolver`, which is absent when no resolver
is configured. A call to the consent pre-filter updates `consent` by the same
rule: a decision, or an answer below `500`, is `up`, a `5xx` is `failing`, and
no answer is `down`. It is absent when no pre-filter is configured
([Consent](consent.md)). A call to the localizer updates `localizer`
by the same rule: a candidate set, or an answer that no member holds the
patient, is `up`, a failure answered below `500` is `up`, a `5xx` is
`failing`, an XCPD exchange whose audit message could not be recorded is
`failing`, and no answer, a silent localizer past its budget included, is
`down`. It is absent when no localizer is configured
([Node selection](registry.md#node-selection)). A call to the PDQm
Supplier updates `demographics` by the same rule: an answer, no match or an
ambiguous one included, is `up`, a `5xx` is `failing`, an exchange whose
audit record could not be stored is `failing`, and no answer is `down`. It
is absent when `[pdqm]` is not configured
([Demographics first](identity.md#demographics-first-pdqm)). A refresh of the mCSD
directory the registry is read from updates `directory`: an answer the
gateway accepts is `up`, an answer whose change the gateway refuses is
`degraded`, an HTTP error (a `4xx` included), an answer that breaks ITI-90
or ITI-91 or one past a cap is `failing`, and no answer before the deadline
is `down`. While the directory is `degraded`, `directory_fault` names the
class of the refusal: `registry-invalid` when the changed registry breaks a
rule of its own, and `configuration-mismatch` when it is sound and the rest
of the configuration does not fit its members. The gateway then serves the
registry it last accepted, and the directory stays `degraded` until a later
refresh is accepted. A directory that answers `401` or `403` is `failing`
with `directory_fault = "refused-credentials"`: check the credentials of
`[registry.mcsd]`. `directory_fault` is absent in every other state, and it
names a class only, never a member, an endpoint or a URL. Both are absent when the
registry is a document
([The registry](registry.md#the-registry-read-from-an-mcsd-directory)). The
[audit repository](localization.md#the-audit-repository) shows as
`audit_repository`, read from its spool at each request: `up` when the last
delivery succeeded and nothing waits, `degraded` while the gateway retries a
failed delivery, while audit messages wait in the spool and while any sits in
its quarantine, and `unknown` before the first message. It is absent when the audit messages go elsewhere. The
FHIR Feed repository of `[audit]` shows as `audit_feed` in the same states,
absent unless `destination = "repository"` ([The audit trail](audit.md)). The
PMIR Patient Identity Registry shows as `identity_registry`, from the
identity feed's last check: `up` while the gateway holds a subscription in
`requested` or `active`, `failing` after a refusal, an answer that breaks
ITI-94, or a create the gateway cannot manage, and `down` when the Registry
did not answer. `identity_registry_fault` names why it is not up:
`unreachable`, `refused`, `malformed`, `unmanageable`, or `audit-failed`
when an exchange's audit record could not be stored. Both are absent
without `[pmir]`
([The identity feed](identity.md#the-identity-feed-pmir)). The
body names endpoint ids and states only, never a URL, a credential or a
body.

## Stopping without dropping a request

On `SIGTERM` or `SIGINT` the gateway stops in three steps. No specification
governs this: our own design.

1. Readiness answers `503` at once, with the phase `draining`, and the
   listener keeps accepting for `server.drain_delay_ms`. A request that
   arrives in this window is served like any other.
2. The listener closes. Each connection finishes the request it is serving
   and is closed, and no new connection is accepted.
3. The requests in flight get `server.shutdown_timeout_ms` to finish. A
   connection still open after that is dropped.

The delay exists because a load balancer does not stop routing the moment a
process is asked to stop. A balancer that polls readiness needs up to one
polling period, plus its failure threshold, to see the `503`. In Kubernetes,
removing a terminating pod from a Service's endpoints runs alongside the
`SIGTERM`, not before it
([Pod termination](https://kubernetes.io/docs/concepts/workloads/pods/pod-lifecycle/#pod-termination)).
Set the delay to the longest of those times. It is `0` by default, which
closes the listener at once: right for a single process with nothing in
front, and the reason a gateway behind a balancer sets it.

`server.shutdown_timeout_ms` defaults to `server.request_timeout_ms`, and
`config check`, `serve` and a reload refuse a value shorter than that,
naming both keys. A request accepted just before the listener closes may
run for the whole request timeout, so a shorter drain would cut it, and a
federated query that runs to its overall budget would be lost on every
rolling restart.

The runtime's grace period must outlast both steps. Docker sends `SIGKILL`
after `stop_grace_period`, and the kubelet after
`terminationGracePeriodSeconds`, counted from the moment the pod is deleted.
Keep each above `drain_delay_ms` plus `shutdown_timeout_ms`, with room for
the background tasks to stop and for the last metrics and spans to be
pushed. The shipped examples keep 10 seconds of room:

| Example | `drain_delay_ms` | `shutdown_timeout_ms` | Grace period |
|---|---|---|---|
| `deploy/kubernetes/` | 5000 | 30000 | `terminationGracePeriodSeconds: 45` |
| `deploy/compose/` | unset, `0` | 30000 | `stop_grace_period: 40s` |
| the quickstart `compose.yaml` | unset, `0` | unset, the 30-second request timeout | `stop_grace_period: 40s` |

`scripts/checks/kubernetes-example.sh` and
`scripts/checks/release-compose.sh` fail when an example's grace period
does not outlast its delay plus its drain.

## `ferrofed healthcheck`

```text
ferrofed healthcheck --config /etc/ferrofed/ferrofed.toml
```

The command reads the configuration the way `serve` does and asks the
gateway running on this host for `GET {base}/health/readiness`. It connects
to the port of `server.listen`, on `127.0.0.1` or `[::1]` when the address
is a wildcard and on the address itself otherwise, and prints one line. It
exits `0` only when readiness answers `200` within three seconds, and `1` for
any other status, a refused connection, no answer in time, or a
configuration that does not load: the two codes a container runtime's health
check reads. With `[server.tls]` set it asks over `https` and accepts exactly the
certificate `certificate_file` holds, presenting `healthcheck_identity_file`
where the listener requires a client certificate
([TLS on the listeners](configuration.md#tls-on-the-listeners)). The [image](container.md#the-image) runs it as its
`HEALTHCHECK`, and so does the `compose.yaml` gateway service.

A Kubernetes pod probes the routes directly
([Kubernetes](container.md#kubernetes)): startup and readiness on
`{base}/health/readiness`, liveness on `{base}/health`. Point every probe at
the base path you configured, because every path outside it answers `404`.
