<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroFED is a pure-Rust openEHR federation gateway, one of the
[FerroHEALTH](https://ferrohealth.eu/) family. A record held by another
organisation is out of reach today. FerroFED closes that gap as a
transparent ITS-REST intermediary: a client sends it an ordinary AQL query and
never learns it was federated. The gateway resolves the patient first, outside
the query, so no directly identifying identifier travels to a node. It then
sends standard AQL to each node, scoped to that node's own EHR id, and merges
what comes back with each node's provenance. It holds no clinical data of its
own.

FerroFED follows the openEHR Federation Working Group's
[Federation Tier with AQL](https://syntaric.github.io/openehr-federation-spec/)
specification, a release candidate at v0.9.0 with a 1.0 release expected.

## Where the project is

The architecture of record was decided on 2026-10-01
([`docs/architecture.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/architecture.md)),
and the gateway is built against it one milestone at a time. The
[latest release](https://github.com/FerroHEALTH/FerroFED/releases/latest)
ships the `ferrofed` binary for Linux and the image
`ghcr.io/ferrohealth/ferrofed`, both signed, and
[the container page](operate/container.md) runs it beside four member CDRs.

FerroFED serves the federated query, `POST {base}/v1/query/aql` and its `GET`
form, over the members of a registry. It resolves the patient outside AQL
through an IHE PIXm PIX Manager, sends each member standard AQL scoped to its
own `ehr_id`, refuses a query that would carry the patient identifier to a
node, and merges the answers with each node's provenance, failing the query
when a node that was asked does not answer unless you ask for a partial
answer. It routes the EHR resources under a path `ehr_id` to the one node that
holds them, reads an EHR by subject, sends a definition request to the node
you name, and runs stored queries by name where a deployment offers the
stored-query registry. The DEMOGRAPHIC area is never federated, and the
ITS-REST paths outside these answer `501`; [the client
contract](integrate/client-contract.md) lists each. Each page says what is
built, and names the issue of what is planned.

## How this book is organised

The four parts follow what you came to do.

- **Evaluate** answers whether FerroFED fits your problem: what the Federation
  Tier is, what FerroFED claims and what is planned, the conformance matrix,
  the version pins, and the licence.
- **Operate** covers installing and configuring the gateway, and what it needs
  around it.
- **Integrate** covers what a client sends and what it gets back.
- **Contribute** covers how the work is tracked and which checks a change has
  to pass.

The tracker is the scope. Open issues are the worklist, milestones are
releases, and the
[roadmap](https://github.com/FerroHEALTH/FerroFED/milestones) is the public
view of both.
