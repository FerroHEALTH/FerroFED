<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Identity resolution

A patient query names its patient by an identifier and that identifier's
issuing namespace. The gateway resolves the pair, outside the query, to the
local `ehr_id` each member holds for the patient, and sends each member a
query scoped to that `ehr_id` alone (§5.2, N3, N7). This page covers the
cross-reference the gateway asks and how you configure it.

See how it works: [where the patient identifier stops](../how-it-works/identifier-hygiene.md).

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
- `url` is the Manager's FHIR base, `https`, with no query or fragment. The
  Manager is asked for patient identifiers, so `http` is refused naming
  `pixm.manager[N].url`, with or without credentials, unless the profile is
  `development`
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)). A user name or password in it refuses the configuration, naming
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

### `[pixm]` as the localizer

Under `federation.node_selection = "localized"` with no `[xcpd]`, the
`[pixm]` resolver is also the localizer, the "demographic-registration" kind
of §14.2. The candidates are the members whose domain holds an identifier
for the patient at its Manager. Every other member is `not-localized` and is
never asked. The resolution of the same query reuses that ITI-83 answer, so
each query still asks each Manager once.

A member whose domain holds two identifiers, or one that is no `ehr_id`, is a
candidate, because the Manager holds the patient there; its resolution then
reports why it could not be asked. A Manager that fails, answers in a form
the gateway cannot read, or does not answer within
`[federation.localization] timeout_ms` fails the localization closed
(§14.1): every member is `not-localized` with the error, and no member is
asked unless `on_failure = "ask-all"`. `OPTIONS {base}/` declares
`localization.mode` as `"pixm"`. With `[xcpd]` set, XCPD localizes and
`[pixm]` only resolves.

## Demographics first: `[pdqm]`

A client may name the patient by an identifier the cross-reference does not
know, such as a hospital-local number. Annex A places a PDQm query ahead of
localization and resolution for that case (Annex A §A.2 and §A.7): the
gateway asks a Patient Demographics Supplier which person the identifier
names, takes the identifier that person carries in the master domain, and
localizes and resolves that identifier as it would the client's own.

```toml
[pdqm]
url = "https://pdq.example.org/fhir/"
transaction = "iti-78"                 # or "iti-119"
master = "urn:oid:2.999.1"             # the master domain's identifier system
timeout_ms = 1000                      # each exchange with the Supplier

[pdqm.namespaces]
"urn:oid:2.999.7" = "urn:oid:2.999.7"  # a client namespace = the system it is sent in

[pdqm.credentials]
bearer_token_file = "/run/secrets/pdq-token"
```

- Only an identifier in a namespace `[pdqm.namespaces]` names goes to the
  Supplier. Every other one is resolved as it is, and a namespace
  `[pixm.namespaces]` maps is refused here, naming the key.
- `transaction = "iti-78"`, the default, sends the Mobile Patient
  Demographics Query: `identifier=<system>|<value>` and
  `identifier=<master>|`, which asks the Supplier for identifiers in the
  master domain alone (PDQm §2:3.78.4.1.2.3). `"iti-119"` sends the Patient
  Demographics Match instead, with the identifier on the input Patient and
  `onlyCertainMatches` set (§2:3.119.4.1.2); declare it only where the
  Supplier offers `$match`.
- `master` is the identifier system of the master domain. The master
  identifier is resolved in the namespace of the same name, so `[pixm]` or
  `[dev]` must resolve it; `[pdqm]` without either refuses the
  configuration.
- `url`, the credentials and the `http` rule are those of a PIX Manager: the
  Supplier is sent the patient identifier, so plain `http` is refused,
  naming `pdqm.url`, unless the profile is `development`. No redirect is
  followed, and an answer is read up to 8 MiB.
- A `SIGHUP` reload applies a change to `[pdqm]`, as it does to `[pixm]`:
  the reloaded federation asks the Supplier the new table names.

| The Supplier answers | The members are | The query |
|---|---|---|
| one active Patient carrying exactly one identifier in the master domain (for ITI-119, graded `certain`) | localized and resolved by the master identifier | goes on |
| no Patient, or one with no master identifier (PDQm §2:3.78.4.1.3 Case 3, §2:3.119.4.1.3 Cases 4, 5 and 7) | `not-resolved` | goes on; `complete` is false (N6) |
| more than one Patient, a Patient with two master identifiers, or an ITI-119 match not graded `certain` | `not-resolved`, with the reason in `error` | fails `424` under all-or-nothing |
| a failure, or no answer within `timeout_ms` | `not-localized` with the error under `on_failure = "closed"`; `not-resolved` under `"ask-all"` and on a directed query | `200` under `"closed"` (§14.1); `424` otherwise |

