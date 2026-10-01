<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED will claim

Until the architecture of record exists, FerroFED claims only what is decided.
This page lists the decided parts and the open ones, so a reader can tell the
two apart.

## Decided

- **The specification is the authority.** The Federation Tier with AQL text
  decides; its reference implementation is read as evidence, never as an
  oracle. Where the specification is silent, the decision is recorded as
  FerroFED's own and labelled that way in the code and the documentation.
- **The openEHR surface is not reimplemented.** The ITS-REST contract, the
  AQL parser and printer, the RM and the typed identifiers come from the
  published `openehr-*` crates. A gap in one of them is fixed in that crate,
  not worked around in the gateway.
- **No clinical data of its own.** The gateway holds the registry, the index
  and the stored-query definitions it is authoritative for. The record stays on
  the nodes.
- **Identifier hygiene is a hard rule.** Nothing the gateway composes for a
  node carries a directly identifying patient identifier, and every carrier the
  specification names gets a negative test.
- **Pure Rust, a single binary**, with the generated layer kept apart from the
  hand-written one, as across the FerroHEALTH family.

## Open, on the research program

The research program
([#16](https://github.com/FerroHEALTH/FerroFED/issues/16)) answers these with
cited evidence before the workspace exists:

- how the AQL rewrite sits on `openehr-query`'s typed AST, checked against the
  reference implementation's golden cases;
- which identity, localization and addressing bindings ship first, and the
  seam between them;
- where the registry lives and how it is stored;
- the fan-out and merge design: timeouts, completeness, `ORDER BY` with
  `LIMIT`, `OFFSET`, `DISTINCT`, aggregates and de-duplication;
- the security handoff from client to gateway to node;
- what is generated from the specification's two JSON schemas and what is
  written by hand;
- the crate layout, and whether the library crates are published;
- the conformance instrument that scores every conformance point.

## Not claimed

FerroFED does not claim conformance to any conformance point today, and will
not until a test scores it. The specification is a release candidate; when 1.0
is published, the vendored text is re-pinned and every citation on the tracker
is checked against it
([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)).

The [conformance matrix](conformance.md) records where each point stands:
every point, its requirements and tracks as the specification states them,
and the issue that scores it.
