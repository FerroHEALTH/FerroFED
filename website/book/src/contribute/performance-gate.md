<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The performance gate

Every pull request that changes what the gateway is built from runs the
`perf` job, and so does every push to `main` and every release. It fails
when the gateway does work while idle, or is slower, heavier or busier under
load than the build of its base, measured beside it on the same runner in
the same job. FerroPIX shipped an outbox sender that busy-looped at about
8,000 PostgreSQL transactions a second while idle, and nothing caught it
before release; this gate catches that shape. No specification governs it:
it is FerroFED's own design over the family's measuring tool,
`ferrohealth-perf`.

## Why the head is judged against its base

GitHub's hosted runners differ: two runners that report the same CPU model
and memory measured one commit 25 to 45 % apart in p50 latency and CPU per
request
([FerroEHR #5095](https://github.com/FerroHEALTH/FerroEHR/issues/5095)).
A baseline stored from one machine failed unchanged code on another, so the
gate keeps none. It builds the base of the change and the change itself,
the head, runs both on one runner, and judges the head over the base. The
difference between machines reaches both alike and cancels.

The base is:

- for a pull request, the merge base of the commit CI checks out with the
  pull request's base branch;
- for a merge group, its base on `main`;
- for a push to `main`, the commit before the push;
- for a manual dispatch, the previous release tag;
- for the release commit, the previous release tag, or the oldest commit
  after it that carries the gate contract when that release carries none
  (see below).

## What runs

The `perf` workflow's build job installs `ferrohealth-perf` at its pin in
`docs/VERSIONS.md`, and `scripts/perf/gate.sh build` builds the release
gateway of the base, from `git archive` of the base commit under
`target/perf-base/`, and of the head, and keeps the base tree's
configuration contract beside its binary. `scripts/perf/gate.sh pack` hands all
of them to the attempt jobs as one artifact, so an attempt builds nothing
and a retry measures at once (#1807). In each attempt,
`scripts/perf/gate.sh unpack` restores them and `scripts/perf/gate.sh run`
then:

1. starts the quickstart's four FerroEHR nodes and FerroPIX from
   `compose.yaml`, with `perf/compose.yaml` publishing FerroPIX on loopback,
   and seeds both with `docker/quickstart/patients.tsv`;
2. starts a PostgreSQL server over TLS for FerroFED's own store, with a
   database for each gateway;
3. checks with `ferrofed config check` that the base binary reads its own
   tree's configuration, and starts both gateways over the same nodes and
   FerroPIX, each under its own tree's configuration contract: the head on
   port 8080 and the base on port 8070, each with the stored-query registry
   on its own store database;
4. runs `ferrohealth-perf ab` over `perf/plan.toml`;
5. plants a poller of the head's store that never waits, and runs the idle
   phase of `perf/busy-loop.toml` beside it, which must fail on idle
   transactions. That proves the gate still catches the case it was built
   for.

The A/B run has two parts:

1. **Idle.** No request for 60 seconds. The head's CPU time must stay under
   2 % of one core and its store under one committed transaction a second,
   with the store's sweep and every other background worker running. These
   limits are absolute and compared with nothing.
2. **Load, in pairs.** Four pairs, each a load phase against both gateways
   at once: 20 arrivals a second to each for 60 seconds after a 10-second
   warm-up, open loop, one gateway's arrivals half an interval after the
   other's, the gateway that leads alternating by pair. Run side by side,
   both arms see the same runner, whose speed can move by a third from one
   minute to the next. The arrivals go over the gateway's hot
   paths: the self-description, a patient's federated query, a population
   query, a stored query run by name, a template list routed to the one
   member the registry names, and a national contact point's patient
   summary under its treatment token and as an emergency access (see
   below). Every request must be answered with the status the plan names.

## A national contact point under load

`perf/ferrofed.toml` trusts a synthetic national contact point whose
tokens are bound to a `DPoP` key (RFC 9449), with the FHIR face for its
patient summary and the emergency access limit `[emergency] threshold = 5`
per `window_s = 1`. `gate.sh` makes the contact point's signing key, its
key set and the P-256 `DPoP` key under `target/perf-run/`, and mints its
tokens for the treatment and the emergency purposes, which the plan names
as `{ncp_token}` and `{ncp_emergency_token}`. Two request groups use them
([#1744](https://github.com/FerroHEALTH/FerroFED/issues/1744)):

| Group | Request | Token |
|---|---|---|
| `contact-point-summary` | `GET /fhir/Patient/$summary?identifier=urn:oid:2.999.1.1%7Cffd-test-0001` | the treatment token |
| `contact-point-emergency` | the same summary, as an emergency access | the emergency token |

Each request names the key in `dpop_key_file`, so `ferrohealth-perf` sends
its token in the `DPoP` scheme with a fresh proof, and carries the
`ncp-correlation-id` header the contact point declares. The tool sends the
plan's requests in turn, so the emergency group gets one arrival in seven,
under three a second at the plan's rate of 20, below the limit; a
self-test check fails a plan whose rate over its request count reaches 5.

The gateway counts every emergency access in its store, with or without an
access log ([#1737](https://github.com/FerroHEALTH/FerroFED/issues/1737)).
After the A/B run the gate sums the emergency accesses the head answered
as expected in the A/B report and reads how many attempts the head's store
wrote to `ferrofed.emergency_access`, from PostgreSQL's cumulative
statistics (one insert or update per attempt). Fewer attempts than answers
fail the gate, and the job summary names both counts. The base of the
change serves the same groups once it carries the contact point, which
every release from v0.0.17 does; a base that answers them otherwise only
adds a note.

## The onward lane under load

Node A is reached through an onward lane
([The onward lane of a node](../operate/onward-credentials.md#the-onward-lane-of-a-node)),
so every request a caller sends to node A has the caller's token exchanged
for one of node A's issuer, and the gateway caches the issued token. The
test kit's `ferrofed-perf-token` stands in for node A's authorization
server: `gate.sh` starts it before the nodes, it answers the
client-credentials grant and RFC 8693 token exchange with ES256 tokens, and
it writes its key sets under `target/perf-run/onward`, which
`perf/compose.yaml` mounts into node A beside `perf/ferroehr-onward.toml`,
the quickstart's node configuration with the token endpoint's issuers
trusted. The other three nodes keep their Basic user.

Node A's share of the load, the definition reads and its part of every
query, carries the exchange and the cache in both arms. A missing or broken
cache shows as one more round trip per request, which a 25 % latency
tolerance can miss, so the gate also counts the exchanges. After the A/B run
it sends 200 definition reads to node A through the head and reads the
token endpoint's `/calls`: more than 5 % as many exchanges as requests fails
the gate. Then it starts a third gateway, the head with `cache = false` on
node A's onward table, and sends it the same 200 reads; it must fail that
limit, which proves the count still catches an uncached lane. The job
summary names both counts.

## Each arm runs its own tree's configuration

Each arm boots under its own tree's configuration contract: the tree's
`perf/ferrofed.toml` and the `FERROFED__` overrides of its `perf/arm.env`,
one `NAME=VALUE` a line. The gate fills `@PORT@` with the arm's port and
`@DIR@` with its work directory, where it writes the store's connection
string, and sets `FERROFED_CONFIG` and `FERROFED__USAGE_REPORT__ENABLED=false`
itself. It refuses any other placeholder and any line that sets no
`FERROFED__` variable, with exit 2. A pull request that renames a key
changes both files for the head alone, so the base keeps reading the keys
its binary knows. Both arms start from the checkout, so a relative path in
either configuration reads the head's tree and the files `run` makes under
`target/perf-run/`.

An earlier gate started both arms with the head's `perf/ferrofed.toml`, so
both did the same work: when
[#1741](https://github.com/FerroHEALTH/FerroFED/pull/1741) gave the head
the contact point, the FHIR face and the PDQm client, its peak memory came
out at 1.24 of the base. That gate could not judge a change that renamed a
key, because the base binary refused the head's file at boot
([#1813](https://github.com/FerroHEALTH/FerroFED/issues/1813)). A change to
`perf/ferrofed.toml` that enables more work is now measured as part of the
head, the family rule FerroBRIDGE, FerroTERM and FerroPIX follow.

A base from before `perf/arm.env` has its overrides derived from its own
`scripts/perf/gate.sh`: the gate evaluates that script's `gateway_env()`
with its own `own_urls()` and constants and keeps the `FERROFED__` lines.
A tree with neither file, or a `gateway_env()` that sets nothing, is
refused with exit 2.

Before the gateways start, the gate runs `ferrofed config check` with the
base binary over the base's own configuration and overrides. A base that
refuses them stops the run with exit 2, naming each unknown key or quoting
the refusal:

```text
perf-gate: the base at <commit> does not know the key(s) emergency of its own perf/ferrofed.toml: the pair is not judged
```

### The release base

The release lane's base is the previous release. A release that carries no
gate contract (neither `perf/arm.env` nor a gate whose `gateway_env()` sets
a gateway's overrides; v0.0.16 has none) cannot be booted as an arm, so the
base is then the oldest commit after that tag that carries one.
`scripts/perf/gate.sh build` resolves it and writes the base and why to
`target/perf-base/base.md`, which the job summary shows, and the release
workflow adds that line to the release notes:

```text
Performance: The base is `<commit>`, the oldest commit after v0.0.16 that carries the gate contract, since v0.0.16 carries neither perf/arm.env nor a scripts/perf/gate.sh whose gateway_env() sets a gateway's overrides.
```

The release workflow refuses to draft a release whose gate named no base.

## Reading the report

Each load metric (throughput, latency p50, p95 and p99 per group, peak
memory, CPU time per request and transactions per request) is the median of
an arm's four phases. Its noise floor is the worse of the two arms' spread,
how far an arm's worst phase strayed from its median. A metric breaches when
the head's median is worse than the base's by more than the larger of its
tolerance and that floor. The tolerances are the plan's `[tolerance]`
table: latency and CPU 25 %, throughput 20 %, memory 20 %, transactions per
request 10 %. A metric fails only when the head also breaches against the
base in most of the pairs, so a stall in one phase clears and a regression,
which repeats in every pair, fails.

The job summary names the base commit, the head's idle limits, and per
metric the base, the head, their ratio, the tolerance, the floor, the
threshold and in how many pairs the head breached. Requests the head
answered other than expected fail the gate; those the base answered other
than expected, such as a route the change adds, are notes. Each attempt
uploads its reports as `perf-report-attempt-<n>`: the JSON reports under
`target/perf/`, both gateways' logs, the compose and seed logs, the base
commit and `base.md`, the line naming the base, so `ferrohealth-perf compare --report target/perf/latest.json`
judges a report again without a gateway.

The tool exits 0 for a pass, 1 for a breach or a request answered other
than expected, and 2 for a run it cannot judge: a phase whose generator
could not hold its rate, whose second half differs from its first, or that
stalled (every route's p99 over ten times its p50 at once), and a base that
fell behind the schedule.

## Retries

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). An
attempt that ends with exit 2 runs again on a fresh runner on any CPU model,
since a run that cannot be judged says nothing about the code. An attempt
that ends with exit 1 runs again only on a CPU model the perf workflow lists
in `STALL_PRONE`. An attempt job cancelled while the run itself goes on, by
a runner that shut down or a job that reached its time limit, has no exit
code and says nothing about the code either, so the next attempt runs. At
most two retries run, and `scripts/perf/verdict.sh` passes the gate when the
last attempt that ran passed. A breach on any other model fails at once, and
a run cancelled by its concurrency group or by a person retries nothing. The
job summary names each attempt's CPU model, its verdict and why a retry ran.

## Changing the plan or the tolerances

The plan is the head's: a change to `perf/plan.toml` is measured on both
gateways in the same pull request. A tolerance is the product's and moves
only in a pull request that says why.

## Running it yourself

The gate reads each gateway's CPU and memory from `/proc`, so it runs on
Linux with Docker, Compose, `curl`, `git` and `openssl`:

```sh
cargo install ferrohealth-perf --version =0.5.0 --locked
bash scripts/perf/gate.sh build origin/main
bash scripts/perf/gate.sh run
```

`bash scripts/perf/gate.sh --self-test` checks the pin reader, the plan
rendering, the registry rewrite, the base resolution with the release
base, each arm's overrides from `perf/arm.env` and from an older tree's
gate, the base's refusals and the two plans' idle limits on any machine.
