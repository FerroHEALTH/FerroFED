<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The usage report

`ferrofed serve` sends a small JSON report to FerroPULSE, a collector that
Cadasto B.V. runs for every FerroHEALTH product. It goes out at each process
start and once a day. It says that the gateway is alive, which version it
runs, how many member nodes it federates as a range, which identity and
consent services it is configured with, and how fast it answers, in coarse
aggregates. The report is on by default. Every boot logs one INFO line that
says whether it is on and how to switch it off.

This page lists every field the report carries, says when it is sent, gives
the switch, and carries the privacy notice for the data the collector stores.
The wire format is FerroPULSE's report envelope v2; no openEHR or Federation
Tier specification governs any of it, and the report is FerroFED's own
design.

<!-- toc -->

## When a report is sent

There are two kinds of report, and both are one HTTPS `POST` to
`https://report.ferropulse.eu/v1/report` with a JSON body of at most 64 KiB.

- **`start`:** at each `serve` start, once the gateway listens. A gateway
  sends at most one start report per ten minutes: the time of the last one is
  kept in the state directory, so a container in a restart loop sends one
  start report per ten minutes, not one per restart.
- **`daily`:** the first attempt 10 minutes after the start report, then
  about every 24 hours (23.5 to 24.5 hours after the previous one, spread at
  random). It covers the requests since the previous daily report of the same
  process, or since the gateway started.

The jobs `config check`, `healthcheck`, `admission check`, `conformance run` and
`report` send nothing.

The report needs a state directory, `[usage_report] state_dir`
(`FERROFED__USAGE_REPORT__STATE_DIR`), where the gateway keeps the instance
id. Without one, or with one it cannot write, the gateway sends nothing:
without a stable id each restart would count as a new installation. The
release's compose file, the quickstart's `compose.yaml` and the Kubernetes
example turn it on: each sets
`FERROFED__USAGE_REPORT__STATE_DIR=/var/lib/ferrofed/usage-report` on the
writable volume at `/var/lib/ferrofed`, and the two compose files also set
`FERROFED__USAGE_REPORT__DEPLOYMENT=compose`. The container image sets no
state directory, so a plain `docker run` sends nothing until you mount a
writable volume and point the key at it:

```shell
docker run -v ferrofed-state:/var/lib/ferrofed \
  -e FERROFED__USAGE_REPORT__STATE_DIR=/var/lib/ferrofed/usage-report \
  ghcr.io/ferrohealth/ferrofed:<version>
```

A bind mount needs a host directory that uid 65532 can write.

The boot line, with the report on, reads:

```text
usage report ON: instance id, version, licence type, deployment, uptime, host size, member count as a range, the configured identity services, coarse request latency per route group and node request counts, at each start and once a day; no patient data, AQL, identifiers, node names, URLs or request paths. Off: FERROFED__USAGE_REPORT__ENABLED=false. Details: https://ferrofed.eu/docs/operate/usage-report.html
```

It carries the configured endpoint as the structured field `endpoint`. With
the report off, the line says why, for example:

```text
usage report OFF: switched off (FERROFED__USAGE_REPORT__ENABLED=false); nothing is sent. Details: https://ferrofed.eu/docs/operate/usage-report.html
usage report OFF: CI detected (GITHUB_ACTIONS); nothing is sent. Details: https://ferrofed.eu/docs/operate/usage-report.html
usage report OFF: no state directory (FERROFED__USAGE_REPORT__STATE_DIR is unset), and without a stable instance id each restart would count as a new installation; nothing is sent. On: mount a writable volume and set FERROFED__USAGE_REPORT__STATE_DIR to it. Details: https://ferrofed.eu/docs/operate/usage-report.html
```

## What a report contains

Every report carries these fields:

