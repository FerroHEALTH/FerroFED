<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED runs beside

The binary serves the federated query over a registry of member CDRs
([Configuration](configuration.md)). This page describes what a running
gateway needs around it, taken from the roles the specification names, and
says which binding FerroFED ships for each role and which is planned.

## Supported platforms

FerroFED runs on Unix only. The server drains on `SIGTERM` and `SIGINT` and
reloads its registry on `SIGHUP`, and both use Unix signals. Every release
binary is Linux, for `x86_64` and `aarch64` on glibc and on musl, and the
container image is Linux ([The release binaries](container.md#the-release-binaries)).
Nothing is built or tested for Windows: building the server crate for a
non-Unix target stops at a compile error that says so. No specification
governs this: our own design.

## The services a gateway consumes

| Role | What it does | Binding |
|---|---|---|
| Member CDRs | answer standard AQL scoped to one `ehr_id`, and the follow-up reads and writes routed to them | openEHR ITS-REST 1.1.0, in v0.0.3 for the query; follow-up routing planned ([#61](https://github.com/FerroHEALTH/FerroFED/issues/61), [#64](https://github.com/FerroHEALTH/FerroFED/issues/64)) |
| Identifier cross-reference | maps a patient identifier to each node's local `ehr_id`, or reports it not found | IHE PIXm ITI-83, in v0.0.3 |
| Localization (optional) | returns the candidate communities for a patient; without it the gateway asks every known node | ask-all in v0.0.3; IHE XCPD planned ([#85](https://github.com/FerroHEALTH/FerroFED/issues/85)) |
| Addressing | resolves each community to its cross-reference service and CDR base URLs | the registry document in v0.0.3; IHE mCSD planned ([#86](https://github.com/FerroHEALTH/FerroFED/issues/86)) |
| Authentication and authorization | authenticates the client, and the gateway to each node | per-endpoint credentials to each node in v0.0.3; the §13 profiles planned ([#80](https://github.com/FerroHEALTH/FerroFED/issues/80), [#81](https://github.com/FerroHEALTH/FerroFED/issues/81)) |

The specification references the internals of each service out (§2.2): how
an MPI matches identities, how a locator decides where data is, and the
transport trust framework all belong to their own profiles. A region may
supply its own realisation; Annex B describes the Dutch Generic Functions as
one.

## What the gateway keeps

The gateway holds no clinical data. It keeps the registry of organisations,
endpoints and the `system_id` mapping that routing depends on (§3.1, N21), and,
if the deployment offers it, the federated stored-query definitions it is
authoritative for (§12.7). The specification is silent on storage, so this is
FerroFED's own design: the registry is a reviewed TOML document, loaded at boot
into an immutable snapshot, and the resolution bindings of each client session
are held in memory with a bounded lifetime. The stored-query registry is the
one durable store: an embedded `redb` file at the path
[`[stored_queries]`](configuration.md#stored-queries) names, opened by one
gateway process at a time, holding parameterised AQL and never a patient
identifier.

## Failure behaviour you should know before you run it

- When a node that was asked does not answer, the default is to fail the
  query. A client that wants flagged partial rows has to ask for best-effort
  explicitly (§11.4).
- Every response, a failing one included, reports each node in scope with a
  status such as `active`, `offline`, `time-out` or `not-resolved` (§11.1).
- Consent is enforced by each node before it releases data. A node's refusal
  is reported; the gateway never treats its own pre-filter as the only gate
  (N27).
