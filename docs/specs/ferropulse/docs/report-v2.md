# Report v2: the contract

Envelope v2 is one report shape for every FerroHEALTH product: FerroEHR,
FerroTERM, FerroBRIDGE and FerroFED. A new product reports on it with no
FerroPULSE change (owner decision 2026-10-16). The collector accepts v1
([report-v1.md](report-v1.md), FerroEHR only, for installed versions) and v2
side by side; the body's `schema` selects the envelope. The JSON Schema is
`schemas/report-envelope.v2.json`, with examples under
`schemas/examples/report-envelope.v2/`.

## Transport

Unchanged from v1: `POST https://report.ferropulse.eu/v1/report` (the path
stays `/v1/report`), `Content-Type: application/json`, at most 64 KiB. The
collector answers `204` when it stored the report, `400` with an empty body
when it refused it, and `429` when the source sent too much (60 requests a
minute with a burst of 20 per source address, an IPv6 one per /64) or a
second start event within 10 minutes. A client uses a 5 s connect and 10 s
total timeout, follows no redirect, and uses https except to a loopback host.

Two events per instance:

- `start`: once per process start, at most one per instance per 10 minutes.
  No `metrics`.
- `daily`: every 24 hours, one per instance per UTC day; a second one on the
  same day replaces the first, whatever envelope version either came on.
  Carries `metrics`.

## Envelope

Every member is required unless noted, and an unknown member is refused.

| Field | Type | Values |
|---|---|---|
| `schema` | integer | `2` |
| `event` | string | `start`, `daily` |
| `first_start` | boolean | `start` only: `true` when this start created the `instance_id` |
| `product` | string | `ferroehr`, `ferroterm`, `ferrobridge`, `ferrofed` |
| `instance_id` | string | UUID, random per installation, hyphenated |
| `version` | string | SemVer, at most 64 characters, at most 9 digits per number |
| `git_sha` | string | 7 to 40 lower-case hex characters |
| `licence` | string | `non-commercial`, `commercial` |
| `deployment` | string | `helm`, `compose`, `binary`, `unknown` |
| `uptime_s` | integer | 0 to 315,576,000 (ten years) |
| `cpu_bucket` | string | `1-2`, `3-4`, `5-8`, `9-16`, `17+` |
| `memory_bucket` | string | GiB: `<4`, `4-8`, `8-16`, `16-32`, `32+` |
| `status` | object | both events: the product's state facts (below) |
| `metrics` | object | `daily` only, required there and refused on `start` (below) |

The v1 members `spec_profile`, `db_ok`, `migrations_ok` and
`postgres_major` are refused on v2; FerroEHR sends them in `status`.

## `status`

An object of 0 to 32 members. A member name matches
`^[a-z][a-z0-9_]{0,31}$`. A value is one of:

- a boolean;
- an integer from 0 to 10,000,000;
- a token: a string matching `^[a-z0-9][a-z0-9._+-]{0,31}$`;
- an array of 0 to 16 unique tokens, in any order.

Nothing else: no `null`, no object, no fraction, no free text.

## `metrics`

```json
{
  "window_24h": {
    "requests_bucket": "1k-10k",
    "groups": {
      "lookup": {
        "p50_ms": 1.2, "p95_ms": 4.0, "p99_ms": 9.5, "errors_5xx": 0,
        "histogram": [1200, 800, 300, 90, 20, 6, 2, 1, 0, 0, 0]
      }
    },
    "counters": { "fhir_r4": 120, "fhir_r4b": 2300, "fhir_r5": 0, "fhir_r6": 3 }
  }
}
```

- `window_24h` is the only member, and it has exactly these three.
- `requests_bucket`: `0`, `1-100`, `100-1k`, `1k-10k`, `10k-100k`, `100k+`.
- `groups`: 0 to 32 members, names as for `status`, one per group of
  requests with requests in the window, grouped by route template. Each
  value has exactly `p50_ms`, `p95_ms` and `p99_ms` (0 to 600,000),
  `errors_5xx` (a count) and `histogram`: 11 counts per latency bucket with
  upper bounds in ms `5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, +Inf`.
  Buckets add up across instances, so the fleet percentile comes from sums,
  never from averaging p95s.
- `counters`: 0 to 64 members, names as for `status`, each a count.
- Every count is at most 10,000,000 per 24 hours.

