<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What FerroFED runs beside

The binary serves the federated query over a registry of member CDRs
([Configuration](configuration.md)). This page describes what a running
gateway needs around it, taken from the roles the specification names, and
says which binding FerroFED ships for each role and which is planned.

See how it works: [deployment](../how-it-works/deployment.md) and [trust and keys](../how-it-works/trust-and-keys.md).

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
| Member CDRs | answer standard AQL scoped to one `ehr_id`, and the reads and writes routed to them | openEHR ITS-REST 1.1.0, over each node's own base URL |
| Demographics (optional) | names the master identity of a patient the client names by an identifier the cross-reference does not know | IHE PDQm ITI-78 or ITI-119 ([Demographics first](identity.md#demographics-first-pdqm)) |
| Identifier cross-reference | maps a patient identifier to each node's local `ehr_id`, or reports it not found | IHE PIXm ITI-83 ([Identity resolution](identity.md)) |
| Localization (optional) | returns the candidate communities for a patient; without it the gateway asks every member's cross-reference | none, where every member is a candidate (`ask-all`); under `node_selection = "localized"`, IHE XCPD ITI-55 ([XCPD localization](localization.md#xcpd-localization-xcpd)), the Dutch NVI ([Dutch localization](localization.md#dutch-localization-nl_gfnvi)) or the PIX Manager itself |
| Consent pre-filter (optional) | names the members that may not be asked about the patient, never the gate | the Dutch Mitz closed authorization question ([Dutch consent](consent.md#dutch-consent-nl_gfmitz)) |
| Addressing | resolves each community to its CDR base URLs | the registry document, in TOML or as FHIR `Organization` and `Endpoint` resources, or an mCSD care services directory read with ITI-90 and kept in step with ITI-91 ([The registry](registry.md)) |
| Authentication and authorization | authenticates the client, and the gateway to each node | client authentication by RFC 9068 access tokens from the issuers you trust, or an explicit edge mode ([Client authentication](authentication.md)); outbound credentials per endpoint, a bearer token, basic credentials, OAuth 2.0 client credentials or token exchange with an RFC 7523 assertion and optional DPoP, or the Nuts and FAPI 2.0 grants of the Dutch binding ([below](#authentication)) |

The specification references the internals of each service out (§2.2): how
an MPI matches identities, how a locator decides where data is, and the
transport trust framework all belong to their own profiles. A region may
supply its own realisation; Annex B describes the Dutch Generic Functions as
one.

## Authentication

Every client authenticates to the gateway (§13.1, N25): a request to the
ITS-REST surface, and `OPTIONS {base}/`, carries an RFC 9068 access token
from an issuer you trust, with a SMART on openEHR scope for the operation and
a purpose of use, or, in the explicit edge mode, an assertion your proxy
signed. A request without one is refused before any node is asked
([Client authentication](authentication.md)). The listener speaks plain HTTP,
so terminate TLS in front of it. The gateway never forwards a client's
`Authorization` header to a node. The resolution bindings of §12.5.1 are
kept per verified caller
([The registry](registry.md#resolution-bindings)).

Toward the nodes, the gateway authenticates with credentials you configure
per endpoint ([Configuration](configuration.md#the-file)):

- an RFC 6750 bearer token, or an RFC 7617 user and password, each inline or
  read from a file, sent on every request to that endpoint and to nothing
  else;
- OAuth 2.0 client credentials with an RFC 7523 signed JWT client assertion,
  the default mechanism of §13.1 (N25): the gateway obtains a token at the
  node's token endpoint and sends that
  ([OAuth 2.0 to a node](onward-credentials.md#oauth-20-to-a-node)), or
  RFC 8693 token exchange for a token per verified caller
  ([Token exchange](onward-credentials.md#a-token-per-caller-token-exchange)),
  either bound to a key of the gateway's with DPoP where you configure one
  ([DPoP](onward-credentials.md#tokens-bound-to-a-key-dpop));
- the Nuts grant and the FAPI 2.0 grant of the Dutch binding (Annex B §B.4,
  §B.4a; [Onward credentials](onward-credentials.md)).

For the OAuth 2.0 grant, the node's authorization server needs the gateway
registered as a client under its `client_id`, with the gateway's JWK Set
location. The gateway serves the set at `{base}/.well-known/jwks.json`
without client authentication, and declares its location as
`federation.auth.jwks_uri` in `OPTIONS {base}/`
([Signing keys and the JWK Set](onward-credentials.md#signing-keys-and-the-jwk-set)).
Keep that route reachable from every node's authorization server; client
authentication guards the rest of the surface and never this route. A PIX Manager takes a
bearer token or a user and password, or none where the transport
authenticates the gateway ([Identity resolution](identity.md)).

Every request to a node also carries the verified caller in an
`openEHR-federation-client` token the gateway signs with its own key (§13.1,
N24, [#82](https://github.com/FerroHEALTH/FerroFED/issues/82)), so
`[signing]` is required on every federating gateway, and every node that
verifies the token needs the same JWK Set location
([What a node is told about the caller](authentication.md#what-a-node-is-told-about-the-caller)).

## What the gateway keeps

The gateway holds no clinical data. It keeps the registry of organisations,
endpoints and the `system_id` mapping that routing depends on (§3.1, N21), and,
if the deployment offers it, the federated stored-query definitions it is
authoritative for (§12.7). The specification is silent on storage, so this is
FerroFED's own design: the registry is a reviewed document, loaded at boot
and on each reload into an immutable snapshot, and the `ehr_id` index and the
learned `creating_system_id` routes are held in memory, bounded, and lost on a
restart. The stored-query registry is the
one durable store, holding parameterised AQL and never a patient identifier,
over the backend [`[stored_queries]`](queries-and-areas.md#stored-queries)
names: an embedded `redb` file for one gateway process, a shared PostgreSQL
database for several replicas, or read-only definition files.

## Running several replicas

Several gateway replicas behind one address share nothing in memory: each
holds its own resolution bindings, `ehr_id` index and learned routes, and a
miss on one replica costs a probe or an explicit target, never a wrong route.
For a write the miss is visible to your clients. A follow-up write under a
path `ehr_id` that names no node is routed by the caller's resolution
binding or the `ehr_id` index (§12.5.1, N41), and the gateway never probes
for a write's owner, so the replica that resolved the patient routes it and
any other replica answers `400` `target-required`. Nothing reaches a node,
and behaviour then changes with the replica count. Give your clients one of
two remedies:

- Clients send `openEHR-federation-endpoint` on every write, with the
  `endpoint_id` of the row they act on. It routes on every replica, and it
  is what the client contract recommends
  ([Writes through a gateway with several replicas](../integrate/client-contract.md#writes-through-a-gateway-with-several-replicas)).
- The balancer keeps each client on one replica, such as a Kubernetes
  Service with `sessionAffinity: ClientIP`
  ([Session affinity](https://kubernetes.io/docs/reference/networking/virtual-ips/#session-affinity)).
  The bindings belong to the verified caller, which a balancer cannot read
  from a token, so affinity by client address is an approximation: a replica
  restart, a scale-down or the affinity timeout still moves a client to a
  replica without its bindings.

The replicas never share the bindings or the index: each lives in the
memory of one process and is lost on its restart, and the gateway writes
nothing derived from a patient identifier to a shared store.

The stored-query registry is the exception, because a stored version must be
the same on every replica and a second `PUT` of it refused on every replica
(§12.7, N44):

- A `redb` file is opened by one process at a time, so replicas cannot share
  one, and a file per replica would let two replicas each accept a different
  first `PUT` of the same name and version. Run the registry on the
  [`postgres` backend](queries-and-areas.md#several-replicas-postgres): every
  replica uses one database, a version one replica stores is served by all
  of them, and two replicas storing the same new version at once store
  exactly one.
- Replicas that only serve definitions an operator publishes can use the
  [read-only `files` backend](queries-and-areas.md#read-only-files) instead,
  each loading the same directory, with no database. A `PUT` is then refused
  `405` on every replica.
- Without `[stored_queries]` no replica offers the registry, and the
  replicas need no shared state at all.

Two more settings take the replica count into account:

- **The identity feed.** Replicas behind one `[pmir] callback_url` share one
  PMIR subscription, and a drain keeps it for the others (`on_drain =
  "keep"`, the default). Each identity change reaches one replica; the
  others keep a stale binding until its lifetime ends. A replica with a
  `callback_url` of its own holds its own subscription and hears every
  change ([Several replicas](identity.md#several-replicas)).
- **Signing-key rotation.** Every replica must publish a new key before any
  replica signs with it, so a rotation takes three rolling restarts:
  publish it as `next_key_file`, make it `key_file`, then retire the old
  one ([Rotating the signing key](onward-credentials.md#rotating-the-signing-key)).

## Failure behaviour you should know before you run it

- When a node that was asked does not answer, the default is to fail the
  query. A client that wants flagged partial rows has to ask for best-effort
  explicitly (§11.4).
- Every response, a failing one included, reports each node in scope with a
  status such as `active`, `offline`, `time-out` or `not-resolved` (§11.1).
- Consent is enforced by each node before it releases data (N27). A node's
  `403` is reported `consent-denied`, and fails nothing, only when its
  ITS-REST `Error` carries a code the registry lists for that endpoint;
  every other refusal is `node-error`. An optional Step-1 pre-filter drops
  members before dispatch: the development table, or Mitz in the
  Netherlands ([Consent](consent.md)).
