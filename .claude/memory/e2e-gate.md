---
name: e2e-gate
description: Container-backed tests run only with FERROFED_E2E=1 through the testkit harness (two FerroEHR instances as the two nodes, each with its own database and system_id, each behind a capturing and fault proxy, EHRbase for the node profile, the reference implementation for the differential run, all images pinned by digest); unset, they return early and the suite stays offline
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Landed with #39 (2026-10-01), on the FerroBRIDGE gate model; the topology
became two FerroEHR nodes with #155 (2026-10-02, [[two-ferroehr-nodes]]). The
harness is `tools/ferrofed-testkit`:

- `containers.rs`: `two_nodes()` starts one pinned FerroEHR PostgreSQL
  container holding a database per node (`ferroehr_a`, `ferroehr_b`, through
  `docker/postgres/20-ferrofed-node-databases.sh`, decision A47), then node A
  (`NODE_A_SYSTEM_ID`) and node B (`NODE_B_SYSTEM_ID`) on the pinned FerroEHR
  image with `FERROEHR__SERVER__SYSTEM_ID` set, and puts a proxy in front of
  each; `ferroehr(system_id)` starts one node on a database server of its
  own, and `ferroehr_restricted(system_id)` one with its access controls on.
  `Node::stop()` makes a node offline the way an outage does.
  `containers/ehrbase.rs` starts the pinned EHRbase (`ehrbase`,
  `ehrbase_restricted`), the second CDR product the node profile runs
  against (#549).
- `proxy.rs`: `CapturingProxy` journals every request (method, path, query,
  headers, body) and injects `Fault::Refuse`, `Fault::Delay`,
  `Fault::Status` or `Fault::Reply` (a status with an ITS-REST `Error` body)
  per proxy, at any point in a test. Track 10 is judged on that journal,
  never on the gateway's logs (§16.3). `CapturingProxy::start_reachable`
  listens on every interface, so a container reaches the proxy through the
  Docker host gateway.
- `seed.rs`: `seed()` writes EHRs, the vendored template and the vendored
  compositions over ITS-REST alone; `PatientId` can only be built inside
  `urn:oid:2.999.1.<n>`. Every e2e case seeds the subject on both nodes, so a
  leaked subject predicate would match there.
- `node_profile.rs`: where the harness writes the Federation-Node profile
  findings of each CDR product (#93, #549). The checks are the gateway's
  own, `ferrofed_server::conformance::node_profile`, which `ferrofed
  conformance run --node-profile` runs against a deployment's members.
- `reference.rs`: the reference implementation as a second gateway for the
  differential run (#94), built from its vendored source with the Maven
  manifest fetched from the pinned commit and held to its digest, in
  digest-pinned images, reaching the nodes through `start_reachable`
  proxies. It is evidence, never an oracle.

The Connectathon tracks of §16.3 are scenarios under
`app/ferrofed-server/tests/it/e2e/` (`scenario.rs` and a `track<n>.rs` per
track, #545; Track 8 is deferred in `conformance/tracks.tsv`), sharing their
checks with `ferrofed conformance run` (#546, #573); the differential run is
`tests/it/e2e/differential/` there.

Every container test begins with `containers::e2e_enabled()` and returns early
when `FERROFED_E2E` is not `1`, so `cargo nextest run --workspace` stays
offline and green. CI runs the gated tests in the `e2e (containers)` job with
every feature on, so both regional bindings (`binding-ihe`, `binding-nl`) are
built: the testkit's suite and the `e2e` module of every crate's test binary,
selected by `test(/^e2e::/)`. The job uploads its JUnit report, the node
profile findings and the differential report, and the `conformance report`
job joins them with the offline run into the per-run report of §16.4. A
gated test outside `tests/it/e2e.rs` or `tests/it/e2e/` never runs anywhere,
which is how #272 found one; `scripts/checks/e2e-placement.sh` (tier 1) now
refuses it.

The operator console's browser journeys (#608) follow the same model behind
their own gate, `FERROFED_JOURNEYS=1`: they live in the testkit's
`tests/it/journeys/`, need a chromedriver at `FERROFED_WEBDRIVER` (default
`http://127.0.0.1:9515`) and the site bundle `scripts/release/viewer-site.sh
--release` writes, and the `journeys (browser)` job runs them with
`test(/^journeys::/)`; the placement guard refuses a journey anywhere else.

**How to apply:** a new container-backed test uses the harness and the gate,
never its own `docker` calls, and sits under `tests/it/e2e/`; a new image is a
`PinnedImage` constant plus a `docs/VERSIONS.md` row, which
`scripts/checks/versions.sh` compares. A check that must run `docker
compose` lives in `scripts/checks/` and a CI job, never in a Rust test, as
`scripts/checks/release-compose.sh` does. Locally: `FERROFED_E2E=1 cargo nextest
run --locked --workspace --all-features -E 'package(ferrofed-testkit) or
test(/^e2e::/)'` with Docker running. Linked: [[postgresql-18]],
[[strict-over-reference]].
