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
set: their token is bound to the client certificate of the connection, which
`ferrohealth-perf` 0.2.0 cannot present.

## Reading the report

The job's summary names each breach by its metric, such as
`idle.transactions_per_second` or `patient-query.p95_ms`. The artifact
`perf-report` holds the JSON reports, the gateway's log and the baseline the
run would write. Exit 1 is a breach. Exit 2 means the run cannot be judged:
no entry for the runner's CPU model, a generator that lagged, or a window
that was not steady.

## Updating the baseline

`perf/baseline.toml` holds one entry per CPU model, because GitHub's hosted
runners mix CPU generations and a latency is comparable only on a like CPU.
The `perf` job of `ci.yml` runs with `--update`, so the `perf-report` artifact
always carries the baseline with the runner's entry written or lowered.

- A run on a CPU model with no entry exits 2 and names the model. Take
  `perf/baseline.toml` from that run's artifact and commit it.
- An entry only moves to the better value on its own. A raise is a hand edit
  in a pull request that says why.
- Each entry's `[baseline.tolerance]` is FerroFED's own: latency may rise 50 %
  past the larger of the entry and its noise floor, since a p99 of 240
  samples at 20 requests a second moved by more than 25 % between two runs
  of one commit on one CPU model; throughput 20 %, memory 20 %, CPU 25 % and
  transactions 10 %, the tool's defaults.
- A change of the plan's rate, window or request set writes each entry
  afresh, so commit an entry for every model the runner pool serves after
  such a change.

## Running it yourself

The gate reads the gateway's CPU and memory from `/proc`, so it runs on Linux
with Docker, Compose, `curl` and `openssl`:

```sh
cargo install ferrohealth-perf --version =0.2.0 --locked
cargo build --release --locked -p ferrofed-server --bin ferrofed
bash scripts/perf/gate.sh run
```

`bash scripts/perf/gate.sh --self-test` checks the pin reader, the plan
rendering, the registry rewrite and the two plans' idle limits on any
machine.