A Patient with `active` set to `false` is a deprecated record and never a
match (§2:3.78.4.1.3 Case 6). The gateway never picks one of several
matches: ITI-78 counts them in `Bundle.total` (§2:3.78.4.1.3 Case 1), and
ITI-119 returns one entry for each (§2:3.119.4.1.3 Case 2). The step feeds
localization, so an outage on an undirected query follows the localization
failure policy, `closed` by default, as a localizer outage does; an exchange
whose audit record cannot be stored fails closed under every policy. The
read of an EHR by subject answers `404` for no match and `424` for the
rest.

The client's identifier reaches the Supplier and nowhere else; the master
identifier reaches the cross-reference and nowhere else. The outbound gate
withholds both from every request to a node, and no log line, metric label
or error carries either (§5.4.1, N33). Each exchange is audited through
[`[audit]`](audit.md), which is required outside development. The step's
state shows as `demographics` on
[`GET {base}/health/dependencies`](health.md), and each call is counted in
`ferrofed_demographics_requests_total` ([Metrics](metrics.md)).

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
The outage is never silent: the answer to a query carries it as
`meta.federation.consent.error`, mirroring `localization.error` of §14.1
([The client contract](../integrate/client-contract.md)), the pre-filter's
state on `GET {base}/health/dependencies` turns `down` or `failing`
([Health probes](health.md)), and each call is counted in
`ferrofed_consent_prefilter_requests_total` ([Metrics](metrics.md)). The
pre-filter applies to every patient route: a federated query and the read
of an EHR by subject
([Follow-ups](../integrate/follow-ups.md#reading-an-ehr-by-subject)).
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

In the Netherlands the pre-filter is Mitz, below. Set `[[dev.consent_denied]]`
rows or `[nl_gf.mitz]`, never both: both refuse the configuration.

### Dutch consent: `[nl_gf.mitz]`

Mitz keeps the consent Dutch patients record and answers one closed
question about it, the *gesloten autorisatievraag* (Annex B §B.6): may this
data holder make this patient's data of these categories available to this
data user, for this purpose? The gateway asks it once per data holder among
the candidates, after localization and before resolution. A member whose
holder Mitz denies for every category asked is `consent-denied` and never
asked. Every other member is asked, and its node checks consent itself:
Mitz is a filter in front of the gate, not the gate (§14.3, N27).

The wire is the VZVZ *Implementatiehandleiding Open en gesloten
autorisatievraag* 3.8.2: a SOAP 1.2 request carrying one XACML 3.0
`XACMLAuthzDecisionQuery`, over mutual TLS, with an `X-Request-Id` on every
request. VZVZ states no licence for the document, so the repository pins it
by sha256 and does not ship it; `scripts/vendor/mitz.sh` fetches it for
reading.

```toml
profile = "production"

[nl_gf.mitz]
url = "https://mitz.example.org/geslotenautorisatievraag"
client_identity_file = "/run/secrets/mitz-client.pem"   # mutual TLS
trust_roots_file = "/etc/ferrofed/mitz-roots.pem"       # optional
credentials = { bearer_token_file = "/run/secrets/mitz-token" }  # optional
namespaces = ["urn:oid:2.999.1"]  # client namespaces that stand for the BSN
purpose = "TREAT"                 # TREAT or COC
data_categories = ["GGC002"]      # the Mitz data categories asked about
timeout_ms = 1000                 # one round of questions, within the query's budget

[nl_gf.mitz.holders]              # each member's care provider
"node-a" = { type = "V6" }        # the URA from [nl_gf.nvi.custodians] or the directory
"node-b" = { type = "V6", ura = "ura-test-0002" }

[[auth.issuer]]                   # the issuer of your callers' tokens
issuer = "https://issuer.example.org"
jwks_uri = "https://issuer.example.org/jwks"

[auth.issuer.requester]           # the claims of its tokens that name the requester
professional = "uzi_number"       # the professional's UZI number
role = "uzi_role"                 # the professional's UZI role code
organisation = "ura"              # the organisation's URA
organisation_type = "organisation_type"
```

What a deployment must provide:

- **The BSN.** Mitz is asked by BSN. A client names the patient in a BSN
  system (`http://fhir.nl/fhir/NamingSystem/bsn`, or the BSN's OID as
  `urn:oid:2.16.840.1.113883.2.4.6.3` or dotted) or in one `namespaces`
  lists. A patient named by the pseudonymised BSN, as the NVI requires,
  cannot be asked about: the pre-filter then carries no consent signal and
  every candidate is asked. The pseudonym's system is never accepted in
  `namespaces`. The BSN reaches Mitz and nothing else: never a node, a log
  line or an error.
- **A holder per member.** Every registry member needs a `type`, and one
  URA from its `ura`, `[nl_gf.nvi.custodians]` or a directory that
  publishes URAs; where more than one gives it they must agree. A member
  with no holder, a holder with no URA, or a holder naming no member
  refuses the configuration.
- **The requester, in the caller's token.** The question names the
  professional who asks, by UZI number and role, and their organisation, by
  URA and type. That is always the verified caller: Mitz records the
  professional and decides on their role, so the gateway never asks for
  anyone else. Map, per trusted issuer, the four token claims that carry
  them under `[auth.issuer.requester]`
  ([Client authentication](authentication.md#configuration)); no
  specification the gateway binds names these claims, so each is configured
  and none has a default. A caller whose token does not carry all four is
  not asked about: Mitz is not called, no member is filtered, and each node
  checks consent itself (N27).
- **TLS.** The `url` must be `https` outside `profile = "development"`.
  `credentials` takes a bearer token or basic credentials, never an OAuth
  2.0 grant. Whether a gateway may ask Mitz at all is a matter of admission
  to the Mitz afsprakenstelsel.

Mitz answers `Permit` or `Deny` per category. `Indeterminate`, a fault, a
status other than `200`, silence past `timeout_ms` and an answer that does
not hold to the question are no decision: the members of that holder are
asked, and the failure is carried in `meta.federation.consent.error`. When
Mitz denies one holder and fails for another, the denied members are
`consent-denied`, the others are asked, and the failure is still carried.
`OPTIONS {base}/` declares the pre-filter as `"nl-gf-mitz"`.

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

## XCPD localization: `[xcpd]`

The `[xcpd]` table makes the gateway an IHE XCPD Initiating Gateway, the
specification's proposed localization binding (N4, §14.1, Annex A.3). Under
`federation.node_selection = "localized"`, each undirected patient query
first asks every configured Responding Gateway, by the patient's identifier
alone, which communities hold the patient (ITI-55 Cross Gateway Patient
Discovery, ITI TF-2 §3.55, Revision 20.1). The members serving those
communities are the candidates; every other member is `not-localized` and is
not asked. A match is a candidate to ask, never a release decision: each
node still enforces consent (§14.3).

```toml
profile = "production"

[federation]
node_selection = "localized"

[federation.localization]
on_failure = "closed"   # the default (§14.1)
timeout_ms = 5000

[xcpd]
sender_device = "2.999.40.1"           # the gateway's device OID
home_community = "2.999.40"            # optional: the gateway's own community
audit = "log"                          # required: "log", or "off" in development
assertion_file = "/run/secrets/xua.xml"            # optional
client_identity_file = "/run/secrets/xcpd-client.pem"
trust_roots_file = "/etc/ferrofed/xcpd-roots.pem"  # optional

[[xcpd.gateway]]
url = "https://xcpd.region.example.org/RespondingGateway"
device = "2.999.50.1"                  # the receiver device OID
# community = "2.999.50"               # optional: ask for this community only

[xcpd.communities]                     # every member needs one
"2.999.50" = "node-a"
"urn:oid:2.999.60" = "node-b"

[xcpd.namespaces]                      # only for a namespace that is no OID
"region-mrn" = "2.999.1"
```

The request names the patient by the shared identifier mode of ITI-55:
one `LivingSubjectId` whose `root` is the assigning authority and whose
`extension` is the value, with no name, birth date or other demographics.
A namespace that is an OID, dotted or as `urn:oid:`, is the assigning
authority; any other needs an entry in `[xcpd.namespaces]`. A namespace with
no mapping fails the query closed.

What a deployment must provide:

- **The device OIDs** of the gateway (`sender_device`) and of each
  responding gateway (`device`). ITI TF-2 Appendix O requires an ISO OID for
  each, and every identifier here is refused at boot unless it is one.
- **The community map.** Every registry member must be served by a
  community, or boot is refused, since no discovery could ever name it. A
  community a gateway answers that the map does not name belongs to no
  member and adds no candidate.
- **TLS.** Every XCPD actor is an ATNA Secure Node or Secure Application
  (ITI TF-1 Table 27.1.3-1), so a gateway URL must be `https`;
  `client_identity` (or `client_identity_file`) holds the PEM client
  certificate chain and private key for mutual TLS, and `trust_roots_file`
  adds the network's roots to the platform's. A plain `http` URL is refused
  at boot, naming its key, unless the configuration is
  `profile = "development"`.
- **The XUA assertion, where the network requires one.** ITI-55 requires
  none, but many networks require a SAML 2.0 assertion (IHE XUA, ITI-40).
  The gateway signs nothing: your identity provider or security token
  service issues and signs the assertion, and the gateway sends its bytes
  unchanged in a WS-Security header, so its signature still verifies.
  `assertion` (or `assertion_file`) must hold exactly one
  `saml2:Assertion` element that declares every namespace prefix it uses;
  anything else is refused at boot. An assertion expires: replace the file
  before its `NotOnOrAfter` and [reload](registry.md#reloading-the-registry).
- **An audit destination.** The Initiating Gateway records an audit message
  for every exchange (ITI TF-2 §3.55.5.1.1), so `audit` has no default.
  `audit = "repository"` sends each message to an ATNA Audit Record
  Repository over ITI-20 ([The audit repository](#the-audit-repository)).
  `audit = "log"` writes each message as a structured event at the log
  target `ferrofed::audit`: the event, its outcome (`0` success, `4` the
  gateway answered with a failure, `8` no answer), this process's id, the
  responding gateway's endpoint and host, and the `homeCommunityId` the
  request named. The query parameters, which name the patient identifier,
  are never logged; the event says only that they were recorded. Route that
  target to your audit repository. `audit = "off"` records nothing and is
  refused outside `profile = "development"`. `OPTIONS {base}/` declares the
  choice as `localization.audit`. A message the destination cannot accept
  fails the discovery closed under every policy, `on_failure = "ask-all"`
  included, since that policy covers a localizer outage and never an
  exchange the gateway could not audit: no answer is used without its audit,
  and no member is asked.

The discovery fails closed as a whole. One responding gateway that faults,
answers an error (Case 5 of §3.55.4.2.3), asks for demographics (Case 3),
answers outside ITI-55, or stays silent past `timeout_ms` leaves every member
`not-localized` with the error, and the query asks no node; a community
behind that gateway might hold the patient. `on_failure = "ask-all"` asks
every member instead ([Node selection](registry.md#node-selection)).
`OPTIONS {base}/` declares `localization.mode` as `"xcpd"`.

FerroFED sends the synchronous exchange only, with an immediate response: it
claims neither the Asynchronous Web Services Exchange nor the Deferred
Response option (ITI TF-1 §27.2), and it caches no correlation between
queries. `[xcpd]` takes effect on a reload, except where its audit messages
go, which takes a restart; under `node_selection = "ask-all"` it refuses the
configuration.

### The audit repository

With `audit = "repository"`, the gateway sends each exchange's audit
message to an ATNA Audit Record Repository as ITI-20 Record Audit Event
(ITI TF-2 §3.20): the DICOM PS3.15 audit message in an RFC 5424 syslog
message with the PRI `<85>` and the MSGID `IHE+RFC-3881`, over TLS (RFC
5425). That message must name the gateway's own `homeCommunityID` (ITI TF-2
§3.55.5.1.1), so `[xcpd] home_community` is required with
`audit = "repository"`: `config check`, `serve` and a reload refuse the
configuration without it, naming `xcpd.home_community`. Under
`audit = "log"` it stays optional, and the log event carries it when it is
set.

```toml
[xcpd]
audit = "repository"
home_community = "2.999.40"            # required with audit = "repository"

[xcpd.audit_repository]
url = "tls://arr.example.org:6514"     # the port defaults to 6514
hostname = "gateway.example.org"       # syslog HOSTNAME and the message's host
spool_dir = "/var/lib/ferrofed/audit-spool"
# app_name = "ferrofed"                # syslog APP-NAME
# source_id = "gateway.example.org"    # AuditSourceID, the hostname by default
# enterprise_site = "2.999.40"         # AuditEnterpriseSiteID
# spool_max_bytes = 67108864
# spool_max_events = 100000
# connect_timeout_ms = 5000           # opening the TCP connection
# send_timeout_ms = 5000              # the TLS handshake, each write and flush
# retry_max_ms = 60000                # the longest wait between two attempts
client_identity_file = "/run/secrets/atna-client.pem"  # when the repository asks
trust_roots_file = "/etc/ferrofed/atna-roots.pem"      # optional
```

Every message is written to the spool first, flushed to disk, and
delivered from there in order, so a message counts as recorded once it is
on disk. ITI-20 has a sender that cannot reach its repository store the
record and send it when it can (ITI TF-2 §3.20.4.1.1): while the repository
is down, discovery goes on and the messages wait in the spool, and a restart
keeps them. Only a message the gateway can neither deliver nor store is an
audit failure: a full spool (`spool_max_events` or `spool_max_bytes`) or one
that cannot be written fails the discovery closed, as any audit failure does
above. Recording an exchange only ever writes to the spool, so a slow or
hung repository never holds a query.

Delivery is bounded at every step: the TCP connection by
`connect_timeout_ms`, and the TLS handshake, each write and each flush by
`send_timeout_ms`, so a repository that accepts a connection and then stops
answering cannot hold the sender. After any timeout or transport failure the
gateway drops the connection, keeps the message in the spool, and tries
again after a wait that doubles from 250 ms up to `retry_max_ms`, with
jitter. A spooled message that cannot be read, or is no whole syslog frame,
is moved to the `quarantine` subdirectory, logged at error level with its
sequence number and never its content, and the drain goes on with the next
one; a quarantined message stays counted under `spool_max_events` and
`spool_max_bytes` until you remove it. Syslog over TLS has no
acknowledgement (RFC 5425), so only a transport failure is retried, and a
message written to a connection the repository has just closed can be lost;
the gateway checks the connection before each write to keep that window
short. A file in the spool directory the gateway did not write refuses the
start, naming the file to move out.

The spool is the one place FerroFED writes a patient identifier to disk:
each message carries the query parameters, base64-encoded, as the audit
table requires. The gateway creates the directory readable by its own user
alone (`0700`) and every file `0600`, and refuses to start, and
`config check` refuses the configuration, when the directory gives its group
or other users any access or cannot be written. Put it on an encrypted
volume: the gateway holds no key to encrypt it with, so encryption at rest
is the deployment's.

The url is `tls://` outside `profile = "development"`; under that profile
`tcp://host:port` is accepted and named on the banner, and without
`spool_dir` the spool is held in memory, which a restart loses and the
banner says so. `GET /health/dependencies` reports the repository as
`audit_repository`: `up`, `degraded` while the gateway retries a failed
delivery, while messages wait in the spool and while any sits in
quarantine, and `unknown` before the first message. The metrics carry the
spool's depth, its quarantine, the deliveries and the retries
([Metrics](metrics.md)). The PIXm, mCSD and PMIR transactions are audited
over the other option of ITI-20, the FHIR Feed of RESTful ATNA, under
`[audit]` ([The audit trail](audit.md)).

## Dutch localization: `[nl_gf.nvi]`

A deployment in the Netherlands can localize through the national index of
the Dutch Generic Functions, the NVI, in place of XCPD (Annex B §B.1,
GF-Localization of the Generic Functions IG `fhir.nl.gf` 0.3.0). Under
`federation.node_selection = "localized"`, each undirected patient query
first asks the NVI's Localization Service which care providers hold data
for the patient: `GET [url]/DocumentReference?patient.identifier=<pseudonym>
&type=http://loinc.org|55188-7`. The service answers with one localization
record per care provider, named by its URA, and the members that hold those
providers' data are the candidates. Every other member is `not-localized`
and is not asked.

```toml
profile = "production"

[federation]
node_selection = "localized"

[federation.localization]
on_failure = "closed"   # the default (§14.1)
timeout_ms = 5000

[nl_gf.nvi]
url = "https://nvi.example.org/fhir"
credentials = { bearer_token_file = "/run/secrets/nvi-token" }  # optional
client_identity_file = "/run/secrets/nvi-client.pem"            # optional, mutual TLS
trust_roots_file = "/etc/ferrofed/nvi-roots.pem"                # optional
namespaces = ["pseudo-bsn"]   # client namespaces that stand for the pseudonym

[nl_gf.nvi.custodians]        # optional with a directory that publishes URAs
"ura-test-0001" = "node-a"
"ura-test-0002" = "node-b"
"ura-test-0003" = "node-b"    # one member may hold several providers' data

[[pixm.manager]]              # resolution, Step 1c of Annex B §B.7
url = "https://pix.example.org/fhir/"

[pixm.manager.members]
"node-a" = "urn:oid:2.999.21"
"node-b" = "urn:oid:2.999.22"
```

The NVI is keyed on a pseudonymised BSN, never on the BSN. A client names
the patient by the pseudonym, in the namespace
`http://fhir.nl/fhir/NamingSystem/pseudo-bsn` or in one `namespaces` lists,
as in the walkthrough of Annex B §B.7. The gateway never pseudonymises: a
patient in any other namespace, a BSN included, cannot be localized, so the
query fails closed and the NVI is never asked. The pseudonym is personal
data like the BSN it stands for, so it is handled as every patient
identifier is: it reaches the NVI and the PIX Manager, never a node, a log
line or an error. A BSN system is never accepted in `namespaces`: listing
`http://fhir.nl/fhir/NamingSystem/bsn`, or the BSN's OID as
`urn:oid:2.16.840.1.113883.2.4.6.3` or dotted, refuses the configuration, so
a BSN can never reach the NVI labelled as a pseudonym.

What a deployment must provide:

- **The custodian map.** Every registry member must be mapped from at least
  one URA, or boot is refused, since no localization could ever name it. A
  care provider the NVI returns that the map does not name is outside the
  federation and adds no candidate. When the registry comes from a care
  services directory (`[registry.mcsd]`, or a registry document in FHIR
  form) whose member organisations publish their URA, as the Dutch
  directory does with the LRZa as its source (Annex B §B.2), the gateway
  reads the map from it: each organisation's URA maps to the members it
  operates. The `custodians` table is then optional. If you write it anyway
  it must give exactly the same map, or the configuration is refused; the
  gateway never merges the two. An organisation that publishes two different
  URAs is refused as well. The map is rebuilt on every reload and directory
  refresh ([The registry from an mCSD directory](registry.md)).
- **TLS and credentials.** The `url` must be `https` outside
  `profile = "development"`; a plain `http` URL is refused at boot, naming
  its key. `credentials` takes a bearer token or basic credentials, never an
  OAuth 2.0 grant, and a URL that carries a user name or a password is
  refused. The IG asks a requester for authorization attributes (its
  organization, practitioner and role); the gateway sends only the
  credential configured here, and GF-Authentication on the Nuts profile is
  tracked in [#88](https://github.com/FerroHEALTH/FerroFED/issues/88).
- **A resolver.** The NVI answers where; the PIX Manager of `[pixm]` still
  answers under which `ehr_id` each candidate knows the patient.

The NVI checks the requester's access at each data holder before it
returns a record, so its answer is consent-aware. It is still only a list of
candidates: each node checks consent before it releases data (§14.3, N27).
The localization fails closed as a whole. A service that answers a failure,
answers outside the IG, or stays silent past `timeout_ms` leaves every
member `not-localized` with the error, and the query asks no node.
`on_failure = "ask-all"` asks every member instead. `OPTIONS {base}/`
declares `localization.mode` as `"nl-gf-nvi"`.

Set `[nl_gf.nvi]` or `[xcpd]`, never both: both refuse the configuration.
`[nl_gf.nvi]` takes effect on a reload; under `node_selection = "ask-all"`
it refuses the configuration. The consent pre-filter of the Dutch binding is
[Mitz](#dutch-consent-nl_gfmitz).

## The identity feed: `[pmir]`

A merge at the identity source can leave a caller's
[resolution bindings](registry.md) naming the `ehr_id` of an identity that no
longer exists. The bindings expire with their lifetime in any case. With
`[pmir]` set, the gateway also hears of each change as it happens: it
subscribes at your IHE PMIR Patient Identity Registry with ITI-94, and the
Registry sends every Patient Master Identity change to the gateway as an
ITI-93 message (PMIR 1.6.0, Annex A.4).

```toml
[pmir]
url = "https://pmir.example.org/fhir"                    # the Registry's FHIR base
callback_url = "https://gateway.example.org/pmir/feed"   # where the Registry sends the feed
path = "/pmir/feed"                                       # the route under {base}, the default
feed_token_file = "/run/secrets/pmir-feed-token"          # the token the Registry sends
# identifier_system = "urn:oid:2.999.1"                  # only Patients with an identifier here
# timeout_ms = 5000
# check_interval_s = 60

[pmir.credentials]                    # how the gateway authenticates to the Registry
bearer_token_file = "/run/secrets/pmir-registry-token"
```

- **The subscription.** At start the gateway creates a `Subscription` at
  `url` with a `message` channel to `callback_url` and a FHIR JSON payload.
  Its criteria is `Patient`, or `Patient?identifier=<system>|` with
  `identifier_system` set, so the Registry sends only the Patients that
  carry an identifier from that authority. Before it creates one, the
  gateway searches the Registry for its own (`GET
  [base]/Subscription?url=<callback_url>`) and adopts the one it finds, so a
  create the Registry answered late is never made twice. A Registry that
  answers that search `400` or `404` does not support it, and the gateway
  creates. Every `check_interval_s` the gateway reads the subscription back.
  It deletes and recreates one the Registry reports `error` or `off`, and
  recreates one the Registry no longer holds. Each failed check doubles the
  wait before the next, up to 32 times `check_interval_s`, and a check that
  succeeds resets it. If the Registry answers a create `201` with no
  `Location`, or with one outside `url`, the gateway cannot manage that
  subscription. It creates no other until a restart and reports the fault as
  `unmanageable`; delete that subscription at the Registry. On a drain the
  gateway stops checking, lets the check in flight end, and then deletes its
  subscription, found by search when it never learned where it was.
- **Authenticating the feed.** The subscription carries no credential for the
  feed (PMIR §2:3.94.5), so you agree the feed token with the Registry's
  operator and configure it on both sides. The Registry sends it as
  `Authorization: Bearer <token>`. A message without that token is answered
  `401` and changes nothing.
- **The route.** `POST {base}{path}` takes one ITI-93 message. The route
  sits outside the ITS-REST surface and its client authentication, so the
  path may not be under `/v1`, `/health` or `/.well-known`. A message that
  does not hold to the PMIR profiles is answered `400` (`415` for a media
  type other than FHIR JSON) with an `OperationOutcome`, and changes
  nothing. An applied message is answered with the ITI-93 response.
- **What a message changes.** Only routing state, never a record. A merge
  or a delete drops every binding of the `ehr_id`s its Patient carries in a
  member's `ehr_id` domain, the domains `[pixm]` maps. An update states the
  identity as it now is and never what it lost, so it drops every binding,
  as does a merge or delete that carries no such `ehr_id`. A create drops
  nothing. A dropped binding costs one resolution, never a wrong node. The
  `ehr_id` index stays: a merge moves no EHR between nodes.
- **What is logged.** The kind of each change and their counts, and how many
  bindings went. The messages carry patient identifiers and demographics, so
  no identifier reaches a log line, a metric or an answer.

`url` and `callback_url` carry patient identities, so both are `https`
outside `profile = "development"`, and neither may carry a user name or a
password. `GET /health/dependencies` reports the Registry as
`identity_registry`: `up` while the gateway holds a subscription in
`requested` or `active`; `failing` after a refusal, an answer that breaks
ITI-94, or a create it cannot manage; and `down` when the Registry did not
answer. `identity_registry_fault` names the reason ([Health](health.md)). Each message is counted by result in
`ferrofed_identity_feed_messages_total` ([Metrics](metrics.md)). A change to
`[pmir]` takes a restart.

PMIR 1.6.0 defines no unmerge (§2:3.93.4.1.3), so the gateway hears of a
merge and never of a split. The specification marks this lifecycle track
provisional, and FerroFED claims no propagation of an identity change to a
later resolution: the [conformance matrix](../evaluate/conformance.md) keeps
track 8 deferred.
