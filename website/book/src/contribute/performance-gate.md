<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The performance gate

Every pull request that changes what the gateway is built from runs the
`perf` job, and so does every push to `main` and every release. It fails when
the gateway does work while idle, or is slower, heavier or busier under load
than the baseline the repository commits. FerroPIX shipped an outbox sender
that busy-looped at about 8,000 PostgreSQL transactions a second while idle,
and nothing caught it before release; this gate catches that shape. No
specification governs it: it is FerroFED's own design over the family's
measuring tool, `ferrohealth-perf`.

## What runs

`scripts/perf/gate.sh run` builds nothing itself. The `perf` workflow builds
the release binary and installs `ferrohealth-perf` at its pin in
`docs/VERSIONS.md`, then the script:

1. starts the quickstart's four FerroEHR nodes and FerroPIX from
   `compose.yaml`, with `perf/compose.yaml` publishing FerroPIX on loopback,
   and seeds both with `docker/quickstart/patients.tsv`;
2. starts a PostgreSQL server over TLS for FerroFED's own store;
3. starts `target/release/ferrofed` with `perf/ferrofed.toml`: the
   quickstart's gateway with the stored-query registry on the store;
4. runs `ferrohealth-perf` with `perf/plan.toml` against
   `perf/baseline.toml`;
5. plants a poller of the store that never waits, and runs the idle phase of
   `perf/busy-loop.toml` beside it, which must fail on idle transactions.

The idle phase holds the gateway to 2 % of one core and one committed
transaction a second for 60 seconds, with the store's sweep and every other
background worker running. The load phase sends 20 requests a second for 60
seconds over the gateway's hot paths: the self-description, a patient's
federated query, a population query, a stored query run by name and a
template list routed to the one member the registry names. A national contact point's routes are not in the
set yet: their token is bound to the client certificate of the connection or
to a `DPoP` key, which the tool presents from 0.4.0 on, and
[#1702](https://github.com/FerroHEALTH/FerroFED/issues/1702) adds them.

## Reading the report

The job's summary names each breach by its metric, such as
`idle.transactions_per_second` or `patient-query.p95_ms`. A load metric that
breaches is judged again on up to two fresh load phases, and the gate fails
only when it breaches in most of the runs; the check says in how many. The
artifact `perf-report-attempt-<n>` holds the JSON reports, every phase included, and the
gateway's log. Exit 1 is a breach. Exit 2 means the run cannot be judged: no
entry for the runner's host shape, a generator that lagged, a window that was
not steady, or a runner that stalled (every route's p99 over ten times its
p50 at once).

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). An
attempt that ends with exit 2 runs again on a fresh runner on any CPU model,
since a run that cannot be judged says nothing about the code. An attempt
that ends with exit 1 runs again only on a CPU model the perf workflow lists
in `STALL_PRONE`. At most two retries run, and the gate passes when the last
attempt passed. A breach on any other model fails at once. The job summary names each attempt's CPU model, its verdict and why a
retry ran, and each attempt uploads its reports as `perf-report-attempt-<n>`.

## Updating the baseline

`perf/baseline.toml` holds one entry per host shape, the CPU model and the
memory size to the GiB together, because GitHub's hosted runners mix CPU
generations and sizes and a latency is comparable only on a like host.

- An entry is measured by an update: set `update: true` on the `perf` job's
  call of `.github/workflows/perf.yml` in `ci.yml` on a branch. An update
  measures five load phases, each after its own warm-up, and writes each
  metric as their median and its noise floor as how far the worst phase
  strayed from it.
- Machines of one shape still differ, so re-run the update job several
  times, take each run's `perf/baseline.toml` from its `perf-report-attempt-<n>`
  artifact, and fold them into the committed file with
  `ferrohealth-perf merge --baseline perf/baseline.toml job-1.toml job-2.toml`.
  Each entry then takes the median over the jobs, and its floor the spread
  between them.
- Every tolerance stays at the tool's default: latency and CPU 25 %,
  throughput 20 %, memory 20 % and transactions 10 %. A raise is a hand edit
  in a pull request that says why.
- A run on a host shape with no entry exits 2 and names the shape; its entry
  is added the same way. A change of the plan's rate, window or request set
  writes each entry afresh.

## Running it yourself

The gate reads the gateway's CPU and memory from `/proc`, so it runs on Linux
with Docker, Compose, `curl` and `openssl`:

```sh
cargo install ferrohealth-perf --version =0.4.1 --locked
cargo build --release --locked -p ferrofed-server --bin ferrofed
bash scripts/perf/gate.sh run
```

`bash scripts/perf/gate.sh --self-test` checks the pin reader, the plan
rendering, the registry rewrite and the two plans' idle limits on any
machine.