## Each product's keys

The collector checks the shape above and leaves the key names to the
products. The lists below document what each product sends; a change to a
product's keys updates its list here.

### FerroEHR (from its next release; earlier versions send v1)

- `status`: `db_ok` (boolean), `migrations_ok` (boolean), `postgres_major`
  (integer, 13 to 99), `spec_profile` (token): the four v1 envelope members.
- `groups`: the v1 route groups `ehr`, `composition`, `contribution`,
  `query`, `definition`, `directory`, `admin`, `other`, plus `aql`: the AQL
  execution histogram, its p50, p95 and p99, and `errors_5xx` 0.
- `counters`: `aql_executions`, `aql_slow` (over the configured threshold).

### FerroTERM

- `status`: `index_ok` (boolean, at least one artifact loaded and serving),
  `code_systems` (tokens from `snomed`, `loinc`, `icd10`, `icd10cm`, `icd11`,
  `rxnorm`, `dhd`, `gstandaard`, `labcodeset`, `fhir`, `other`; families
  only, never an edition, a module id or a URL), `persistence` (boolean, a
  resources database is configured). FerroTERM serves no user interface since
  its console became a binary of its own, so its status carries no `viewer`.
- `groups`, by matched route template under a FHIR base: `lookup`,
  `validate_code`, `expand`, `subsumes`, `translate`, `closure`, `metadata`,
  `read` (read, history, vread), `search`, `write` (create, update, delete),
  `other`. Routes outside a FHIR base are not counted.
- `counters`: `fhir_r4`, `fhir_r4b`, `fhir_r5`, `fhir_r6`: FHIR-base requests
  per FHIR version.

### FerroFED (proposed in FerroHEALTH/FerroFED#1122)

- `status`: whether the registry and the identity cross-reference service
  answer, the member node count as a bucket token, whether the viewer is
  mounted.
- `groups`: federated query, EHR, composition, admin, other.
- `counters`: node timeouts, node errors, and the fan-out per query.

### FerroBRIDGE (proposed in FerroHEALTH/FerroBRIDGE#581)

- `status`: the enabled targets as tokens (`fhir`, `omop`), the loaded
  mapping count as a bucket token, whether the openEHR CDR answers.
- `groups`: openEHR to FHIR, FHIR to openEHR, openEHR to OMOP, other.
- `counters`: mapping failures.

Each product settles its proposed names in its issue and then lists them
here.

## Where the collector keeps them

As v1: a `daily` report is a row in `pulse.report` and a `start` report a
row in `pulse.start_event`; both update `pulse.instance`. A v2 row stores
`status` as sent and leaves the four v1 columns empty. The column `facts`
holds the state facts of both versions alike: the v2 `status`, or for a v1
FerroEHR row its v1 members under the same names. The dashboards read
`facts`, and read latency groups and counters through
`pulse.metric_groups` and `pulse.metric_counters`, which present the v1
routes, AQL entry and counts in the v2 shape.

## What the collector adds

`received_at`, `report_day` (the UTC date of receipt), `source_ip` (`inet`,
from Caddy's `X-Forwarded-For`), `country` (ISO 3166-1 alpha-2 from an
offline IP-to-country database, null when unknown), and `ci`.

`ci` names the CI provider whose runner address range held the source
address, and is null for every other sender. The collector reads the ranges
of GitHub-hosted Actions runners from the `actions` member of
`GET https://api.github.com/meta`
(<https://docs.github.com/en/rest/meta/meta#get-github-meta-information>)
at start and in its daily job, stamps each report as it arrives, and after
each refresh stamps the stored rows inside the retention period that a new
range matches. A failed refresh keeps the previous ranges. The dashboards
leave stamped rows out. The products also stay silent when the process
environment has `GITHUB_ACTIONS` or `CI` set; the stamp catches what that
misses, such as a container that does not inherit the runner environment.

## Retention and erasure

As v1: everything is deleted after 13 months, and
`ferropulse erase <instance_id>` deletes every report and start event of an
instance, its status, facts and stamp included.

Never part of a report: patient data, AQL text, request paths, codes,
system or value set URLs, ids other than `instance_id`, licence ids, host
names, customer or staff names. The v2 shape holds this by construction:
only booleans, bounded numbers and short tokens.
