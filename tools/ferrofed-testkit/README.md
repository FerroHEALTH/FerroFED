<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-testkit

Test support for the FerroFED suites, never shipped: the pin-matrix reader,
the container harness for the two member CDRs behind the `FERROFED_E2E` gate
(two FerroEHR instances on one PostgreSQL server with a database per node),
the capturing and fault proxy in front of each node,
the track 10 leakage search over the proxy journal (`leak`), the
synthetic seed builder that writes over ITS-REST alone, and the PIX
Manager test device the seed builder feeds over ITI-104 (a test device, not a
PIXm implementation).

A tool crate under `tools/`: never published.

## Licence

Business Source License 1.1 (`LICENSE` at the repository root): free for every
non-production use and for non-commercial production use; a commercial licence
for other production use; Apache License 2.0 four years after each version.
