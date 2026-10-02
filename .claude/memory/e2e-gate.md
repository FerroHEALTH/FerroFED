---
name: e2e-gate
description: Container-backed tests run only with FERROFED_E2E=1 through the testkit harness (two FerroEHR instances as the two nodes, each with its own database and system_id, each behind a capturing and fault proxy, all images pinned by digest); unset, they return early and the suite stays offline
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Landed with #39 (2026-10-01), on the FerroBRIDGE gate model; the topology
became two FerroEHR nodes with #155 (2026-10-02, [[two-ferroehr-nodes]]). The
harness is `tools/ferrofed-testkit`:

- `containers.rs`: `ferroehr(system_id)` starts the pinned FerroEHR image on
  its own pinned database container with `FERROEHR__SERVER__SYSTEM_ID` set,
  and `two_nodes()` starts node A (`NODE_A_SYSTEM_ID`) and node B
  (`NODE_B_SYSTEM_ID`) and puts a proxy in front of each. `Node::stop()` makes
  a node offline the way an outage does.
- `proxy.rs`: `CapturingProxy` journals every request (method, path, query,
  headers, body) and injects `Fault::Refuse`, `Fault::Delay` or
  `Fault::Status` per proxy, at any point in a test. Track 10 is judged on
  that journal, never on the gateway's logs (§16.3).
- `seed.rs`: `seed()` writes EHRs, the vendored template and the vendored
  compositions over ITS-REST alone; `PatientId` can only be built inside
  `urn:oid:2.999.1.<n>`. Every e2e case seeds the subject on both nodes, so a
  leaked subject predicate would match there.

Every container test begins with `containers::e2e_enabled()` and returns early
when `FERROFED_E2E` is not `1`, so `cargo nextest run --workspace` stays
offline and green. CI runs the gated tests in the `e2e (containers)` job: the
testkit's suite and the `e2e` module of `ferrofed-server`.

**How to apply:** a new container-backed test uses the harness and the gate,
never its own `docker` calls; a new image is a `PinnedImage` constant plus a
`docs/VERSIONS.md` row, which `scripts/checks/versions.sh` compares. Locally:
`FERROFED_E2E=1 cargo nextest run -p ferrofed-testkit -p ferrofed-server -E
'test(/^e2e::/)'` with Docker running. Linked: [[postgresql-18]],
[[strict-over-reference]].