| Field | Sent in | What it is |
|---|---|---|
| `schema` | both | Always `2`, the version of the report format. |
| `event` | both | `start` or `daily`. |
| `first_start` | `start` | `true` when this start created the instance id, which marks a new installation; `false` on every later start. |
| `product` | both | Always `ferrofed`. |
| `instance_id` | both | A random UUID (version 4), created at the first start and stored in `<state_dir>/instance-id`. It is not derived from the host or anything else about the deployment. |
| `version` | both | The gateway version, for example `0.0.12`. |
| `git_sha` | both | The commit the release lane built the binary from, 7 to 40 hex digits. A local build sends `0000000`. |
| `licence` | both | The licence grant type, `non-commercial` (what the Business Source License grants everyone). |
| `deployment` | both | How the gateway was deployed: `helm`, `compose`, `binary` or `unknown`, from `[usage_report] deployment`, which the compose file sets to `compose`. |
| `uptime_s` | both | Seconds since this process started. |
| `cpu_bucket` | both | The CPUs available to the process, as a range: `1-2`, `3-4`, `5-8`, `9-16` or `17+`. |
| `memory_bucket` | both | The memory available to the process in GiB, the smaller of the host's memory and the container limit, as a range: `<4`, `4-8`, `8-16`, `16-32` or `32+`. Off Linux the gateway cannot read it and sends `<4`. |
| `status` | both | The state facts below. |
| `metrics` | `daily` | The performance aggregates below. |

The `status` object:

| Member | What it is |
|---|---|
| `profile` | The deployment profile, `production` or `development`. |
| `members` | The member nodes of the registry the gateway federates, as a range: `0`, `1-2`, `3-5`, `6-10`, `11-50` or `51+`. Never a node name, an endpoint id or a URL. |
| `services` | The identity, localization and consent services the configuration names, sorted: `mcsd`, `mitz`, `nvi`, `pdqm`, `pixm`, `pmir`, `xcpd`. Never a service URL or a credential. |
| `stored_queries` | Whether the gateway offers the stored-query registry. |
| `fhir` | Whether the gateway serves the FHIR R4 face of the exchange format. |

A start report, formatted here for reading (the gateway sends it on one
line):

```json
{
  "schema": 2,
  "event": "start",
  "first_start": true,
  "product": "ferrofed",
  "instance_id": "5f0c6b2e-3d4a-4c8b-9e1f-2a3b4c5d6e7f",
  "version": "0.0.12",
  "git_sha": "1a2b3c4d",
  "licence": "non-commercial",
  "deployment": "compose",
  "uptime_s": 4,
  "cpu_bucket": "1-2",
  "memory_bucket": "<4",
  "status": {
    "profile": "production",
    "members": "3-5",
    "services": ["pdqm", "pixm"],
    "stored_queries": true,
    "fhir": false
  }
}
```

### The daily metrics

The daily report adds one object that covers the window since the previous
daily report:

```json
"metrics": {
  "window_24h": {
    "requests_bucket": "1k-10k",
    "groups": {
      "query": {
        "p50_ms": 180.2, "p95_ms": 610.0, "p99_ms": 1400.5,
        "errors_5xx": 0,
        "histogram": [0, 0, 4, 40, 300, 900, 500, 60, 5, 1, 0]
      }
    },
    "counters": {
      "node_requests": 3620, "node_timeouts": 4, "node_errors": 2,
      "responses_4xx": 12
    }
  }
}
```

- **`requests_bucket`:** the number of counted requests in the window, as a
  range: `0`, `1-100`, `100-1k`, `1k-10k`, `10k-100k` or `100k+`. Each range
  includes its lower bound and excludes its upper one, so `1-100` means 1 to
  99. The health probes (`/health`, `/health/readiness`) and the JWK Set are
  not counted.
- **`groups`:** one entry per group of requests that saw at least one in the
  window. A request is placed in a group by the route template it matched
  under `{base}`, never by the path or the parameters the client sent:

  | Group | Routes |
  |---|---|
  | `query` | `{base}/v1/query/aql`, and a stored query under `{base}/v1/query/` |
  | `composition` | `{base}/v1/ehr/{ehr_id}/composition` and every path under it |
  | `ehr` | every other route under `{base}/v1/ehr` |
  | `definition` | every route under `{base}/v1/definition/` |
  | `fhir` | every route under the FHIR face's base |
  | `operator` | every route under `{base}/operator/` |
  | `other` | every other route, `OPTIONS {base}/` among them, and a request no route matched |

