<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Localization

Under `federation.node_selection = "localized"`, the gateway first asks a
localization service which members may hold the patient, and asks only
those (§14, N4). This page covers IHE XCPD, the Dutch NVI, and the audit
repository XCPD reports to. A PIX Manager and the development table can
localize as well ([Identity resolution](identity.md)).

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
# spool_write_timeout_ms = 2000       # storing one message in the spool
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
audit failure: a full spool (`spool_max_events` or `spool_max_bytes`), one
that cannot be written, or one that does not store the message in time
fails the discovery closed, as any audit failure does above. Recording an
exchange only ever writes to the spool, so a slow or hung repository never
holds a query, and a slow disk holds it for a bounded time
([A slow disk](audit.md#a-slow-disk)).

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
credentials = { bearer_token_file = "/run/secrets/nvi-token" }  # optional, or the Nuts grant
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
  its key, as is a URL that carries a user name or a password.
  `credentials` takes a bearer token, basic credentials or the Nuts grant of
  GF-Authentication ([The NVI's Nuts grant](onward-credentials.md#the-nvis-nuts-grant)),
  one of them; an `oauth2` or `fapi2` table is refused, naming its key, as
  the IG defines no such grant for the Localization Service. The IG asks a
  requester for authorization attributes (its organization, practitioner and
  role); the gateway sends only the credentials configured here.
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
[Mitz](consent.md#dutch-consent-nl_gfmitz).
