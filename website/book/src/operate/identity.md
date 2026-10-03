<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Identity resolution

A patient query names its patient by an identifier and that identifier's
issuing namespace. The gateway resolves the pair, outside the query, to the
local `ehr_id` each member holds for the patient, and sends each member a
query scoped to that `ehr_id` alone (§5.2, N3, N7). This page covers the
cross-reference the gateway asks and how you configure it.

## Where the identifier comes from

The gateway reads the patient on either carrier §5.4.3 names (N33, CP-38):

- the subject predicate of §7,
  `e/ehr_status/subject/external_ref/id/value = '…'`, with the namespace in
  `e/ehr_status/subject/external_ref/namespace`;
- an `ENTRY`-level subject, `…/subject/identifiers/id = '…'`, with the
  namespace in `…/subject/identifiers/issuer` or `…/subject/identifiers/type`.

`GET {base}/v1/ehr?subject_id=…&subject_namespace=…` names the patient in
its query string instead
([Reading an EHR by subject](../integrate/follow-ups.md#reading-an-ehr-by-subject)).

A query that names no namespace resolves in `federation.default_namespace`
when you set one, and is refused `400` (`no-namespace`) when you do not.
§5.2 requires a namespace and names no default, so the setting is
FerroFED's own:

```toml
[federation]
default_namespace = "urn:oid:2.999.1"
```

## A PIX Manager: `[pixm]`

The identity binding FerroFED ships is IHE PIXm, the Patient Identifier
Cross-reference Manager's ITI-83 query (Annex A.1). Each registry member is
resolved by one Manager, in its `ehr_id` domain there: the assigning
authority whose identifiers are that member's `ehr_id`s.

```toml
[[pixm.manager]]
url = "https://pix.example.org/fhir/"

[pixm.manager.members]
"hospital-a" = "urn:oid:2.999.10"   # a registry node id = its ehr_id domain
"clinic-b" = "urn:oid:2.999.20"

[pixm.manager.credentials]
bearer_token_file = "/run/secrets/pix-token"

[pixm.namespaces]
"2.999.1" = "urn:oid:2.999.1"       # a client namespace = the PIX assigning authority
```

- Name one `[[pixm.manager]]` per Manager. Every member of the registry is
  resolved by exactly one: a member no Manager names, a member two Managers
  name, and a name the registry does not hold each refuse the configuration.
- Each domain, and each value of `[pixm.namespaces]`, is an absolute URI.
- `url` is the Manager's FHIR base, `http` or `https`, with no query or
  fragment. A user name or password in it refuses the configuration, naming
  `pixm.manager[N].url` and never quoting it; the credentials go in
  `[pixm.manager.credentials]`, with the keys of an endpoint's
  [credentials](configuration.md#the-file): `bearer_token`, or `user` and
  `password`, each with its `_file` sibling. Leave the section out when the
  transport, such as mutual TLS or a private network, authenticates the
  gateway.
- `[pixm.namespaces]` maps the namespace a client writes to the assigning
  authority the Manager knows it by. A namespace that is itself an absolute
  URI needs no entry.

For each query, the gateway asks each Manager once, with one `targetSystem`
per member that Manager resolves, and reads the identifier the answer holds
in a member's domain as that member's `ehr_id`. The patient identifier goes
to the Manager and nowhere else; no node, log line or error body carries it
(§5.4.1, N33).

| The Manager answers, for one member | The member is | The query |
|---|---|---|
| exactly one identifier in the domain, which reads as an `ehr_id` | sent its node query | goes on |
| no identifier in the domain, or the patient is unknown | `not-resolved` | goes on; `complete` is false (N6) |
| two identifiers, one that is no `ehr_id`, an error, no answer within the budget, or a namespace with no mapping | `not-resolved`, with the reason in `error` | fails `424` under all-or-nothing |

A member the Manager could not answer for may hold the patient, so the
gateway fails the query by default rather than answer without it (§11.1;
§11.3 covers only an answered lookup, so this rule is FerroFED's own). With
`openEHR-federation-completeness: partial`, the members that did resolve
answer. Resolution runs inside the request's overall budget
([Timeouts](queries-and-areas.md#timeouts)).

## The development cross-reference: `[dev]`

For a laptop or a test, the gateway can resolve from a static table in the
configuration instead. It binds nothing, it is no identity binding of N3, and
it is accepted only in a configuration that declares itself for development
(no specification governs this: our own design):

```toml
profile = "development"

[[dev.crossref]]
namespace = "urn:oid:2.999.1.1"
value = "ffd-test-0001"
member = "node-a"
ehr_id = "aaaaaaaa-aaaa-4aaa-8aaa-000000000001"
```

Each row maps one synthetic patient to its `ehr_id` at one member; a patient
with no row at a member is `not-resolved` there. Under any other `profile`,
`[dev]` refuses the configuration. A development deployment prints a red
notice in its [startup banner](configuration.md#the-startup-banner) that it
must not hold or reach real patient data. The
[quickstart](container.md#the-quickstart) runs on this table.

## Consent

Each node checks consent before it releases data, whatever the gateway did
first (N26, N27). The gateway never decides on release itself, and a node it
dispatches to is not thereby cleared: a localization or consent service that
named the node only means nothing upstream ruled it out (§14.3).

**A node's own refusal.** ITS-REST defines no consent signal, so the gateway
does not infer one from a status code. A node's answer is reported
`consent-denied` only when it is a `403` whose ITS-REST `Error` body carries a
`code` that the registry lists for that endpoint in `consent_refusal_codes`
([The registry](registry.md#the-registry-document)). Every other refusal is
`node-error`. The list is empty by default, so until you name the codes a
node uses, its consent refusal fails the query `424` as any node error does.
This key is FerroFED's own design, because no specification defines the
signal. A refusing node contributes no rows, never fails the query in either
completeness mode, clears `meta.federation.complete`, and its record carries
the `latency_ms` of the request it refused (§11.3, N40).

**The optional Step-1 pre-filter.** A deployment with a consent service may
drop members before dispatch (N27a). The pre-filter runs after localization
and before resolution. Each member it denies is reported `consent-denied` with
no `latency_ms`, is never resolved and never sent a request, and any `ehr_id`
the client session cached for it is dropped. A member it does not deny is
asked, and its node decides. When the consent service cannot answer, Step 1
carries no consent signal, which is the state of a deployment with no consent
service at all, so every candidate is asked and each node checks consent
itself (§13.2.1, N27a). This `pass-to-node` policy is FerroFED's own design.
`OPTIONS {base}/` declares a configured pre-filter under `federation.consent`,
with its mode and that policy; a deployment with no pre-filter declares
nothing there.

For development, rows under `[[dev.consent_denied]]` beside the
cross-reference are a static pre-filter, accepted only under
`profile = "development"` and declared as `development-static`:

```toml
[[dev.consent_denied]]
namespace = "urn:oid:2.999.1.1"
value = "ffd-test-0001"
member = "node-b"        # this patient's consent denies asking node-b
```

The pre-filter of the Dutch binding, Mitz, is planned for v0.0.8
([#87](https://github.com/FerroHEALTH/FerroFED/issues/87)).

## Choosing one

Set `[pixm]` or `[dev]`, never both: both refuse the configuration. With
neither, the gateway still starts, and every patient query fails closed: each
member is `not-resolved` with "no cross-reference service is configured",
and the query is a `424`. A query that names no patient needs no resolution
and runs as written.

Both sections take effect on a
[reload](registry.md#reloading-the-registry). `GET {base}/health/dependencies`
reports the resolver's last observed state
([Health probes](health.md)).

Under `federation.node_selection = "localized"` the `[dev]` table is also the
localizer: it names the members that hold a row for the patient, and every
other member is `not-localized`
([Node selection](registry.md#node-selection)).

Two further bindings of the specification are planned for v0.0.8: IHE XCPD
for localization, failing closed when the localizer does not answer
([#85](https://github.com/FerroHEALTH/FerroFED/issues/85)), and PMIR
notifications of a merge or split
([#147](https://github.com/FerroHEALTH/FerroFED/issues/147)).
