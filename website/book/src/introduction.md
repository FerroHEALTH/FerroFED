<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroFED is a pure-Rust openEHR federation gateway, one of the
[FerroHEALTH](https://ferrohealth.eu/) family. It sits in front of several
openEHR CDRs as a transparent ITS-REST intermediary. A client sends it an
ordinary AQL query and never learns it was federated. The gateway resolves
the patient first, outside the query, through an identifier cross-reference
service. It then sends each node standard AQL scoped to that node's own
`ehr_id`, so no directly identifying patient identifier travels to a node,
and it merges the answers with each node's provenance. It holds no clinical
data of its own.

FerroFED implements the openEHR Federation Working Group's
[Federation Tier with AQL](https://syntaric.github.io/openehr-federation-spec/)
specification, release candidate v0.9.0, and reaches every node over openEHR
ITS-REST 1.1.0.

## What it does today

The [latest release](https://github.com/FerroHEALTH/FerroFED/releases/latest)
ships the `ferrofed` binary for Linux and the image
`ghcr.io/ferrohealth/ferrofed`, both signed, and a `compose.yaml` that runs
the gateway alone in front of your own CDRs
([The gateway from a release](operate/container.md#the-gateway-from-a-release)).
[The quickstart](operate/container.md#the-quickstart) runs it beside four
member CDRs. This book describes the gateway as it is built on `main`, and
the [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md)
says which release first shipped each part. Over the members of a registry,
the gateway:

- answers the federated query, `POST {base}/v1/query/aql` and its `GET`
  form, with one ITS-REST `RESULT_SET` and a `meta.federation` report that
  names every member with its status;
- resolves the patient through an IHE PIXm PIX Manager (ITI-83), and refuses
  any query that would carry the patient identifier to a node;
- fails a query when a node it asked does not answer, unless you ask for a
  partial answer, and shapes the merged rows as one CDR would: `ORDER BY`
  with `LIMIT`, bounded `OFFSET` pages, `DISTINCT`, recombined aggregates and
  opt-in de-duplication of copied versions;
- lets you direct a query at named nodes, in the AQL or in a header;
- routes the EHR resources under a path `ehr_id` to the one node that holds
  that EHR, and a versioned write only to the CDR that controls the version;
- sends a template or another definition request to the node you name, and
  can fan a template upload out to several;
- holds stored queries itself, as immutable versions it runs by name, and
  can distribute them to the members;
- describes itself at `OPTIONS {base}/`;
- gives you, the operator, a check to run before you admit a node, a
  registry reload on `SIGHUP`, health probes, and metrics for Prometheus or
  an OpenTelemetry collector.

It authenticates no client yet: client authentication is planned for v0.0.8
([What FerroFED claims](evaluate/what-ferrofed-claims.md)). Each page says
what is built and names the issue of what is planned.

## How this book is organised

The four parts follow what you came to do.

- **Evaluate:** whether FerroFED fits your problem. What the Federation Tier
  is, what FerroFED claims and what is planned, the conformance matrix and
  the obligations checklist, the version pins and the licence.
- **Operate:** installing, configuring and running the gateway, and what it
  needs around it.
- **Integrate:** what a client sends and what it gets back.
- **Contribute:** how the work is tracked and which checks a change has to
  pass.

The tracker is the scope. Open issues are the worklist, milestones are
releases, and the
[milestones page](https://github.com/FerroHEALTH/FerroFED/milestones) is the
public view of both.