- **`p50_ms`, `p95_ms`, `p99_ms`:** the median, 95th and 99th percentile
  latency of the group in milliseconds, estimated from its histogram.
- **`errors_5xx`:** the responses with a 5xx status.
- **`histogram`:** 11 request counts, one per latency bucket. The bucket upper
  bounds in milliseconds are 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000,
  and no bound for the last.
- **`counters`:**
  - `node_requests`: the requests the gateway sent to a member node, a
    federated query's fan-out, a routed request and an ask-all probe alike.
    `node_requests` over the `query` group's requests is the average fan-out.
  - `node_timeouts`: the node requests reported `time-out` (§11.1).
  - `node_errors`: the node requests reported `node-error` or `offline`
    (§11.1).
  - `responses_4xx`: the responses with a 4xx status over every counted
    group.

## What a report never contains

A report carries no patient data, no patient identifier, no AQL text, no
query parameter, no result row, no `ehr_id` or other openEHR identifier, no
request path, no node name, endpoint id or URL, no service URL, no
credential, no error message, no user name, no client IP address, no host
name and no customer or staff name. The report types in the gateway have no
field that could hold any of these, and the collector refuses a report with a
value that is not a boolean, a number or a short lower-case token.

The collector does record the IP address the report arrives from. See
[What the collector stores](#what-the-collector-stores).

## Switch it off

```shell
FERROFED__USAGE_REPORT__ENABLED=false
```

or, in the configuration file:

```toml
[usage_report]
enabled = false
```

The release's compose file and the quickstart's `compose.yaml` forward this
variable from your shell or `.env`, so
`FERROFED__USAGE_REPORT__ENABLED=false docker compose up` switches the report
off there too. The quickstart's FerroEHR nodes and its FerroPIX report to
FerroPULSE as well, each as an installation of its own product, and the same
`FERROFED__USAGE_REPORT__ENABLED=false` stops theirs.
`FERROEHR__USAGE_REPORT__ENABLED`, when you set it, decides for the nodes
alone. In a CI job the report is off by itself: when
`GITHUB_ACTIONS` or `CI` is `true` or `1`, the gateway sends nothing, and
`enabled = true` does not change that. A container does not inherit the
runner's environment, so a test lane that starts the image passes
`--env FERROFED__USAGE_REPORT__ENABLED=false`; the guard
`scripts/checks/usage-report-off.sh` fails a launcher in this repository that
does not.

With the report off, the gateway sends nothing to FerroPULSE and writes
nothing to the state directory, and the boot line says `usage report OFF`.
Nothing else in the gateway changes. Switching it off stops new reports only.
Reports already stored at the collector stay until the retention period ends,
unless you ask for them to be erased. A change of `[usage_report]` takes a
restart ([Reloading the registry](registry.md#reloading-the-registry)).

## Network

- **Destination:** outbound HTTPS to `report.ferropulse.eu`, port 443.
  `[usage_report] endpoint` moves it; the gateway refuses to start on an
  endpoint that is plain HTTP (except to a loopback host) or that carries
  credentials.
- **One direction only:** the gateway reads the status code of the answer and
  nothing else. It does not follow redirects, and nothing the collector
  answers changes the gateway's configuration or behaviour.
- **Failures stay local:** a send gives up after 5 seconds without a
  connection or 10 seconds in total. A failure is logged at DEBUG, and a
  refusal by the collector at WARN. Boot, readiness and request handling
  never wait on the report, so a slow or unreachable collector affects none
  of them. A daily report that does not reach the collector is tried again an
  hour later with the same window; one the collector answered, even with a
  refusal, is not resent.

## Privacy notice

This section gives the information that Articles 13 and 14 of the
[GDPR](https://eur-lex.europa.eu/eli/reg/2016/679/oj) list, for the personal
data a report and its stored record can contain. A report names no person.
It can still relate to one: the collector stores the address each report
comes from, and a gateway run by one person on their own internet connection
can be linked to that person through the address or through the instance id.
Where that is so, the report is personal data, and this notice applies to it.
The collector is not anonymous.

### Controller

Cadasto B.V., the Netherlands. Write to
[info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>. Cadasto has not appointed a data
protection officer.

### Purposes and lawful basis

Cadasto processes the reports for three purposes:

- to know which FerroFED versions run in the field, so it can tell how many
  installations a security release concerns and whether they upgrade;
- to know which identity and consent services the installations use and how
  large their federations are, as ranges;
- to see how the routes and the member nodes perform in real use, so it can
  find and fix slow paths.

The lawful basis is Art. 6(1)(f) GDPR, legitimate interest. The interest is
Cadasto's, as the maker of FerroFED: keeping the product it publishes secure
and maintained, which needs the versions in use and the reach of each
security release, and improving it, which needs the service mix and field
performance. The data come from the FerroFED gateway itself. You are not
obliged to send them: the report can be switched off, and switching it off
changes nothing else in FerroFED.

### What the collector stores

- Every report as it arrives, with the fields listed above.
- With each report: the time of receipt, the UTC day, the source IP address
  (IPv4 or IPv6), and the country derived from that address.
- Per instance id: when it was last seen, its current version, and whether it
  is alive.
- Per day: latency totals summed across the whole fleet, without instance ids.

The country comes from an IP-to-country database bundled with the collector,
so no address leaves the collector host for the lookup. IP geolocation by
[DB-IP](https://db-ip.com), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Reports from the
address ranges of GitHub-hosted CI runners are marked as such and left out of
every dashboard.

### Recipients and location

Hetzner Online GmbH (Gunzenhausen, Germany) hosts the collector in its cloud
and processes the reports as Cadasto's processor, under a data processing
agreement (Art. 28 GDPR). The collector runs in Hetzner's data centre in
Falkenstein, Germany. The FerroPULSE dashboard that shows the reports sits
behind a GitHub login with no public or anonymous access; only members of the
FerroHEALTH organisation can sign in.

### Retention

A daily job deletes reports and start reports 13 months after they were
received, an instance record 13 months after the instance was last seen, and
the fleet-wide daily totals 13 months after their day. Backups of the
collector's database are kept for 14 days, so a deleted report can remain in a
backup for at most 14 days more.

### Your right to object

> **You have the right to object** at any time, on grounds relating to your
> particular situation, to the processing of reports that are personal data
> concerning you (Art. 21(1) GDPR).
>
> **To stop future reports**, switch the report off as described in
> [Switch it off](#switch-it-off). No request to anyone is needed.
>
> **To have the reports already stored erased**, write to
> [info@cadasto.com](mailto:info@cadasto.com) with the instance id, which the
> gateway keeps in `<state_dir>/instance-id`. Under the release's compose
> file it sits on the `audit-spool` volume:
> `docker run --rm -v <project>_audit-spool:/s busybox cat /s/usage-report/instance-id`
> prints it. Cadasto then deletes every report, start report and instance
> record stored for that instance id, source addresses included
> (Art. 17(1)(c) GDPR). Switch the report off first, or the next report starts
> a new record.
>
> The fleet-wide daily totals hold no instance ids, so an erased instance's
> share in days already summed stays in those totals.

### Your other rights

At the same address you can ask for access to the reports stored for your
instance id (Art. 15), for their rectification (Art. 16) or erasure (Art. 17),
or for restriction of their processing (Art. 18). You have the right to lodge
a complaint with a supervisory authority, in particular in the Member State of
your habitual residence, place of work or place of the alleged infringement
(Art. 77).
