---
name: e2e-gate
description: Container-backed tests run only with FERROFED_E2E=1 through the testkit harness (FerroEHR and EHRbase as the two nodes, each behind a capturing and fault proxy, all images pinned by digest); unset, they return early and the suite stays offline
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Landed with #39 (2026-10-01), on the FerroBRIDGE gate model. The harness is
`tools/ferrofed-testkit`:

- `containers.rs`: `ferroehr()`, `ehrbase()` and `two_nodes()` start the pinned
  images through testcontainers, each node on its product's own documented
  database image (EHRbase on 16.2, decision A40), and `two_nodes()` puts a
  proxy in front of each. `Node::stop()` makes a node offline the way an
  outage does.
- `proxy.rs`: `CapturingProxy` journals every request (method, path, query,
  headers, body) and injects `Fault::Refuse`, `Fault::Delay` or
  `Fault::Status` per proxy, at any point in a test. Track 10 is judged on
  that journal, never on the gateway's logs (§16.3).
- `seed.rs`: `seed()` writes EHRs, the vendored template and the vendored
  compositions over ITS-REST alone; `PatientId` can only be built inside
  `urn:oid:2.999.1.<n>`.

Every container test begins with `containers::e2e_enabled()` and returns early
when `FERROFED_E2E` is not `1`, so `cargo nextest run --workspace` stays
offline and green. CI runs the gated tests in the `e2e (containers)` job.

**How to apply:** a new container-backed test uses the harness and the gate,
never its own `docker` calls; a new image is a `PinnedImage` constant plus a
`docs/VERSIONS.md` row, which `scripts/checks/versions.sh` compares. Locally:
`FERROFED_E2E=1 cargo nextest run -p ferrofed-testkit` with Docker running.

**A node divergence found on the way:** EHRbase 2.36.0 refuses a
`PARTY_REF.namespace` containing `.` (its pattern is
`[a-zA-Z][a-zA-Z0-9-_:/&+?]*`), while BASE `object_ref.adoc` §Attributes
allows `[a-zA-Z][a-zA-Z0-9_.:/&?=+-]*`, so `urn:oid:2.999.1.2` is valid RM and
FerroEHR accepts it. The arc is not bent to suit the node: an EHR on EHRbase
is seeded with no subject, which costs nothing because FerroFED resolves a
patient through the cross-reference, never through a node's
`EHR_STATUS.subject`. Linked: [[postgresql-18]], [[strict-over-reference]].
