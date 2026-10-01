<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Pinned versions

Every version the repository depends on is pinned once, in
[`docs/VERSIONS.md`](https://github.com/rubentalstra/FerroFED/blob/main/docs/VERSIONS.md),
and `scripts/checks/versions.sh` fails a change that lets a repeated pin drift
from it. This page summarises the pins that shape the design.

| Item | Pin | Why |
|---|---|---|
| Federation Tier with AQL | 0.9.0, release candidate, commit `7162d0c` | the governing specification; re-pinned when 1.0 is published |
| openEHR ITS-REST | 1.1.0 | the façade a client sees and the API each node exposes |
| openEHR AQL | 1.1.0 | the query language on both sides of the gateway |
| `openehr-query`, `openehr-its` | 0.0.72 | the published crates the gateway builds on, moved together as one family |
| Rust | 1.98.1, edition 2024 | the toolchain, once the workspace exists |

The identity and directory bindings (IHE PIXm, PDQm, PMIR, mCSD, XCPD and the
Dutch Generic Functions) are listed in `docs/VERSIONS.md` with the latest
published version, but none is pinned. Choosing them is part of the research
program.

## Vendored specifications

The specification, its reference implementation, the ITS-REST OpenAPI
documents and the AQL source are vendored verbatim under
[`docs/specs/`](https://github.com/rubentalstra/FerroFED/tree/main/docs/specs),
each fetched by a committed script and stamped with a `PROVENANCE.md` that
records the source, the pin, the licence and a tree digest.
