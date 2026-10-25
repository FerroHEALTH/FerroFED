<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Maintenance rule: every pull request that changes user-visible behaviour adds
its entry in the same PR as a fragment under `changelog.d/`, in the format
`changelog.d/README.md` describes, and leaves this file alone. Cutting a
release runs `scripts/release/changelog.sh --assemble <version> <date>`, which
writes the fragments and any entry still under **[Unreleased]** into the new
version's section and moves the link references on.

The architecture is `docs/architecture.md`, the output of the research
program on the v0.0.1 milestone. Releases on the 0.0.x line started with the
repository, its gates, its documentation and the server shape in 0.0.1; the
federated query and identity resolution shipped in 0.0.3; cross-node result
shaping, single-node routing and the targeting mechanisms in 0.0.6.

## [Unreleased]

## [0.0.13] - 2026-10-25

Security fixes until 2031-10-25, 5 years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)), shipped only in the newest release (Art 13(10)). This release is supported while it is the newest; upgrade to the newest release to get a fix, as [SECURITY.md](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions) says.

### Upgrade notes

- The Kubernetes example moved to `deploy/kubernetes/base/`: apply it with
  `kubectl apply -k deploy/kubernetes/base` in place of
  `kubectl apply -f deploy/kubernetes/`. Its NetworkPolicy now admits only
  your reverse proxy's namespace (`ingress-nginx` until you edit it) and the
  console to the gateway port, so a client inside the cluster that called
  the gateway directly needs a peer of its own; the StatefulSet's
  `terminationGracePeriodSeconds` is 50 and its governing Service is the
  new `ferrofed-admin`. Kubernetes refuses a change of a StatefulSet's
  `serviceName`, so run `kubectl delete statefulset ferrofed
  --cascade=orphan` before you apply; the pods and their audit spool claims
  stay, and the new StatefulSet adopts them (#638).
- Each `[[fhir.receive]]` names, in `dependencies`, every package of its
  `package`'s dependency closure (each a `.tgz` or an unpacked directory),
  and the package of the MyHealth@EU value sets its profiles bind. A
  `[[fhir.receive]]` whose packages lack one a manifest names is refused at
  start and by `config check`, naming the missing package. For the HL7
  Europe Patient Summary, the book's receiving page lists the packages
  (#808).

### Added

- The three medication clusters of the openEHR international Clinical
  Knowledge Manager that the medication order's slots admit,
  `CLUSTER.medication_authorisation`, `CLUSTER.medication_order_summary` and
  `CLUSTER.medication_supply_amount` (all DRAFT v0), vendored under CC BY-SA
  by `scripts/vendor/openehr-ckm.sh` for the openEHR mapping of
  prescriptions (#1043).
- The console is ready for more languages: every string it draws, its
  handlers' plain-text answers and a revision diff's change words among
  them, comes from its catalog, and the stray-literal check runs with an
  empty baseline. A pseudo-locale, `[server] locale = "qps"`, draws every
  word accented and two fifths longer, and a browser journey holds every
  page to fitting 1440 and 390 px in it (#1162).
- The patient summary the FHIR face writes translates a code in no preferred
  code system through the terminology server `[eehrxf.terminology]` names:
  each `equivalent` or `equal` match of `ConceptMap/$translate` into a
  preferred system is added after the original coding, and a translation the
  server refuses or does not answer fails the summary. With no server
  configured, the code is kept as recorded, under Art 10(7) of the eHealth
  Network Guidelines on Patient Summary
  ([#1213](https://github.com/FerroHEALTH/FerroFED/issues/1213)).
- `OPTIONS {base}/` reports `latency_ms_p50` and `latency_ms_p95` for each
  member endpoint, the median and the 95th percentile of the last 100
  requests the gateway sent it, and leaves both out for an endpoint it has
  not asked; the operator console shows the two side by side on the node,
  endpoint and self-description lists and in the topology (#1220).
- The admin listener answers the running configuration at
  `GET /admin/config/running`, behind the operator scope: each
  deployment-wide setting with the value the replica runs, where it comes
  from (the file, an environment override, the running revision or the
  default), its default, and whether a change waits for the next start,
  with the value that start takes. A secret is named by its file, never by
  its value. Under `config.ui = "managed"`, `PUT /admin/config/settings`
  saves a revision with named settings changed, applied as any revision
  (#1235).
- The running configuration of the admin listener and the operator
  console's new Authentication tab show who may call: the audience, the
  operator scope, each token issuer and each trusted issuer, with every claim
  name, the correlation header and the added countries of a national contact
  point the file declares. The tab is read-only, since only the file sets
  `[auth]` and `[trust]`, and a secret is named by its file alone (#1269).
- The public MyHealth@EU, eHDSI and Xt-EHR material is vendored with its
  provenance: the MyHealth@EU FHIR guides (core, Patient Summary,
  ePrescription and eDispensation, laboratory, discharge and imaging reports,
  vaccination), the NCPeH API guide at its current build 9.1.1-ci, the OpenNCP
  Transformation Manager's CDA Schematrons and eHDSI coded element list, the
  newer HL7 Europe laboratory (2.1.0) and discharge report (1.0.0) releases,
  the Xt-EHR models 0.3.0 the MyHealth@EU guides use, and the Xt-EHR Metadata
  Foundation guide; the eHDSI CDA reference test documents and every published
  Xt-EHR deliverable are cached, since they state no licence. Decisions A51,
  A60 to A72, A81 and A102 are re-checked against them, and A66 adds a patient
  lookup by identifier on the FHIR face, which Xt-EHR D5.1 asks of every
  producer EHR system (#1290).
- The FHIR face looks a patient up by identifier, `GET {fhir-base}/Patient?identifier=system|value`, as Xt-EHR D5.1
  §3.4.6 asks of a producer EHR system: the patient is resolved at the gateway and no member is asked, the answer is
  a `searchset` with the `Patient` the PDQm Supplier holds when a member holds the patient, an identifier no member
  holds and a restricted patient both answer the empty `searchset`, a search by demographics is refused, a lookup
  that finds the patient is recorded in the access log, and `metadata` declares the search (#1294).
- Vendored the packages the MyHealth@EU guides and HL7 Europe Laboratory Report 2.1.0 and Hospital Discharge
  Report 1.0.0 depend on and the repository lacked: HL7 Europe Imaging Report 1.0.0-alpha, HL7 Europe Extensions
  0.1.0, IHE MHD 4.2.2 and International Patient Summary 2.0.1, committed, and HL7 Terminology 7.2.0 and 7.4.0 and
  the R5 to R4 cross-version extensions 0.0.1-snapshot-2, pinned in the cache, with a test that fails when one of
  those manifests names a package no corpus holds (#1306).
- `deploy/kubernetes/` is a kustomize package, and every release attaches
  it as `ferrofed-vX.Y.Z-kubernetes.tar.gz` with the gateway and console
  images pinned by digest. Its base adds a `preStop` sleep matching the
  drain delay, topology spread over zones and nodes, a headless Service for
  the admin listeners and a NetworkPolicy that admits the reverse proxy,
  the scraper and the console alone; its components add the operator
  console, an Ingress or a Gateway API route for each, a Prometheus
  Operator ServiceMonitor and an egress policy. The book's Kubernetes page
  says how a changed configuration is reloaded without a restart (#638).
- `telemetry.otlp_endpoint` and `metrics.otlp_endpoint` take an `https://`
  collector, reached over TLS with the platform's trust roots, and
  `[telemetry.otlp_tls]` and `[metrics.otlp_tls]` add `trust_roots_file`
  and a client identity (`client_identity` or `client_identity_file`) for
  mutual TLS, each read and checked at start, so a remote collector needs
  no forwarder beside the gateway (#644).
- A national contact point's national connector end to end: the harness
  national connector in `ferrofed-testkit`, with synthetic Implementing
  Regulation (EU) 2026/2099 Annex attributes, reads the patient summary, the
  document list and the prescriptions over two FerroEHR nodes and posts a
  dispensation, behind `FERROFED_E2E`, and a restricting member leaves no
  trace in any answer. The book's new page, A national contact point, says
  how to connect and configure one and what stays with the contact point
  (#735, #524).
- A document received on the FHIR face is validated in full by
  FerroBRIDGE's `ferrobridge-validator`: every constraint of its category's
  profiles and of the profile each entry claims or its slice names, every
  invariant and terminology binding included. An error refuses the document
  with `422`, each warning is returned in the outcome of the write, and
  every constraint the validator did not evaluate is named there. A
  `required` binding the packages cannot decide is asked of the
  `[eehrxf.terminology]` server, and one it does not answer refuses the
  document with `502`. `scripts/vendor/fhir-packages.sh` fetches the
  packages of the Patient Summary's dependency closure that the repository
  does not commit (#808).

### Changed

- Moved the quickstart and the end-to-end harness to FerroEHR 5.4.1 and Keycloak 26.8.1, pinned by digest, and the CI tools to zizmor 1.31.1,
  promtool 3.15.1, the HL7 FHIR Validator 7.0.3 and Chrome for Testing 156.0.8078.47, with the W3C DID Resolution draft of 2026-10-24
  vendored (#1164).
- The console's configuration page opens on the deployment-wide settings,
  one tab per section as FerroUI's design draws them (Federation
  behaviour, Fan-out and paging, Consent, Stored queries, Access log and
  FHIR face), each setting with its value, its source, its key and its
  Next start mark, read from the gateway's running configuration. Edit on
  a tab saves the changed settings as a revision with a plain form post.
  With configuration revisions off, the settings tabs show read-only.
  `/configuration` now opens Federation behaviour; the members are at
  `/configuration/members` (#1235).
- Decisions A67 to A72 on the national contact point are re-checked against
  the Xt-EHR deliverables D6.1 and D6.2, the closest published text of the
  exchange format while the Commission has published no draft act, and each
  is confirmed. The deliverables are pinned by `scripts/vendor/xt-ehr.sh`
  (cache only, no licence statement), and the book's page A national contact
  point names the sources and records that the MyHealth@EU Requirements
  Catalogue 9.0.0 is behind EU Login and not read (#1265).
- `SECURITY.md` states the response times every product of the Ferro family
  states: an acknowledgement within about 5 business days in place of seven
  days, an initial assessment within about 10 business days, both best
  effort, and a fixed release published within 7 days of the confirmed
  assessment for a Critical vulnerability, 30 days for High, 90 days for
  Medium and with the next release for Low (#645).
- The interoperability component asks its terminology server
  (`[eehrxf.terminology]`) through FerroBRIDGE's published
  `ferrobridge-term` 0.1.131 in place of FerroFED's own client: the same
  `$translate`, `$lookup` and `$validate-code` in FHIR R4, over the same
  credential and TLS material, mutual TLS and private trust roots included.
  A code the server does not know is now named as such rather than by the
  HTTP status it came with, and every other refusal still carries the
  server's status. `eehrxf` 0.0.28 changes its terminology seam:
  `TerminologyServer` takes the system, code, target, value set and
  candidate it asks about and answers the matches, the display or the
  result, in place of the operations' whole `Parameters`, and
  `TerminologyError` gains `NotFound`; an implementation of the seam
  outside FerroFED changes with it
  ([#925](https://github.com/FerroHEALTH/FerroFED/issues/925)).

### Fixed

- `scripts/gh/project.sh updates` finds the FerroFED Roadmap again: it looks
  the board up through `repositoryOwner(login:)`, which resolves an
  organisation project, in place of `user(login:)`, which cannot (#1229).
- The compose file every release carries runs the operator console beside
  the gateway under a `console` profile (`docker compose --profile console
  up`), from `ghcr.io/ferrohealth/ferrofed-console` at the release's
  version, with the gateway's hardening, bound to 127.0.0.1, its OIDC
  client secret read as a file secret (#1258).
- A read of a prescription or a dispensation by its locator id on the FHIR
  face is held to the consent pre-filter as the search is: the gateway names
  the patient behind the id's `ehr_id` in the `[access_log]
  patient_namespaces`, and a member the pre-filter denies is asked nothing
  and answers as an id the face never wrote, with no trace of the
  restriction. Before, the read went to that member (#1270).
- A document list on the FHIR face that lists the gateway's summary writes
  one access record even when it asked no member, naming the members the
  patient resolved at and the `ehr_id` at each, since the list tells the
  caller that they hold the patient's record (Regulation (EU) 2025/327
  Annex II 3.2). Before, it wrote none (#1271).
- An ITI-20 syslog record the gateway wrote just before it ended its
  connection to the Audit Record Repository could be lost: the connection
  was dropped with the repository's TLS session tickets unread, so the
  socket was reset. The gateway now stops every audit trail on its `SIGTERM`
  drain, and each forwarder closes its connection with a TLS `close_notify`
  first (RFC 5425 §4.4), as it now does before it reconnects. `ihe-atna`
  and `ehds-logging` move to 0.2.6 and `ihe-iti` to 0.0.50 (#1278).
- A received dispensation's answer named as not evaluated only the
  invariants of `MedicationDispense-eu-mpd`'s root element, and none of its
  `required` bindings, so the invariants of every other element it carries
  and every binding were passed in silence. Each invariant of an element the
  dispensation carries or a slice it fills, and each `required` binding of
  the profile and of `Medication-eu-mpd`, is now listed as a `warning` of
  type `not-supported` (FHIR R4 conformance-rules.html §2.1.0.6,
  terminologies.html §4.1.5.1, #1291).
- The quickstart `compose.yaml` sends the usage report by default, as every
  FerroHEALTH compose file does, with its instance id on a new
  `ferrofed-state` volume: `FERROFED__USAGE_REPORT__ENABLED=false`, and
  `FERROEHR__USAGE_REPORT__ENABLED=false` for the four nodes, in your shell
  or `.env` stop it. `scripts/checks/usage-report-off.sh` holds test lanes
  alone again and no longer reads the compose file (#1296).
- The end-to-end harness's SanteMPI no longer fails a run on SanteMPI's
  transient `IOException` from its publish-and-subscribe queue. Provisioning
  a security application or an identity domain, and an ITI-93 feed, are
  tried again a bounded number of times with a backoff. A record SanteMPI
  already holds is not created again, and any other refusal still fails at
  once (#1302).
- The end-to-end harness's SanteMPI reads the identity domains it holds
  from `ASGN_AUT_TBL`, the table of the pinned 2.5 line, where it read a
  SanteDB 3 table that image lacks and stopped every run that provisions
  SanteMPI (#1308).

## [0.0.12] - 2026-10-24

Security fixes until 2031-10-24, 5 years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)), shipped only in the newest release (Art 13(10)). This release is supported while it is the newest; upgrade to the newest release to get a fix, as [SECURITY.md](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions) says.

### Upgrade notes

- The release's compose file and the Kubernetes example now set
  `FERROFED__USAGE_REPORT__STATE_DIR`, so a gateway started from them sends
  the installation report to FerroPULSE. Set
  `FERROFED__USAGE_REPORT__ENABLED=false` before you upgrade to keep it off
  (#1122).
- `[auth]` is now the `ferrohealth-smart` section every FerroHEALTH product
  reads (#1174). Move each issuer's FerroFED policy (`backend_clients`,
  `demographic_clients`, `client_tokens_act_for_professional`,
  `professional_issuing_authority`, `[auth.issuer.patient]`,
  `[auth.issuer.requester]`, `[auth.issuer.assurance]`,
  `[auth.issuer.national_contact_point]` and an introspected issuer's
  `introspection_endpoint`, `client_id` and `client_secret_file`) to a
  `[[trust.issuer]]` entry naming the same `issuer`, and `auth.mode`,
  `[auth.edge]` and `[auth.purpose_of_use]` to `trust.mode`, `[trust.edge]`
  and `[trust.purpose_of_use]`. Drop `jwks_uri` from each issuer: the key
  set is read from the issuer's RFC 8414 metadata, or from `jwks_file`.
  Replace each issuer's `operator_scope` with the one `auth.product_scope`,
  a string starting with `__` or a full URI you control. `auth.audience` is
  a list, and the clock skew, key set and timeout keys are
  `clock_skew_seconds`, `jwks_max_age_seconds`,
  `jwks_refresh_interval_seconds` and `timeout_ms`.
- Every static bearer token is gone (#1163). Replace `bearer_token` and
  `bearer_token_file` in a `[credentials."<endpoint>"]` section or a
  service's credentials with `user` and `password_file`, or with an `oauth`
  table, the client credentials grant of `ferrohealth-smart`; Mitz, the NVI
  and the terminology server take `user` and `password_file`. Replace
  `[metrics] scrape_token_file` with `scrape_users`, naming users of
  `[[auth.basic.users]]` whose Argon2id `password_hash_file` you write, and
  give Prometheus `basic_auth`. Replace `[pmir] feed_token_file` with
  `feed_users`, naming the Registry's own `[[auth.basic.users]]` user, and
  give the Registry its password.
- An `oauth2` section that asks for a plain client-credentials token, with
  no token exchange, no `DPoP` key, no binding to or authentication by the
  TLS client certificate and an assertion naming the token endpoint, is
  refused: move it to an `oauth` table, whose `private_key_jwt` signs with
  its own `private_key_file`.
- A log filter that names `ihe_iti::atna` (the audit spool and forwarder)
  names `ihe_atna::atna` instead; the events are the same.
- The operator console is now `ferrofed-console` (#1219). Pull
  `ghcr.io/ferrohealth/ferrofed-console` in place of
  `ghcr.io/ferrohealth/ferrofed-viewer`, rename every
  `FERROFED_VIEWER__<SECTION>__<KEY>` override to
  `FERROFED_CONSOLE__<SECTION>__<KEY>` and `FERROFED_VIEWER_CONFIG` to
  `FERROFED_CONSOLE_CONFIG`, and run the binary as `ferrofed-console`. The
  session and theme cookies are renamed, so every operator signs in once
  more after the upgrade.
- If you edited the `ferroehr-config` block of the quickstart's
  `compose.yaml`, FerroEHR 5.3.2 refuses the 4.3.9 Basic user keys at boot:
  rename `username` to `user`, give each user its `scopes` (the quickstart
  grants `user/composition-*.cruds`, `user/template-*.cruds`,
  `user/aql-*.cruds` and `__ferroehr.platform`), and move each user's
  `roles` to `[authz.rbac.basic_roles]` keyed by the user (#1226).
- Bookmarks of the operator console's `/members`, `/routing` and `/federation` pages answer `404`: use `/organisations`, a node's Routes tab under `/organisations/{org}/{node}/routes`, and `/self-description` (#1230). Each preference is now a cookie of its own (`ferrofed_console_theme`, `ferrofed_console_density`, `ferrofed_console_motion` and the others); the theme cookie is `ferrofed_console_theme` in place of 0.0.11's `ferrofed_viewer_theme`, so each operator chooses the theme once more. A deployment that builds the site bundle itself runs `cargo leptos build --split`, as `scripts/release/console-site.sh` does.

### Added

- The gateway sends the FerroHEALTH installation report to FerroPULSE on its
  envelope v2: one `start` report per process start and one `daily` report a
  day, carrying a random instance id, the version, the deployment kind,
  coarse host size, the member count as a range, the configured identity and
  consent services, latency per route group and the node request counts, and
  no patient data, AQL, path, node name or URL. It is on by default once
  `[usage_report] state_dir` names a writable directory, which the release's
  compose file and the Kubernetes example set; `enabled = false`
  (`FERROFED__USAGE_REPORT__ENABLED=false`) stops it, a CI environment stops
  it, and one boot line says which. The book's
  [usage report page](https://ferrofed.eu/docs/operate/usage-report.html)
  lists every field and carries the privacy notice (#1122).
- Mitz takes the OAuth 2.0 client credentials grant of
  `[nl_gf.mitz.credentials.oauth]` beside basic credentials. The gateway
  obtains the token at the token endpoint, which must be `https` outside
  `profile = "development"`, caches it while it is fresh, and sends it with
  each closed authorization question as a bearer token; a question Mitz
  answers `401` is asked once more with a fresh token. An `oauth2`, `nuts`
  or `fapi2` grant is still refused at load (#1180).
- The configuration page explains how it works in a dialog that How configuration works opens. The History tab filters its revisions by origin in an Origin menu that works with no WebAssembly, and each origin is a badge of its own colour. A secret's value field shows what you typed while you hold its eye, and stays masked otherwise (#1233).
- OpenVEX statements for FerroFED under `security/vex/`: the Rust advisory
  the dependency gate accepts (RUSTSEC-2024-0436, `paste`) with its
  argument and dependency path, generated from `deny.toml`. Each release
  attaches `ferrofed-vX.Y.Z-vulnerability-record.json`, every finding of a
  full scan of both images and the binaries with the statement that judges
  it, and is refused while a finding has none; each image is scanned before
  it is tagged, and the published images are scanned every week with the
  VEX applied (#1238).

### Changed

- `scripts/checks/changelog-guard.sh` and `crate-version-guard.sh` take
  `origin/main` as the base when run with no refs, and `stray-literals.sh`
  reads the console's sources when given no directory, so a local run of the
  guards passes on a clean branch; `docs/ci-cd.md` names the guards that need
  a built input first (#1151).
- The book's dashboard and Integrity screenshots show an open incident, a
  resolved one and a cleared collision, each with its time, so the visual
  review covers the console's timestamp rows (#1154).
- CI checks the operator console's `view!` formatting with leptosfmt 0.1.33,
  the version the `ui-gates` skill runs, before rustfmt, and the console's
  sources pass it (#1155).
- Only the newest release is supported. `SECURITY.md`, the book and the
  release notes say that every earlier release stops receiving fixes when a
  new one is published, with no maintenance branches, no long-term-support
  line and no backports, and that upgrading is free. Cadasto B.V. commits to
  keep publishing security fixes for at least five years after each release
  (Regulation (EU) 2024/2847 Art 13(8)) and ships them only in the newest
  release (Art 13(10)); the end date each release states is the date until
  which that commitment runs. The startup banner's `Support` line reads
  `newest release only, fixes until YYYY-MM-DD`, the notice for a release
  past its end date says to upgrade to the newest release, and the console
  labels the date `Fixes until`. The `supported_until` member of
  `OPTIONS {base}/` keeps its name and value.
- Client authentication, the SMART on openEHR grants and the gateway's own
  OAuth 2.0 client credentials run on `ferrohealth-smart` 0.3.1, the
  resource-server crate every FerroHEALTH product shares (#1174). Every
  token that does not verify is answered one `401` with
  `error="invalid_token"` and no description (RFC 6750 §3.1, RFC 9068 §4),
  and the security log names the failed check as `check`. Two
  `Authorization` fields, or one that is no credential, are a `400`
  `credential-malformed` with `error="invalid_request"`. A `403` names the
  error code and no scope. An issuer's metadata and key set are read when
  its first token arrives, so an issuer that cannot be reached is a `503`
  `authentication-unavailable` then, never a refused start.
- The metrics scrape and the PMIR feed authenticate HTTP Basic users of
  `[[auth.basic.users]]`, named by `metrics.scrape_users` and
  `pmir.feed_users`; one from another Basic user is answered `403`.
- The audit trail and the access record come from the FerroHEALTH audit
  library, FerroAUDIT: the ATNA ITI-20 sender, the BALP `AuditEvent` with
  its FHIR Feed, and the European logging component are the published crates
  `ihe-atna` and `ehds-logging` 0.1.0, pinned exactly, and their local copies
  in `crates/ihe-iti` and `crates/ehds-logging` are gone. What the gateway
  records, where it sends it and how it spools it are unchanged. The log
  events of the spool and the forwarder now carry the targets
  `ihe_atna::atna::spool` and `ihe_atna::atna::forwarder` in place of
  `ihe_iti::atna::spool` and `ihe_iti::atna::forwarder`.
- The `openehr-*` crates move to 0.0.90 with `fhirconnect` 0.1.173,
  `openehr-mapping-core` 0.1.124 and `fhir-types` 0.1.127, the line
  `ferrohealth-smart` 0.4.0 builds on, so the build holds one copy of
  `openehr-sdt` and of every other family member (#1181).
- `ferrohealth-smart` 0.4.0 (#1186). Every `[<section>.oauth]` table takes
  the keys the crate adds: `client_auth = "tls_client_auth"` or
  `"self_signed_tls_client_auth"` with `client_certificate_file` and
  `client_key_file` for RFC 8705 §2 mutual TLS to the token endpoint,
  `ca_file` for trust roots added to the platform's, and
  `assertion_audience = "issuer"` (RFC 7523 §3). An issuer's metadata and
  key set are still read on its first token, now by the crate; the
  `key-set-unavailable` event is written once per failed read. The answers
  do not change.
- The `e2e-placement` guard now refuses a CI workflow that could skip the
  container tests: a path filter on its triggers, a condition on the
  `e2e (containers)` job beyond the workspace check, or a `conclusion` check
  that does not wait for it. A change to the auth, credentials or
  configuration code, an e2e fixture or the configuration reference always
  runs the container tests (#1187).
- The audit library and the identity crates move together: `ihe-atna` and
  `ehds-logging` 0.2.1 from FerroAUDIT, `ihe-iti` 0.0.41 from FerroPIX on
  that `ihe-atna`, and `nl-generic-functions` 0.0.26 from FerroPIX on
  `ferrohealth-smart` 0.4.0, so the gateway carries one copy of it. What the
  gateway records, where it sends it and how it spools it are unchanged; the
  log targets of the spool and the forwarder stay `ihe_atna::atna::spool`
  and `ihe_atna::atna::forwarder`, now a stable interface of `ihe-atna`. The
  identity binding's FHIR Feed reaches an `http` base through `ihe-atna`'s
  `FeedRepository::cleartext`, which names the operator's acceptance of clear
  text (#1194).
- The workspace builds with Rust 1.99.1 in place of 1.98.1, and the MSRV
  stays at 1.98: the FerroHEALTH family builds with the latest stable
  toolchain and declares the release before it as its MSRV.
- The audit library moves to `ihe-atna` and `ehds-logging` 0.2.3, FerroAUDIT's
  only supported release, with FerroPIX's `ihe-iti` 0.0.43 on it. Nothing the
  gateway records changes: FerroFED declares its category map with exact
  matching, so its map digest is unchanged (#1211).
- Breaking: the operator console is renamed from `ferrofed-viewer` to
  `ferrofed-console` (#1219): the crate and its binary
  (`app/ferrofed-console`), the image (`ghcr.io/ferrohealth/ferrofed-console`
  in place of `ghcr.io/ferrohealth/ferrofed-viewer`, which is not published
  again), the release workflow and its attestation signer
  (`.github/workflows/release-console.yml`), the Dockerfile
  (`docker/console/Dockerfile`), the site and bundle scripts
  (`scripts/release/console-site.sh`, `console-tools.sh`,
  `scripts/checks/console-bundle.sh`, `console-paths.sh`), the CI job
  (`console`), the configuration file variable (`FERROFED_CONSOLE_CONFIG`),
  the environment prefix (`FERROFED_CONSOLE__`), the cookies
  (`ferrofed_console_session`, `ferrofed_console_sign_in`,
  `ferrofed_console_theme`, each with the `__Host-` prefix where secure) and
  the catalog (`locales/en/ferrofed-console.ftl`).
- The quickstart's four member CDRs and the test harness's FerroEHR nodes
  move from FerroEHR 4.3.9 to 5.3.2, the release on the same corrected base,
  with the node database image beside them (#1226). FerroEHR 5.1.0 changed
  its Basic user keys and checks a Basic user's SMART on openEHR scopes on
  every route, so the quickstart's FerroEHR configuration names its user with
  `user`, grants it the three resource-family scopes and FerroEHR's product
  scope, and moves its roles to `[authz.rbac.basic_roles]`; the restricted
  harness node does the same for its two users.
- The operator console is rebuilt on FerroUI 0.3.6 (`ferrohealth-ui`,
  pinned exactly, with its `split` feature) in islands mode (#1230, #1233,
  FerroHEALTH/FerroUI#161). The server draws every page whole from
  FerroUI's components, stylesheet and FerroFED theme, and the browser
  hydrates only FerroUI's interactive parts and the AQL editor, so every
  page reads and every action works with no WebAssembly. Every action is a
  plain form post to the page it sits on, answered by that page drawn again
  with the outcome at its top; a form the console or a replica refuses is
  drawn again as you typed it, the refused field marked. The pages follow
  FerroUI's reviewed FerroFED design: a dashboard, the topology (with "How
  it is drawn"), the organisations with their nodes (each node with its
  endpoints and its routes), the services, the integrity incidents, the
  stored queries with their versions, the query console, the
  self-description on five tabs with the manufacturer the gateway
  declares, the configuration, Your preferences and a find in the top bar.
  The top bar names the replica that answered. Every state is FerroUI's
  state mark (a shape and a word), and organisations and services carry
  their own glyphs. On the configuration's Members and Credentials and
  bindings tabs, each row's menu (⋯) holds Edit and Remove…, whose
  confirmation asks for the reason the revision keeps; endpoints, settings
  and new secrets are sub-cards whose rows Add and Remove change in place.
  The site bundle is built with `cargo leptos build --split`.
- The operator console is ready for more languages, English the one that
  ships (#1162). Its words and FerroUI's come from Fluent catalogs, the
  console's under `app/ferrofed-console/locales/en/`, so a language joins as
  a catalog directory with no code change. The locale is chosen once per
  request: the language the operator chose in Your preferences, else the
  best language of `Accept-Language` the catalog holds, else the new
  `[server] locale` (default `en`), and the page writes it as `<html lang>`.
  `scripts/checks/stray-literals.sh` fails CI on a console string outside
  the catalog beyond the per-file baseline (94 literals) the remaining
  strings hold while they move.
- The release notes end with a link to the CRA information and
  instructions to the user, and the release lane refuses a cut whose
  Security section names an entry without its GitHub security advisory
  (`GHSA-`) or VEX statement, or sits beside other changes without
  "Shipped with functional changes because:" and its reason (Regulation
  (EU) 2024/2847 Annex I Part II(2), (4), (8)). `SECURITY.md` states that a
  security fix ships alone or the release says why, how a fixed
  vulnerability's advisory is published with its release, and the
  `info@cadasto.com` route for a report without a GitHub account.
  `scripts/release/preflight.sh <tag>` runs the release plan's checks
  before the tag (#1237).
- `ihe-iti` comes from crates.io at `=0.0.40`, published by
  FerroHEALTH/FerroPIX, which owns the crate from here on; the local copy
  under `crates/` is gone. Its features and its behaviour are the ones the
  gateway already used: the PIXm, PDQm, mCSD, PMIR and XCPD clients, with
  `balp` for the FHIR profiles' audit records and `atna` for the ITI-55
  audit message, both written through `ihe-atna`.
- `nl-generic-functions` comes from crates.io at `=0.0.25`, published by
  FerroHEALTH/FerroPIX, which owns the crate from here on; the local copy
  under `crates/` is gone. Its Nuts grant now holds the authorization
  server's issuer to the RFC 8414 rule of `ferrohealth-smart`, so a plain
  `http` issuer is accepted on a loopback host only. The NVI, Mitz, LRZa and
  Nuts behaviour is otherwise unchanged.
- The patient summary the FHIR face writes applies the exchange format's
  term-mapping rule: an RM `TERM_MAPPING` whose `match` is broader, narrower
  or unknown is written as no coding, and the section's narrative carries
  it, naming the element, the relation, the target and the member it came
  from (eHN PS Art 10(3)). An equivalent mapping stays a coding after the
  original (#926).

### Removed

- The Tailwind CSS CLI pin and its download are gone: the operator console
  draws with ferrohealth-ui's stylesheet, so `scripts/release/console-tools.sh`
  installs wasm-opt alone and the site build needs no Tailwind (#1150).
- Static bearer tokens, inbound and outbound: `bearer_token` and
  `bearer_token_file` in every credentials section, `[metrics]
  scrape_token` and `scrape_token_file`, and `[pmir] feed_token` and
  `feed_token_file` (#1163). Machine-to-machine authentication is HTTP Basic
  or the OAuth 2.0 client credentials grant.
- The operator console's routes `/members`, `/members/{node}`, `/routing` and `/federation` are gone: the members are `/organisations` and `/organisations/{org}/{node}`, a node's routing table is its Routes tab, and the self-description is `/self-description` (#1230). The theme toggle is gone from the top bar; the theme is chosen in the operator's menu and in Your preferences. The console's server functions under `/api/` are gone, and with them its own stylesheet (`style/theme.css`) and scripts: it serves FerroUI's as the crate ships them.

### Fixed

- The configuration journeys read the focused element and the credential's
  value field afresh while they wait, so a re-render after a save no longer
  fails them with a stale element (#1149).
- The endpoint details show a dash for a value the gateway did not report
  (the latency and row count of an excluded node, an organisation, or the
  error of a node that answered), where a stacked card on a phone showed a
  label with nothing after it.
- The configuration screen labels `node_id` and `system_id` as "Node id"
  and "System id", with the configuration key as the field's hint (#1168).
- The dashboard's card-header link is FerroUI's shared card action, so its
  arrow keeps the space before it (#1169).
- The book's screenshot of the console's Credentials and bindings tab, in
  the light and the dark theme, is taken again and no longer shows the
  removed `bearer_token` row, and the environment override example on the
  configuration page sets a password file in place of the removed
  `bearer_token_file` (#1179).
- The interoperability component's reference property admits `#` from the
  Patient entry, which FHIR R4 resolves to the holding resource itself, so
  the test no longer fails when its generator draws that input; the case is
  kept as a regression seed (#1195).
- The trace hygiene tests name their synthetic patient with letters, so a
  span's `busy_ns` or `idle_ns` can no longer spell the identifier and fail
  the scan by chance (#1198).
- A browser journey whose chromedriver loses its port to another process
  starts another chromedriver, up to three times, so parallel journeys no
  longer fail before they drive the browser (#1209).
- The operator console's dashboard draws its cards two to a row, so the recent incidents table no longer overflows its card when an incident kind is long (#1207).

### Security

Shipped with functional changes because: the three fixes reached `main`
between 2026-10-20 and 2026-10-23, and the owner cut this release from
`main` on 2026-10-24 with no security-only patch of 0.0.11 before it; only
the newest release is supported (SECURITY.md), so such a patch would be out
of support the moment this release lands and give no one a release to stay
on. Each fix is a dependency or base-image bump with no behaviour change of
its own.

- The TLS stack takes `rustls-webpki` 0.103.16 and `webpki-root-certs`
  1.0.10, the releases that fix RUSTSEC-2026-0382 and RUSTSEC-2026-0383
  (distrusted emSign intermediates; GHSA-w3hc-w475-673c).
- The gateway and console images no longer trust the compromised eMudhra
  (emSign) intermediates in the system trust store (Debian DSA-6572-1,
  GHSA-w3hc-w475-673c, #1226). The distroless base
  `gcr.io/distroless/static-debian13:nonroot` moves to index digest
  `d620678`, which ships `ca-certificates` 20250419+deb13u1, in the `FROM`
  of `docker/Dockerfile` and `docker/console/Dockerfile` and in the pin
  row. `rustls-webpki` 0.103.16 and `webpki-root-certs` 1.0.10 were
  already in the lock file.
- `ctutils` moves from 0.4.2 to 0.4.4 (RUSTSEC-2026-0385,
  GHSA-rv83-4vc9-vqxg): before 0.4.4 its constant-time mask generation and
  `u128` comparisons could compile to a branch on a secret value. It
  reaches the gateway through `digest` and the Argon2 password hashing of
  `ferrohealth-smart` (#1232).

## [0.0.11] - 2026-10-16

Supported until 2031-10-16, 5 years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)). A security fix ships as its own patch release on each supported line, as [SECURITY.md](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions) says.

### Upgrade notes

- The operator console refuses `offline_access` in `[oidc] scopes`. Remove
  it from the list and set `[oidc] offline_access = true` instead (#643).
- A receiving member's template that admits several `COMPOSITION.category`
  codes and assumes none now refuses every received document of its
  category with a `422` (`MissingRequiredSuffix` on `<root>/category|code`).
  Give the template's `category` an assumed value (AOM 1.4 `assumed_value`)
  (#714).
- `ferrofed_node_requests_total` carries a new label, `class`: `member`
  for a member's own answer or failure, `client-error` for a `4xx` it
  answered a routed request with. Such a `4xx` now counts as
  `outcome="node-error"`, where it was `active`. The shipped
  `FerroFEDMemberFailing` rule leaves `class="client-error"` out, so it
  pages on the same member failures as before; a rule or a panel of your
  own that reads `outcome="node-error"` should leave it out too (#982).

### Added

- Information a representative inserts, marked as theirs (#1003,
  Regulation (EU) 2025/327 Art 4(2) and Art 5): an issuer that sets
  `act_is_representative = true` on `[auth.issuer.patient]` declares that
  the RFC 8693 `act` claim of its patient tokens names a proxy or a legal
  representative. A document the FHIR face receives under such a token is
  written with every entry naming the representative as its `provider`, a
  `PARTY_IDENTIFIED` whose one identifier is the `act` `sub`, never the
  person's `PARTY_SELF`, and its access record carries the
  `ehds-representative-inserted` entity. A token whose `act` names the
  representative by nothing, by its own `sub` or by an identifier of the
  document's patient is `403` with nothing sent. Without the declaration
  the gateway reads no representative, and the person's own insertion
  stays `PARTY_SELF`. `eehrxf` 0.0.19 adds `Representative` and
  `ReceivedComposition::inserted_by_representative`; `ehds-logging` 0.0.24
  replaces `AccessRecord::subject_inserted` with `inserted`, an
  `Insertion` of the subject or a representative. RFC 8693 is vendored.
- A `configuration-invalid` refusal on the operator console's
  configuration page shows the gateway's message, which names the key it
  refused and never a value, beside the status and the code.
- The operator console is styled on the "Azure & Iron" brand tokens, in a
  light and a dark theme: tables with a header row and row separators,
  labelled form fields, refusals and incomplete answers in boxes that say
  what they are, and health states as words in a tinted cell. Every page
  fits a 320 px screen: a wide table scrolls sideways in its own box, never
  the page. The stylesheet is Tailwind CSS 4.3.3, built by cargo-leptos with
  no Node and no new JavaScript (#1037). The shell these pages sit in, its
  top bar, navigation and theme switch, is the one under Changed (#1075).
- The electronic prescription and dispensation datasets of the
  interoperability component (#1038, Regulation (EU) 2025/327
  Art 14(1)(b) and (c)): `eehrxf` 0.0.21 carries, under the features
  `prescription` and `dispensation`, the crosswalks from the Xt-EHR
  `EHDSMedicationPrescription` and `EHDSMedicationDispense` 1.0.0 models to
  the HL7 Europe MPD 1.0.0 `MedicationRequest-eu-mpd` and
  `MedicationDispense-eu-mpd` profiles, held to both vendored packages. With
  `fhir-r4`, the new `medication` module reads a member's
  `MedicationRequest` and `MedicationDispense` into the datasets with a typed
  refusal for every rule a resource breaks, writes them back in the MPD
  form, and checks a resource and each `Medication` it contains against the
  vendored MPD profiles, naming what the check cannot evaluate. The number
  of dispensations a prescription item has left travels in the extension
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` on
  `dispenseRequest`, written only when the member states a value.
- The members that serve electronic prescriptions and dispensations on the
  FHIR face (#1039, Regulation (EU) 2025/327 Art 14(1)(b) and (c)):
  `[[fhir.serve]]` declares, per category, the members that serve it on
  their own `hl7-fhir-rest` endpoint in the HL7 Europe MPD 1.0.0 profiles,
  and `config check` names them. A category no member is declared for is
  answered as not served on `GET {fhir-base}/MedicationRequest`,
  `GET {fhir-base}/MedicationRequest/{id}` and
  `POST {fhir-base}/MedicationDispense`: `404` with an `OperationOutcome`
  of the issue type `not-supported`, never an empty list, and no member is
  asked. `GET {fhir-base}/metadata` lists `MedicationRequest` and
  `MedicationDispense` only for a declared category. The start is refused
  for a declared member the registry does not hold, one with no endpoint of
  its own serving the category, a native endpoint of either category that
  leaves out its MPD profile, and a face with no resolver.
- The search of a patient's prescriptions on the FHIR face (#1040,
  Regulation (EU) 2025/327 Art 14(1)(b)):
  `GET {fhir-base}/MedicationRequest?patient.identifier=system|value` asks
  every member declared for `Electronic-Prescriptions` that holds the
  patient at its own endpoint, by its own `ehr_id` alone, following the
  member's `next` links under its own base. The answer is one `searchset`
  with each member's `MedicationRequest` as the member sent it except its
  `id` and its entry's `fullUrl`, which carry the face's locator, the `id`
  the read takes (#1041); one `Provenance` naming the member, and a
  `warning` when two members hold a prescription under the same
  identifier. `_count` pages the answer through the gateway's own `-offset`
  parameter. A silent member, and a member the cross-reference could not
  answer for (`not-resolved`), fails the search under all-or-nothing and is
  named under `partial`; an unknown patient and a patient a consent
  decision hides get the same empty `searchset`. The search takes the
  `composition-` search scope over every template and is recorded under
  `Electronic-Prescriptions`.
- The read of one prescription on the FHIR face (#1041, Regulation (EU)
  2025/327 Art 14(1)(b)): `GET {fhir-base}/MedicationRequest/{id}` reads
  a prescription by the `id` the search now gives it, which names the
  member's own endpoint, its `ehr_id` and its own `id` of the prescription,
  so the gateway holds no state between the two. The member the `id` names
  is asked alone, by its `ehr_id` and its own `id` (`_id`), and its
  `MedicationRequest` is answered as it sent it, with the `id` read and the
  member named in `openEHR-federation-endpoint`. An `id` with no owner
  answers `404` with one text whatever the reason; an `ehr_id` two members
  claim answers `409` and raises the integrity incident; a member answering
  two prescriptions for one `id` answers `409`; a caller confined to one
  patient reads that patient's prescriptions alone. The read takes the
  `composition-` read scope over every template and is recorded under
  `Electronic-Prescriptions`. The search names each prescription by the
  same `id`, and its `fullUrl` by the read; a prescription whose parts do
  not fit an `id` is listed as its member sent it, with a `warning` naming
  the member and quoting no identifier.
- The receipt of a dispensation on the FHIR face (#1042, Regulation (EU)
  2025/327 Art 14(1)(c)): `POST {fhir-base}/MedicationDispense` takes one
  `MedicationDispense`, held to the HL7 Europe MPD 1.0.0 profile before any
  member is asked, and writes it once to the one member that owns the
  prescription it names, its body as it was sent. The gateway resolves the
  patient from `subject`, asks every member holding the patient for its
  prescriptions by that member's own `ehr_id`, and matches the
  prescription's business identifier itself, so the identifier never
  reaches a member. A prescription named by the face's
  `MedicationRequest/{id}` alone is refused `422`, since the member
  receives the body unchanged and can link it only by the identifier. A
  written dispensation answers `201` with a `Location` naming it by the
  face's id; no owner answers `404` as an unknown patient does; two members
  holding the prescription answer `409` and raise the new
  `PrescriptionIdentifierCollision` integrity incident; the owner's refusal
  answers `409` and its silence `504`, the outcome unknown and the write
  never sent again. The receipt takes the `composition-` create scope over
  every template, is recorded under `Electronic-Dispensations` and
  `Electronic-Prescriptions`, and is counted by outcome in
  `ferrofed_fhir_dispensations_total`. Serving `Electronic-Dispensations`
  now needs `fhir.receive_namespaces`, the incident report keeps the last
  20 incidents of each kind, and the capability statement declares exactly
  the interactions the face serves (#1110).
- The documentation of electronic prescriptions and dispensations on the
  FHIR face (#1045, Regulation (EU) 2025/327 Art 14(1)(b) and (c)): the
  book's prescriptions page now carries the member contract a serving
  member must meet, the duplicate-dispensation rule among it, every route's
  answers in one table, the locator ids, completeness under `partial`, the
  remaining-dispensations extension and its published definition, the
  access records and the dispensation counter, and the capability
  statement. The configuration reference, the instructions for use, the
  information sheet, the Annex II checklist and the rights of natural
  persons now name what the face serves and receives, and the information
  sheet lists the HL7 Europe MPD 1.0.0 package among its standards.
- The FHIR R4 `StructureDefinition` of the remaining-dispensations extension
  (#1053): `eehrxf` 0.0.23 carries it as
  `medication::prescription::REMAINING_DISPENSATIONS_DEFINITION`, a simple
  extension of `MedicationRequest.dispenseRequest` with a
  `valueUnsignedInt`, at most once, and the documentation site serves it at
  `https://ferrofed.eu/fhir/StructureDefinition/remaining-dispensations` and
  beside it as `remaining-dispensations.json`. It is tested against the
  FHIR R4 4.0.1 Extensibility, Defining Extensions and StructureDefinition
  pages, now vendored by `scripts/vendor/fhir-r4.sh`.
- The cybersecurity risk assessment of the Cyber Resilience Act (#1058,
  Regulation (EU) 2024/2847 Art 13(2) and (3), Annex VII point 3): a book
  page reads the threat model and the clinical safety risk file against
  each point of Annex I, Part I, with the justification where a point does
  not apply, and is reviewed at each release. CRA information to the user
  (#1059, Art 13(18), Annex II points 3 to 9): a book page gives the
  identification, the intended purpose, the foreseeable risks, the support
  end date of every release, the instructions for secure use, update and
  decommissioning, and where the SBOMs are. The technical documentation
  index and the information sheet link both.
- A decommissioning page in the book (#1063, Regulation (EU) 2024/2847
  Annex II point 8(d)): what a deployment holds and where, then the steps
  to take it out of service, from moving what a replacement needs and
  letting the audit spools deliver to keeping the access records for their
  retention period, retiring the signing keys and credentials, and removing
  the stores. CRA information to the user links it for point 8(d).
- The technical documentation indexes every point of Annex VII of
  Regulation (EU) 2024/2847 (#1065): each point names the page, file or
  release attachment that answers it, or the issue that completes it, in
  `conformance/cra-technical-documentation.tsv`, which
  `scripts/checks/technical-documentation.sh` holds to the vendored Act and
  the tree as it holds the Annex III index.
- A FHIR R4 definition behind every `https://ferrofed.eu/fhir/` URL the
  registry document in FHIR form and the access log use (#1072): the
  `consent-refusal-code`, `payload-profile` and `ehr-id-system` extensions of
  `Endpoint`, the `connection-type` and `access-log-origin` code systems, and
  a `NamingSystem` for each of the five `sid/*` identifier systems. The
  documentation site serves each at its URL and beside it as `.json`, and
  the book's new page, The registry in FHIR form, says how a FHIR client
  reads them from a host that sets the content type by extension, and why
  the archetype-local code systems, one per archetype, do not resolve. Each
  definition is tested against the FHIR R4 4.0.1 pages that govern its
  resource type, now with the CodeSystem, NamingSystem and terminologies
  pages vendored by `scripts/vendor/fhir-r4.sh`, and tied to the code that
  reads it, and a test fails on any other `ferrofed.eu` FHIR URL without a
  published definition.
- The basis of the five-year support period, under Regulation (EU)
  2024/2847 Art 13(8), in the book's regulatory status (#1088): the time a
  release is expected to stay in use between upgrades of the hospital
  platforms around it, the users' expectations, the nature of the product,
  the five-year floor, the release cadence and the optional matters. The
  technical documentation index marks Annex VII, point 4, documented.
- The operator console names a development gateway in an amber banner
  under the top bar, with the static development cross-reference when it
  is in service, and its status pill shows the gateway's version and
  replica. An operator without the operator scope sees the pill as before
  (#1096).
- `GET {base}/operator/deployment`, on the operator surface behind the
  operator scope, reports the deployment profile the gateway started with
  (`production` or `development`), whether patients resolve through the
  `[dev]` cross-reference, the product version and the replica name, so
  the operator console can warn that it reads a development gateway. It
  carries no setting value, URL, credential or patient identifier.
- The operator surface marks each stored-query version `read_only` when the
  gateway takes no `PUT` under its name (its own `eu.ferrofed.eehrxf`
  namespace, or the read-only definition files), and
  `GET {base}/operator/stored-queries/{name}` answers one stored query's
  versions by its qualified name, a `404` when none is held (#1111).
- The read of one prescription on the FHIR face answers an `ETag` and a
  `Last-Modified` (#1118, FHIR R4 `http.html` §3.1.0.2): when the member's
  `MedicationRequest` carries `meta.versionId`,
  `GET {fhir-base}/MedicationRequest/{id}` answers `ETag: W/"<versionId>"`,
  and when it carries `meta.lastUpdated`, it answers `Last-Modified` with
  that instant as an RFC 9110 HTTP-date in GMT. A prescription with no
  `versionId` or no `lastUpdated` is answered without that header, and so
  is one whose `versionId` is not a valid FHIR `id` or whose `lastUpdated`
  is not a valid FHIR `instant`, which the gateway logs by its shape alone,
  never by its value.
- The read of a received dispensation on the FHIR face (#1129, Regulation
  (EU) 2025/327 Art 14(1)(c)): `GET {fhir-base}/MedicationDispense/{id}`
  reads a dispensation by the `id` its receipt's `Location` named, at the
  member that `id` names alone, by the member's own `ehr_id` and its own
  `id`, with the read rules, the `ETag` and the `Last-Modified` of a
  prescription read. It takes the `composition-` read scope, is recorded
  under `Electronic-Dispensations`, and the capability statement declares
  it beside the `create`. A test now holds every interaction and operation
  the capability statement declares to a route that answers it.
- The version read of a dispensation and a prescription on the FHIR face
  (#1136, FHIR R4 `http.html` §3.1.0.3):
  `GET {fhir-base}/MedicationDispense/{id}/_history/{vid}` and
  `GET {fhir-base}/MedicationRequest/{id}/_history/{vid}` read as the read
  does, at the member the `id` names alone, with its scope, `ETag`,
  `Last-Modified` and access record, and answer the resource when the
  member's `meta.versionId` is `{vid}`, `404` otherwise. The `Location` a
  dispensation's receipt answers now reads back what it named, carries the
  member's version only when that version is a FHIR `id`, and the
  capability statement declares `vread` for both resources.
- Configuration revisions (#576): `config.ui = "managed"` keeps the
  configuration, less its file-only bootstrap core (`profile`, `[server]`,
  `[metrics]`, `[auth]`, `[config]` and `[dev]`), as a history of revisions
  in an embedded `redb` file or a PostgreSQL database the replicas share.
  The file load is a revision, and an operator saves, compares, applies
  through the reload path and rolls back revisions on the admin listener
  under `/admin/config`, behind the issuer's operator scope. Each revision
  is checked as `config check` checks the file and records who, when and
  why; a save on a stale base answers `409 revision-stale`; a secret is
  held only as its `_file` reference. `config.ui = "off"`, the default,
  changes nothing.
- Under `config.ui = "managed"`, the operator console manages members and
  stored queries beside the file (#577). Every member and stored-query
  version has one owner, which `GET /admin/config/resources` names: a member
  the registry document or a care services directory provides is read-only,
  and a console member, its organisation, node and endpoints together, is
  held in the revisions as `[[registry.members]]` and joined to the
  document's. `PUT /admin/config/members/{node_id}` and
  `POST /admin/config/members/{node_id}/remove` save a revision you preview
  with the diff and then apply or roll back; a change to a member the file
  provides is refused `409 resource-file-owned`, and the removal of no
  console member `404 resource-unknown`. A console member survives a
  `SIGHUP` reload, and when the file later provides the same member the
  file's runs and the console's copy is reported as shadowed.
  `PUT /admin/config/stored-queries/{name}/{version}` stores a stored-query
  version, which is never edited or removed (§12.7, N44). The console's new
  Configuration page shows the revisions, the owners and the history, with a
  confirmed form for each change. The configuration file never sets
  `registry.members`.
- Under `config.ui = "managed"`, the operator console manages each onward
  credential and each binding section (`[pixm]`, `[pdqm]`, `[xcpd]`,
  `[nl_gf]`, `[federation.localization]`, `[federation.consent]`) as one
  resource owned whole (#578): one the file sets is read-only and refused
  `409 resource-file-owned`, and the console's survive a `SIGHUP` reload.
  `PUT /admin/config/credentials/{endpoint_id}` and
  `PUT /admin/config/bindings/{section}` save a revision with the resource's
  settings and secret changes; `…/remove` saves one without it. A revision
  holds a secret only as its `_file` reference: a secret value is written to
  a new file of mode `0600` in the new `config.secrets_dir`, never logged,
  echoed or answered, and refused `409 secret-store-unavailable` without
  that directory. An applied change to a credential or a binding section
  awaits a confirmation, `POST /admin/config/revisions/{id}/confirm`, within
  the new `config.confirm_ms` (120000 ms by default); without one the
  gateway rolls it back, on every replica and across a restart. The new
  `config.approval` names resources a revision changes only once an
  operator other than its author approved it,
  `POST /admin/config/revisions/{id}/approve`. Client authentication stays
  in the file. The Configuration page shows each credential and binding
  section with its owner and its secrets by reference, and offers the
  confirmation and the approval with their deadline and who acts.
- Under `config.ui = "managed"`, any configuration revision exports as TOML
  and TOML imports as a dry-run revision (#579).
  `GET /admin/config/revisions/{id}/export` answers the revision as an
  attachment under a comment header naming it, its author and its time,
  with every secret as its `_file` reference and without the bootstrap core.
  `POST /admin/config/import?base=&reason=` takes a TOML body of at most
  1 MiB, checks it as a save, refuses a change to a resource the file
  provides `409 resource-file-owned`, and saves it unapplied with the origin
  `import`, answering its changes against the running revision; you apply it
  as any revision. The Configuration page offers each revision of its
  history as a download and an import form that shows the dry run's
  changes. The book describes the file-first and console-first ways of
  working and how to move between them.
- `[auth]` and the listeners' certificates reload without a restart
  (#636): `SIGHUP`, and the new admin listener action `POST /admin/reload`
  for a container no signal reaches, read the configuration, `[auth]` and
  the `[server.tls]` and `[metrics.tls]` certificate files again and apply
  them together or not at all. An issuer a reload adds is trusted from the
  next request on, on the client face and the admin listener alike; a
  request already being admitted finishes on the issuers it started with.
  A reload whose `[auth]` does not load, trusts no issuer of a federating
  gateway (class `auth`), or whose certificate files do not form a pair
  (class `listener-certificate`) is refused as a whole. `auth.audience`
  waits for a restart. `POST /admin/reload` needs the operator scope, is
  recorded with its operator, and answers what it applied, or `422`
  (`configuration-invalid`) naming the refusal's class.
- Every report of the operator surface names the gateway replica that
  produced it, since each replica keeps its own incidents, routing table
  and dependency states (#637). The name is the new `[server] replica`
  setting, else the `HOSTNAME` the process runs with (a Kubernetes pod's
  name), else `unnamed`; a configured name outside 1 to 64 ASCII letters,
  digits, `.`, `_` and `-` is refused at load. Each answer under
  `{base}/operator/` carries it in a `ferrofed-replica` header and each
  report in a `replica` member, and the identity-state export names
  `replica` in place of `scope`. The operator console shows on its
  members, integrity and stored-query views which replica answered; it
  reads one replica at a time, as the book's operator console page says.
- The operator console refreshes the operator's access token on its server
  when the provider issued a refresh token (RFC 6749 §6), within 60 seconds
  of its expiry or in the second half of a shorter lifetime, so a session
  lasts to its idle and absolute timeouts. A
  rotated refresh token replaces the old one, the refresh token never
  reaches the browser or a log line, and a refused or failed refresh ends
  the session and sends the operator to sign in again. The new
  `[oidc] offline_access` setting, off by default, asks for the
  `offline_access` scope with `prompt=consent` (OpenID Connect Core 1.0
  §11); `oidc.scopes` no longer admits `offline_access` itself (#643).
- The technical documentation of Regulation (EU) 2025/327: a book page that
  indexes every element of Annex III to where FerroFED documents it, with
  the forms a release is placed on the market in and the hardware it runs
  on, held by `conformance/technical-documentation.tsv` and the
  `technical-documentation` guard in CI. A version whose harmonised
  components changed since the previous release is not released until the
  clinical safety risk file and the technical documentation have a review
  row for it (Art 30(2)); the release lane's `plan` job runs the same
  check. The page also says how each release's documentation and
  declaration are kept for ten years (Art 30(3), Art 39(4)) and how a
  translation is provided within 30 days of a request (Art 37(4)).
- The register of economic operators of Regulation (EU) 2025/327 Art 35
  and Regulation (EU) 2024/2847 Art 23: the procedure on the book's
  Complaints and incidents page and in `docs/post-market.md` (what a row
  holds, who enters it and when, how long it is kept, how a market
  surveillance authority's request is answered, and why a public download
  is not a row), the register's public format in
  `docs/registers/economic-operators.tsv`, which holds no row because the
  rows are kept outside the public repository, and the
  `economic-operators` guard in CI. The guard's `--register` mode checks
  the private register's rows and refuses, across its git history, a row
  removed before its retention date or changed after it was entered.
- Each access record carries a security label per category of the data it
  reached and per origin of the data, in `meta.security` and on its
  `ehds-categories` and `origin` entities, so your Audit Record Repository
  can limit who reads which records by category and origin (Regulation
  (EU) 2025/327 Annex II 3.4). A category is labelled with its own code
  and an origin with its endpoint id in
  `https://ferrofed.eu/fhir/CodeSystem/access-log-origin` unless
  `[access_log.labels]` declares other labels, and an unclassified access
  carries every category's label. A malformed label, or one for a
  category or endpoint the gateway does not know, is refused when the
  configuration loads (#797).
- The operator can export the routing state a gateway process learned, for
  a system that replaces FerroFED (Regulation (EU) 2025/327 Annex II 2.6),
  with `POST /admin/identity-state/export` on the admin listener (#798). It
  needs a token with the issuer's `operator_scope` and is recorded with the
  operator who ran it. The answer is JSON with a `format_version`, the
  federation id and every learned `creating_system_id` mapping with its
  node and endpoint, documented on the book's "Exporting the identity
  state" page. It leaves out the resolution bindings and the `ehr_id`
  index, so it carries no patient identifier; each replica is exported on
  its own.
- The access record of a query over many patients' data (Regulation (EU)
  2025/327 Art 9(1)): when the query projects the `ehr_id`, the record names
  each `ehr_id` its delivered rows hold, at the member that answered it, and
  the patient behind it, up to `[access_log] max_named_ehrs` (50 by
  default); otherwise it carries an `ehds-population` entity stating the
  query's scope and why no `ehr_id` is named, beside the members it asked.
  The query each member receives is unchanged (#819).
- The operator console runs the operator's two admin actions on the
  gateway's admin listener (#909): the clear of a held `ehr_id` collision,
  below the integrity incidents, at one replica or at every replica in turn,
  and the stored-query drift repair, below the stored queries, through the
  first replica. The console's new `[[admin.replicas]]` table names each
  replica's `admin_url`, which its server calls with the signed-in
  operator's own access token; without it no action is offered. Each action
  asks for a confirmation and names the replica it acted on, and a refusal
  shows its status and stable error code. The admin listener must then bind
  an internal address the console's server can reach, never a public one.
- The interoperability component renders an archetype-local code (`local`
  terminology) with the text its operational template defines for it, in the
  language asked, instead of sending it under eHN Patient Summary Art 10(7)
  (#927). A template that holds the term in another language only leaves the
  code to Art 10(7), and a local code the archetype does not define is
  refused. The component also ships the eHN Patient Summary §4 preferred code
  systems as URIs (`eehrxf::terminology::ehn`), and the terminology rules
  prefer them by default.
- `ferrofed config check` names each Art 14(1) category the FHIR face
  receives, with the member its `[[fhir.receive]]` entry declares, and the
  categories no member receives: the set `GET {fhir-base}/metadata` names.
  A configuration with no `[fhir]` gets neither note (#930).
- The EU declaration of conformity of Regulation (EU) 2025/327 Art 39,
  with every item of Annex IV, written for each release by
  `scripts/release/declaration-of-conformity.sh` from the records at the tag
  and the values only the manufacturer supplies in
  `conformance/eu-declaration-of-conformity.toml`. The release lane attaches
  it as `ferrofed-vX.Y.Z-eu-declaration-of-conformity.md` once that file
  issues it, and refuses a release whose declaration would hold a blank;
  until then a release carries none and says so in a warning (#940).
- The Annex II checklist of Regulation (EU) 2025/327: a book page that
  holds every essential requirement of Annex II, by the Regulation's own
  numbering, to its status (met, partial, planned or not-applicable), the
  page and the tests that show it, and the issue for any gap. It is kept as
  `conformance/annex-ii.tsv`, and the `annex-ii` guard in CI checks each
  requirement against the vendored text and each named heading and test
  against the tree. The technical documentation cites it for Annex III,
  point 4, which stays partial while a point is unmet.
- Under `config.ui = "managed"`, a revision applied or rolled back on one
  replica takes effect on every replica that shares the revision store.
  Each replica reads the running mark every `config.poll_ms` (5000 ms by
  default, 100 to 600000) and applies a revision it does not run through
  the reload path. A replica that refuses the revision keeps its running
  configuration, logs the refusal and counts it in
  `ferrofed_config_revision_follows_total`. `GET /admin/config` names the
  revision the replica runs as `running`, whether it is the current one as
  `in_step`, and a revision it refused as `refused`. The gauge
  `ferrofed_config_revision_in_step` and the alert
  `FerroFEDConfigurationDrift` report a replica out of step.
- The book's evaluate chapter holds each right Chapter II of Regulation (EU)
  2025/327 gives a natural person against the logging and interoperability
  components, with who acts, the tests that show it and the gaps, and Annex II
  point 1.3 cites it (#961).
- The interoperability component ships NPU among the eHN Patient Summary §4
  preferred code systems, as `http://npu-terminology.org`, the URI the HL7
  Europe Laboratory Report 2.0.0 includes it by (`eehrxf::terminology::ehn::NPU`),
  and the terminology rules prefer it by default (#968). EMDN is not shipped,
  because no vendored source names its system URI.
- The access record of a routed EHR create names the EHR it left
  (Regulation (EU) 2025/327 Art 9(1)): the `ehr_id` the member's `ETag` or
  `Location` named (ITS-REST 1.1.0 EHR API), held to the `HIER_OBJECT_ID`
  form, is an `ehr` entity with the patient the identity binding holds
  behind it, and the origin's `created-ehr`; an answer that names none, no
  `ehr_id` or two that differ records `created-ehr-unread` as `unnamed` or
  `malformed` and names no value. `ehds-logging` 0.0.21 adds `CreatedEhr`
  and the origin's `created_ehr` (#983).
- The log of each semantic transformation the FHIR face applies (#985;
  eHealth Network General Guidelines Release 3.1 Art 6(3)): the access
  record of a patient summary and of a received document carries one
  `ehds-transformation` entity per transformation, naming the direction
  (`provided` or `received`), the Art 14(1) category, the context mapping,
  its template, profile and mapping files, the package a received document
  was held to, the terminology releases and code system versions in force,
  and the elements transcoded, left untranscoded (eHN PS Art 10(7)) and
  dropped, each counted at its FHIR element path or the path a declared loss
  names. No value, code value or patient identifier is written.
  `eehrxf` 0.0.18 adds `transformation::Report`, `Mapping::files` and
  `Mapping::pin`; `ehds-logging` 0.0.23 adds `transformation::Transformation`
  and `AccessRecord::transformations`.
- The configuration page of the operator console edits a console member's
  FHIR endpoints (`hl7-fhir-rest`, the exchange-format endpoints) beside its
  Query API endpoints: the member form shows each with its `endpoint_id`,
  base URL, `ehr_id` identifier system, categories, profiles and status, and
  a save adds, edits or removes one as the form shows them, merged onto the
  held endpoint of the same `endpoint_id`. A form drawn without the field
  keeps the held FHIR endpoints whole, the form that adds a member refuses a
  `node_id` the gateway already holds, a base URL that is not an absolute
  `http` or `https` URL is refused, and every refusal names the endpoint by
  its position. Before, a FHIR endpoint was kept on save and changed only
  through the admin API.
- A person's own patient summary (#991): an electronic health data access
  service (Regulation (EU) 2025/327 Art 3(1), Art 4) asks
  `{fhir-base}/Patient/$summary` with a token confined to the person and
  the scope `patient/aql-eu.ferrofed.eehrxf::*.s`, and gets the summary from
  every member that holds them. A request for another patient is `403`
  (`patient-confinement`) before the PDQm Supplier or any member is asked.
  The access record names the person as the caller and as the data
  subject. A professional's request is unchanged.
- Information a person inserts, marked as theirs (#992, Regulation (EU)
  2025/327 Art 5): a document the FHIR face receives under a person's own
  grant, a token whose `sub` names the person and whose `patient/` scopes
  are confined to them, is written with every entry naming the record's
  subject, a `PARTY_SELF` with no identifier, as its `provider` (openEHR
  RM `ENTRY.provider`). Its access record carries the
  `ehds-subject-inserted` entity. A person's document that maps to no
  entry, or to a `GENERIC_ENTRY`, is `422` with nothing sent. A
  professional's receipt is unchanged. `eehrxf` 0.0.17 adds
  `ReceivedComposition::inserted_by_subject`, and `ehds-logging` 0.0.22
  adds `AccessRecord::subject_inserted` and its BALP entity, which
  `ehds-logging` 0.0.24 replaces with `inserted` (#1003, above).

### Changed

- The access record of a semantic transformation counts each loss the
  mapping run declared at the FHIR R4 element the loss is located at, read
  from the engine's located losses, in place of the mapping, cell or output
  path the loss's text named; a loss with no FHIR element is counted with no
  path.
- The harness and the quickstart run FerroEHR 4.3.9, node and database image
  alike, pinned by digest, in place of 4.3.6 (#1012).
- The FHIRconnect mapping engine builds on the same `openehr-*` release as
  the rest of the gateway, so the server links one copy of that family in
  place of two (#1028). This moved it to `fhirconnect` 0.1.111 on
  `openehr-*` 0.0.86; the release ships the later versions the
  `openehr-*` 0.0.87 entry below names.
- A security fix ships as its own patch release on each supported line,
  the releases that share a `<major>.<minor>` version, with no feature
  changes in it (#1064, Regulation (EU) 2024/2847 Annex I Part II(2)).
  SECURITY.md, CRA information to the user, the cybersecurity risk
  assessment and the support sentence that opens each release's notes say
  so.
- The operator console reads a refused configuration file through the same
  TOML fault classification as the gateway and the registry, so a console
  refusal names its key, line and kind of fault by the same rules and never
  quotes the file.
- The operator console has a new shell. The top bar shows the gateway's
  status as a pill, a light, dark and system theme switch, and the
  operator's name with a menu that holds Sign out. The theme is light
  until the operator chooses another; the server keeps the choice in a
  second `HttpOnly` cookie, `ferrofed_viewer_theme`, which holds only the
  theme's name, and draws each page in it, so no page shows the wrong theme
  first. The navigation is grouped under Overview, Federation, Queries
  and Gateway, with an icon beside each entry and the current page marked.
  On a narrow screen it is a drawer: the keyboard opens it, Escape closes
  it, and the focus moves in and back. The sign-in is a centred card, a
  page the console does not serve and a fault are centred notices with the
  status and the way back, and the landing page is a dashboard. The footer
  is one line with the product, its version, Cadasto B.V. and a link to the
  new About page, which names the manufacturer in full as Regulation (EU)
  2025/327 Art 30(1)(g) asks, with the gateway the console reads (#1075).
- The operator console's dashboard shows the federation at a glance: the
  members active, the nodes up, the open integrity incidents, the stored
  queries and the running configuration revision with the one pending, and
  each endpoint's health and the recent incidents, each linking on. The
  members page shows health as a pill and leads to a page per member with
  its endpoints. The `creating_system_id` routing table moves to its own
  Routing page with a filter, and the integrity page filters its incidents
  between open and resolved. A replica with no name is no longer named
  "unnamed" (#1076).
- The operator console's stored queries are listed by namespace, each version a
  chip and the gateway's own namespace marked read-only, with no AQL in the
  list. A query's detail at `/stored-queries/{name}` shows its versions with a
  switch between them, when each was stored, and its AQL highlighted, and
  "Open in query console" chooses the query there by its name and version
  alone. The self-description shows what `OPTIONS {base}/` declares as cards,
  with the manufacturer it names, and the whole document behind a disclosure
  (#1077).
- The console's query console is an editor card and a results card (#1078).
  The editor has AQL and Stored query tabs, parameters as name and value
  rows, offset and fetch on one row, and federation options in which the
  declared endpoints and organisations are checkbox chips and best effort is
  a switch. The results card shows a summary strip, one chip per node with
  its status, rows and latency, the rows in pages of 25 under a header that
  stays, the raw result set on its own tab, and a refused query as the
  card's error state. The focus moves to the results after a run.
- The operator console's configuration page is redesigned: five tabs, each
  at its own path (Members, Credentials and bindings, Stored queries,
  History, Import and export), with the card of the revision saved and not
  applied above each, its changes marked `−` and `+`, and its apply, a
  running revision's confirm and roll back, a removal and a stored version
  each confirmed in a step of their own instead of a checkbox. A member's
  endpoints, a credential's grant settings and a binding section's
  settings are rows of their own fields, never one line of separated or
  escaped text, and each secret is kept, set by a value, read from a file
  or cleared. A form's outcome shows at the top of its card with the focus
  moved there, a refused field is marked where it stands, and the history
  compares any revision with the running one (#1079). The Integrity page's
  collision clear and the stored queries' drift repair confirm in the same
  step.
- The operator console page of the book shows every console screen in the
  light theme, with a link to the same screen in the dark theme: the sign-in,
  the dashboard, the members and a member's page, the routing table, the
  integrity incidents, the query console empty and with a complete, an
  incomplete and a refused answer, the stored queries and one query's
  detail, the self-description, each configuration tab and the member form,
  the About page and the page for an unknown path (#1081). The testkit takes
  them behind `FERROFED_SCREENSHOTS=1`, and a test fails when a console
  route has no screenshot.
- A security patch release cut on an older supported line no longer becomes
  the repository's latest release or moves the `latest` tag of the gateway
  and console images: only the highest version released does. The release
  checklist describes how a security patch is cut from a supported line's
  last tag on a `release/<major>.<minor>.x` maintenance branch (#1087).
- The notice for a release past its support period, in the startup banner,
  `ferrofed config check` and the log, now says to move to the latest patch
  release of a supported line, since a security fix ships as a patch release
  on each supported line (#1090).
- `ferrofed serve` logs the address each listener bound on its `listening` and
  `serving the admin listener` lines, so a listener configured on port 0 names
  the port the system gave it, and `server.listen` and `metrics.listen` may
  both name port 0 on one address, since each bind takes a port of its own.
- Hardening, the instructions for use and Upgrading state the security-fix
  policy of `SECURITY.md` (#1095): a security fix ships as its own patch
  release on each supported line, so an operator moves to the security
  patch release of the line it runs. They no longer say fixes go to the
  latest release only.
- The capability statement of the FHIR face declares the `create` on
  `Bundle` only when a member receives a category, as it lists the
  prescription resources only when a member serves them (#1110); a
  deployment with no `[[fhir.receive]]` no longer declares a `create` that
  refuses every document.
- The console's stored-query pages mark a version read-only as the gateway's
  operator surface marks it, with no copy of the gateway's reserved-namespace
  rule, and the detail of one stored query reads that name's versions with
  `GET {base}/operator/stored-queries/{name}` instead of every page of the
  listing (#1111).
- The `openehr-*` crates move to 0.0.87, with `fhirconnect` 0.1.117,
  `openehr-mapping-core` 0.1.111 and `fhir-types` 0.1.113 on the same
  release, so the server still links one copy of each family. A received
  document is mapped to a composition whose `category` the template decides:
  its assumed value, or its one admitted code, or `event` when it leaves the
  category open. A template that admits several categories and assumes none
  now refuses the mapping, naming `<root>/category|code`, where `event` was
  taken before.
- The Complaints and incidents and Regulatory status pages, `SECURITY.md`
  and the post-market procedure name Commission Delegated Regulation (EU)
  2026/881: it sets when the CSIRT that first receives a CRA Art 14
  notification may delay passing it on, binds that CSIRT and adds no duty
  for the manufacturer, and a notification now says when a security update
  or user guidance is expected, the one ground that starts with the
  manufacturer (its Art 3(a)). Delegated Regulation (EU) 2025/1535 is read
  and does not reach FerroFED (#844, #852).
- `SECURITY.md`, `CONTRIBUTING.md`, the README and the book keep what is
  particular to FerroFED and link the FerroHEALTH family book at
  <https://ferrohealth.eu/docs/> for what every product shares: the
  manufacturer, the vulnerability-handling policy, the Cyber Resilience Act
  from the manufacturer's side, licensing and trademarks, and how the
  products fit together (#848).
- A double-quoted query parameter on `GET {base}/v1/query/aql` and
  `GET {base}/v1/query/{qualified_query_name}[/{version}]` binds the string
  it names, quotes removed and escapes decoded, as the same string does in a
  `POST` body: `name="123"` binds `123`, where the quotes were kept before.
  A patient identifier bound that way, quoted, inside text, namespace
  qualified or spelled in JSON escapes, is still refused
  `400 identifier-elsewhere` before any member is asked, and one bound
  quoted as the subject now resolves as the bare identifier does.
- `columns[].path` of a federated result set keeps the AQL variable as the
  client wrote it, `c/uid/value` and `p/id`, as the Federation Tier §9.4
  example renders it, where it was `/uid/value` and `/id` before. Two
  variables of one class, such as `c1/uid/value` and `c2/uid/value`, now
  render apart. A client that matched the stripped form reads the variable
  first.
- The access record of a write names the version it left at the member
  (Regulation (EU) 2025/327 Annex II 3.2): a routed create, update or delete
  records the `OBJECT_VERSION_ID` the node's `ETag` or `Location` names as
  the origin's `written-version`, and the `version-uid` of the composition
  it wrote; an answer that names no version, or no `OBJECT_VERSION_ID`,
  records `written-version-unread` as `unnamed` or `malformed` and names no
  value. A received document's record names its version the same way. The
  origin `status` of a routed read or write, and of a received document, is
  now the Federation Tier §11.1 status (`active`, `node-error`, `time-out`,
  `offline`, `consent-denied`) a query's origins already carry, in place of
  the HTTP status or `failed` (#946).
- The clinical safety risk file rates hazard L-07 controlled for the access records the gateway writes, which are reviewed with ITI-81 at the Audit Record Repository and each state their retention period; keeping them for that period stays with the repository (#962).
- `ferrofed_node_requests_total` reads the `outcome` of a routed request and
  of an ask-all probe with the rule the access record names a routed
  request's origin by (Federation Tier §11.1, N16): a node's `4xx` answer is
  now `node-error` where it was `active`, so the counter and the record never
  read one answer apart, and a new `class` label, `member` or
  `client-error`, tells a member's failure from a client's mistake it
  answered correctly. A probed member's `404`, which says it holds no such
  EHR, stays `active`, and a request answered with a `DPoP` nonce challenge
  no proof could answer is now counted as `node-error`. The dashboard's
  member request panel splits by `class` (#982).

### Fixed

- The patient summary document holds no `OperationOutcome`: a mapping run
  that declares a loss reports it in the transformation record of the
  access, and the document `Bundle` carries only the summary's content, as
  the HL7 Europe Patient Summary `Bundle` profile slices it (#1008).
- A reload names a changed `server.public_url`, `server.trusted_proxies`,
  `server.forwarded_header`, `server.replica` or `signing.rotation_overlap_s`
  under the settings that take effect only on a restart; it kept their start
  values without saying so. A test changes every key of a maximal
  configuration in turn and holds each to being applied by a reload or named
  as needing a restart (#1017).
- The configuration page's member form writes a semicolon inside an
  endpoint as `%3B` and reads it back once, so a member whose endpoint URL,
  identifier system or profile holds a `;` saves back through the edit
  form. Before, the form split such an endpoint at the semicolon and
  refused it.
- The operator console answers `408 Request Timeout` to a request body not
  read whole within 30 seconds, and `400` to one that breaks off, before any
  session check or server function runs.
- The operator console answers `413 Payload Too Large` to a request body
  sent chunked that grows past its limit, as it answers one whose
  `Content-Length` declares it. Before, it answered `500`.
- A stored configuration revision that does not read is logged by its
  position, key and kind of fault, never by a line of the revision the TOML
  or JSON reader quotes, so a credential in a URL query never reaches the
  log. A configuration parse refusal now uses the same fault words as a
  registry document refusal: `unknown field`, `duplicate key`, `invalid
  type, expected …`.
- The operator console draws its navigation and the Sign out button only for a
  signed-in operator, decided by the server from the session; a visitor's
  landing page shows what the console is and the sign-in. When the session
  store cannot be read, a page answers 500 and says so, offering neither
  (#1036).
- The documentation of the receive check in `eehrxf` 0.0.23 says what the
  code does (#1054): the `required` terminology bindings are held by
  `ReceivedDocument::check_bindings` and `terminology::bindings::check_required`,
  whose results convert into the check's findings, and `Category::document_type`
  and `Category::profiles` give `None` for a prescription or a dispensation in
  every build, because HL7 Europe MPD 1.0.0 has no document profile.
- A stored-query row the embedded or PostgreSQL store cannot read is
  reported by the field and the kind of fault, never by the reader's error,
  which can quote the held text, so a damaged store's log line carries no
  part of a stored definition.
- The document list on the FHIR face no longer answers `200` when the
  cross-reference could not answer for a member (#1104): under
  all-or-nothing it fails with `424` naming that member as `not-resolved`,
  as the patient summary and the prescription search do, and under
  `partial` it names the member in an `outcome` entry beside the documents
  it lists.
- A patient the cross-reference could not answer for at any member is no
  longer answered on the FHIR face as a patient no member holds (#1107).
  Under `partial`, the patient summary names each such member as
  `not-resolved` in every section, where it answered `404`, and the
  document list names each one in an `outcome` entry, where it answered an
  empty `searchset`. A patient summary under `partial` now also names a
  single member the cross-reference could not answer for. A patient every
  member answers it does not hold still gets the empty answer.
- The operator console's muted text in the light theme is a shade darker
  (`#58687d`), so it meets the 4.5:1 contrast of WCAG 2.2 SC 1.4.3 on every
  ground it can sit on, the inset panels and the accent tint included. A test
  computes the contrast of every text and ground pair of both themes from the
  stylesheet and fails a pair below 4.5:1, or a control edge below 3:1.
- The operator console's page of one member and the detail of a stored
  query no longer fail now and then with a cut-off answer. Each page's
  title read the route's parameters after the server had cleaned them up,
  which panicked the render; the title now falls back to the name the page
  rendered with. A test fetches the stored query's detail fifty times in a
  row and checks its title each time.
- The four FerroEHR nodes of the local `compose.yaml` stack, and every
  FerroEHR container the test harness starts, run with
  `FERROEHR__USAGE_REPORT__ENABLED=false` (#1128). A container does not
  inherit the CI runner's environment, so FerroEHR's own CI detection never
  fired, and each test run or local stack reported itself to FerroPULSE as an
  installation.
- A reload applies a change to `[federation.consent]`, and to
  `[federation.localization]` beside the node selection and budgets the
  process started with; it kept both at their start values without saying
  so, and an applied configuration revision now changes them too (#578).
- A write whose access record the audit spool has no room for is refused
  `503 access-unrecorded` before any member is asked. Before, a routed
  ITS-REST write or a document received on the FHIR face reached its
  member first and was then answered `503`, so a client that sent it again
  wrote it twice. The gateway now reserves the record's place in the spool
  before the write, and two writes never take its last place. Once a member
  was asked, its answer stands; a reserved record that still cannot be
  stored raises the security event `write-unrecorded` and the alert
  `FerroFEDWriteUnrecorded`. `AccessSink` in `ehds-logging` 0.0.17 gains
  `reserve`, and `ihe-iti` 0.0.35 adds `Spool::reserve` and
  `AuditRecorder::reserve`.
- Under `config.ui = "managed"`, a changed configuration file that a
  `SIGHUP` reload records as a revision is marked `pending-restart`, with
  the keys a restart applies, when the reload leaves settings for the next
  start. Before, it was marked `applied`, so `GET /admin/config` claimed a
  configuration ran that only partly did, while a revision saved and
  applied through the API was marked `pending-restart`.
- The interoperability component no longer writes an openEHR terminology id
  such as `local` as a FHIR `Coding.system` (#967). An archetype-local code
  is written under the code system of the archetype it is defined in,
  `https://ferrofed.eu/fhir/CodeSystem/archetype-local/` followed by the
  archetype id; `SNOMED-CT`, `LOINC` and `ICD10` are written as their FHIR
  URIs; and a code of a terminology with no known URI, or a local code with
  no template context, is written as no coding: its code is in the
  narrative, the result names it as unmapped, and the item follows eHN
  Patient Summary Art 10(7).
- Under `config.ui = "managed"`, a `SIGHUP` reload whose changed file the
  reload refuses records the file's revision and a `config-revision-refused`
  record naming the class of the refusal, and leaves the running mark where
  it was, so no other replica on the store follows it. The revision is
  marked running only once the reload applied it, and the same file read
  again is tried again. A start likewise marks the file's revision only once
  the gateway built what it runs, and one that fails leaves the mark and
  writes the same record (#972).
- The operator console's integrity view lists the collisions an operator
  cleared recently, newest first, as the gateway reports them beside its
  incidents. Before, the view left them out. A replica's refusal of an
  admin action, a body the console cannot read, and an admin listener it
  cannot reach are each logged as that action, with the replica, the
  status and the code, and never the `ehr_id` or the stored query the
  operator named. Before, each was logged as a view the gateway refused.
- The interoperability component writes an openEHR terminology version as a
  FHIR `Coding.version` only in the form the code system's versions take
  (#977). A SNOMED CT version passes only as a URI naming an edition, such
  as `http://snomed.info/sct/900000000000207008/version/20260201`, and an
  HL7 Administrative Gender, RoleClass, RoleCode or ObservationCategoryCodes
  version only as dot-separated digits; any other is left out. Every other
  system's version passes as recorded, because no vendored source shows its
  form.
- The operator console logs a query the gateway refuses as a refused query,
  and a configuration action a replica refuses as a refused configuration
  action, naming the action (a member save, an apply, a rollback, a
  stored-query store and the rest) and the replica, with no identifier,
  AQL text or token. Before, a refused query was logged as a refused view
  and a configuration action as an admin action.

### Security

- A registry error that names an endpoint's base URL, such as two endpoints
  sharing one, shows its scheme, host, port and path alone, never a user
  name, password, query or fragment. Since the operator console shows a
  `configuration-invalid` message, it never shows a credential carried in
  a URL.
- A registry document the TOML reader refuses is reported by its position,
  key and kind of fault, never by the line the reader quotes, so a
  `configuration-invalid` message on the operator console never shows text
  of the document, such as a credential in a URL query.
- The operator console's site build installs the Tailwind CSS CLI and
  wasm-opt at pinned releases, checks each against its pinned SHA-256
  before it runs, and refuses to build unless the tools on PATH match those
  digests, so cargo-leptos never downloads or runs an unchecked binary in
  the attested release lane (#1051).

## [0.0.10] - 2026-10-15

Supported until 2031-10-15, 5 years from this release (Regulation (EU) 2024/2847 Art 13(8), (19)). A security fix ships in the latest release, as [SECURITY.md](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions) says.

### Upgrade notes

- A gateway with a registry outside the development profile must set
  `[audit] destination` (#623, #693), and it must be `"repository"` with an
  `[audit.repository]` table: `"log"` is refused there, because it records no
  caller and no patient and so cannot hold the access log Regulation (EU)
  2025/327 Annex II 3.2 asks for. Point the gateway at your Audit Record
  Repository, with `spool_dir` on durable storage, before you upgrade.
- Set `server.shutdown_timeout_ms` to at least `server.request_timeout_ms`,
  or remove it so it takes the request timeout (#625). `config check` and
  `serve` now refuse a shorter drain, and the v0.0.9 example configuration
  set 10000 beside a request timeout of 30000. Then give the container
  longer to stop than `server.drain_delay_ms` plus the drain: the examples
  use `stop_grace_period: 40s` in Compose and
  `terminationGracePeriodSeconds: 45` in Kubernetes.
- If you run the Kubernetes example, move from `deployment.yaml` to
  `statefulset.yaml` (#628). Replace the `encrypted` storage class with
  your own. Wait until the dependency report, `GET
  {base}/operator/dependencies` with an operator token (#751), reads `up` for
  the audit repository, so the spool on the old `emptyDir` is drained, then
  delete the Deployment and apply the StatefulSet.
- The listeners need no change: unset, `[server.tls]` and `[metrics.tls]`
  leave them on plain HTTP as before (#632). Once you set either, remove it
  before rolling back to an earlier release, which refuses it as an unknown
  key.
- If `[metrics] listen` names an address other than a loopback one outside
  `profile = "development"`, set `[metrics] scrape_token_file` to a file
  holding a scrape token and give your Prometheus the same token with
  `authorization.credentials_file`, or set `[metrics.tls] client_ca_file`
  (#635). Without either, `serve` and `config check` refuse the
  configuration. The Kubernetes example reads the token from the
  `metrics-scrape-token` key of the `ferrofed-secrets` Secret.
- To run the admin listener's stored-query distribution outside the
  development profile, from the gateway's host or another, set
  `operator_scope` on the `[[auth.issuer]]` your operators use and send a
  token carrying it (#629, #635). Without one it answers
  `401 unauthenticated`, from a loopback peer as from any other, where a
  loopback peer was served before.
- If a backend client reads patient data through the gateway with a
  `system/` scope or a token whose `sub` is its `client_id`, set
  `client_tokens_act_for_professional = true` on its `[[auth.issuer]]` and
  have its tokens name the professional it acts for in
  `extensions.ihe_iua.national_provider_identifier` (#661). Until both
  hold, such a request is answered `401 natural-person-required`. Then set
  `[auth.issuer.assurance]` on each issuer whose tokens reach patient data;
  without it no assurance level is required, and `config check` notes the
  issuer.
- A document received on the FHIR face whose codings carry no `display` is
  now refused with a `422` unless `[eehrxf.terminology]` names a terminology
  server to look the displays up (#687). Senders that omit displays need the
  server configured.
- Read the dependency report at `GET {base}/operator/dependencies` with a
  token that carries your issuer's `operator_scope` (#751). `GET
  {base}/health/dependencies` is no longer served and answers `404`, so move
  every monitor that read it, and give it an operator token. `ferrofed report`
  reads the report with `--operator-token-file`. A proxy rule that kept
  `/health/dependencies` inside can go; the health family left open,
  `{base}/health` and `{base}/health/readiness`, names no member.
- Move any stored-query definition you hold in the namespace
  `eu.ferrofed.eehrxf`, or a namespace under it, to a namespace of your own
  before you upgrade (#776). The gateway now reserves that namespace for its
  own read-only queries: a `redb` or PostgreSQL store, or a definition file,
  holding one there refuses the start, and `config check` refuses such a
  file.
- If you wrote an `[access_log]` map on a development build, rewrite its
  categories with the codes of HL7 Europe's
  `EEHRxFDocumentPriorityCategoryCS` (#824), which are case-sensitive:
  `patient-summary` becomes `Patient-Summaries`, `electronic-prescription`
  `Electronic-Prescriptions`, `electronic-dispensation`
  `Electronic-Dispensations`, `medical-imaging` `Medical-Imaging`,
  `medical-test-result` `Laboratory-Reports` and `discharge-report`
  `Discharge-Reports`, in `[access_log.templates]`,
  `[access_log.archetypes]` and the keys of
  `[access_log.retention.categories]`. Declare each national category in
  `national_categories` as `<system>|<code>`, the absolute URI of your
  Member State's code system (or one you control) and its code, and name it
  the same way in the map and the retention table. The old spellings and a
  national code without a system are refused when the configuration
  loads. A search of your Audit Record Repository for a category matches
  the `ehds-category` detail `<system>|<code>`, never the bare code.
- The registry document refuses an `[[organisation]]` that manages no
  `[[endpoint]]` (§8.1: "an Organization manages 1..* Endpoints"), so
  `config check` and the gateway's start fail on it, naming the
  organisation (#874). Give the organisation an endpoint it manages, or
  remove it. A registry read from an mCSD directory or in the FHIR form
  keeps such an organisation, and selecting it still answers `404` with
  `no-destination`.

### Added

- The identity page of the book says how each member's `ehr_id`s reach the
  PIX Manager: the PIXm ITI-104 feed or the PMIR ITI-93 feed from each
  member, the bulk load of the EHRs that already exist, why the gateway
  feeds no Manager itself, and what to do when the hospital MPI does not
  answer PIXm (#622). It names SanteMPI 2.5.12 as a PIX Manager verified
  against two CDRs, with its setup and its limits, and the products checked
  that do not answer as the gateway needs.
- The end-to-end lane resolves a patient through SanteMPI, a deployable PIX
  Manager each member feeds over ITI-93, and gets a federated query answered
  by both members (#622).
- The access log: every federated query, stored-query execution, routed
  read and routed write that reaches patient data at a node is recorded
  with the verified caller, as Regulation (EU) 2025/327 Annex II 3.2 asks
  of an EHR system's logging component and the GDPR asks of an access
  trail (#623). Each record is a BALP `AuditEvent` sent through the
  `[audit]` spool to the Audit Record Repository. It names the provider,
  the person and their purposes of use, the patient and each `ehr_id`, the
  time, each endpoint asked with its node and outcome, and the categories
  of the data. The categories come from a map you declare in the new
  `[access_log]` table, from template and archetype ids to the Art 14(1)
  categories, national categories or `none`. An access the map cannot
  classify exactly is recorded `unclassified`, with its ids as evidence,
  and is never refused. The gateway stores the record before the answer
  leaves. An access whose record cannot be stored answers
  `503 access-unrecorded` with none of the data. A query run from the
  operator console is recorded with the operator as the caller. The book's
  audit page describes the record and gives an Annex II 3.2 checklist.
- `crates/ehds-logging` 0.0.1, the European logging component as a library
  (#623): the Annex II 3.2 access record, the Art 14(1) categories, the
  category map and its classification, the sink trait, and, behind
  `balp`, the record written as a BALP `AuditEvent` through `ihe-iti`. It
  depends on nothing in FerroFED and on no interoperability component.
- The book has a production guide that walks a deployment from nothing to a
  first federated query over two CDRs (#624): what to have in place, a
  checklist each CDR meets before admission, verifying and installing the
  release, the registry, an issuer recipe for Keycloak 26.2 or later that
  issues the RFC 9068 tokens the gateway accepts with the SMART on openEHR
  scopes and the purpose of use, the onward credentials, the PIX Manager,
  the access log, TLS, the checks before the start, admission, the first
  query and the operations that follow. The Keycloak recipe was verified
  against Keycloak 26.8.0.
- `deploy/nginx/ferrofed.conf` is a reverse proxy for the gateway: TLS on the
  public address, the base path, the JWK Set and PMIR feed routes passed, the
  health family kept inside, and an access log that names the gateway's
  request id and never a query string (#624).
  `scripts/checks/production-guide.sh` runs `config check` over every TOML
  block of the guide, then serves that configuration behind the shipped
  file in nginx and checks each route; CI runs it in the release compose job.
- `[signing] next_key_file`, a signing key the JWK Set at
  `{base}/.well-known/jwks.json` publishes ahead of a rotation and that never
  signs, so every replica publishes a new key before any replica signs with
  it (#627). `next_key_algorithm` names the algorithm the next key is meant
  for, and `config check` refuses a next key on another curve, naming
  `signing.next_key_file`, or one that is already the current or previous
  key. The book gives the three-step rotation for any number of replicas
  and the node cache time it assumes.
- The metrics now show what the gateway's own clients see (#629).
  `ferrofed_http_requests_total` counts every answered request by method,
  route template and status class, and the OpenTelemetry HTTP server
  metrics `http_server_request_duration_seconds` and
  `http_server_active_requests` time them and count those in flight. The
  cross-reference resolver's calls are counted by outcome in
  `ferrofed_resolver_requests_total` and timed, and the localizer and
  demographics calls are timed beside their existing counters.
  `ferrofed_security_events_total` counts every caller refused at client
  authentication by its reason, and every identifier-hygiene event of the
  `ferrofed::security` log, the outbound-gate stops among them, by kind.
  Every label comes from a closed set: a route is a template, never a path,
  and nothing a request sends reaches a label.
- Each release attaches a Grafana dashboard, `ferrofed-dashboard.json`, and
  a Prometheus rule file, `ferrofed-alerts.yaml`, from
  `deploy/observability/`, held to the exported metrics by
  `scripts/checks/observability.sh` and `promtool check rules` in CI. The
  Kubernetes example serves the admin listener on port 9464 of each pod,
  annotates the pods for scraping, and opens that port to the monitoring
  namespace's Prometheus alone in a new `networkpolicy.yaml`.
- Upgrade notes, a configuration compatibility policy, and a versioned
  stored-query store (#630). Each release's changelog opens with "Upgrade
  notes", saying what an operator must change: `changelog.d/` takes an
  `upgrade` fragment, and CI runs `config check` from the new build over the
  example configuration the last release attached, and fails when it is
  refused and no upgrade note is pending. A renamed key is read under its
  old name for at least one release, and `config check` and the start-up
  log warn about it, naming the release that will refuse it; setting the
  old and the new key together is refused. The `redb` and PostgreSQL
  stored-query stores record a schema version (`ferrofed_schema` and
  `ferrofed.schema_version`), migrate forward in order when the gateway
  starts, and refuse to start over a store a newer FerroFED migrated, saying
  so. The book has an Upgrading page, with the notes for v0.0.8 and v0.0.9
  and the compatibility policy, and a Rollback page.
- The gateway bounds the load it takes on and passes on (#631). Past
  `server.max_concurrent_requests` (512 by default) a request is answered
  `503 overloaded` with `Retry-After` (`server.overload_retry_after_s`)
  before it reaches client authentication or a member; the health family is
  never refused. The optional `[server.caller_rate]` gives each verified
  caller, by issuer and `client_id` and never by a forwarded address, a
  bucket of `burst` requests refilled at `requests_per_second`; past it the
  answer is `429 rate-limited` with `Retry-After`. At most
  `federation.max_in_flight_per_node` requests (64 by default) go to one
  member endpoint at once; one past it waits for a slot until its per-node
  deadline and is then `time-out` with nothing sent (§11.5, N38), so one
  slow member holds no other member's requests. A zero in any of them is
  refused by `config check`, and `ferrofed_overload_refusals_total` counts
  every refusal by its limit. The errors page lists `overloaded` and
  `rate-limited`.
- TLS on the gateway's listeners (#632). `[server.tls]` serves HTTPS with a
  certificate chain and key from `certificate_file` and `key_file`, and
  `[metrics.tls]` does the same for the admin listener. An optional
  `client_ca_file` requires every client to present a certificate that CA
  signed, so only the proxy in front of the gateway is admitted. The
  listener negotiates TLS 1.3 or TLS 1.2 and nothing older, prefers TLS 1.3,
  and offers TLS 1.2 only with the four suites RFC 9325 §4.2 recommends
  (BCP 195). `SIGHUP` reads the files again, so a renewed certificate is
  presented from the next handshake with no restart; files that do not form
  a valid pair are refused and the running certificate stays. `ferrofed
  healthcheck` follows `[server.tls]`, accepting exactly the certificate the
  file holds and presenting `healthcheck_identity_file` where a client
  certificate is required, and says when the file was replaced without a
  `SIGHUP`. `config check` reads every file and refuses a missing, unreadable
  or mismatched one naming its key, never its content. Unset, the listeners
  speak plain HTTP as before, for a proxy that terminates TLS; the
  configuration page says which mode to use when. `url_scheme` on the HTTP
  server metrics reads `https` on a TLS listener.
- The book's Evaluate part has a data-protection page (#633): every store
  and channel that carries personal data, with its retention and its
  protection; the controller and processor roles of a hospital and a
  regional deployment; DPIA inputs and a template for the GDPR Art 30
  record; what the gateway offers an entity under NIS2; and what the
  documents claim about Regulation (EU) 2017/745, with what counsel must
  confirm.
- The book has a threat model and a hardening guide (#634). The threat
  model names the assets, the trust boundaries, the attacker positions and
  each mitigation with the setting or test behind it, and lists the risks a
  deployment carries with their issues. The hardening guide is a checklist
  for network placement, TLS, issuers, secrets, key rotation, the admin
  listener, the audit spool, resource limits, logs and the console.
- A troubleshooting page in the book leads from what an operator sees (a
  status and error code, a log line under its target, a metric, a refusal
  at start) to the cause, how to confirm it and the setting that fixes it,
  with a row for every error code the gateway answers, which a test holds
  to the gateway's own table (#640).
- The client contract and the replicas section say how a follow-up write
  behaves behind several gateway replicas (#641): each replica holds its
  own resolution bindings, so a write that names no node is routed by the
  replica that resolved the patient and refused `400` `target-required` by
  the others. A client sends `openEHR-federation-endpoint` on every write,
  or the operator keeps each client on one replica with balancer affinity.
- The book's Regulatory status page names the two implementing acts adopted
  under the EHDS Regulation, Commission Implementing Regulations (EU)
  2026/2083 on MyHealth@EU and (EU) 2026/2099 on cross-border identification
  and authentication, with their dates (#652). Both are vendored beside the
  Regulation, with Commission Recommendation (EU) 2019/243 on the exchange
  format. A weekly check reads EUR-Lex and the Commission's "Have your say"
  register and files an issue when an act the harmonised components wait for
  is adopted (#653), and the weekly pin-freshness read now reports a newer
  stable tag of every container image the test harness starts (#678).
- An access to patient data in the vital interests of the data subject is
  marked in its access record (Regulation (EU) 2025/327 Art 11(5)): each
  `[[access_log.emergency_purpose]]` names a purpose of use, such as the
  HL7 v3 `ActReason` code `BTG`, and a record whose verified token declares
  one carries the entity `ehds-emergency-access`, with a description in
  words and the purposes that marked it. The mark is read from the token's
  purposes alone and never inferred. It changes no dispatch and no answer:
  the purpose reaches each node in the conveyance, the node decides, and
  a node's refusal stands. The gateway writes one log line under its
  request id for each marked access, and `config check` notes an access
  log that names no emergency purpose. The book's audit and authentication
  pages name the codes a deployment maps (#659).
- `crates/ehds-logging` 0.0.9: the `emergency` module, `EmergencyPurposes`
  and `Emergency`, the record's `emergency`, and its BALP entity (#659).
- The authentication assurance of every caller that reaches patient data
  (#661, Regulation (EU) 2025/327 Annex II 3.1). Per issuer,
  `[auth.issuer.assurance]` names the claim that carries the assurance,
  `acr` by default (RFC 9068 §2.2.1), the values that stand for the levels
  `low`, `substantial` and `high` of Regulation (EU) No 910/2014 Art 8(2),
  and the least level a query, an EHR request or a DEMOGRAPHIC request
  needs. A token below it, without the claim, or with a value listed at no
  level is answered `401 authentication-assurance-insufficient` with the
  RFC 9470 §3 challenge `insufficient_user_authentication`; no node is
  asked and no access record is written. The level is read from the token
  or the introspection answer, and `config check` notes each issuer that
  declares none. Each node is told who acts (`acting`), the level reached
  (`assurance_level`) and the professional's IHE IUA `subject_name` and
  `national_provider_identifier` in the caller's
  `openEHR-federation-client` token, and in nothing else the gateway sends.
- Every request to a node now tells it the professional's roles, from the IHE IUA
  `subject_role`, and the agency that issued the professional's identifier, from the
  claim an issuer's new `professional_issuing_authority` names, beside the
  identifier and the provider's identifier it already carried, so a node can
  identify the professional and provider of every registration (Regulation (EU)
  2025/327 Art 13(4), Implementing Regulation (EU) 2026/2099 Annex Table 1). The
  outbound gate reads the new claims as it reads every caller claim (#662).
- The patient summary's `Patient` carries the header the demographics
  binding holds (eHN PS A.1.1, A.1.2, #663): the names, birth date,
  administrative gender, addresses and phone numbers and email addresses the
  PDQm Supplier of `[pdqm]` gives for the identifier the summary was asked
  by, asked before any member and never sent to one (N33). An element the
  Supplier does not hold is left out, and a missing birth date carries a
  `data-absent-reason` of `unknown`. A patient the Supplier does not know is
  a `404`, several patients under one identifier a `422`
  `multiple-matches`, a patient with no name a `422` `required`, and a
  Supplier that does not answer a `502` or `504`; none of them asks a member.
- The running system names its manufacturer, Cadasto B.V., with its postal
  address and its single point of contact, as Regulation (EU) 2025/327 Art
  30(1)(g) asks (#669): `GET {base}/` and `OPTIONS {base}/` carry a
  `manufacturer` object (`name`, `postal_address`, `email`, `website`), the
  startup banner and `ferrofed --version` print it, as does
  `ferrofed-viewer --version`, and every page of the operator console names
  it in its footer. Both container images carry it in the
  `org.opencontainers.image.vendor` and `.authors` labels and a new
  `eu.ferrofed.manufacturer` label. The details are written once, and a test
  holds every surface and `LICENSE` to them.
- The information sheet and the instructions for use of Regulation (EU)
  2025/327 (#670). The book's new "Information sheet" page gives every item
  of Art 38(2): the manufacturer, the name, version and date of release, the
  intended purpose, the six Art 14(1) categories and the standards with
  their versions; `scripts/checks/versions.sh` holds its standards and
  version to `docs/VERSIONS.md` and its date to `CHANGELOG.md`. The new
  "Instructions for use" page covers installation, the configuration of a
  deployment in the EU (`[federation.consent] disclose = false`, the
  assurance level per issuer, the access records, the category map), the
  sizing of the budgets and limits, how a client reads an answer safely,
  maintenance and its frequency, the limitations Art 28(b) requires, and
  accessible formats. The release checklist updates both at each cut.
- The manufacturer's post-market procedures under Regulation (EU) 2025/327
  (#672): a complaint channel at info@cadasto.com, a register of complaints
  and a register of non-conforming versions under `docs/registers/`,
  corrective action, recall and withdrawal with the notices to authorities
  and users, and the serious-incident report to the market surveillance
  authorities "not later than three days" after awareness (Art 44(7)),
  separate from any NIS 2 notification. The book's new "Complaints and
  incidents" page is the public account and `docs/post-market.md` the
  working checklist; `SECURITY.md`, `SUPPORT.md` and the issue chooser point
  to them.
- A clinical safety risk file for the two harmonised software components of
  Regulation (EU) 2025/327, Annex II, point 1.1 (#673). The book's new
  "Clinical safety risk file" page follows the risk management process of
  EN ISO 14971:2019 as a method, with no claim of conformity: severity and
  occurrence scales, an acceptability rule, and a hazard log for the logging
  component, the interoperability component and the hazards they share.
  Each hazard names its control and the test or code that implements it, or
  the issue that plans it; the silent node, duplicate entries across nodes,
  an unmapped code and a demographic mismatch are among them. The release
  checklist reviews the file whenever a release changes a component.
- The IHE FHIR services take the OAuth 2.0 client-credentials grant (RFC 6749
  §4.4; IHE IUA ITI-71) in `[pixm.manager.credentials.oauth2]`,
  `[pdqm.credentials.oauth2]`, `[pmir.credentials.oauth2]` and
  `[registry.mcsd.credentials.oauth2]` (#677). The gateway authenticates with
  a client secret, in the Basic scheme (`client_auth = "client_secret_basic"`,
  as ITI-71 prescribes) or the request body (`"client_secret_post"`), read
  inline or from `client_secret_file`, or with a client assertion the
  `[signing]` key signs (`"private_key_jwt"`), and asks for the `scope` the
  service's authorization server defines. The token is cached until 30
  seconds before it expires and incorporated in each request as a bearer
  token (ITI-72); a `401` gets one more request with a fresh token. `config
  check` names every key it refuses: token exchange, a `DPoP` key, a
  certificate binding or TLS client authentication in an identity service's
  grant, a secret beside `private_key_jwt`, a scope outside RFC 6749 §3.3,
  and a token endpoint that is not `https` outside the development profile.
  A node's `oauth2` grant refuses a client secret. The `ihe-iti` PIXm,
  PDQm, mCSD and PMIR clients take an `authorizer::Authorizer` for the
  headers of each request.
- `crates/eehrxf` 0.0.1, the European interoperability component of
  Regulation (EU) 2025/327 as a library (#684). Its dataset model reads the
  Xt-EHR *EHDS Logical Information Models* package as data: every logical
  model and element path, with what a producer and a consumer must do with
  each element. One feature per Art 14(1) category names the dataset of that
  category. Behind `fhir-r4`, it compiles FHIRconnect mappings against an
  operational template and runs them in process over a canonical-JSON
  composition, through FerroBRIDGE's `fhirconnect` 0.1.108, answering a
  FHIR R4 Bundle with a `Provenance`. It depends on nothing in FerroFED and
  on no logging component. An architecture test fails when the two
  components reach each other or the application, when a crate other than
  the server links both, or when the engine reaches either. `fhir-types`
  moves to 0.1.108, the release `fhirconnect` builds on. `cargo deny` now
  checks every feature, so a dependency behind a feature no crate enables
  yet is checked too.
- A member's own exchange-format endpoint, the `native` mode (#686): a
  second `[[endpoint]]` of a node with `connection_type = "hl7-fhir-rest"`,
  its `categories`, `profiles` and the `ehr_id_system` the member's
  `ehr_id` is written in. For a patient summary, the gateway leaves such a
  member out of its section queries and asks its endpoint
  `Patient/$summary` by `identifier=<ehr_id_system>|<ehr_id>` and
  `profile`, through the outbound identifier-hygiene gate, with the onward
  credential and the signed caller identity of the member's Query API
  endpoint, so no patient identifier reaches it (N33). The member's
  document comes back byte for byte, and every section of the gateway's
  document names the member as serving its own summary. A member whose
  endpoint does not answer makes the summary incomplete, `424` or `504`
  under all-or-nothing, and is named in every section under `partial`. The
  endpoint is absent from `OPTIONS {base}/`, and a directive or a targeting
  header naming it is `400`. `config check` refuses a native endpoint with
  no `ehr_id_system`, no category or no profile, a category code that is no
  Art 14(1) priority category, and a patient summary endpoint that does not
  declare the EPS composition profile.
- The interoperability component carries the terminology rules of the
  exchange format (#687): a code in a preferred system passes as recorded,
  an openEHR `TERM_MAPPING` of match `=` adds a coding after the original, a
  broader, narrower or unknown mapping adds no coding and is written into
  the narrative, and a code in no preferred system follows Art 10(7) of the
  eHealth Network Guidelines on Patient Summary.
- An optional FHIR terminology server, `[eehrxf.terminology]`, is asked for
  `$translate` (only an `equivalent` or `equal` match becomes a coding),
  `$lookup` and `$validate-code`. A refusal or a time-out is an error that
  names the operation and the server's status.
- A received document is held to the `required` bindings of its `Bundle`
  and `Composition` profiles, against the value set expansions its package
  carries; a binding no expansion and no terminology server decides is
  listed as not evaluated. A received coding with no display is given one by
  `$lookup`, and refused when no terminology server is configured.
- The access record of a query answered by several endpoints now carries
  the categories of each origin (#694), as Regulation (EU) 2025/327 Annex II
  3.2(c) and (e) ask. Each endpoint's categories are classified from the
  rows it answered with, before the merge, under the same rules as the
  access's own, so an id the map does not hold marks that origin
  `unclassified`. The merged answer, its columns and `DISTINCT` are
  unchanged. An origin's set can name a category of a row the merge then
  cut by `LIMIT` or `DISTINCT`, and never misses one it delivered.
- `ferrofed report` writes one tar archive a deployment attaches to a
  complaint or a serious-incident report (Regulation (EU) 2025/327 Art 44):
  the version, the commit and target the release build recorded, the
  bindings, the pins, the release attestations to verify, the effective
  configuration redacted fail-closed (every key and value not known to be
  safe, in any casing or nesting, and every `[dev]` value), and, from the
  gateway on the same host, readiness, the
  dependency states, the integrity incidents without their `ehr_id`s
  (with `--operator-token-file`) and the metrics with only the gateway's
  own labels. A part it cannot read is listed in the manifest with the
  reason. The book's Complaints and incidents page documents the format.
- `scripts/release/withdraw.sh` withdraws a non-conforming release without
  editing anything published. It refuses a version with no finding row in the
  register of non-conforming versions. It moves the `<major>.<minor>` and
  `latest` image tags to the correcting release and keeps the version's own
  tag and digest. It publishes the advisory, writes the upgrade note, lists
  the version under "Withdrawn versions" in `SECURITY.md` and records the
  withdrawal in the register. It runs dry by default. `docs/release.md`
  describes the route.
- The access record names the client's address behind a reverse proxy (#722). `server.trusted_proxies`
  lists the proxies, each an IP address or a CIDR block, whose forwarded
  client address the gateway takes, and `server.forwarded_header` names the
  header they write: `forwarded` (RFC 7239), the default, or
  `x-forwarded-for`. A request from a listed proxy is named by the rightmost
  address in that header that is no listed proxy; a request from any other
  peer is named by that peer, whatever headers it sends. The list is empty by
  default, so every request is named by its peer as before. The per-caller
  rate limit keeps keying on the verified caller. The shipped nginx file sets
  `X-Forwarded-For` to the address it received the request from and drops
  `Forwarded`, and the production guide trusts it on loopback.
- `server.public_url` names the gateway's public base URL once (#723). Set,
  an unset `auth.audience` is that URL, an unset `signing.jwks_uri` is the
  JWK Set route under it, and an unset `pmir.callback_url` is the feed route
  under it. Its path must be `server.base_path`, and a `signing.jwks_uri` or
  `pmir.callback_url` that is still written must name the route the gateway
  serves under it, or `config check` refuses it, naming the key and the URL
  it expects. Unset, every key is read as written, as before. The
  production guide shows the shorter form.
- The end-to-end lane holds the production guide's Keycloak recipe (#724): it
  reads the `kcadm.sh` commands, the protocol mapper files and the `[auth]`
  table from the book page, applies them to Keycloak 26.8.0 pinned by digest,
  and gets a federated answer over two FerroEHR nodes with a signed-in
  user's token and a client-credentials token, and a `401` naming `at+jwt`
  with the token-type setting off. The weekly pin-freshness read reports a
  newer Keycloak, and it now follows a registry that pages its tag list.
- Every release attaches the nginx reverse proxy configuration of the
  production guide as `ferrofed.conf`, with its SHA-256 as
  `ferrofed.conf.sha256sum`, and the release lane fails when it is missing
  (#725). The container page and the production guide download it from the
  release with the compose files.
- `ferrofed admission check --read-only` checks a member whose governance
  forbids test data in its production CDR (#726, §12b.1, §12b.2, N42a): it
  creates nothing, reads the `ehr_id` and `system_id` of up to `--count`
  existing EHRs through one AQL query, judges `ehr_id` generation and
  `system_id` uniqueness on them, and ends its report with every condition
  a run without writes leaves unproven, the `ehr_id` exchange of §5.5
  (N34) among them. The admission page says when to run it and when to run
  the full check against a staging copy instead.
- An issuer can be declared a national contact point,
  `[auth.issuer.national_contact_point]` (Implementing Regulation (EU)
  2026/2099 Art 6 and 7; #734). A patient-data request from it needs every
  attribute of the 2026/2099 Annex Tables 1 and 2, read from the IHE IUA
  claims where they coincide and from the claims the table names otherwise,
  and a purpose of use whatever `auth.purpose_of_use.required` says; one
  that lacks either is `403 contact-point-attributes-required` or
  `403 purpose-of-use-required`, with nothing sent. Its requests are served
  with consent exclusions withheld whatever `[federation.consent] disclose`
  says (Regulation (EU) 2025/327 Art 8, Art 11(5)). The professional and the
  provider reach each node in the `national_contact_point` claim of the
  conveyance and the access record in an `ehds-relayed` entity, both marked
  as asserted by the contact point, and the outbound gate reads them like
  every other caller claim; the IUA `person_id` is never read. A correlation
  identifier the connector sends in the declared `correlation_header` is
  recorded with the request, and one out of form is
  `400 correlation-invalid`. `OPTIONS {base}/` declares the setting under
  `federation.national_contact_point`, and the book's authentication and
  §13.4 pages name who authenticates the foreign professional (CP-39).
- `crates/ehds-logging` 0.0.6: `Accessor` carries what a contact point
  relays (`Relayed`), `Request` a correlation identifier, and the BALP
  record writes both (#734).
- The access record names the person behind each access as client
  authentication verified them (Regulation (EU) 2025/327 Annex II 3.1 and
  3.2(b); #742). The BALP `agent:user` carries the professional's IHE IUA
  `subject_name` as `who.display` and `national_provider_identifier` as an
  `ihe-otherId` extension typed `NPI`, the assurance level of the
  authentication as an `ihe-assuranceLevel` extension, and who acts,
  `person` or `client`, as its `role`. The level is recorded only when the
  issuer's `[auth.issuer.assurance]` table maps the value the token states,
  never inferred. No other part of the record, and no log line, carries
  them.
- `crates/ehds-logging` 0.0.4: `Accessor` carries the acting mode, the
  assurance level and the professional's name and identifier, and the
  requester's professional number moves to `alt_id`. `crates/ihe-iti`
  0.0.30: a BALP `User` takes a name, a provider identifier, roles and an
  assurance level, written to the `agent:user` (#742).
- The reporting procedure of the Cyber Resilience Act, Regulation (EU)
  2024/2847 Art 14, which applies to every FerroFED release from 11
  September 2026 (Art 69(3), Art 71(2)). `SECURITY.md`, the book's
  Complaints and incidents page and the maintainers' checklist in
  `docs/post-market.md` describe the early warning within 24 hours, the
  notification within 72 hours and the final report, sent through the
  single reporting platform to the CSIRT designated as coordinator in the
  Netherlands and to ENISA; how users are told through the repository's
  GitHub security advisories (Art 14(8)); and how these reports sit beside
  the serious-incident report of Regulation (EU) 2025/327 Art 44(7).
- A support period for every release: five years from its release date, the
  minimum of Regulation (EU) 2024/2847 Art 13(8). `SECURITY.md` lists each
  release with its end date (Art 13(19), Annex II point 7) and says how
  fixes ship in the latest release (Art 13(10)).
  `scripts/release/changelog.sh --assemble` now opens a release's notes
  with its end date, adds its row to `SECURITY.md` and prints the date; a
  pre-release gets no support period.
- The Regulatory status page states FerroFED's position under the Cyber
  Resilience Act, Regulation (EU) 2024/2847 (#764): every tagged release is
  one product Cadasto B.V. places on the market as its manufacturer, Art 14
  reporting applies to the v0.0.x releases now, and from 11 December 2027
  each release has one technical documentation set, one EU declaration of
  conformity and one CE marking for the CRA and the EHDS Regulation. The
  page and the information sheet state the support period of at least five
  years, with its end date published, and link to `SECURITY.md`. A
  deployment's counsel point on placing on the market now asks whether the
  deployment is an economic operator Cadasto B.V. supplied.
- Every write action an operator runs on the admin listener is recorded
  under `ferrofed::security` (#767): `admin-write-admitted` before the
  action has any effect, counted under the same name, and
  `admin-write-finished` with its outcome, `abandoned` when it never
  answered. Each names the operator's issuer and subject, the method and
  route template, and the time, and nothing else: never the token, a path
  value, a body or a stored query, and the issuer and subject are never a
  metric label.
- Every release attaches its own copy of the information sheet and the
  instructions for use, which Regulation (EU) 2025/327 Art 30(1)(d) has an
  EHR system accompanied by, as `ferrofed-vX.Y.Z-information-sheet.md` and
  `ferrofed-vX.Y.Z-instructions-for-use.md`: the Markdown source of the two
  book pages at the tag, plain text a screen reader reads (#769). The
  release lane refuses an information sheet that does not describe the
  version being released, and publishes no release without both files. The
  gateway and console images name the download address of each copy in the
  labels `eu.ferrofed.information-sheet` and
  `eu.ferrofed.instructions-for-use`.
- The patient summary's stored section queries (#776): wherever the
  stored-query registry is offered, it holds eleven read-only queries of the
  gateway's own under the reserved namespace `eu.ferrofed.eehrxf`, at the
  immutable version `1.0.0`, one per patient summary section openEHR
  content feeds (`eu.ferrofed.eehrxf::patient-summary-problems` and so on).
  Each selects, from every member, the whole compositions that contain one
  of the archetypes the openEHR International Patient Summary template names
  for its section, once each, with the composition's uid and template id.
  The patient is bound through `$patient` and `$namespace`, so no node
  receives the identifier. They are listed, read and run by name as any
  stored query. A `PUT` into the namespace answers `409`
  `stored-query-reserved`, and a store that holds a definition there refuses
  the start. The selection, and the four sections no query feeds, are in the
  book's clinical safety risk file, pending the clinical review.
- `app/ferrofed-eehrxf` 0.0.1, the interoperability component's federation
  half, which builds the section queries as AQL syntax trees with
  `openehr-query`; `eehrxf` still links nothing of FerroFED (#776).
- The openEHR International Patient Summary template of the openEHR
  international Clinical Knowledge Manager, vendored with its source under
  CC BY-SA by `scripts/vendor/openehr-ckm.sh` (#776).
- `eehrxf` 0.0.4: the patient summary crosswalk (#777). Under the
  `patient-summary` feature, each row of `crosswalk::patient_summary` keys on
  an Xt-EHR `EHDSPatientSummary` element path and names the eHealth Network
  element ids of the patient summary guideline Release 3.4, the producer
  obligation, the element or slice of the HL7 Europe Patient Summary
  composition that carries it, and the profiles a FHIRconnect context may map
  to in order to feed it. `Crosswalk::check` fails when an element a
  producer `SHALL:able-to-populate` has no row, when a row names a slice the
  pinned profile lacks, or when a required EPS section slice is uncovered.
  The medical alert (A.2.1.2) and the functional status (A.2.3.4) are listed
  as gaps for the clinical safety review. `dataset::ResourceProfile` reads a
  FHIR resource profile and its slices from a package, and each element now
  carries its target profiles.
- The gateway knows the end of its release's support period from its build,
  never from the network (Regulation (EU) 2024/2847 Art 13(19)). The startup
  banner names it on a `Support` line, and `OPTIONS {base}/` declares it as
  the top-level `supported_until`, or `null` for a build from source or a
  pre-release. Once the date has passed, the banner shows an `UNSUPPORTED`
  notice, `serve` logs it at `WARN` and `ferrofed config check` prints it as
  a warning. `scripts/checks/versions.sh` fails when a release in
  `CHANGELOG.md` has no row in the `SECURITY.md` support table or a row
  whose end date is not five years after its release date.
- Each access record states how long it is kept (Regulation (EU) 2025/327
  Art 9(2), Annex II 3.4): `[access_log.retention]` declares whole years
  for every record, per category and per registry endpoint, each at least
  three, and a record is kept for the longest its categories and origins
  call for, an unclassified one for the longest declared anywhere. The BALP
  record carries `ehds-retention-years`, `ehds-retention-ends` and
  `ehds-retention-ground`, so the Audit Record Repository can apply it. A
  period under three years, an unknown category and an endpoint the
  registry does not hold are refused when the configuration loads. The
  book's audit page names the ITI-81 searches the log is read with at the
  repository (#795, #660).
- `crates/ehds-logging` 0.0.7: the `retention` module, the period a record
  is kept by category and origin with the three-year floor of Art 9(2),
  the record's `retention`, its three BALP `detail` entries, and
  `CategoryMap::category` (#795).
- The access record of a request addressed by `ehr_id`, such as a routed
  read or write or a query scoped to one `ehr_id`, names the patient the
  identity binding holds under it, in each namespace the new `[access_log]
  patient_namespaces` names, so a search of the log by the person's
  identifier (ITI-81 `patient.identifier`) finds it (Regulation (EU)
  2025/327 Art 9(1)). Under PIXm the gateway asks the member's PIX Manager
  with one ITI-83, on behalf of the caller, within the per-node budget.
  Each `ehr` entity says how its patient was looked up in a
  `patient-lookup` detail (`request-named`, `found`, `not-found`,
  `unavailable`, `not-configured`, `unsupported`) and keeps the `ehr_id`;
  the access is recorded and answered whatever the lookup says, and no
  identifier reaches a node or a log line (#796).
- `crates/ehds-logging` 0.0.8: `PatientLookup` and the `patient` of each
  `EhrAt`, every patient found written as a BALP `entity:patient`, and the
  `patient-lookup` detail (#796).
- `eehrxf` 0.0.6: the interoperability component's receive side (#801, part
  of #665). Under `fhir-r4`, `receive::ReceivedDocument::read` reads a
  document received in the exchange format once: it refuses an object that
  repeats a name, decodes the text against FHIR R4, refuses a resource of a
  type R4 does not define and a document that does not encode back to what
  was read, holds it to the R4 document rules (`bdl-7` to `bdl-11`, and
  `fullUrl`s that agree with their resource), and resolves its subject to
  its one `Patient` entry. Every reference, found by its R4 type at any
  depth and resolved by one resolver (`receive::reference::resolve`, which
  reads only canonical spellings and answers exactly one entry or a typed
  refusal), is held to that patient: a `subject`, `patient`, `beneficiary` or
  `for` must name it, and a reference that could name another patient, a
  `type` that disagrees with its target, a second or contained `Patient` and
  an element the R4 table does not describe are refused. The
  text is kept byte for byte. `ReceivedDocument::check` holds the decoded
  document to a `Bundle` and a `Composition` profile read from the vendored
  package: cardinalities, `fixed[x]` and `pattern[x]` values, and `value`,
  `pattern`, `exists` and `type` slices, an entry of a type no slice names,
  and the invariants that require a reference on listed entry types, with
  every constraint it cannot evaluate listed. Under `openehr`, `Mapping::to_openehr` maps the document
  into one canonical composition through FHIRconnect and keeps the document
  in the composition's `FEEDER_AUDIT.original_content` as a `DV_PARSABLE`.
  `dataset::Element` now carries its `fixed[x]` or `pattern[x]` value and
  its slicing (`dataset::constraint`). The gateway does not serve the
  receive path yet; writing a received document to a member is #802.
- Receiving a document in the exchange format (Regulation (EU) 2025/327
  Annex II 2.2 and 2.3, #802): `POST {fhir-base}/Bundle` takes a document
  `Bundle` and writes it as one openEHR composition to the member
  `[[fhir.receive]]` declares for its Art 14(1) category, by ITS-REST
  `composition_create` at that member's own `ehr_id`. The document is held
  to the category's HL7 Europe profiles from the package the deployment
  supplies, mapped by its FHIRconnect mapping, and kept whole in the
  composition's `FEEDER_AUDIT.original_content`. Its patient is resolved
  from every `Patient.identifier` in a namespace `fhir.receive_namespaces`
  names, at the receiving member alone, and every identifier must name the
  same `ehr_id`; no patient identifier is sent in the path, the query string
  or the headers. The caller needs `user/composition-*.c` (a read scope is
  refused), and a `patient/` grant writes only into its own patient's EHR.
  A category with no member, a document that breaks its profiles, a patient
  the member holds no EHR for, identifiers that name two patients, and a
  member's refusal or silence are each an `OperationOutcome` error, never a
  success; every receipt that reached the member writes one access record.
  A member the registry does not hold, or receiving with no resolver,
  refuses the start and `config check`.
- `eehrxf` 0.0.8: `ReceivedDocument::category` names a received document's
  priority category from its `Composition.type`, `Category::profiles` and
  `Category::document_type` name the profiles and the document type of each
  category, `Category::code` and `Category::of_code` name each category by
  its HL7 Europe priority category code, such as `Patient-Summaries`, and
  `ReceivedComposition::model` is the mapped composition as the RM model
  holds it (#802).
- The FHIR face's capability statement, `GET {fhir-base}/metadata`,
  declares the `create` on `Bundle` that receives a document in the
  European exchange format. It names the Art 14(1) categories the
  deployment receives and those it has no receiving member for, and lists
  the `Bundle` profile of each category received (#803).
- The information sheet lists the categories a release can receive in the
  exchange format, patient summaries alone in this one, and says where a
  deployment states the ones it receives and the ones it has no receiving
  member for (#803).
- The FHIR R4 face of the European exchange format (Regulation (EU)
  2025/327 Annex II 2.1, #809): with `[fhir]` set, `GET` and
  `POST {fhir-base}/Patient/$summary` answer the patient summary as an HL7
  Europe Patient Summary document `Bundle`, and `GET {fhir-base}/metadata`
  the face's `CapabilityStatement`. The face sits on its own base beside
  `{base}`, so no ITS-REST path changes. A summary runs the gateway's own
  eleven section queries over every member that holds the patient, with or
  without `[stored_queries]`, resolving the patient once and sending no
  patient identifier to a node, and maps each composition through the
  FHIRconnect mappings `[[fhir.mapping]]` names. Each member whose data a
  section holds is named as its author with one `Provenance` per mapped
  composition; a silent member fails the summary under all-or-nothing and is
  named in every section under partial; what no mapping covers is counted,
  never dropped. Every request passes client authentication, a summary takes
  the `aql-` search scope of each section query, every summary that reached
  a member writes one access record with the `patient-summary` category, and
  every error is an `OperationOutcome`. The book's new page "The patient
  summary over FHIR" describes the request, the document and the
  configuration.
- `eehrxf` 0.0.7: the `document` module writes the EPS document `Bundle`
  from the resources and sections a caller gives, naming every entry under a
  base and keeping two runs over one composition apart; `Mapping` names its
  compiled contexts and runs under a FHIRconnect call context (#809).
- `ehds-logging` 0.0.9: `Evidence::constructed` classifies an access that
  serves one category by construction, such as a patient summary, beside
  what its data show (#809).
- The document list on the Annex II 2.1 FHIR face (#810):
  `GET {fhir-base}/DocumentReference?patient.identifier=system|value`, with
  the optional `type` token of the patient summary, LOINC `60591-5`, answers
  a `searchset` of `DocumentReference`s that meet the HL7 Europe EU Health
  Data API `EehrxfMhdDocumentReference` profile: the summary the gateway
  writes on request, retrievable through `Patient/$summary`, and each
  `native` member's own document, named by `attachment.url`
  `Binary/{token}` with its media type, SHA-1 hash and size. The gateway
  asks each such member's own endpoint by its `ehr_id` alone, through the
  outbound identifier-hygiene gate, with the same budget, onward credential
  and signed caller identity as for a summary, and runs no section query. A
  member that does not answer fails the list, `424` or `504`, under
  all-or-nothing, and is named in an `OperationOutcome` entry with
  `search.mode` `outcome` under `partial`. `GET {fhir-base}/Binary/{token}`
  reads a listed document through the gateway: the token is a JWS the
  gateway signs with its `[signing]` key, of its own JOSE type, valid for
  `fhir.document_url_lifetime_secs` (300 unless set) and for the caller it
  was listed for, and carries no patient identifier; the member is asked
  again and its bytes are answered unchanged, with nothing stored. Both
  take the scopes of a summary and write an access record naming every
  member endpoint asked, and `metadata` declares the search and the read.
- `eehrxf::receive::conform::check_resource` holds any one resource to a
  profile by the rules the document check applies, and the component's own
  tests hold the patient summary it writes to `bundle-eu-eps`,
  `composition-eu-eps`, the profile each entry claims and the EU core
  profile its mapped entries are written to (#815).
- `[federation.consent] emergency` chooses what the consent pre-filter does
  for a request whose verified token declares an
  `[[access_log.emergency_purpose]]` (#833). Under `"apply"`, the default,
  the pre-filter applies as to any request. Under `"pass-to-node"`, the
  pre-filter is still asked, and every member it denies is asked all the
  same, so its node decides (Regulation (EU) 2025/327 Art 11(5); N26,
  N27). The access record names each such member in an
  `ehds-consent-set-aside` detail of its `ehds-emergency-access` entity,
  and the answer never shows it. `"pass-to-node"` needs a declared
  emergency purpose to load, `config check` notes it beside
  `[nl_gf.mitz]`, and `OPTIONS {base}/` declares the setting as
  `federation.consent.emergency`. `crates/ehds-logging` 0.0.13 carries
  `Emergency::with_consent_set_aside`. The book states that the pre-filter
  is asked with the configured `TREAT` or `COC`, never the caller's
  purpose.
- A directed query can order on an ENDPOINT attribute through the
  directive's variable, `ORDER BY p/organisation DESC` alone or among other
  keys (#874). The gateway takes the key out of every node query, orders
  the merged rows on the value it holds per endpoint in the place the client
  wrote it, and breaks a tie on the endpoint id and then the uid; each node
  keeps the other keys and the `LIMIT` (§8.1, §9.3, §11.6.1). Under
  `DISTINCT` the attribute must also be selected, and under
  version-identity dedup with a `LIMIT` the key is refused `400` with the
  new code `endpoint-order-dedup` (§10.2). The variable anywhere else is
  still refused with `endpoint-variable`.
- A stored-query `PUT {base}/v1/definition/query/{name}` with no version is
  stored as a new version the registry assigns: `1.0.0` for a name it holds
  no version of, else the next patch of the newest version by number (after
  `1.9.0` and `1.10.0`, `1.10.1`), answered `200` with `Location` naming it
  as `./{name}/{version}` (§12.7 `stored-query-versioning`; ITS-REST
  `definition_query_store`). A held version is never replaced: the embedded
  store reads and writes in one transaction, and over PostgreSQL a replica
  that finds the version taken assigns the next, up to eight times, then
  answers `409 stored-query-held` and stores nothing. The literal-patient
  refusal applies before any version is assigned, the reserved namespace
  stays closed, a read-only registry still answers `405`, and a distribution
  sends the assigned version. The code `query-version-required`, the `400`
  this `PUT` answered before, is retired, and the error page lists it as
  retired (#875).
- The operator can clear a held `ehr_id` collision once the node at fault
  has fixed it, with `POST /admin/ehr-ids/{ehr_id}/clear` on the admin
  listener (#876). It needs a token with the issuer's `operator_scope`, is
  recorded with the operator who ran it, takes the node-local `ehr_id`
  alone, and forgets it from this replica's `ehr_id` index and from every
  caller's resolution bindings. The next read asks every member again and
  is refused `409` again while the collision stands; a write is never probed
  for. The answer says what the replica held, and the incident report lists
  each cleared collision under `cleared`.
- A registry in FHIR form or read from an mCSD directory declares a member's
  own exchange-format endpoint, as the TOML form does (#883). An `Endpoint`
  with `connectionType` `hl7-fhir-rest` in HL7's `endpoint-connection-type`
  names its categories as `payloadType` codings in HL7 Europe's priority
  category code system, its profiles in `payload-profile` extensions and the
  identifier system of the member's `ehr_id` in one `ehr-id-system`
  extension. Without them it is refused as before, under the rules the TOML
  form's native endpoint meets; the registry page of the book has the
  mapping. `ihe-iti` 0.0.34 reads `payloadType` codings and `valueCanonical`
  and `valueUri` extensions of a directory `Endpoint`.

### Changed

- FerroFED implements the 1.0.0 release of the Federation Tier with AQL
  specification, approved by the openEHR SEC on 2026-10-12, in place of the
  0.9.0 release candidate (#17). `OPTIONS {base}/` now reports
  `federation.spec_version` `"1.0"` (§7a.2), and the startup banner names
  Federation Tier 1.0.0. A client that matches `"0.9"` must accept `"1.0"`;
  no other wire structure changes, and no requirement or conformance point
  moved. `openehr-federation` 0.0.45 carries the new `FEDERATION_SPEC`. The
  book links the specification site at its `1.0` path.
- `nl-generic-functions` 0.0.19 cites the did:web Method Specification by its
  section titles, Method name and Method-specific identifier, where it cited
  a section number the specification does not use (#620). No behaviour
  changes.
- `oauth-server-metadata` 0.0.5 says in its manifest that its name is not yet
  reserved on crates.io and must be claimed before publishing is turned on.
  No behaviour changes.
- Outside the development profile, a gateway with a registry needs an
  `[audit] destination`, because every access to patient data is now
  recorded (#623). `config check`, `serve` and a reload refuse the
  configuration without one, naming `audit.destination`. Outside
  development a gateway with a registry also refuses `destination = "log"`,
  naming `audit.destination`: the log target names no caller and no
  patient, so an access record written there names no one
  (Regulation (EU) 2025/327 Annex II 3.2). `log` stays accepted under
  development, and for a gateway with no registry. The example Kubernetes
  ConfigMap and the release compose example send the records to an Audit
  Record Repository, from the spool on their durable volume. The `log`
  destination never names the client of a request the gateway received.
- `ihe-iti` 0.0.27 writes a BALP entity with `detail` entries, a user's
  organisation as an agent of its own and their alternative identity as
  `altId`, and a record that claims no pattern. `openehr-federation` 0.0.43
  reads the archetype and template ids a bound query constrains its data
  to, and whether they bind every class it reads (#623).
- The Kubernetes example runs the gateway as a StatefulSet,
  `deploy/kubernetes/statefulset.yaml` in place of `deployment.yaml`, so
  each replica keeps its audit spool on a persistent volume claim of its own
  instead of an `emptyDir` a reschedule deletes (#628). The claim names an
  `encrypted` storage class to replace with your own, and is kept when the
  StatefulSet is deleted or scaled down. The container page says what a
  lost spool means for the audit trail. To move an existing Deployment,
  wait until the dependency report reads `up` for the audit repository,
  then delete the Deployment and apply the StatefulSet.
- The front-door documents match the code on `main` (#639). The README's
  status names what v0.0.10 has built so far and what it plans, each with its
  issue. The README, the container page and the configuration page say that
  a secret written inline is accepted under every profile, and that a
  `_file` key keeps it out of the configuration, as `config check` does; a
  test holds `config check` to it. The claims page no longer lists the
  professional's identification, the assurance level and TLS on the listener
  as missing. A test holds the registry page's reload table to the sections
  a reload applies.
- A request that reaches patient data needs a natural person behind its
  token (#661, Regulation (EU) 2025/327 Annex II 3.1). A token whose `sub`
  is its `client_id` (IHE IUA ITI TF-2 3.71.4.2.2.1), or one only `system/`
  scopes cover (SMART on openEHR master08), is answered
  `401 natural-person-required`, unless its issuer declares
  `client_tokens_act_for_professional = true` and the token names the
  professional, in the IUA `national_provider_identifier` or the
  `[auth.issuer.requester]` professional claim. Definitions, `OPTIONS` and
  the operator surface are unchanged. A refused request asks no node and
  writes no access record.
- `[fhir]` needs `[pdqm]`: configuration load refuses the FHIR face
  without a demographics binding, because the HL7 Europe Patient Summary
  `Patient` requires a name (`ips-pat-1`) and the members federate no
  demographics (#663).
- The public texts are reviewed against Regulation (EU) 2025/327 Art 28 at
  every release cut, on the book's new "Claims review" page, which also
  assesses the completeness default, the budgets and the consent pre-filter
  against Annex II, point 2.5 (#674). The first review corrects the claim
  that a client "never learns it was federated" (`meta.federation` names
  every member), says that the identifier rule covers every request the
  gateway composes while a written body travels as sent, credits mutual TLS
  to a node to v0.0.9, fixes the landing page's quickstart, which sent no
  access token, and the README's, which sent the wrong body, and brings the
  stated status up to v0.0.9. The claims page gains a list of the
  limitations to know before deploying FerroFED.
- The access record needs no regional binding: `[audit]`, its spool and the
  BALP access record are part of every build of `ferrofed-server`, so a
  build without the `binding-ihe` feature records every access to patient
  data and runs a registry outside development. `binding-ihe` keeps only
  the IHE transactions' own records, and the audit spool metrics are on the
  metrics surface in every build (#697).
- The book's data-protection and threat-model pages describe the access log
  as built (#711): the access records are a row of the processing inventory,
  the `log` audit destination row says it names no client of a request the
  gateway received and is refused for a registry outside development, and
  boundary B6 and the deployment's risks name what the access log covers and
  what stays planned. The hardening guide asks for `[audit] destination =
  "repository"` once a registry is configured. The authentication page no
  longer says the open health routes name no member:
  `GET {base}/health/dependencies` names every member endpoint id and its
  state. The operator console page says to set `Strict-Transport-Security`
  at the proxy. The README, `llms.txt`, the claims page's limitations and the
  regulatory status page no longer say the European logging component is not
  built or that access to patient data is not audited; the limitations now
  say the access records are kept by your Audit Record Repository and are
  recorded unclassified until you declare a category map.
- The engine compiles no FHIR model (#730). `nl-generic-functions` 0.0.21 moves
  the request authorizer a Generic Functions client takes out of the
  FHIR-bearing `nvi` feature into a feature of its own, `authorizer`, at
  `nl_generic_functions::authorizer`; `nvi` turns it on, and the engine's
  Nuts grant implements it with `nuts-auth` and `authorizer` alone. The
  architecture test now fails when the engine compiles `fhir-types` through
  any crate.
- `eehrxf` 0.0.2: its `fhir-r4` feature compiles no openEHR crate (#730). The
  FHIRconnect mapping from openEHR moves behind a new `openehr` feature,
  which turns on `fhir-r4`, and the architecture test fails when `fhir-r4`
  alone reaches an openEHR crate or the mapping engine.
- A national contact point's `country_code` must be the ISO 3166-1 alpha-2
  code of an EU Member State (Implementing Regulation (EU) 2026/2099 Annex
  Table 1), or the request is refused `403 contact-point-attributes-required`.
  `[auth.issuer.national_contact_point] additional_countries` admits a
  further country per contact point, such as a third country whose contact
  point the Commission connected to MyHealth@EU (Regulation (EU) 2025/327
  Art 24(3)) (#734).
- The node profile checks of `conformance run --node-profile` send each
  `ehr_id`-scoped query as a fixed AQL template with the `ehr_id` as the
  value of its `$ehr_id` parameter in `query_parameters`, so a node must
  bind ITS-REST query parameters, in a `WHERE` clause and in an `EHR`
  predicate, to pass them (#739). The scenario queries of
  `conformance run` are built and printed through `openehr-query`, so a
  value always reaches the gateway as an escaped AQL literal. A new CI
  guard, `scripts/checks/aql-splice.sh`, refuses AQL built by string
  formatting or concatenation outside the tests.
- `[telemetry] filter` no longer quiets the security log (#767): the
  gateway adds `ferrofed::security=info` after the configured filter, so
  every security line at `info` or above is written whatever it says.
- The access record writes each category as a coded value (#824): the six
  Art 14(1) categories carry the codes of HL7 Europe's
  `EEHRxFDocumentPriorityCategoryCS` (`hl7.fhir.eu.health-data-api`
  1.0.0-ballot), written `<system>|<code>` in the `ehds-category`,
  `ehds-category-basis` and `ehds-retention-ground` details, with a new
  `ehds-category-version` detail naming the code system's version. A
  national category is its Member State's code in that state's system.
  No category and unclassified stay states of the record, and the reasons
  an access is unclassified are a closed set. `crates/ehds-logging`
  0.0.12 carries the change: `Category::system`, `version` and `token`,
  `NationalCategory` in place of `NationalCode`, and `classify::Unreadable`
  in place of a free reason string. The four packages the codes are read
  against are vendored with their provenance.
- An audit record names the caller's purposes of use at `purposeOfEvent`
  too, each once, beside the `agent:user` `purposeOfUse` that BALP 1.1.4
  places them at, so an Audit Record Repository that keeps the DICOM audit
  message form keeps them, an emergency purpose such as `BTG` among them
  (RESTful ATNA Tables 3.20.4.2.2.1-1 and 3.81.4.2.2.1-1, ITI TF-2
  §3.20.7.1.2). The access log's records carry them there as well
  (`ihe-iti` 0.0.32, `ehds-logging` 0.0.14; #830).
- The harness and the quickstart run FerroEHR 4.3.6, node and database image
  alike, pinned by digest, in place of 4.3.3. The conformance statement names
  4.3.6 as the FerroEHR product the tests run against.
- `GET {base}/v1/ehr?subject_id=…` whose subject resolves at more than one
  member now answers `400` (`subject-several`) instead of `409`, as N41 has
  it when no one node is found, with the endpoints a client may name in
  `openEHR-federation-endpoint` listed in the message (#869). An `ehr_id`
  two members claim keeps N42's `409` (`ehr-id-collision`). A client that
  branched on the `409` reads the `400` and the `subject-several` code.
- The `409` (`controlling-system-unreachable`) for a versioned write whose
  controlling system is no member now names that system by the version's
  `creating_system_id`, read as an RM `system_id`, and says it is outside the
  federation, as §12.2 and N36 ask (#872). The message still points at where
  the request names the version and quotes no other part of the request.
- A DEMOGRAPHIC request now goes to the endpoint
  `federation.demographic_endpoint` declares without an
  `openEHR-federation-endpoint` header (#873). One destination the operator
  fixes and `OPTIONS {base}/` declares is the explicit choice §12.6 asks
  for, so the request no longer answers `400` `target-required`. A header
  naming another endpoint is still `400` `targeting-conflict`, the query
  string and headers are held as before, and `its_rest.demographic` says the
  header is optional.

### Fixed

- A rolling restart no longer cuts requests in flight (#625). The drain,
  `server.shutdown_timeout_ms`, now defaults to `server.request_timeout_ms`,
  and `config check`, `serve` and a reload refuse a drain shorter than the
  request timeout, naming both keys. The new `server.drain_delay_ms` keeps
  the listener accepting for that long after `SIGTERM`, with readiness
  already `503`, so a load balancer stops routing to the gateway before it
  stops taking requests; it is `0` by default. The example configurations
  drain for 30 seconds, the Kubernetes example waits 5 seconds first, and
  the grace periods outlast both: `terminationGracePeriodSeconds: 45` and
  `stop_grace_period: 40s`. A configuration that set a drain shorter than
  its request timeout, such as the earlier examples' 10 seconds beside 30,
  is refused until the drain is raised or removed.
- The PMIR identity feed across several replicas (#626). A draining gateway
  no longer deletes the ITI-94 subscription that the other replicas behind
  the same `[pmir] callback_url` rely on: the new `on_drain` key keeps it by
  default, for the other replicas and the next start, and `on_drain =
  "unsubscribe"` deletes it, for the last gateway or a replica with a
  `callback_url` of its own. Replicas that find several subscriptions for
  their `callback_url` adopt the same one and delete the others, and a
  delete the Registry answers `404` or `410` counts as done. The book's
  identity page describes the two ways to run the feed across replicas and
  the window during which a replica the change did not reach keeps a stale
  binding.
- Each resolution binding now expires `federation.binding_ttl_ms` after the
  last resolution that returned it. A caller that kept resolving used to keep
  every binding it had ever recorded, so a binding made stale by an identity
  merge or split could route a follow-up to a node the patient no longer maps
  to for as long as the caller stayed active (#650).
- On a stop, the bindings' drain, such as deleting the PMIR subscription,
  runs under its own budget, `server.bindings_drain_timeout_ms` (5 seconds
  by default), after the requests in flight have drained; a binding still
  stopping when it elapses is abandoned with a warning (#676). The shipped
  grace periods are derived from the drain delay, the drain and this budget,
  and the health page shows the arithmetic.
- `scripts/checks/versions.sh` fails when a file the release ships or builds
  its images from is missing, where it skipped the check, and
  `scripts/checks/kubernetes-example.sh` and
  `scripts/checks/release-compose.sh` read every timeout of their grace
  period check from the example's configuration, where they assumed the
  gateway's default request timeout (#676).
- The PIXm resolver reads an ITI-83 answer that repeats one identical
  `(system, value)` `targetIdentifier` in a member's domain as that one
  identifier, so a PIX Manager that returns the same identifier from a master
  and a local record no longer fails the query with `424` (PIXm 3.1.0
  §2:3.83.4.2.2.1, §5.2). Two different values in one domain stay ambiguous
  (#679).
- The `request` span records `http.request.method` as `_OTHER` for a method
  no RFC the gateway knows defines, and names that span `HTTP {route}`, as
  the OpenTelemetry HTTP conventions ask, so a client can no longer mint a
  span value with its method token (#692). An ask-all probe still waiting for
  a slot of a member's in-flight cap when its deadline passes is reported as
  capped with nothing sent, never as a member abandoned without an answer, so
  it no longer counts as a sent time-out in the member metrics (§11.1, §11.5,
  N38).
- The registry page's reload table lists every section a `SIGHUP` reload
  applies, `[pdqm]`, `[nl_gf]` and each endpoint's onward TLS included, and
  every one that takes a restart, `[registry.mcsd]` and `[pmir]` included.
  The SanteMPI setup on the identity page says why the client-credentials
  policies are granted by hand (#695).
- A federated query still waiting for a slot of a member's in-flight cap
  when the overall budget runs out is reported as capped with nothing sent,
  never as a member abandoned without an answer, so it no longer counts as a
  sent time-out in the member metrics (§11.1, §11.5, N38). The request log
  writes the closed method label in its `method` field, `_OTHER` for a
  method no RFC the gateway knows defines, so a client's method token no
  longer reaches the log (#708).
- The two federated-query examples on the book's container page post the
  query from the here-document with `-d @-`; they posted a number, which the
  gateway refused. A guard, `scripts/checks/doc-curl.sh`, refuses a `curl`
  example that posts a bare number, or opens a here-document and passes `-d`
  anything but `@-` or `@<file>` (#721).
- `config check` writes nothing to disk (#727). It created the
  `[audit.repository]` and `[xcpd.audit_repository]` spool directories, so it
  failed on a read-only root filesystem and left a directory behind on a
  host. It now checks each spool directory where it stands: one that exists
  must give its owner write access and no one else any, and hold no file the
  gateway did not write; for one that does not exist, the nearest directory
  above it must admit a writer. Only modes are read, and `serve` still
  creates the directory. A spool `serve` would refuse is refused by the key
  that names it, and `serve` names that key too. The production guide's
  check runs on a read-only root with no durable volume.
- The access log no longer names an endpoint as an origin of data it never
  sent (#731). An endpoint whose request waited out its deadline for a slot
  of `federation.max_in_flight_per_node`, or never left the gateway for
  another reason, is reported `time-out` in the answer and is no origin in
  the record, and a query that left the gateway for no endpoint writes no
  record.
- The book's data-protection page lists the professional's name and
  identifier, the assurance level of the authentication and whether a person
  or a client acted among the personal data the access record carries (#773).
- Configuration load refuses a `[fhir]` base and a PMIR feed route that
  overlap, ASCII case ignored, naming `fhir.base` and `pmir.path`, so the
  FHIR face and the identity feed never share a path (#822).
- The receive path admits the patient summary the gateway writes: an entry
  of a type no slice names is admitted where the profile slices
  `Bundle.entry` open, as `bundle-eu-eps` does, and refused only where the
  slicing is closed, or open at the end and the entry comes before a sliced
  one; an untyped reference by identifier alone is admitted at
  `Provenance.entity.what`, where the summary names each source composition,
  and refused everywhere else as before. Such an entry stays in the kept
  original and becomes no openEHR content (#842).
- The component maps the patient summary it writes back to openEHR (#862).
  The summary names its patient in one form, the relative reference
  `Patient/{id}`, in `Composition.subject` and in each mapped resource,
  where the absolute `fullUrl` in `Composition.subject` read as a second
  subject. Each mapped resource now claims in `meta.profile` the profile its
  FHIRconnect context maps to, once, so a receiver selects the mapping by it.
- Every query the gateway sends a node now carries an explicit `fetch`, so no
  node's own default page can cut its share unseen: the `LIMIT` the query
  carries, or for a query with no `LIMIT` the new `federation.max_node_rows`
  plus one (9999 rows by default, so a node is asked for 10000, the page FerroEHR
  serves by default; declared in `OPTIONS {base}/` as
  `paging.max_node_rows`). A node that answers more rows than that is
  `node-error` and its rows are never merged: the query fails `424` under
  all-or-nothing, and a `partial` answer is incomplete and names the node
  (§11.4, §11.5). The ITS-REST `offset` and `fetch` members now select a
  window over the AQL result (the skip is the AQL `OFFSET` plus `offset`, the
  count the smaller of `fetch` and the rows the AQL `LIMIT` leaves), paged
  under the declared `OFFSET` strategy. The code `paging-conflict`, a `400`
  for a member that differed from its clause, is retired: no request is
  refused with it any more, and the error page lists it as retired (#868).
- `OPTIONS {base}/` now declares that a stored query invoked by name,
  `GET` or `POST {base}/v1/query/{name}[/{version}]`, answers `501 Not
  Implemented` on a gateway without the stored-query registry, as N32
  requires of an unsupported area (#871). With the registry, `its_rest.query`
  says those paths are served from it. The behaviour is unchanged.
- Every stored-query answer that carries `Location` names the stored
  query's own URL, `{base}/v1/definition/query/{name}/{version}` (ITS-REST
  `Location_Query`). With `server.public_url` set it is that absolute URL
  on every route. Without it, the `PUT` answers keep a reference relative to
  the request URL, and the operator's redistribution on the admin listener,
  whose bare version resolved to `…/{version}/{version}`, carries no
  `Location`, because no reference from the admin listener reaches the
  client listener (#899).
- A `native` member's answer on the FHIR face counts as its document only
  when it is a FHIR document `Bundle` whose `identifier` has a system and
  a value (FHIR R4 `bdl-9`), which the `EehrxfMhdDocumentReference`
  profile needs as the list entry's `masterIdentifier` (#901). Any other
  answer, a document with no identifier included, makes the member
  `node-error`. Under all-or-nothing, `Patient/$summary` and the document
  list fail with `424` and name the member. Under `partial`, the summary
  names it as a member that did not answer, never as one covered by its
  own document, and the list leaves its answer out and names it in an
  `outcome` entry. `GET {fhir-base}/Binary/{token}` refuses such an answer
  with `424` and returns none of its bytes. The access record counts no
  document the answer did not deliver.
- A stored query named `.` or `..` with no namespace is refused with
  `400 query-name-invalid`, on `PUT`, `GET` and a run by name, spelled
  plainly or percent-encoded. Such a name is an RFC 3986 dot-segment, so its
  URL resolved to another resource and the gateway could not name it in a
  `Location` or reach it at a node. A name with a namespace, or of three or
  more dots, is no dot-segment and is still accepted (#907).

### Security

- The admin listener authenticates its callers (#629, #635), where it served
  its write actions to every peer that reached it. A write action, such
  as the stored-query distribution, needs an access token that client
  authentication verifies and that carries its issuer's `operator_scope`,
  from any peer, loopback included; it answers `401 unauthenticated` or
  `403 scope-insufficient` otherwise. Only `profile = "development"` still
  admits a loopback peer without a credential. `GET /metrics` takes an
  optional bearer token, `[metrics] scrape_token_file`, compared in
  constant time, and outside the development profile a listener off
  loopback is refused unless that token or `[metrics.tls] client_ca_file`
  authenticates the scrape. Each refusal is counted as `admin-write-refused`
  or `scrape-refused`. The Kubernetes example sets a scrape token, and the
  diagnostic report reads `GET /metrics` with it. The Kubernetes example's
  network policy admits the scraper alone to the admin port, as defence in
  depth.
- The access log no longer classifies a query by fewer categories than the
  data it delivers (#694). A value a query selects through an archetype
  predicate in its path, such as `c/content[openEHR-EHR-OBSERVATION.….v1]/…`,
  is classified by that archetype too, and an archetype id compared as
  `archetype_details/archetype_id/value` binds its class as
  `archetype_node_id` does. A selected path that names its archetype by a
  pattern, a parameter or any comparison other than `=` marks the record
  `unclassified`. An archetype id written in another case, with another
  version or a namespace is held to the map's exact keys and marked
  `unclassified`, never matched to a near key. `crates/openehr-federation`
  0.0.44 carries the reader.
- The gateway reads at most `federation.max_node_answer_bytes` (16 MiB by
  default) of one answer from a member or its token endpoint (#710). A
  longer answer, or one that never ends, is dropped unread, so a faulty or
  hostile member can no longer make the gateway buffer an unbounded body on
  every request. The member is `node-error` with an `error` naming the
  bound, which fails the query `424` under all-or-nothing and clears
  `meta.federation.complete` (§11.1, §11.4); a routed request answers `424`.
  `config check` refuses a zero, and a reload that changes the bound logs it
  as needing a restart.
- `ferrofed admission check --read-only` no longer prints or keeps the
  `ehr_id`s of the existing EHRs it reads. Those are real patients'
  pseudonymous identifiers at a production node, so the report now names each
  EHR by its row and keeps only how many it read; a run with writes still
  names the synthetic EHRs it created.
- The dependency report, which names every member endpoint and which are
  down, moved to the read-only operator surface, `GET
  {base}/operator/dependencies`, behind the issuer's `operator_scope`, so a
  reader without a token no longer learns the federation's membership (#751).
  The operator console and `ferrofed report` read it there with the
  operator's token. It is no probe, so the overload limit now holds it.
- The access record names only whose data the access reached (#756). A
  federated query's record names an `ehr_id` only at a member the query was
  sent to, at the endpoint it was sent through. A routed request's record
  reads the path `ehr_id` and the subject of `GET {base}/v1/ehr` with the
  parsers the route is served by, so a `subject_id` holding a `+` is
  recorded as the patient the gateway resolved, where it was recorded with
  a space. A routed answer whose `ehr_id` or subject the record cannot read
  is withheld with `503 access-unrecorded`.
- `rustls` moves to 0.23.46, which fixes RUSTSEC-2026-0346,
  RUSTSEC-2026-0347 and RUSTSEC-2026-0348, and the workspace requires that
  version or later, so no build resolves one open to them; `ihe-iti` moves
  to 0.0.33 and `ehds-logging` to 0.0.15.

## [0.0.9] - 2026-10-05

### Added

- The operator views of the operator console (#276). After sign-in, which
  now exchanges the code at the provider's token endpoint with the PKCE
  verifier and checks the ID Token's signature, issuer, single audience,
  expiry and `nonce` before a session begins (the new `[oidc]` keys `token_endpoint` and
  `jwks_uri`), the console shows the member endpoints with their membership
  standing and last observed health (`/members`), the integrity incidents
  and the `creating_system_id` routing table (`/integrity`), the stored
  queries with their AQL (`/stored-queries`), and the gateway's
  self-description (`/federation`), each rendered on the server from the
  gateway's surface with the operator's own token. A view without a
  signed-in session sends the browser to sign-in, and a refusal shows the
  gateway's status and stable error code, a `401` with a link back to
  sign-in. The routing table and the stored queries are paged, 100 rows a
  page. With `secure_cookie` on, the console's cookies carry the `__Host-`
  prefix, and a new sign-in ends the session the browser held.
- A read-only operator surface on the gateway: `GET {base}/operator/incidents`
  (each incident kind's count and the last 25 incidents of each kind),
  `/operator/creating-systems` (the routing table) and
  `/operator/stored-queries` (every held version, as the ITS-REST
  `StoredQuery`), admitted only for a token carrying the `operator_scope` its
  `[[auth.issuer]]` entry names as one whole scope token. The last two answer
  one page, `{"items", "offset", "total"}`, under the optional `offset` and
  `limit` parameters, with `limit` at most 100. It answers
  routing ids, counts and stored definitions only, and an incident names an
  `ehr_id` only when it is a bare UUID (§5.4.1, N33).
- `ferrofed conformance run --node-profile` reports CP-18, CP-19 and CP-27
  against every active member (#573; §16.2; N26, N27, N34). The
  Federation-Node profile checks are the gateway's own: each member is read
  through its node client, with its onward credentials and the outbound
  gate, for the patient's EHR by its `ehr_id` alone, an EHR created with no
  subject, and its error statuses for an unknown EHR and an unparsable
  query, and the admission check runs as before. The access and consent
  checks need a refusal arranged with the node's own policy, so a run
  records them not observable with that reason. The harness runs the same
  checks against FerroEHR and EHRbase. The run also scores a new track 3
  scenario: the `ENDPOINT` attributes a client selects are each row's
  endpoint id and `system_id` (§9.4, N12, CP-37).
- Every release attaches the conformance seed data,
  `ferrofed-conformance-seed-data.json`, with its SHA-256 in
  `ferrofed-conformance-seed-data.json.sha256sum` (#573). The file holds the
  vendored template and compositions `conformance run` writes, with their
  Apache-2.0 licence and notice, and `--seed-data` takes it in place of the
  vendored directory, checking each file it holds by SHA-256.
- A mutual-TLS token endpoint alias on a host you name (#560; RFC 8705 §5).
  A node's `[credentials."<id>"]` section takes `mtls_alias_hosts`, each a
  host name with an optional port, and a `fapi2` grant that uses mutual TLS
  then takes a `token_endpoint` of `mtls_endpoint_aliases` on one of those
  hosts, over `https` alone, as well as on the issuer's origin. An alias on
  any other host is still refused before a token request is sent. An entry
  that is no host, or the key in a section with no `fapi2` grant over mutual
  TLS, refuses the configuration, naming the key. `oauth-server-metadata`
  0.0.2 adds `AliasHost` and `Issuer::mtls_alias`.
- Mutual TLS toward a node and its authorization server, with RFC 8705
  client authentication and certificate-bound tokens (#492; §13.1, §13.4,
  N25, CP-17; FAPI 2.0 Security Profile §5.3.2.1). A node's
  `[credentials."<id>"]` section takes `client_identity_file` and
  `trust_roots_file`, the keys and the TLS type every identity service
  takes, and the node and its token endpoint are reached over one transport
  that presents that certificate, `https` under every profile; a section
  may name that material alone. An `oauth2` grant, and a `fapi2` grant, may
  authenticate with `client_auth = "tls_client_auth"` or
  `"self_signed_tls_client_auth"`, sending `client_id` and no assertion
  (RFC 8705 §2), and with `tls_client_certificate_bound_access_tokens =
  true` take only tokens bound to the certificate (§3): a token whose `cnf`
  names another certificate, in the token response or as a JWT claim, is
  refused before the node is sent anything (§3.1, §3.2). It composes with
  token exchange per caller, whose actor token the gateway still signs. A
  `fapi2` grant over mutual TLS uses the `mtls_endpoint_aliases` token
  endpoint (§5), needs the metadata to list `tls_client_auth` and state
  `tls_client_certificate_bound_access_tokens` as the grant uses them, and
  needs a client key only for a token exchange. A grant that sets both `dpop_key_file` and certificate binding,
  either without a client identity, an identity with no certificate, an
  `http` token endpoint or node, and TLS material in a service's own
  `credentials` section refuse the configuration. RFC 8705 is vendored
  under `docs/specs/ietf-oauth/`.
- The operator console's skeleton, `app/ferrofed-viewer` (#275): a Leptos app
  rendered on the server and hydrated in the browser, with a
  backend-for-frontend, as its own binary, `ferrofed-viewer`, and its own
  image, `ghcr.io/ferrohealth/ferrofed-viewer`. It reaches the gateway over
  HTTP alone, as any client does, and holds no clinical data. Built so far: a
  landing page, `GET /health` and the `healthcheck` command, operator sign-in
  at an OpenID Provider with the authorization code grant, a `nonce` and
  PKCE, its `state`, `nonce` and verifier held on the server behind an
  opaque `HttpOnly` cookie in a bounded, short-lived pool of pending
  sign-ins that a flood of login starts cannot grow and that never touches
  the separate, bounded pool of signed-in sessions with their idle and
  absolute timeouts, configuration refusals that name a key and a line and
  never a value, and a typed client of the gateway's `OPTIONS {base}/`
  and ITS-REST surface called with the operator's own token. Every answer
  carries a per-response script nonce in its Content-Security-Policy. The
  console reads a TOML file with `FERROFED_VIEWER__` overrides and a
  `client_secret_file`. The code exchange, the operator views (#276) and the
  query console (#277) are planned. The release lane builds, attests and
  pushes the image with the same SLSA provenance and SBOM attestations as
  the gateway image, CI lints the browser half for `wasm32` and builds the
  site bundle, and the book's "The operator console (planned screens)" page
  says what is built and what is planned.
- The NVI localizer authenticates with the Nuts grant (#539; Annex B §B.1,
  §B.4; the IG's Localization page, GF-Authentication, GFI-004 and
  GFI-005). `[nl_gf.nvi.credentials.nuts]` takes the table a node's onward
  credentials take and runs the same grant: a Verifiable Presentation of the
  gateway's credentials for a token bound to its `DPoP` key, cached until 30
  seconds before it expires and dropped on the NVI's `401`. Every search
  carries the token with a proof of the key over the search URL without its
  query, and a nonce the NVI demands is answered once (RFC 9449 §7.1, §9).
  The token goes to the NVI alone. A refused grant leaves the localization
  unavailable, so the query fails closed (§14.1). An `oauth2` or `fapi2`
  table under `[nl_gf.nvi.credentials]` is refused, naming its key, since the
  IG defines no such grant for the Localization Service. The
  `nl-generic-functions` NVI client takes an authorizer that makes each
  request's headers and is handed the request URL without the query that
  holds the pseudonym (0.0.12).

- `ferrofed conformance run --config <file>` scores a configured deployment
  against the Connectathon tracks 1 to 7, 9 and 11 over ITS-REST (#546;
  §16.3, §16.4). It starts the gateway in-process from the configuration, or
  drives one already serving at `--gateway <url>`, as a client holding a
  bearer token from `--token-file`. It seeds a synthetic patient in the
  `urn:oid:2.999` arc at every member the deployment's own cross-reference
  names, through each node's ITS-REST writes: the patient's EHR, the vendored
  template and one vendored composition, three EHRs with no subject, and the
  writes the scenarios make. It reads no EHR it did not create and removes
  nothing. It refuses to start without `--allow-writes`, against a profile
  other than `development` without
  `--i-understand-this-writes-synthetic-data-to-the-nodes`, for a patient
  outside the example arc, and for seed files other than the vendored ones,
  which it checks by SHA-256. The `--gateway` URL and every member endpoint
  are held to `https`, or `http` to a loopback host under the development
  profile alone, which the report names; the gateway client follows no
  redirect, and the caller's token is held as a secret that no report, log
  line or error carries. It writes the per-track and per-point report the
  harness run writes, with the same columns and results, and reports every
  scenario that needs fault injection, node-side capture or a gateway
  configured for it `not-run` with the reason, never `pass`; `--node-profile`
  adds the admission check of every member as the node profile findings. The
  end-to-end tracks call the same scenario checks, and a run against the
  harness reads what the harness scores and nothing more.

- An end-to-end check that the `AVG` the gateway declares decomposable in
  `OPTIONS {base}/` is the mean weighted by each node's count (§11.6.3, N39,
  CP-10, CP-32), behind `FERROFED_E2E`. With two values at one FerroEHR node
  and one at the other, each node is asked its `SUM` and `COUNT`, and the
  answer equals the mean weighted by the counts each node returns for the
  query it was sent, which differs from a mean of the node means.
- The gateway serves the `did:web` DID document of each Nuts grant's
  holder (#503; Annex B §B.4, Nuts RFC021 §4.2). It is served at the path
  the DID resolves to (the did:web Method Specification, Read (Resolve)),
  such as `/.well-known/did.json`, under any base URL, as
  `application/did+ld+json` and with no client authentication, like the JWK
  Set. The document is built from the holder keys the `nuts` sections name
  and nothing else, one `JsonWebKey2020` verification method per `kid`, so
  a key change shows up on the next reload with no file to edit. A DID
  whose document would sit under `{base}/v1/`, two DIDs that share one
  document path, or two keys under one `kid` refuse the configuration.
  `nl-generic-functions` 0.0.13 adds the `nuts_auth::did_document` module,
  `HolderKey::public` and `Did::document_path`.
- A differential run against the Federation Tier reference implementation,
  behind `FERROFED_E2E` (#94; §16.3 tracks 1 to 7 and 9). The testkit builds
  `syntaric/openehr-federation-ref` at its pinned commit from the vendored
  source and the manifest held to its recorded sha256, on digest-pinned Maven
  and Java images, and runs it beside FerroFED over the same two FerroEHR
  nodes, each gateway behind capturing proxies of its own. Some thirty
  requests go to both, and the run compares the status, the federation
  headers, the envelope against the vendored schema, `meta.federation`, the
  rows and what reached each node, after normalising only what the
  specification leaves free. Every difference is held in a register with its
  verdict and where it is recorded: a new or vanished difference fails the
  run. The `e2e (containers)` job uploads the Markdown and TSV report as the
  `differential-report` artifact. No difference is a FerroFED defect; three
  reference divergences and one specification question are new upstream
  reports (#212 T190 to T193).
- A `fapi2` grant's client key rotates with an overlap (#514): the new
  `previous_client_key_file` names the previous P-256 client key, which
  the JWK Set at `{base}/.well-known/jwks.json` publishes after the current
  key until it is removed from the configuration. Only the current key
  signs. An authorization server that fetches the set during the rotation
  verifies both the new assertions and one the previous key signed before
  it (FAPI 2.0 Security Profile §5.4.2). A previous key that is no P-256
  key, or is the current key, refuses the configuration, naming the key.
- `[signing]` holds a P-256 key as well as a P-384 key, and reads the
  algorithm from the key: a P-256 key signs ES256 and a P-384 key ES384
  (RFC 7518 §3.4) (#513). The current key signs the
  `openEHR-federation-client` conveyance and every `oauth2` client assertion
  with its own algorithm, and the JWK Set publishes each key with its `alg`,
  through the same previous-key rotation window, which may cross curves. A
  federation whose nodes hold to the FAPI 2.0 Security Profile, which admits
  ES256 and not ES384 (§5.4.1), signs with a P-256 key. A key on any other
  curve refuses the configuration, naming the key.
- The Federation-Node profile runs against a second CDR product, EHRbase,
  behind `FERROFED_E2E` (#549; §16.2, N26, N27, N34, CP-18, CP-19, CP-27,
  CP-33a). The testkit pins `ehrbase/ehrbase` 2.36.0 and the
  `ehrbase/ehrbase-v2-postgres` 16.2 database image by digest, each a
  `docs/VERSIONS.md` row the versions guard compares, and every node profile
  check runs against it in the `e2e (containers)` job and writes its findings
  beside FerroEHR's. For the access check (CP-18) an EHRbase started with
  Basic authentication withholds the request path of one EHR from its user
  role, the narrowest refusal its `security.additionalAuthorizations`
  expresses; it records no consent decision, so the consent check (CP-19)
  reports not observable with that reason. The conformance report shows the
  findings of each product side by side under the node class, then each
  product's evidence. EHRbase passes the error pass-through and subjectless
  EHR checks. It refuses an `EHR_STATUS` subject in a namespace such as
  `urn:oid:2.999.1.1`, which the BASE `OBJECT_REF.namespace` pattern admits.
  The admission check therefore creates no test EHR there, and its integrity
  conditions fail on that cause. Its EHR read omits `ehr_access`, which the RM
  `EHR` class makes mandatory. As at FerroEHR, both `ehr_id`-scoped query
  forms release the EHR whose read the node refuses. The invocation finding
  names why an EHR body does not decode. No specification governs which
  products the harness runs: our own design.
- Each IHE audit record the gateway makes for a client's request names the
  verified caller (#500; PIXm §2:3.83.5.2.1, BALP 1.1.4 §3:5.7.5.4, ITI TF-2
  §3.55.5.1.1, IUA ITI TF-2 §3.72.5.1). The ITI-83, ITI-78 and ITI-119
  `AuditEvent` carries the BALP `agent:user` with the token's `iss` and
  `sub`, `requestor` true and its purposes of use, and an Application agent
  with its `client_id`; the ITI-55 DICOM message carries the Human Requestor
  with the `sub` as `UserID` and `aud<sub@iss>` as `UserName`, as its one
  requestor. A record the gateway makes on its own behalf (the admission
  check, the mCSD directory reads, the PMIR subscription and feed) names no
  caller. The identity reaches the Audit Record Repository alone: the log
  destinations record only whether a record was made for a caller or by the
  gateway, and no metric carries it.
  `ihe-iti` 0.0.23 takes whom each PIXm, PDQm and XCPD exchange is made for
  (`ihe_iti::user::OnBehalfOf`), and the resolver, localizer and
  demographics roles take it too. An ITI-83 answer a localization read is
  reused only by the resolution of the same caller, so another caller's
  access sends, and records, an ITI-83 of its own. The security event
  `edge-identity-asserted` names the caller by `subject_ref` and
  `client_ref`, keyed HMAC-SHA256 references stable for the process, in
  place of the subject and the client, and a verified caller's `Debug`
  shows neither them nor the issuer.
- Track 11, the integrity suite, against the two FerroEHR nodes of the
  harness behind `FERROFED_E2E` (#91; §16.3 track 11, §12.5.1, §12.5.2,
  §12b, N41, N42, N42a, CP-33, CP-33a). The harness seeds one `ehr_id` at
  both nodes for two different synthetic patients. A read and a query scoped
  to it are refused `409` `ehr-id-collision` listing both claimants, with one
  integrity incident, and the proxy journals show the ask-all probe as the
  only request either node received. Once two targeted reads teach the
  `ehr_id` index both claimants, which raises the index-insert alarm once, a
  read and two writes are refused the same way and reach neither node. A
  versioned write no earlier step of §12.5.1 routes is refused `400`
  `target-required` and probes nobody, although its `If-Match` names node
  A's `system_id`. As the federation operator, the harness then applies the
  admission check to a FerroEHR candidate deployed with member A's
  `system_id`: recorded under that `system_id`, the registry load refuses it
  as a duplicate, and recorded under one of its own, the check fails the
  `system_id` condition naming member A. The conformance matrix records
  track 11 as covered.
- The Connectathon tracks 1 to 7 and 9 of §16.3 run as scenarios in the
  end-to-end harness over two FerroEHR nodes: transparency, resolution
  through the PIX Manager, the directed query and the endpoint pin, partial
  results under node faults, de-duplication and cross-node shaping,
  follow-up routing, onward authentication with consent left to the node,
  and the REST surface with its self-description. Track 8 stays deferred,
  as the specification marks it provisional (#92).
- `scripts/conformance/report.sh` writes the report of §16.4 from a test
  run: every track and every conformance point as pass, fail, not run,
  deferred, not applicable or open, with the issue and reason of each
  deferral. CI writes it from the offline and the container runs and uploads
  it as the `conformance-report` artifact (#92).
- The Federation-Node profile of §16.2 as operator and harness tooling
  (#93; N26, N27, N34, CP-18, CP-19, CP-27). The testkit's `node_profile`
  checks what a member CDR's ITS-REST interface shows of each node
  obligation. They check that an EHR, its `EHR_STATUS` and its compositions
  answer to the `ehr_id` alone, that an EHR created with no subject is read
  and queried by its `ehr_id`, and that an unknown `ehr_id` and an
  unparsable query answer the ITS-REST error status. They also check that an
  EHR the node's own policy withholds is refused on the read and on both
  `ehr_id`-scoped query forms, and that a consent refusal an operator
  arranges is held the same way. Each check records pass, fail or
  not-observable with its evidence. The end-to-end run executes every check
  against FerroEHR, including a FerroEHR started with Basic authentication
  and restricted per-EHR access. It also runs the admission check of §12b.2
  against a FerroEHR member. The conformance report shows the Node and
  Operator points as their own class, scored from those findings and never
  as a gateway pass, and lists every finding per product.
- A PIX Manager can be asked by `POST` (#494). `[[pixm.manager]]` takes
  `method = "post"` beside `"get"`, the default: the gateway then posts the
  ITI-83 `sourceIdentifier` and `targetSystem` parameters in a `Parameters`
  body to `[base]/Patient/$ihe-pix` (FHIR R4 Operations §3.2.0.1), so the
  patient identifier is in no request URL a proxy or an access log records.
  `get` stays the default because PIXm 3.1.0 §2:3.83.4.1.2 prescribes the
  `GET` and requires no Manager to accept a `POST`. Any other value refuses
  the configuration naming the key. The ITI-83 audit record of a `POST` holds
  its request line, media type and body. `ihe-iti` 0.0.22 adds
  `pixm::Invocation` and `PixmClient::invoked_by`, and the harness PIX
  Manager answers a posted query.
- Each regional binding is one Cargo feature of `ferrofed-server`:
  `binding-ihe` carries the IHE binding of Annex A (`[pixm]`, `[pdqm]`,
  `[xcpd]`, `[registry.mcsd]`, `[pmir]` and `[audit]`) and `binding-nl` the
  Dutch Generic Functions of Annex B (`[nl_gf]` and the Nuts grant of
  `[credentials."<id>".nuts]`); `[dev]` is always built. Both features are on
  by default, so the released binary and the container image behave as
  before. A build without one refuses its sections as unknown keys (#489).
- The book has a page on adding a country: a specification crate, a binding
  module and a feature line (#489).
- The query console of the operator console (#277), at `/query`. An AQL
  query, or a stored query by name and optional version, runs through the
  gateway's `POST {base}/v1/query/aql` or `/v1/query/{name}` as the
  signed-in operator, with the targeting, dedup and best-effort headers the
  gateway's self-description offers. The answer shows the rows, every
  endpoint's status, latency and error from `meta.federation`, and says in
  words when `complete` is false; a `504` or `424` all-or-nothing failure
  shows its diagnostic envelope, and a refusal its status and stable code.
  A patient is named through a parameter in the request body, and nothing
  entered in the console reaches a URL, the browser history or a log. A
  query runs once the page has loaded; its answer is rendered on the server
  with every value a node sent escaped, and is never cached or compressed.
- Operator sign-out in the operator console (#584). The navigation bar's
  "Sign out" posts to `POST /logout`, which ends the server-side session,
  removes the session cookie and, where the new `[oidc]` key
  `end_session_endpoint` names the provider's end-session endpoint, sends
  the browser there with the ID Token hint, the client id and the optional
  `post_logout_redirect_uri` (OpenID Connect RP-Initiated Logout 1.0). The
  sign-out and every server function the views and the query console call
  are taken only from the console's own pages (`Sec-Fetch-Site`, `Origin`
  or `Referer`); any other request is a `403` before the session is read.
- Browser journeys for the operator console (#608): headless Chrome, driven
  over WebDriver from Rust with `thirtyfour`, signs in at a test OpenID
  Provider, opens each operator view and follows its pagers, runs a query
  that shows every node's status and whether the answer is complete, sends a
  query the gateway refuses, and signs out, against a running gateway over
  stub nodes and the console serving its release site bundle. Each journey
  fails on any error the browser logs. They run behind
  `FERROFED_JOURNEYS=1` in the new `journeys (browser)` CI job, at the Chrome
  for Testing release `docs/VERSIONS.md` pins.
- The conformance statement (#95, #89): a book page under Evaluate that claims
  the Federation-Gateway profile of the Federation Tier with AQL 0.9.0 at
  commit `7162d0c`, with every §17 point and §16.3 track and how each is
  scored, every deferral with its actor and reason, and the FerroEHR and
  EHRbase images the tests ran against. Its generated part and the new page of
  marked tests come from `scripts/conformance/matrix.sh --statement-write`,
  and the docs build and the conformance-matrix guard fail when either
  disagrees with the matrix. The guard now refuses a `planned` point or
  track, so a re-pin scores or defers every point it adds. Six rows of the
  obligations checklist that still read `planned` after their work shipped now
  read `tested` and name their tests.

### Changed

- The Licensor and copyright holder of FerroFED's own work is Cadasto B.V., replacing Vernum Projecten B.V. (#602). The licence, its Additional Use Grant and its Change Date do not change. A new guard, `scripts/checks/copyright-holder.sh`, fails any first-party header that names another holder.
- The `openehr-*` family moves from 0.0.83 to 0.0.84 (FerroEHR #3565,
  #574). A re-send answering a node's `DPoP` nonce challenge that no proof
  could be made for is the node's `node-error`, with the node counted as
  asked, read from the client's `DpopProof { sent }`. The node clients no
  longer keep a record of each call's sends. `openehr-federation` 0.0.40
  takes the new family.
- The `openehr-*` family moves from 0.0.82 to 0.0.83 (FerroEHR #3560,
  #566). A deadline that passes before the re-send answering a node's
  `DPoP` nonce challenge is the node's `time-out`, with the node counted as
  asked, read from the client's `DeadlineElapsed { sent }` in place of the
  call's prover record (#470). The record still reads a re-send that no
  `DPoP` proof could be made for, which the client reports with no such
  field. `openehr-federation` 0.0.39 takes the new family.
- The harness and the quickstart run FerroEHR 4.3.3, pinned by digest at
  its new home under `ghcr.io/ferrohealth`, node and database image alike
  (#566). That release makes AQL honour `EHR_ACCESS` on every query form
  (FerroEHR #3562), which the node profile's "access decided at the node"
  check (CP-18) reads.
- A consent pre-filter that did not ask its service says so, with a closed
  reason, in place of answering as one that was asked and denied nothing
  (#496). Mitz is not asked about a patient named outside the BSN, such as by
  a pseudonymised BSN (reason `namespace`), or for a caller whose token does
  not state the requester claims (reason `caller-claims`). Such a call is
  counted in `ferrofed_consent_prefilter_requests_total` with
  `outcome="not-asked"` and its `reason`, no longer as `no-signal`, and it
  leaves the pre-filter's state on `GET /health/dependencies` as it was.
  The client's answer is unchanged: every candidate is asked, nothing appears
  in `meta.federation.consent`, and under `[federation.consent] disclose =
  false` the answer is the one a pre-filter that found nothing gives.
- Two sections that fill a role exactly one may, two resolvers, two consent
  pre-filters or two localizers of a binding's own, are refused with one
  error that names every section, such as `[nl_gf.nvi] and [xcpd] are both
  localizers; set one`, in place of one error per pair. A localizer section
  under `node_selection = "ask-all"` is refused naming its section, as
  before (#489).
- The `configuration resolved` log line names the compiled bindings, and
  each binding logs what it is configured to reach on a `binding configured`
  line of its own; the registry directory, PIX Manager, PMIR and PDQm fields
  move from the first line to the IHE binding's (#489).
- A build without a binding feature compiles none of that binding's crate:
  `binding-ihe` and `binding-nl` turn on new `ihe` and `nl` features of
  `ferrofed-identity` and an `nl` feature of `ferrofed-engine`, so
  `cargo build -p ferrofed-server --no-default-features` builds neither
  `ihe-iti` nor `nl-generic-functions`. The registry document in FHIR form
  (`registry.format = "fhir"`), read with the mCSD reader, is part of
  `binding-ihe` (#551).
- The authorization server metadata checks of RFC 8414 move out of
  `nl-generic-functions` into a crate of their own, `oauth-server-metadata`
  0.0.1, which the OAuth 2.0 issuer audience, the FAPI 2.0 grant and the
  Nuts grant share. `nl-generic-functions` 0.0.11 drops its
  `oauth-metadata` feature (#551).
- An error that needs a cross-reference resolver names the resolvers the
  build carries, `[dev] or [pixm]` by default and `[dev]` without
  `binding-ihe` (#551).

- `[federation.consent] disclose = false` keeps a consent exclusion out of
  what a client sees, for Regulation (EU) 2025/327 Art 8, under which the
  fact that a patient restricted access "shall not be visible to healthcare
  providers". A member the consent pre-filter excludes is still never sent a
  request, and a federated query reports it `not-resolved`, with the same
  `error` as a member the cross-reference does not know the patient at, so
  `meta.federation.complete` stays `false` and nothing fails. A read of an
  EHR by subject that only an excluded member could serve answers
  `404 subject-unavailable`, the same answer as for a subject no member
  knows, a new code in the error vocabulary. `OPTIONS {base}/` declares the
  choice as `federation.consent.disclose`, the pre-filter metrics still count
  every exclusion for the operator, and the default stays the
  specification's `consent-denied` (N27a) (#493). The setting covers a
  node's own consent refusal too, a `403` with a code the registry lists in
  `consent_refusal_codes`: a federated query reports the node `not-resolved`
  with no `latency_ms`, and a read by subject, a routed request and an
  ask-all probe answer `404 subject-unavailable`. On a request under
  `{base}/v1/ehr/`, the creation of an EHR and a DEMOGRAPHIC request, a
  node's own `404` gets that same answer in this mode, so it never stands
  for a refusal alone; every `subject-unavailable` carries one fixed
  message. The node request metrics now count such a refusal as
  `consent-denied` on every path, whatever the setting.
- `nl-generic-functions` 0.0.16 adds `BSN_SYSTEMS` and `is_bsn_system` to
  `identification`, beside `PSEUDO_BSN_SYSTEM` and `URA_SYSTEM`: the naming
  systems of the BSN itself, the IG's `$bsn` system and its OID as a URN and
  dotted (#587).
- The changelog is written as fragments (#598). A pull request with a
  user-visible effect adds one file under `changelog.d/`, named
  `<issue>-<kebab-slug>.<section>.md` and holding its entry, instead of editing
  `CHANGELOG.md`, so two pull requests no longer conflict on one file. The
  release cut runs `scripts/release/changelog.sh --assemble <version> <date>`,
  which writes the fragments and the entries already under `[Unreleased]` into
  the new version's section and removes the fragments. The new `changelog`
  job of CI runs `changelog.sh --check` over every fragment, and the
  `changelog-guard` job fails a pull request that adds no fragment and leaves
  `CHANGELOG.md` untouched, unless it carries the `no-changelog` label.
- The operator console serves its site bundle brotli- or gzip-compressed, as
  the browser's `Accept-Encoding` chooses, with `Vary: Accept-Encoding`, so a
  browser downloads about 220 KB of WebAssembly where it downloaded 770 KB;
  pages and server function answers stay uncompressed (#600). The release
  bundle names no directory of the host that built it: the build remaps the
  cargo home, the toolchain and the checkout, and CI fails a bundle that
  names a home, runner, registry or toolchain path.
- `ferrofed-identity` names the PMIR subscriber's error `ihe::pmir::PmirConfigError`,
  after its module, where it was `LifecycleConfigError`, and the log target both
  audit log recorders write to is `ihe::audit::AUDIT_TARGET`, where it was
  `ihe::xcpd::AUDIT_TARGET`; its value, `ferrofed::audit`, is unchanged. In
  `nl-generic-functions` 0.0.18, `mitz::BSN_ROOT` takes its value from the BSN
  OID of `identification::BSN_SYSTEMS`, which is unchanged (#606).
- The operator console refuses to start with `[session] secure_cookie = false`
  unless its `[oidc] redirect_uri` is an `http` URL on a loopback host
  (`localhost`, `127.0.0.0/8` or `::1`), so a deployed console never sends its
  session cookie over plain HTTP; the refusal names `session.secure_cookie`
  (#615).
- A commercial licence, and any other business or licensing question, goes to
  Cadasto B.V., the Licensor, at info@cadasto.com or
  <https://www.cadasto.com/contact/> (#616). Every `LICENSE` copy names that
  contact where it named the maintainer, so `openehr-federation` 0.0.42,
  `ihe-iti` 0.0.25, `nl-generic-functions` 0.0.17 and `oauth-server-metadata`
  0.0.4 carry it. The maintainer stays the contact for technical questions.
  The licence terms do not change.

### Fixed

- A caller whose token states the requester claims in a form the Mitz
  question does not take, or a patient value Mitz does not take as a BSN,
  is no longer reported as a Mitz outage, although Mitz was never asked
  (#568). The pre-filter answers that it did not ask, with the reason
  `caller-claims-invalid` or `patient-value`, so the call is counted as
  `not-asked`, `GET /health/dependencies` keeps the pre-filter's state, and
  the client's answer carries no `meta.federation.consent.error`. No claim
  value reaches a label, a log line or an error.
- A stored-query definition that a PostgreSQL or `redb` store holds is now
  checked on every read, as the files backend checks each file (#586). A
  row written past the gateway, by a restore, a manual insert or an older
  version, that names its patient by a literal or does not admit as a
  definition is never served or run: the gateway refuses to start over a
  store holding one, and over a shared PostgreSQL store a read, a listing or
  an invocation that reaches such a row answers `500`. The security log
  records a `held-definition-refused` event naming the definition's name and
  version, never its text.
- The operator console's sign-out reaches the provider's end-session
  endpoint in the browser: the Content-Security-Policy's `form-action` now
  names that endpoint's origin beside the console, where before the browser
  blocked the redirect the sign-out form is answered with (#608). The console
  also serves the brand favicon at `/favicon.ico`, which every browser asks
  for on its first page and which answered `404` before.
- Every operator console page the gateway fills is whole in the HTML the
  server sends, so it reads with no script: the pages render once every
  answer is in, where before a page could arrive with its loading notice and
  the content in a template that only a script moved into place (#608). A
  query or a view the gateway refuses, and a query form the console cannot
  send, are shown with the gateway's status and code or the field at fault
  as before, and the browser no longer logs them as a failed request.

## [0.0.8] - 2026-10-04

### Security

- The trace export records nothing of a client's `traceparent` or
  `tracestate`. The request span recorded the client's trace id and span id
  as a span link, and a client chooses its trace id freely, so a patient
  identifier hex-encoded in it reached the operator's trace collector. The
  gateway now reads neither header: it starts its own trace for every
  request, as before, and every trace id and span id an exported span
  carries is one it generated (#446).

- A node is never sent a request without its configured onward credential,
  and never the caller's own `Authorization` header.
- The `error` a client sees for a node no token could be obtained for is a
  fixed sentence and, when the token endpoint refused with one, its
  registered RFC 6749 §5.2 code. The token endpoint's description, its
  address and the network error go to the log alone, and the assertion and
  the token go nowhere.
- A token endpoint URL with a user name, a password, a query or a fragment is
  refused at load, so no secret can ride in it into a log.
- No request reaches a node, a cross-reference service or a store before its
  caller is verified. A token signed with `none` or an HMAC, of another
  `typ` than `at+jwt`, from an untrusted issuer, for another audience, or
  outside its validity window is refused, and an issuer that cannot be asked
  fails closed with `503`. A `patient/` scope admits nothing, because the
  gateway cannot bind it to the token's patient context, and a
  `system/aql-*` scope counts only for a listed backend client. The
  DEMOGRAPHIC API admits only the clients an issuer lists in
  `demographic_clients`. The client's token is never forwarded to a node.
- Outside `profile = "development"`, `serve`, `config check`,
  `admission check` and every reload refuse a credential sent to a URL that
  is not `https`, naming the key of the URL and of the credential: a registry
  endpoint with a `[credentials]` section, an `oauth2` token endpoint, a PIX
  Manager with credentials, and a `metrics.otlp_endpoint` carrying a user
  name or a password. A reload keeps the profile the process started with.
  Under the development profile the same configuration starts, and the
  startup banner, a `WARN` log line and `config check` name each credential
  that travels unencrypted, by key, an XCPD responding gateway's XUA
  assertion included (#402).
- A PIX Manager and an XCPD responding gateway are sent patient identifiers,
  so outside the development profile each must be `https`, with or without
  a credential configured. Under the development profile they are named in
  the banner, the log and `config check` like a cleartext credential (#402).
- A reload whose file changes `profile` is refused with class `profile`, and
  the running configuration stays, so nothing the development profile admits
  can enter a process that started under another profile. A reload under
  another profile could admit an `http` XCPD gateway before (#402).
- One module holds every transport rule: a URL a credential or a patient
  identifier is sent to must be `https` outside the development profile,
  and a key set or introspection endpoint the gateway verifies callers
  against must be `https`, or `http` to loopback, under every profile (#402).
- Outside the development profile, the stored-query store's PostgreSQL
  connection string must set `sslmode=require` when it carries a password
  to a networked host: `disable` and `prefer`, the driver's default, which
  falls back to no TLS, are refused naming `stored_queries.url` by
  `config check`, the start and a reload, even though the store itself
  changes only on a restart. Under the development profile it starts and is
  named in the banner, the log and `config check` (#416).
- No request reaches a node without the caller's identity: one that reaches
  the dispatcher with no verified caller is answered `500` and nothing is
  sent. The conveyed token never carries a patient identifier (§5.4.1, N33):
  it holds no IUA `person_id`, and the outbound gate reads every caller
  claim, refusing with nothing sent a request whose claims would carry the
  identifier its query was resolved on. A client's own
  `openEHR-federation-client` header never reaches a node.

### Added

- Mutual TLS to the PIX Manager, the PDQm Supplier, the PMIR Patient
  Identity Registry and the mCSD directory: `[[pixm.manager]]`, `[pdqm]`,
  `[pmir]` and `[registry.mcsd]` take `client_identity` (or
  `client_identity_file`) and `trust_roots_file`, as `[xcpd]` does, the
  client identity held as a secret. A file that cannot be read is refused
  naming its key, and material that is no PEM refuses the federation
  without quoting it. The testkit gains a mutual-TLS front that puts any
  harness server behind `https` with client authentication (#507).
- A FAPI 2.0 grant as an onward credential, for a node whose authorization
  server follows the FAPI 2.0 Security Profile, as the BgZ/eOverdracht
  track of the Dutch binding does (Annex B §B.4a): a
  `[credentials."<id>".fapi2]` section names the server's issuer, and the
  gateway reads its RFC 8414 metadata once, holds it to the issuer and to
  everything the grant sends, authenticates with an ES256
  `private_key_jwt` assertion whose `aud` is the issuer, binds every token
  with DPoP, and sends the configured RFC 9396 `authorization_details`,
  taking only a token response that states the details granted. Token
  exchange per verified caller works as for `oauth2`. The client key's
  public half is published in the gateway's JWK Set. The authorization
  code grant, which needs a user agent, refuses the configuration. The
  testkit has a harness FAPI 2.0 authorization server, and the book has an
  Onward credentials page (#497, #88).
- `assertion_audience = "issuer"` in an `oauth2` section makes every client
  assertion name the authorization server's issuer, set in `issuer`, as its
  `aud` in place of the token endpoint (RFC 7523 §3; FAPI 2.0 §5.3.2.1).
  The token endpoint stays the default (#497).
- `nl-generic-functions` 0.0.10 adds the `oauth-metadata` feature: the RFC
  8414 issuer identifier and metadata checks the Nuts grant built, now
  shared by both Annex B authentication tracks (#497).
- The PDQm demographics step of Annex A §A.2, ahead of localization and
  resolution. With `[pdqm]`, a patient named in a namespace the table lists
  is taken to a PDQm Patient Demographics Supplier first, by ITI-78 (an
  `identifier` search that asks for the master domain only) or, where the
  deployment declares `transaction = "iti-119"`, by the ITI-119 `$match`
  with `onlyCertainMatches`. The identifier the one matched Patient carries
  in the master domain is localized and resolved in place of the client's,
  and the outbound gate withholds both from every node request. No match
  leaves every member `not-resolved` and fails nothing (N6); several
  matches, two master identifiers or an uncertain match refuse the
  resolution and fail the query `424`, and the gateway never picks one. A
  Supplier outage follows the localization failure policy (§14.1). The
  Supplier is reached over `https` outside development, with no redirect,
  a bounded answer and its own timeout, which `OPTIONS {base}/` declares as
  `timeout.demographics_ms`; each exchange is audited through `[audit]` as
  the PDQm Query or Match Consumer record, required outside development;
  the step shows as `demographics` on `GET /health/dependencies` and is
  counted in `ferrofed_demographics_requests_total`. The read of an EHR by
  subject takes the same step, and a reload applies a change to `[pdqm]`
  as it does to `[pixm]` (#487).
- The ITI-119 Patient Demographics Match in `ihe-iti` 0.0.19:
  `PdqmClient::match_patient` posts a `MatchInput` to `[base]/Patient/$match`
  and reads the Match Output Bundle, every matched Patient with its score
  and `match-grade`, and its PDQm Match Consumer audit record (#487). The
  testkit gains a harness PDQm Supplier that answers ITI-78 and ITI-119 from
  synthetic Patients in the `urn:oid:2.999` example arc.
- The Nuts grant of the Dutch Generic Functions (Annex B §B.4) as an onward
  credential: a `[credentials."<id>".nuts]` section makes the gateway obtain
  that node's token from its authorization server with a Verifiable
  Presentation of the gateway's own credentials, signed with its `did:web`
  key, by the VP Token Grant Type of Nuts RFC021. The token is always bound
  to the section's `dpop_key_file` with DPoP, and every request to the node
  carries a proof of that key. The gateway checks the authorization server's
  metadata and Presentation Definition before any credential leaves, sends
  no expired credential, answers a demanded DPoP nonce once with a new
  presentation, caches the token until 30 seconds before it expires, and
  never logs or reports a credential, the presentation or the token. The
  client is `nl-generic-functions` feature `nuts-auth`, and the testkit has a
  harness Nuts node (#88).
- The specifications the Annex B authentication tracks cite are vendored:
  the GF-Authentication pages of the NL-GF IG, Nuts RFC003, RFC021 and
  RFC022, nine OAuth RFCs, VC Data Model 1.1, DID 1.0, DID Resolution,
  Bitstring Status List and the did:web method, and DIF Presentation
  Exchange 2.0.0. The OpenID FAPI 2.0, OpenID4VCI and OpenID4VP documents
  are pinned and fetched, never committed (#88).
- Mitz as the consent pre-filter of the Dutch binding (Annex B §B.6,
  N27a), configured under `[nl_gf.mitz]`. Before resolution the gateway asks
  Mitz's closed authorization question once for each candidate's care
  provider: may this data holder make the patient's data of the configured
  categories available to the verified caller, for `TREAT` or `COC`? The
  caller's professional (UZI number and role) and organisation (URA and
  type) come from the token claims each trusted issuer maps under
  `[auth.issuer.requester]`; a caller whose token lacks them is not asked
  about, and nothing is filtered for it. A member whose holder Mitz denies
  for every category is
  `consent-denied`, never asked, and clears `complete` while the query
  succeeds; every other member is asked and its node still checks consent
  (N27, §14.3). `Indeterminate`, a fault, a timeout or a malformed answer is
  no decision: those members are asked and the failure is carried in
  `meta.federation.consent.error`, and a pre-filter that denied some members
  and failed for others is counted as `partial` in
  `ferrofed_consent_prefilter_requests_total`. Mitz is asked by BSN, so a
  patient named by the pseudonymised BSN gets no signal. `OPTIONS {base}/`
  declares the pre-filter as `nl-gf-mitz`. The client is the new `mitz`
  feature of `nl-generic-functions` 0.0.8, which follows the VZVZ
  Implementatiehandleiding Open en gesloten autorisatievraag 3.8.2: SOAP 1.2
  with an XACML 3.0 decision query over mutual TLS. VZVZ states no licence
  for the document, so `scripts/vendor/mitz.sh` pins it by sha256 and
  fetches it into an ignored directory (#475).
- Every IHE transaction the gateway makes or receives is audited as its
  profile requires. The PIXm ITI-83 query, the mCSD ITI-90 search and
  ITI-91 history, the PMIR ITI-93 message received and each ITI-94
  subscription create, read and delete are recorded as their profile's BALP
  `AuditEvent` and, under the new `[audit]` table, posted to an ATNA Audit
  Record Repository over the FHIR Feed of ITI-20 (`destination =
  "repository"`) or written to the `ferrofed::audit` log target without a
  patient identifier (`"log"`). Each record is stored in a spool before it
  is delivered: a repository that is down delays the records, one that
  refuses a record has it quarantined, and a record the spool cannot store
  fails its transaction closed, as the ITI-55 audit does: an ITI-83 query
  fails `424`, a directory read is refused, an ITI-93 message is answered
  `503` unapplied. The repository shows on `GET /health/dependencies` as
  `audit_feed`, and the identity Registry's fault can be `audit-failed`.
  `ihe-iti` 0.0.18 adds the `balp` feature, with an audited mode for its
  PIXm, PDQm, mCSD and PMIR clients. BALP 1.1.4, the RESTful ATNA
  supplement, the ITI-20 page and each profile's audit profiles are vendored
  (#486, #469).
- The registry keeps the identifiers a care services directory publishes
  for each organisation (mCSD `Organization.identifier`), and the NVI
  localizer reads its custodian map from them: each member organisation's
  URA, by the LRZa rules of Annex B §B.2, maps to the members it operates,
  so a Dutch directory needs no `[nl_gf.nvi.custodians]` table. A table
  given beside it must agree, a member no URA maps to is refused, and a
  reload or directory refresh rebuilds the map. `nl-generic-functions` adds
  `lrza::ura_in`, and `ihe-iti` reads every identifier of a directory
  `Organization` (#87).
- `[nl_gf.nvi]` localizes undirected patient queries through the NVI, the
  national localization index of the Dutch Generic Functions (Annex B
  §B.1). The client names the patient by a pseudonymised BSN; the NVI's
  care providers, by URA, map to the members that hold their data, and
  every other member is `not-localized`. A patient named in any other
  namespace, an NVI that refuses, answers outside the IG or stays silent
  fails the query closed. The URL is `https` outside development, the
  section is refused beside `[xcpd]` and under `ask-all`, and
  `OPTIONS {base}/` declares `localization.mode = "nl-gf-nvi"`. The
  pseudonymised walkthrough of Annex B §B.7 runs end to end in the test
  harness, and no node receives the pseudonym (#87).
- The Dutch Generic Functions binding of Annex B starts in
  `nl-generic-functions`: feature `nvi` asks the national localization index
  which care providers, by URA, hold data for a patient named by a
  pseudonymised BSN, and refuses every answer that is not a `searchset` of
  localization records about that patient; feature `lrza` reads the URA of
  an NL-GF `Organization`. The source of the IG (`fhir.nl.gf` 0.3.0,
  EUPL-1.2) is vendored under `docs/specs/nl-gf/` by a new fetch script,
  pinned by tag and commit because the IG has no registry package (#87).
- The PMIR identity feed, `[pmir]`: the gateway subscribes at an IHE PMIR
  Patient Identity Registry with ITI-94 and serves `POST {base}{path}` for
  the ITI-93 messages it sends. A message is applied only with the bearer
  token agreed with the Registry out of band, read from `feed_token_file`,
  and only when it holds to the PMIR profiles; otherwise it is answered `401`,
  `400` or `415` and changes nothing. An applied merge or delete drops the
  resolution bindings of the `ehr_id`s it carries in a member's `ehr_id`
  domain, and an update, or a change that names no member `ehr_id`, drops
  every binding. The log and the new `ferrofed_identity_feed_messages_total`
  counter carry the change kinds, counts and results, never an identifier.
  `GET /health/dependencies` reports the Registry as `identity_registry`, and
  the subscription is found by search before it is created, so a create
  answered late is adopted and never made twice; one in `error` or `off`, or
  one the Registry lost, is replaced; failed checks back off up to 32 times
  `check_interval_s`; a create answered with no usable `Location` stops
  further creates and shows as `identity_registry_fault = "unmanageable"`;
  and the drain stops the loop before it deletes the subscription.
  The Registry URL and the callback URL are `https` outside the development
  profile. The `ihe-iti` crate gains the `pmir` feature: the ITI-94
  subscriber and the ITI-93 feed reader and response, held to the PMIR 1.6.0
  package, now vendored under `docs/specs/ihe-pmir/`; the testkit gains a
  harness Patient Identity Registry. Track 8 stays deferred (#147).
- `[auth.issuer.patient]` honours one issuer's `patient/` grants, off by
  default. It binds the issuer to the one member whose platform issues its
  patient tokens and names the identifier system under which the
  cross-reference knows that member's `ehr_id`s; `config check` refuses a
  binding whose endpoint the registry lacks, or that has no registry or no
  cross-reference. A bound issuer's `patient/` scope counts on an EHR's data
  alone. The gateway reads the token's `ehrId` claim (SMART on openEHR),
  resolves it at every member (§5.2), and sends a request only to the
  patient's own `{node, ehr_id}` pairs. Another patient, a query that names
  no patient, an `ehr_id` the pairs do not place, EHR creation and
  definitions are `403` (`patient-confinement`) with nothing sent, nothing
  probed and, for another patient's read by subject, no resolution binding
  or index entry recorded. A query whose `FROM` reads a class beside its
  one scoped `EHR` (under `AND` or `OR`, a second `EHR`, an `EHR` under
  `NOT CONTAINS`) is refused the same way, and a named patient is checked at
  the bound member alone before any localizer, consent pre-filter or other
  member hears of it. `openehr-federation` 0.0.38 adds
  `Analysis::within_one_ehr`, which says whether every class a query reads is
  contained under that one `EHR`. A token whose resource scopes are all `patient/`
  is refused the DEMOGRAPHIC API the same way, bound or not and even for a
  listed demographic client. A token without the claim is `403`
  (`patient-context-missing`), and a cross-reference that cannot resolve the
  patient is `424` (`patient-context-unavailable`). The bare `ehrId` is never
  compared across members (§12.5). Each node's `openEHR-federation-client`
  token carries the patient's `ehr_id` at that node as `ehrId`, and only the
  covering `patient/` scopes in `scope` (N26); the outbound gate refuses,
  with nothing sent, a request whose token would tell its node another
  `ehr_id` than the one the request is composed for. Without the section a
  `patient/` grant still admits nothing (#443).
- The texts client authentication is held to are vendored under
  `docs/specs/`. The ITS-REST Release-1.1.0 fetch now takes the SMART on
  openEHR source (`docs/smart_app_launch/`), recording that the release
  declares it DEVELOPMENT status. A new fetch script takes the IHE IUA
  supplement, Revision 2.5 Trial Implementation (ITI-71, ITI-72), which IHE
  publishes under CC-BY-4.0. The citations in the authentication code and
  the security handoff of the architecture point at the vendored text and
  name the DEVELOPMENT status, and a test holds every cited section,
  quotation and claim name to the pinned text (#414).
- `[telemetry] trace_sample_ratio` sets the share of client requests whose
  spans the trace export sends, from `0.0` to `1.0`, `1.0` unless set, and
  refused at load outside that range. The decision is made once at the
  request span, from the gateway's own trace id, and the whole span tree
  follows it; a node request of a trace not kept still carries a
  `traceparent`, with its sampled flag off. The export over the OTLP wire is
  tested end to end against an in-process OTLP collector in the testkit: a
  `serve` process's spans arrive under the gateway's resource, are flushed
  on the drain, and carry no synthetic patient identifier (#437).
- Onward token exchange per verified caller (RFC 8693, #439): an `oauth2`
  section with `grant = "token_exchange"` and a required `resource` (RFC
  8707) exchanges the caller's verified access token at the node's token
  endpoint, with an assertion of the gateway as the actor and only the
  caller's granted scopes that cover the operation (N26). Every exchange
  carries its scope and resource; an operation no scope covers, or a
  caller the edge mode asserted, fails that node `node-error` with nothing
  sent. Exchanged tokens are cached per endpoint, caller token and scope
  (the caller token hashed), at most 1024, and dropped on a `401`. The
  gateway's own requests use the client-credentials grant at the same
  token endpoint. The gate keeps the caller's token only while some node
  exchanges it, and no log, span, metric or `openEHR-federation-client`
  claim carries it.
- `DPoP`-bound onward tokens per endpoint (RFC 9449, #439): an `oauth2`
  section's `dpop_key_file`, a P-256 or P-384 key, binds that node's tokens
  to the key. Every request to the node and its token endpoint carries a
  proof binding its method and URL, and the token's hash on the node
  request; the token is sent under the `DPoP` scheme, a bearer token
  answered to such a grant fails the node, and a nonce either server
  demands is answered once.
- The gateway exports its own spans as OpenTelemetry traces over OTLP when
  `[telemetry] otlp_endpoint` names an `http://` collector; off by default.
  A federated query is one trace: the request, the resolution, the fan-out,
  the merge, and one `node_request` span per member asked, with the probe,
  the template, stored-query and drift fan-outs, the localizer and the
  consent pre-filter as spans of their own. Every span attribute is a route
  template, a registry id, an ITS-REST `operationId`, a status or a count,
  never a patient identifier, query text, header value or body, and no log
  event is exported. Every client request starts a trace of the gateway's
  own with a random trace id, and each node request carries a W3C
  `traceparent` from its own span in that trace. A client's `traceparent`
  and `tracestate` are never read, so no trace id a client chose reaches a
  node or an exported span (#353, #446).

- The resolution bindings of §12.5.1 step 2 are kept per verified caller,
  the token's issuer, subject and client together. A federated query and a
  read of an EHR by subject record where each `ehr_id` resolved, and a
  routed request by the same caller is routed by them before the `ehr_id`
  index; another caller never sees them. A consent denial drops the
  caller's bindings naming the denied member (N27a). They live in memory for
  `federation.binding_ttl_ms` and number at most `federation.binding_capacity`
  (100000 by default), and `OPTIONS {base}/` names the step in
  `its_rest.ehr` (#412).
- The registry can be read from an IHE mCSD 4.0.0 care services directory
  and kept in step with it (#86; §15.1, N21, Annex A.5). `[registry.mcsd]`
  names the directory's FHIR base, optional bearer or basic credentials, a
  refresh interval, a deadline for one whole read or refresh, and caps on
  its pages, bytes and entries (by default 30 seconds, 200 pages, 64 MiB
  and 50,000 entries), in place of `registry.document`.
  At start the gateway reads the `Organization`s and `Endpoint`s that carry
  the registry's FHIR-form identifiers with ITI-90, and every interval it
  asks for the changes since the last read with ITI-91 `_history?_since=`,
  from the directory's own clock. The content passes the checks of the FHIR
  form and of a reload: an endpoint relying on `hl7-fhir-rest` (§15.2, N19),
  a missing managing organisation (N20) or a duplicate id or `system_id`
  stops the start, and a refresh that breaks one is refused with the running
  registry kept, logged and counted as a refused reload. A refresh that runs
  past its deadline or a cap is refused and counted the same way
  (`registry-budget`), so a faulty or hostile directory can neither hold the
  gateway nor fill its memory, and a partial answer never becomes the
  registry. A directory that cannot be reached keeps the running registry,
  and every outcome shows on `GET /health/dependencies` as `directory`.
  `config check` reads the directory too. Credentials for the directory
  travel over `https` only, outside the development profile (#402).
  A member organisation of a shared directory may list endpoints that are no
  member: another service's, one that lost the federation's identifier, or
  one the directory deleted. Such a listing is ignored and logged by its
  reference, once per read of the content, at a start and on a refresh
  alike, where the document refuses it, and a deleted endpoint leaves the
  registry (#423, #433).
- `ihe-iti` 0.0.13: the `mcsd` feature carries the ITI-90 Query Client and
  the ITI-91 Update Client (`McsdClient`), every walk bounded by one
  `Budget` (a deadline and caps on pages, bytes and entries), and a replica
  of a directory's `Organization` and `Endpoint` resources kept in step
  with ITI-91 (`Replica`). IHE mCSD 4.0.0 is vendored under `docs/specs/ihe-mcsd/` by
  `scripts/vendor/ihe-mcsd.sh`, and the testkit carries a harness directory
  built from the IG's examples.
- The ITI-55 audit messages sent to an ATNA Audit Record Repository (#418;
  ITI TF-2 §3.20, §3.55.5.1.1): `[xcpd] audit = "repository"` with
  `[xcpd.audit_repository]` sends each exchange's DICOM PS3.15 audit
  message as ITI-20 Record Audit Event, RFC 5424 syslog with the PRI `<85>`
  and the MSGID `IHE+RFC-3881` over TLS (RFC 5425, `tls://`; `tcp://` under
  `profile = "development"` only). Every message is fsynced to a bounded
  spool before it counts as recorded and delivered from there in order, so a
  repository outage delays delivery and fails no discovery (§3.20.4.1.1),
  and a restart keeps the spool. A full or unwritable spool fails the
  discovery closed. The spool directory is created `0700` with `0600`
  files, and a directory open to other users refuses to start;
  `spool_dir` is required outside development, and `[xcpd] home_community`
  is required with `audit = "repository"`, since the audit message names
  the gateway's `homeCommunityID` (§3.55.5.1.1). Recording only writes to
  the spool. Delivery is bounded at every step (`connect_timeout_ms` for the
  connection, `send_timeout_ms` for the TLS handshake and each write and
  flush), and a failure drops the connection and retries with a jittered
  exponential backoff up to `retry_max_ms`. A spooled message that cannot be
  read or is no whole frame is moved to the spool's `quarantine/`, logged by
  its sequence number, and the drain goes on; a file the gateway did not
  write refuses the start, naming the file to move.
  `GET /health/dependencies` reports `audit_repository` (`degraded` while
  retrying, while messages wait and while any sits in quarantine), and the
  metrics `ferrofed_audit_spool_events`, `ferrofed_audit_spool_bytes`,
  `ferrofed_audit_quarantined`, `ferrofed_audit_delivered_total` and
  `ferrofed_audit_retries_total` carry no label. Where the audit messages go
  takes a restart.
- `ihe-iti` 0.0.14: the `atna` feature, the ITI-20 sender (the DICOM audit
  message, the RFC 5424 syslog frame, the RFC 5425 TLS connection, the
  spool and the forwarder), and `AuditEvent::message` for the ITI-55
  Initiating Gateway table. `AuditRecorder::record` is async, awaited before
  the exchange's answer is returned.
- The consent pre-filter applies to the read of an EHR by subject as it does
  to a federated query (#399; N27a, §13.2.1). A member it denies is never
  resolved and never sent a request. When it denies every member that might
  hold the subject and no other member holds one, the answer is
  `403 consent-denied` naming the denied endpoints, never the `404` of a
  subject with no EHR. A pre-filter that cannot answer leaves every member to
  its own consent check, as on a query.
- A consent pre-filter outage is visible (#400). The answer to a query asked
  while the pre-filter could not answer carries `meta.federation.consent.error`,
  beside the `localization.error` of §14.1, with `complete` and the status
  unchanged. `GET /health/dependencies` reports the pre-filter as `consent`
  under the members' rule (a decision or an answer below `500` is `up`, a
  `5xx` is `failing`, no answer is `down`), absent when none is configured.
  Each call is counted in `ferrofed_consent_prefilter_requests_total` by
  `outcome` (`denied`, `no-signal`, `unavailable`). The carriers are
  FerroFED's own design; no specification names one.

- `federation.node_selection = "localized"`: an undirected patient query gets
  its node set from a localizer (N4, N10, §14.1). A member the localizer does
  not name is reported `not-localized` and never asked. A localizer that fails
  or stays silent past `federation.localization.timeout_ms` fails closed: no
  member is asked, every member is `not-localized` with the localizer's error,
  and `meta.federation.localization.error` carries it too.
  `federation.localization.on_failure = "ask-all"` widens to every member
  instead, and `OPTIONS {base}/` declares the policy, the binding as
  `localization.mode` and the budget as `timeout.localization_ms`. A directed
  query is never localized (§8). The development cross-reference serves as
  the localizer under `profile = "development"`; the IHE XCPD binding follows
  (#85).
- Consent stays with the node, and a node's consent refusal can be reported
  as one (#83; §13.2, §13.2.1, §11.1, §11.3, N26, N27, N27a, N40, CP-36).
  ITS-REST defines no consent signal, so a node's `403` is `consent-denied`
  only when its ITS-REST `Error` carries a `code` the registry lists for that
  endpoint in the new `consent_refusal_codes` key (in the FHIR form, one
  `https://ferrofed.eu/fhir/StructureDefinition/consent-refusal-code`
  extension per code). The list is empty by default, so every refusal stays
  `node-error` until you name your nodes' codes; the key is FerroFED's own
  design. A consent-denied node contributes no rows, carries the latency of
  the request it refused, clears `meta.federation.complete`, and fails the
  query in neither completeness mode. The optional Step-1 consent
  pre-filter runs after localization and before resolution: each member it
  denies is reported `consent-denied` with no `latency_ms`, is never
  resolved or contacted, and loses any `ehr_id` the session cached for it. A
  member it does not deny is asked, and its node decides. When the pre-filter
  cannot answer, every candidate is asked (`pass-to-node`), because the node
  checks consent in any case. `OPTIONS {base}/` declares a configured
  pre-filter under `federation.consent`. For development,
  `[[dev.consent_denied]]` rows beside the cross-reference are a static
  pre-filter, accepted only under `profile = "development"` and declared as
  `development-static`.
- A "How it works" part of the book, right after the introduction, explains
  the gateway in Mermaid diagrams (#395; no specification governs the
  documentation: our own design). Its pages cover a federated query end to
  end with each member's status and the answer's status (§4, §11, N37), where
  the patient identifier stops (§5.4, N33), follow-ups and writes (§12, §12a,
  N41, N42), definitions and stored queries (§12.6, §12.7, N43, N44), trust
  and keys (§13.1, N25), and the quickstart and production layouts. Each
  diagram cites the sections it shows, and a planned part is drawn dashed
  with its issue and milestone. The pages of the other parts link to the
  diagram that belongs to them. The diagrams follow the book's theme when
  you choose "Auto" too: `website/book/js/mermaid-theme-sync.js` redraws them
  when the theme crosses between light and dark, which the vendored
  mdbook-mermaid script does only on a click of a named theme.
- The gateway authenticates to a node as itself with OAuth 2.0 client
  credentials and a signed JWT client assertion, the default onward mechanism
  of §13.1 (N25, CP-17 onward half, #81). A `[credentials."<endpoint
  id>".oauth2]` section names the node's token endpoint, the `client_id` and
  the scope, written in the SMART on openEHR grammar and held to the `system`
  compartment, with optional `resource` and `audience`. Each assertion is
  ES384, names the client as `iss` and `sub` and the token endpoint as `aud`,
  lives at most 300 seconds and carries a fresh `jti` (RFC 7523 §3). A token
  is cached until 30 seconds before it expires, with one token request per
  endpoint at a time, and a node's `401` drops it. The admission check
  authenticates the same way.
- `[signing]` holds the gateway's ES384 signing key, read from a file, and
  during a rotation the previous key, published beside it for a configurable
  overlap window of at least the assertion lifetime plus the nodes' JWK Set
  cache time. The gateway serves its public keys as a JWK Set (RFC 7517) at
  `GET {base}/.well-known/jwks.json`, each with its RFC 7638 thumbprint as
  `kid`, and declares the configured location as `federation.auth.jwks_uri` in
  `OPTIONS {base}/` (§13.1, N30).
- XCPD localization (#85, Annex A.3): `[xcpd]` makes the gateway an IHE XCPD
  Initiating Gateway (ITI-55, ITI TF-2 Revision 20.1). An undirected patient
  query asks every configured Responding Gateway by the patient identifier
  alone, and only the members whose communities a gateway names are asked.
  One gateway that faults, answers an error, asks for demographics or stays
  silent fails the discovery closed. Gateway URLs are `https` outside
  `profile = "development"`; a client certificate, extra trust roots and a
  SAML XUA assertion your identity provider signed are configured with
  `_file` keys, and the assertion is sent unchanged. `OPTIONS {base}/`
  declares `localization.mode = "xcpd"`, and `[xcpd]` takes effect on a
  reload.
- `ihe-iti` 0.0.11: the `xcpd` feature, the ITI-55 Initiating Gateway client.
  It is the only feature that compiles `uuid` and `jiff`, and it adds no
  crate to the graph beyond them.
- The read of an EHR by subject is localized (#409; N4, §5.2, §14.1): under
  `node_selection = "localized"` the subject is resolved only at the members
  the localizer names, and no other member learns of the request. A
  localizer that fails closed answers `424 localization-unavailable` with no
  member asked; a targeted read is never localized (§8).
- The localizer on `GET /health/dependencies` as `localizer`, under the
  members' rule, and its calls in `ferrofed_localizer_requests_total` by
  `outcome` (#410).
- The ITI-55 Initiating Gateway audit message (#410, ITI TF-2 §3.55.5.1.1):
  `ihe-iti` 0.0.12 hands every exchange's message to an `AuditRecorder`, and
  `[xcpd] audit = "log"` writes it at the `ferrofed::audit` log target
  without the query parameters, which name the patient. A message the
  recorder cannot accept fails the discovery closed under every
  `on_failure` policy, `ask-all` included, and shows the localizer
  `failing`.
- The PIXm resolver as the localizer (#408; §14.2, N4, §14.1): under
  `node_selection = "localized"` with `[pixm]` and no `[xcpd]`, the members
  whose domain holds an identifier for the patient at the PIX Manager are
  the candidates, and the resolution reuses the same ITI-83 answer, so each
  query asks each Manager once. A Manager that fails or stays silent fails
  the localization closed. `OPTIONS {base}/` declares
  `localization.mode = "pixm"`.
- Client authentication at the gateway (§13.1, N25, CP-17 inbound half,
  #80). A request to the ITS-REST surface and `OPTIONS {base}/` carries an
  RFC 9068 access token from an issuer on the `[[auth.issuer]]` trust list,
  verified against the issuer's JWK Set (fetched from `jwks_uri`, read from
  `jwks_file`, or given inline as `jwks`) or by RFC 7662 introspection. The
  key set is cached, refetched once for a key it does not hold and at most
  once per `auth.key_set_refetch_s`. Scopes are read in the SMART on openEHR
  grammar with `openehr-sdt`, and one table maps every ITS-REST operation to
  what it requires. A purpose of use, in the IHE IUA extension or RFC 9396
  `authorization_details`, is required unless
  `auth.purpose_of_use.required = false` (§13.4). The verified caller rides
  in the request's extensions. An explicit edge mode verifies a proxy's
  signed assertion in a configured header and logs the identity it asserted.
  New codes: `unauthenticated` (401), `scope-insufficient`,
  `purpose-of-use-required` and `operation-refused` (403), and
  `authentication-unavailable` (503). The book has a new page, Client
  authentication.
- `scripts/quickstart/token.sh` mints the token the quickstart gateway
  accepts, from a development issuer whose key pair it generates with
  `openssl` on its first run; the quickstart and the release example
  configuration carry an `[auth]` section.
- The testkit's test issuer: key pairs generated per run, its JWK Set served
  from a mock server, and tokens minted with chosen claims.
- The caller's identity on every request to a node (#82; §13.1, N24, N25,
  §12.4, CP-16). The federated query, every routed read and write, a
  definition request, the template and stored-query fan-outs, the ask-all
  probe and the read of an EHR by subject each carry
  `openEHR-federation-client`: an ES384 JWS, typed
  `openehr-federation-client+jwt`, signed for that one node with the
  gateway's `[signing]` key, the key its JWK Set publishes. Its `aud` is the
  node's endpoint id, `exp` is 60 seconds after `iat`, `jti` is fresh, and
  `iss` is the `client_id` of the endpoint's OAuth 2.0 grant or else the
  federation id. It names the verified caller (`sub`, `iss_upstream`), how
  the gateway verified it (`verified_by`: `signature`, `introspection` or
  `edge`), the caller's organisation (`subject_organization_id`), each
  purpose of use and the scopes as granted, so a node can apply them (N26).
  The admission check and the admin listener's redistribution name the
  gateway itself. The book's
  [Client authentication](https://ferrofed.eu/docs/operate/authentication.html#what-a-node-is-told-about-the-caller)
  page gives the claims and the steps a node verifies the token with. The
  header and its claims are FerroFED's own; §13.1 leaves end-user conveyance
  open.
- `scripts/quickstart/signing-key.sh` writes the compose quickstart's
  development signing key before the first `docker compose up`, and the
  quickstart and the release example configuration carry `[signing]`.
- The book's
  [§13.4 deployment decisions](https://ferrofed.eu/docs/operate/deployment-decisions.html)
  page (#84; §13.4, N25, CP-39): FerroFED's own answer to each of the five
  obligations §13.4 puts on a deployment (the identity verified at each
  boundary, who authenticates the end user, how purpose of use travels, what
  the tokens are bound to, and which risks are carried technically and which
  by agreement), the configuration that changes each answer, and a template
  an operator fills in for what only a deployment can answer. A test holds
  the page to one section per obligation.

### Changed

- Every outbound client of `ferrofed-identity` (PIXm, PDQm, PMIR, mCSD,
  XCPD, NVI, Mitz and the FHIR Feed audit repository) is built through one
  client build and one TLS type, and the server maps a credentials section
  to a credential once, refusing an OAuth 2.0, Nuts or FAPI 2.0 grant
  wherever one is not allowed; the PMIR mapping no longer reads such a grant
  as no credential (#507).
- The container image names FerroHEALTH, the organisation that distributes
  it, as its OCI `vendor`, and FerroHEALTH as its `authors`, in the image
  labels and in the release index annotations (#490).
- A production gateway with `[pixm]`, `[registry.mcsd]` or `[pmir]` set
  needs `[audit] destination`: `config check`, `serve` and a reload refuse
  the configuration without it, naming `audit.destination`, and refuse
  `"off"` outside development. Set `destination = "log"` to keep a
  deployment's behaviour while it adds an Audit Record Repository (#486).
- The quickstart's node database roles have new development passwords (#452):
  each role's password is its name followed by `_example`
  (`ferroehr_a_example`), set by `docker/postgres/20-ferrofed-node-databases.sh`
  and `compose.yaml` together. A quickstart volume created before this
  change keeps the old passwords, so the nodes are refused by their
  databases: run `docker compose down -v` once before the next
  `docker compose up --wait`, which removes the quickstart's data and its
  synthetic patients, then run `scripts/quickstart/seed.sh` again.
- The `openehr-*` family moves from 0.0.81 to 0.0.82 (FerroEHR #3557,
  #3558). An onward OAuth 2.0 grant now requests its configured `scope` in
  the canonical form of the SMART on openEHR grammar, as `openehr-sdt`
  prints it (#456): the permissions in `c`, `r`, `u`, `d`, `s` order and one
  space between scopes, so `system/aql-*.sr` is requested as
  `system/aql-*.rs`. A token exchange asks for the caller's covering scopes
  in the same form. An authorization server that compares scope strings
  sees the canonical form, not the text in the configuration or the
  caller's token.
- A `DPoP`-bound onward token is sent by the `openehr-its` client itself
  (#448): the client writes the `DPoP` scheme and asks the endpoint's key
  for each request's proof, and answers a node's `use_dpop_nonce` challenge
  with one re-send. The token request proves itself and answers a token
  endpoint's `use_dpop_nonce` the same way, with a newly signed client
  assertion, and under token exchange a new actor token, so no `jti`
  repeats (RFC 7523 §3). The token endpoint's nonce and the node's are kept
  apart even on one origin (RFC 9449 §9).
- A node that demanded a `DPoP` nonce and was not sent the request again
  is counted as asked (#470). A deadline that passes before the re-send is
  the node's `time-out`, "no answer before the deadline, which passed
  before the request could be sent again with the DPoP nonce the node
  demanded", and a re-send no proof could be made for is `node-error`, "the
  node demanded a DPoP nonce, and no proof could be made to send the
  request again". The health and metrics surfaces read both as a node that
  gave no answer, never as a request never sent. A request no proof could
  be made for at all is `node-error`, "no DPoP proof could be made for the
  request, so nothing was sent".
- The Kubernetes example bounds the gateway's ephemeral storage (#452): the
  container requests `128Mi` and is limited to `1Gi`, and the audit-spool
  `emptyDir` has a `sizeLimit` of `512Mi`, which holds the default spool
  bounds (`spool_max_bytes` 64 MiB, `spool_max_events` 100000). The kubelet
  evicts a pod past either limit, so raise both with the spool bounds.
- The image ships `/var/lib/ferrofed/audit-spool`, owned by the gateway
  user `65532:65532` with mode `0700` (#442), so the ITI-20 audit spool
  needs no step on the host. The release `compose.yaml` mounts the named
  volume `audit-spool` at `/var/lib/ferrofed`, and the Kubernetes example an
  `emptyDir` there, writable through `fsGroup` `65532`. Set
  `xcpd.audit_repository.spool_dir = "/var/lib/ferrofed/audit-spool"`; the
  spool holds audit records that name patients, so keep the volume on an
  encrypted disk.
- A `patient/` scope grant admits nothing at the gateway, now as a decision
  (#413): an `ehrId` names no namespace and an `ehr_id` means nothing outside
  its CDR (§12.5), so a bare match could admit another patient's EHR at
  another node. The authentication, How it works and deployment-decision
  pages say so, and an issuer-bound opt-in is planned (#443).
- `[signing]` is required whenever a registry is configured, by
  `registry.document` or by `[registry.mcsd]`: a federating gateway without a
  signing key refuses to start, and `config check` refuses
  its configuration with exit code 78, naming the key.
- The ADMIN API under `{base}/v1/admin/` answers `403` (`operation-refused`)
  to every caller, where it answered `501`.
- A gateway that federates and trusts no issuer refuses to start, and
  `config check` refuses its configuration, with exit code 78.
- `[xcpd]` requires `audit` (#410): `"log"`, or `"off"`, which only
  `profile = "development"` accepts and `OPTIONS {base}/` declares as
  `localization.audit`. A configuration without it refuses to boot.
- `ihe-iti` 0.0.12: `XcpdError::Fault` carries the HTTP status it came with,
  `XcpdError::status()` names the status the responding gateway answered
  with, an unreadable answer with a status other than `200` is `Rejected`,
  and `XcpdError::Audit` reports an audit message the recorder refused.
- An onward credential that cannot be obtained fails that node as
  `node-error` with nothing sent to it, on the federated query, the
  definition fan-out and a routed request (`424 node-error`), where it was a
  gateway error before (§13.1, §11.1).
- The localizer error a member and `meta.federation.localization.error`
  carry names the status the localization service answered once, with the
  binding's own reason, for PIXm and XCPD alike (#420). The log keeps the
  whole cause chain.
- The boot refusals that ask for a registry (`[dev]`, `[pixm]` and
  `federation.demographic_endpoint` without one, `federation.node_selection`
  and `federation.id` missing beside one) name both of its sources,
  `registry.document` and `[registry.mcsd]` (#425). So does the refusal of
  the admission check without a registry, and a registry in FHIR form that
  lists an endpoint it does not hold says the reference names no resource,
  where it said it named no `Organization` (#434).
- The book draws the v0.0.8 work on `main` as built (#406). The How it works
  pages show the gate, the onward OAuth 2.0 token with its ES384 assertion
  and cache, the signed caller on every node request, the XCPD and PIXm
  localizers, the mCSD directory and the resolution bindings per verified
  caller; only the ATNA audit repository (#418), PMIR (#147) and the Dutch
  Generic Functions (#87, #88) stay dashed as planned. The claims page lists
  the merged v0.0.8 items as on `main`, in the next release, and v0.0.7 as
  released. The README, `llms.txt` and the introduction say client
  authentication and onward authentication are built.

### Fixed

- The Mitz consent pre-filter's budget, `nl_gf.mitz.timeout_ms`, is now a
  part of the overall budget, as the localizer's and the PDQm step's are:
  `config check`, `serve` and a reload refuse a configuration whose
  pre-filter, PDQm and localizer budgets together reach
  `federation.overall_timeout_ms`, naming the keys, and `OPTIONS {base}/`
  declares the budget as `timeout.consent_ms` (§11.5, #538). Before, a long
  Mitz timeout could take the time left to resolve and ask the members with
  no refusal at startup. The PDQm check now also counts the localizer's
  default budget under `node_selection = "localized"` when
  `[federation.localization]` is not written, where it counted none.
- An XCPD discovery whose audit message is still being stored when the
  localization budget runs out now fails closed under
  `on_failure = "ask-all"` too, as every exchange the gateway could not
  audit does (#529). The gateway stopped waiting at the very instant the
  discovery's own bound fired, so it read the discovery as a localizer that
  did not answer, and ask-all widened the query to every member. The
  localizer and the `[pdqm]` step are now given a deadline a tenth of their
  time short of the gateway's, so the audit failure reaches the gateway
  first and leaves every member `not-localized`.
- An audit record still queued behind another spool write when its
  exchange's time runs out is now written once that write ends, and
  delivered once, in the order it was queued (#532). Before, the record of
  a failed exchange was dropped before its write started, and nothing
  logged it. A failed exchange is still audited, and ITI-20 has a stored
  record sent when the sender is able (ITI TF-2 §3.20.4.1.1). A queued
  record holds its place under `spool_max_events` and `spool_max_bytes`
  as a stored one does, so a stalled disk holds no more records in memory
  than the spool could hold on disk: a record past either bound is
  refused at once as a full spool, its exchange fails as an audit
  failure, and the refusal is logged with its count, never the record,
  and counted in the new `ferrofed_audit_refused_total`.
- A slow or stalled disk under the audit spool no longer holds a patient
  query past its budget (#512). Storing one record is bounded by the new
  `spool_write_timeout_ms` (2000 by default) in `[audit.repository]` and
  `[xcpd.audit_repository]`, and the record of an ITI-83, ITI-78, ITI-119
  or ITI-55 exchange also by the time left to that exchange. A record
  not stored in time is an audit failure under the policy already in
  place (`424` for the query, `503` for an ITI-93 message), and is not
  counted in the spool's depth. Its write goes on: a record it stores is
  delivered like any other, and a write that fails is logged, with the
  sequence number and never the record. One write runs at a time, so a
  stalled disk holds one thread, and the audit metrics and
  `GET /health/dependencies` read the spool's depth without waiting on the
  disk.
- The ITI-55 audit trail starts its delivery again when the runtime it was
  started on has ended, so audit messages recorded after that are delivered
  rather than left in the spool (#486).
- A `[[pixm.manager]]` URL that is not `https` outside
  `profile = "development"` is refused while the configuration loads, as an
  XCPD gateway URL already was, naming its key; before, only `serve`,
  `config check` and a reload caught it (#464).
- The example Kubernetes manifests start a gateway again (#428): the
  ConfigMap trusts an example issuer in `[auth]` and names the gateway's
  signing key in `[signing]`, read from the `ferrofed-secrets` Secret the
  Deployment now mounts at `/run/secrets/ferrofed`. CI runs
  `ferrofed config check` over the ConfigMap's configuration with synthetic
  secrets (`scripts/checks/kubernetes-example.sh`), so the example cannot
  drift from what the gateway accepts.
- A refresh of the mCSD directory whose change the gateway refuses no longer
  reports the directory `up` on `GET /health/dependencies` while the gateway
  serves the registry it last accepted (#481). The directory shows
  `degraded`, with `directory_fault` naming the class of the refusal
  (`registry-invalid` or `configuration-mismatch`) and never a member or a
  URL, until a later refresh is accepted. A directory that did not answer
  still shows `down`, and one whose answer is broken `failing`. A directory
  that answers a refresh with a `4xx` shows `failing` where it showed `up`,
  and a `401` or `403` names `directory_fault = "refused-credentials"`.

## [0.0.7] - 2026-10-03

The v0.0.7 milestone, definitions and membership (§12.6, §12.7, §12b). A
versioned write goes only to the CDR that created the version. A template goes
to the node you name, or to several on request, with each member's outcome
reported. Stored queries live at the gateway as immutable versions, on an
embedded store, PostgreSQL or a read-only directory, distributed to the
members with drift reported and repaired from the admin listener. A check you
run before admitting a node tests the admission conditions. The gateway
exports metrics through OpenTelemetry, as Prometheus and over OTLP, records
each member's state from the calls it makes, holds every credential in a type
that never renders it, and prints a startup banner. Every release now carries
a compose file that starts the gateway alone in front of the CDRs you already
run. The gateway still authenticates no client; that is v0.0.8 (#80).

### Added

- Every release carries `compose.yaml`, `ferrofed.toml` and `registry.toml`,
  which start the gateway alone in front of the CDRs you already run, with no
  checkout of the repository (#385; no specification governs packaging: our
  own design). You download the three, edit the two TOML files (the gateway
  configuration and the registry document, in their documented formats, every
  value to change marked `EDIT`), put each credential in a file under
  `secrets/`, and run `docker compose up --wait`. The compose file runs the
  published `ghcr.io/ferrohealth/ferrofed` image at the release's version and
  mounts the two files and `secrets/` read-only; its only variables are about
  the container (image tag, host address and port, CPU and memory). The
  gateway runs read-only, as uid 65532, with every capability dropped,
  `no-new-privileges`, CPU and memory limits, a 20-second stop grace period,
  the `ferrofed healthcheck` probe and the `unless-stopped` restart policy, so
  a configuration it refuses (exit 78, naming the key) is retried at Docker's
  doubling delay. The release lane attaches the three files to the draft and
  refuses to publish a draft missing any of them, `scripts/checks/versions.sh`
  and the release plan hold the image tag to the product version, and the CI
  job `release compose` renders the compose file and runs `ferrofed config
  check` over the two examples as attached. The repository's `compose.yaml`
  stays the four-node quickstart. The container page of the book has a
  section "The gateway from a release".
- The operator can send a stored-query version the registry holds to the
  members that missed it (#342; §12.7 stored-query-drift, N44, CP-40; no
  specification gives drift repair a request, so this is our own design).
  `POST /admin/stored-queries/{name}/{version}/distribute` on the admin
  listener (`[metrics] listen`), with no body and the members in the
  targeting headers, sends the registry's held copy to them and changes
  nothing at the registry. The answer has the shape and statuses of a first
  distribution, and its new `meta.registry` member says `held`, where a
  first distribution's says `stored`. It is `404` for a version the
  registry does not hold, `400` for a request naming no member, a body, a
  deployment without distribution or an endpoint-targeted definition, and
  `405` with an empty `Allow` at a read-only registry. Without the admin
  listener the action does not exist. Every second `PUT` of a held version
  stays `409` (`stored-query-held`), the same query naming members
  included. The book's stored-query page has a "Repairing drift" section.
- A metrics surface (#281; no specification governs metrics). One
  OpenTelemetry meter provider counts the integrity incidents by `kind`
  (`ferrofed_integrity_incidents_total`), the requests sent to each member
  endpoint by the §11.1 outcome the per-endpoint report gives them
  (`ferrofed_node_requests_total{endpoint, outcome}`), how long each member
  took to answer (`ferrofed_node_request_duration_seconds{endpoint}`), and
  the registry reloads by `result`, `applied` or `refused`
  (`ferrofed_registry_reloads_total`). `[metrics] listen` serves the
  Prometheus text exposition at `GET /metrics` on an admin listener of its
  own, never the gateway's listener, and `[metrics] otlp_endpoint` pushes the
  same metrics to an OTLP gRPC collector; both are off by default. `serve`
  and `config check` refuse a listener on a non-loopback address unless
  `[metrics] allow_remote = true` is set, a listener on `server.listen`, and
  a collector that is no `http://` URL. Every label value is drawn from a
  closed set or the registry document, never from a request (§5.4.1, N33).
  The book's operate section has a Metrics page listing every metric and its
  labels. The integrity incident webhook the architecture named is removed
  from the design: alert on the counter.
- The stored-query registry runs on one of three backends, chosen with
  `[stored_queries] backend` (#268; §12.7, N44, CP-40). `redb`, the default,
  is the embedded file for one gateway, as before. `postgres`, behind the
  `ferrofed-server` cargo feature of the same name and off by default, keeps
  the definitions in one PostgreSQL database every replica shares: the
  primary key on the name and version with `INSERT … ON CONFLICT DO NOTHING`
  stores exactly one of two racing `PUT`s of a new version, the other is a
  `409` (`stored-query-held`), and a read or an invocation reads the store
  again first, so every replica serves what any of them stored. Its
  connection string is a secret, `url` or `url_file`, TLS is rustls with the
  platform's roots, and the schema `ferrofed` and its table are created when
  absent. `files` is read-only: one file per
  `{qualified_query_name}/{version}.aql`, loaded and admitted at start; a
  `PUT` is a `405` with the new code `stored-query-read-only` and `Allow:
  GET, OPTIONS`, `OPTIONS` lists no `PUT`, and a malformed file refuses the
  start and `config check`, naming the file and never its content. The
  startup banner and log line name the backend kind and never its path or
  connection string.

- `PUT {base}/v1/ehr/{ehr_id}` is checked against what the gateway already
  knows of the `ehr_id` before anything is sent (#289; §12.4, §12.5.2, N23,
  N42, CP-15; ITS-REST 1.1.0 `ehr_create_with_id`). When a resolution
  binding of the session or the `ehr_id` index places the `ehr_id` at a
  member other than the one the targeting headers name, the create is a
  `409` with the new code `ehr-id-held`, no node is sent it, and the
  gateway logs a warning naming endpoint ids only; the message names the
  holding endpoints and the targeted one, never the `ehr_id`. A create at
  the member that holds the `ehr_id` is forwarded, so that node answers its
  own `409`, and an `ehr_id` nothing places is forwarded as before and,
  once created, indexed at the targeted member, where a racing create at
  another member raises the index-insert alarm of §12b.2.
- Stored-query definition fan-out, opt-in with `[federation]
  fan_out_stored_queries` beside the registry (#78; §12.7, N44, N43, CP-40).
  Off by default, refused by `config check` and at start without
  `[stored_queries]`, and declared in `OPTIONS {base}/` as
  `definition.stored_query_fan_out`, `true` only beside the registry. Where
  it is on, a stored-query `PUT` whose `openEHR-federation-endpoint` is `*`
  or whose targeting headers name members is stored at the registry first,
  then sent to each member as the registry's canonical AQL through
  `openehr-its`'s generated client, and answered with the registry's
  `StoredQuery` and `meta.federation` on the template fan-out's statuses:
  `200`, `207` with `complete` false, or `424` and `504`, the registry
  holding the definition whatever the members answered and nothing rolled
  back. A definition carrying a `FROM ENDPOINT` or `ORGANISATION` directive
  is refused for distribution with `400 definition-endpoint-targeted`
  before anything is stored, and stays storable and executable federated.
  A `GET` of a version naming members reports drift per member: `active`
  where its copy is the same query, and `node-error` with
  `error.code` `definition-differs` or `definition-missing` otherwise, `207`
  unless every named member matches. An invocation always runs the
  registry's AQL, never a member's copy. With the setting off, a registry
  `PUT` or version `GET` carrying `openEHR-federation-endpoint` or
  `openEHR-federation-organisation` is refused with
  `400 stored-query-fan-out-unsupported`, storing and reading nothing,
  where the header was ignored before.
- The federated AQL answer carries the provenance headers (#288; §7a.3,
  N31, CP-24). `openEHR-federation-endpoint` lists the endpoints that
  contributed rows and `openEHR-federation-system-id` their nodes'
  `system_id`s, position for position, in registry order and in the
  comma-separated form of the §8.4 request header. An endpoint contributed
  when it answered `active` with a `row_count` above 0; one that answered no
  rows, failed or was never asked is not listed, an answer that fails lists
  none, and `meta.federation.endpoints[]` still names every endpoint (§11.1).
  The headers carry registry ids and `system_id`s, never a request value
  (§5.4.1).
- Fan-out template upload, opt-in with `[federation] fan_out_template_upload`
  (#76; §12.6, N43, CP-34). Off by default, and `OPTIONS {base}/` declares
  the configured value as `definition.fan_out_template_upload`. Where it is
  on, an ADL 1.4 or ADL 2 template upload whose `openEHR-federation-endpoint`
  is `*` (every active member) or whose targeting headers select several
  endpoints is sent to each member independently, byte-identical through the
  routed forwarding path, and nothing is rolled back. The answer is
  `meta.federation` with one `endpoints[]` entry per registry member and no
  node's body: `200` when every named member accepted, `207` with
  `complete: false` when some failed, and `424` or `504` when none accepted.
  The provenance headers list the members that accepted. A plain upload still
  names its one node, and `*` stays `endpoint-unknown` on every other
  definition request and whenever the setting is off.
- `ferrofed serve` opens with a startup banner when its log renders for a
  person (#330): the FerroFED wordmark, the version, the maintainer and the
  repository, the Federation Tier, ITS-REST, AQL and `openehr-*` releases it
  serves, then the base path, the listen address, the member and endpoint
  counts of the registry document, and whether the stored-query registry is
  offered. The document is read once, before the banner, and the gateway is
  built over that same read, so the banner counts the registry it serves.
  Under `profile = "development"` a red notice, in plain words where
  there is no colour, says the deployment must not hold or reach real
  patient data. The banner prints only before the `pretty` rendering, so a
  log collector reading JSON never receives it, and `config check`,
  `healthcheck` and `admission check` print none. It never shows a
  credential, a URL, a header value or request data. Each pin comes from the
  crate constant `scripts/checks/versions.sh` holds to `docs/VERSIONS.md`,
  and the `openehr-*` family version is a new constant the guard checks.
- The console honours `NO_COLOR` (#330, <https://no-color.org>): set and not
  empty, it switches colour off in the `pretty` log and the startup banner,
  whatever the format and the terminal.
- The ITS-REST `GET` forms of query execution are served (#287; N1, CP-1).
  `GET {base}/v1/query/aql` and, where the stored-query registry is offered,
  `GET {base}/v1/query/{name}[/{version}]` carry the request in the query
  string: `q` (ad hoc only), `offset` and `fetch` by name, and every other pair
  a `query_parameters` member, so a stored `GET` binds `q=…` to `$q`. The
  generated `openehr-its` parameters decode it (FerroEHR #3540), a `+` is a
  literal plus, and the request runs the pipeline of the `POST` form
  unchanged: the rewrite, the outbound gate, the fan-out, the merge and the
  completeness rules, with the same node requests and the same answer. An
  `ehr_id` is dropped, as from a `POST` body. A query string the decoder
  refuses is `400 body-invalid` and asks nobody; a `null` parameter is
  `400 parameter-invalid`, as in a body. `OPTIONS` on both paths lists `GET`,
  and the request log records neither `q` nor a parameter value.
- The DEMOGRAPHIC area is never federated, and may be routed to one
  declared endpoint that the request names (#68, #311; §7a.1, §12.4, §12.6,
  N23, N32, N31, CP-25). By default every request under
  `{base}/v1/demographic/` answers `501` and no node is asked. The new
  `[federation] demographic_endpoint` setting declares the one registry
  endpoint that serves the area. A DEMOGRAPHIC request names that endpoint
  in `openEHR-federation-endpoint` and then goes to it alone, through the
  single-node path definition requests use: the body byte-identical, only
  what the ITS-REST operation declares, and the node's answer as the node
  sent it with `openEHR-federation-endpoint` and
  `openEHR-federation-system-id`. The setting is never applied as a default:
  a request naming no endpoint is `400 target-required` and no node is
  asked, one naming another endpoint is `400 targeting-conflict`, and
  several endpoints, `*` or an unknown id are refused as on any routed
  request. `config check` refuses an id the registry does not hold, and the
  setting without `registry.document`. `OPTIONS {base}/` declares
  `its_rest.demographic` as `unsupported: 501`, or as `routed-single-node`
  naming the endpoint a request names, and `OPTIONS` on a DEMOGRAPHIC path
  names its ITS-REST methods only when the area is routed.
- Definition requests are routed to one explicitly chosen node (#75; §7a.1,
  §12.6, §12.7, N43, N31, N33, CP-34). Every request under
  `{base}/v1/definition/`, an ADL 1.4 or ADL 2 template upload, list, read or
  example, and stored-query management where the stored-query registry is not
  offered, goes only to the one endpoint the targeting headers name. The body
  reaches that node byte-identical with only what the ITS-REST operation
  declares, and the node's answer, its `404` or validation `400` included,
  comes back as the node sent it with `openEHR-federation-endpoint` and
  `openEHR-federation-system-id`. Without a header the request is
  `400 target-required`; `*` or an unknown id is `400 endpoint-unknown` and
  two endpoints are `400 endpoint-several`, so no node is ever picked
  implicitly and no two nodes' templates are combined into one catalogue.
  Where the registry is offered it keeps answering stored-query definitions
  itself. Without the registry both stored-query `PUT`s are routed, the
  versioned `PUT {base}/v1/definition/query/{name}/{version}` included
  (#298), and `OPTIONS` on either path lists `PUT`.
  `OPTIONS {base}/` declares `its_rest.definition` as `routed-single-node`,
  and `OPTIONS` on a definition path names its ITS-REST methods.
- `ferrofed admission check --endpoint <id>` exercises one configured
  member against the identifier-integrity conditions of §12b.2 and writes a
  report to standard output (#79; §12b.1, §12b.2, N42a, CP-33a). It creates
  test EHRs on the node (3 by default, `--count` from 2 to 50) through
  `POST {base}/v1/ehr`, each for a fresh synthetic subject in
  `urn:oid:2.999.1.0`, reads each back, and reports every condition as
  `pass`, `fail` or `cannot-check` with its evidence: `ehr_id` generation
  (a version-4 UUID, and no `ehr_id` issued twice), `system_id` uniqueness
  (the `system_id` the node reports is the one the registry records, which
  the registry holds unique), and the `ehr_id` exchange (the configured
  cross-reference maps each subject to the `ehr_id` the node created, or
  `cannot-check` where the gateway cannot write to it). Reuse across
  restores and adoption of foreign `ehr_id`s are always `cannot-check`, with
  the reason. A node the check cannot reach fails every condition it
  exercises. Every request passes the outbound gate with the run's subjects
  withheld (§5.4.1, N33), and the report prints no subject and no node error
  body. The command exits `0` when no condition failed, `1` when one did.
  The book's new page "Admitting a node" states what the check proves and
  what it cannot.
- The registry reloads on `SIGHUP` (#282). `ferrofed serve` reads its
  configuration again from where it started and checks it as at boot; a
  valid one replaces the registry for every request that starts after it,
  while a request in flight finishes on the registry it took. The
  `[registry]`, `[credentials]`, `[dev]` and `[pixm]` sections take effect: an
  added endpoint gets its node client and credentials, and a removed one is
  never called again. A learned `creating_system_id` route the new document
  maps elsewhere is withdrawn with a `RegisteredCreatingSystemConflict`
  incident, and `ehr_id` index entries and resolution bindings naming a
  member that left are dropped; one a request in flight learns after the
  swap is dropped at its next lookup, and a read then probes every member
  while a write needs its target, so an entry is never narrowed to the
  claimant that remains (§12.5.2, N42). A change to any other setting is logged as
  needing a restart while the rest applies. A configuration that does not
  load is refused, the running registry stays, and the `ERROR` line names
  the failure class and the files, never their content. `config check` is
  unchanged. There is no file watch.
- `GET {base}/v1/ehr?subject_id=…&subject_namespace=…`, the ITS-REST read of
  an EHR by subject, is served (#218; §5.2, §5.4.1, §12.5.2, N3, N31, N33,
  CP-24, CP-26). The two parameters are resolution input: the gateway
  resolves the subject through the cross-reference service and sends
  `GET {base}/v1/ehr/{ehr_id}` to the one member that holds it, under that
  member's own `ehr_id`, with no query string, and returns the node's answer
  unmodified with `openEHR-federation-endpoint` and
  `openEHR-federation-system-id`. No node request, error body or log line
  carries the subject. A subject several members hold is refused with the new
  code `409 subject-several` listing the endpoints, unless
  `openEHR-federation-endpoint` names one of them; a subject no member holds
  is the operation's own `404 no-destination`; a cross-reference that cannot
  answer is the new `424 resolution-unavailable`. `OPTIONS {base}/v1/ehr`
  now lists `GET`.
- The gateway serves at a base path of the deployment's choosing (#69;
  §4.1, N28, CP-21). `[server] base_path`, `/` by default, puts every route
  under it, `OPTIONS {base}/`, the health family and `{base}/v1/…` included,
  and every path outside it answers `404`. No prefix is reserved, and
  `/rest/openehr` is served only where a deployment chooses it. A base path
  that does not start with `/`, ends with `/`, carries a query or a fragment,
  or has an empty, `.` or `..` segment refuses to boot, naming
  `server.base_path`. Nodes are still asked at their own base, and a node's
  `Location` still passes through unmodified (N31).
- Both AQL forms of N29 scope a query to one `ehr_id` (#69; §7.1, N29,
  CP-22). `FROM EHR e[ehr_id/value='…']` reaches a node as the canonical
  `WHERE e/ehr_id/value = '…'`, so both forms send the same query and answer
  the same rows. An undirected query scoped to one `ehr_id` goes only to the
  member that owns it, found as a path `ehr_id` is (§12.5.1, N41): the
  session's binding, the `ehr_id` index, then the ask-all probe. Every other
  member is reported `excluded`, the answer names the acting endpoint in
  `openEHR-federation-endpoint` (N31), two claimants are `409
  ehr-id-collision` with neither queried (N42), an `ehr_id` no member holds is
  `404 no-destination`, and one that is not a bare UUID and no earlier step
  routes is `400 probe-requires-uuid` with no member asked (§5.4.1, N33).
  A query directed by `FROM ENDPOINT` or a targeting header still goes to the
  endpoints it names.
- Versioned writes reach only their controlling CDR, and a new EHR only an
  explicit target (#65; §12.4, §12a.1, §10.3, N23, N41, CP-15). An update of
  a composition, the `EHR_STATUS` or the directory, a directory delete (each
  naming its preceding version in `If-Match`) and a composition delete
  (naming it in the path) route by the path `ehr_id` as before, and are sent
  only when the registry maps the preceding version's `creating_system_id`
  to that same node, as the member's own `system_id` or a `[[creating_system]]`
  mapping; a learned mapping never counts. Otherwise the write is refused
  `409 controlling-system-unreachable`, naming the controlling node where the
  registry knows it, and no node is sent it. A versioned write that names no
  single preceding version (no `If-Match`, several, `*`, a weak or unquoted
  tag, or no `OBJECT_VERSION_ID`) is refused with the new code
  `400 preceding-version-invalid`. `POST {base}/v1/ehr` is now served, and it
  and `PUT {base}/v1/ehr/{ehr_id}` go only to the one endpoint the targeting
  headers name: without them the request is `400 target-required`, and with
  several endpoints `400 endpoint-several`; neither a binding nor the index
  routes a new EHR. `OPTIONS {base}/v1/ehr` answers `POST, OPTIONS`.
- A `CONTRIBUTION` that amends versions reaches only the CDR that controls
  every one of them (#286; §12.4, §12a.1, §10.3, N23, CP-15).
  `POST {base}/v1/ehr/{ehr_id}/contribution` routes by its path `ehr_id` as
  before, and the gateway now reads the body with the ITS-REST
  `NewContribution` type: each version's `preceding_version_uid` must be one
  the registry maps to that node, by the rule above, or the request is
  refused `409 controlling-system-unreachable` and no node is sent it. A
  `CONTRIBUTION` of creations alone routes as before. A body that is no
  `CONTRIBUTION` in the representation its `Content-Type` selects, a
  `preceding_version_uid` that is no `OBJECT_VERSION_ID`, or an XML body, is
  refused `400 preceding-version-invalid`. The node receives the body byte
  for byte.
- A `CONTRIBUTION` whose versions' `data` is FLAT or STRUCTURED is routed
  as a canonical-JSON one is (#297; §12.4, §12a.1, N23, CP-15). Under
  `Content-Type: application/openehr.wt.flat+json` or
  `application/openehr.wt.structured+json` the envelope stays canonical
  (ITS-REST 1.1.0 `contribution_create`), and the gateway reads it with the
  generic `NewContribution` of `openehr-its` 0.0.80 for each version's
  `preceding_version_uid`, leaving `data` to the node. A `CONTRIBUTION` of
  creations alone goes by its path `ehr_id`, one whose every amended version
  the path node controls reaches that node, and one with a version another
  member controls, or no member is known to, is
  `409 controlling-system-unreachable` and no node is sent it. The node
  receives the body byte for byte, under the declared media type. A
  `CONTRIBUTION` in canonical XML is still refused
  `400 preceding-version-invalid` (#308), now with a message that names the
  XML form.
- Integrity incidents for an `ehr_id` two members claim (#63; §12.5.2,
  §12b.2, N42, CP-33). A request whose `ehr_id` the session's resolution
  bindings, the `ehr_id` index or the ask-all probe finds at two members or
  more is refused `409 ehr-id-collision` listing the claiming endpoints, and
  the gateway logs one `EhrIdCollision` incident: an `ERROR` line under the
  new log target `ferrofed::integrity` with the `ehr_id`, the routing step
  that found the claimants (`detection`) and the claimants' endpoint ids.
  When the index learns an `ehr_id` it already holds at another member, it
  raises the index-insert alarm of §12b.2 once, an `IndexInsertCollision`
  incident naming the members, keeps both and routes neither; a held
  collision lasts until the entry is forgotten or the gateway restarts. An
  incident names an `ehr_id` only when it is a bare UUID, and never a patient
  identifier. The book's configuration page says what an operator sees and
  does, and the metrics surface (#281) counts each incident by `kind`.
- The obligations checklist (#278): `conformance/obligations.tsv` holds one
  row per normative statement of the pinned Federation Tier specification,
  447 across its 26 pages and both JSON schemas, each with the status
  FerroFED holds for it and the test, code, issue or #212 report behind that
  status. 273 are tested, 52 planned, 3 built but untested (#290), 3 missing
  (the GET forms of query execution, #287, and the provenance headers on the
  federated AQL answer, #288), 5 deferred, and 18 are contradictions or
  silences of the text recorded on #212; the other 93 fall on a member node
  or the operator, or are no obligation of the gateway. The book renders the counts and every gap on a new
  page beside the conformance matrix. A new tier-1 guard,
  `scripts/checks/obligations.sh`, fails on an unknown status, a gap that
  names no issue, a test that does not exist, a point or requirement the
  matrix does not hold, a duplicated row, or a stale page, and it holds a
  digest of each vendored page's keyword lines, so a re-pin fails until the
  changed pages are reclassified. The comment-style guard now reads the
  `evidence` cells of the conformance tables as it reads their `reason`
  cells.
- Container readiness (#303). `ferrofed healthcheck` asks the gateway on this
  host for `GET /health/readiness` over loopback, prints one line and exits
  `0` only on `200`, `1` otherwise; the image carries
  `HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3`
  over it, and the `compose.yaml` gateway service the same healthcheck.
  Readiness answers `503` until boot completes and from the moment `SIGTERM`
  or `SIGINT` arrives, before the drain starts, and its body names the phase
  and the gateway's own subsystems (the configuration, the registry, the
  outbound clients, the stored-query store). No member node and no identity
  source gates readiness: the new `GET /health/dependencies` always answers
  `200` with the state the gateway last observed of each member endpoint and
  of the resolver (`up`, `failing`, `down`, `unknown`), from the requests it
  already makes, never a probe of its own, naming endpoint ids and states
  only. `deploy/kubernetes/` holds an example ConfigMap, Deployment, Service
  and PodDisruptionBudget, validated in CI by a digest-pinned `kubeconform`
  in strict mode. No specification governs health probes: our own design.
- The supported platforms are stated (#307): FerroFED runs on Unix only.
  The server drains on `SIGTERM` and reloads on `SIGHUP` through Unix
  signals, and every release binary and the container image are Linux. The
  book says so under Operate, and building `ferrofed-server` for a non-Unix
  target, such as Windows, stops at a `compile_error!` that names the reason.
  No specification governs this: our own design.

### Changed

- CI checks every internal link and anchor of the published site (#377; no
  specification governs this: our own design). The new tier-1 job
  `site-links`, in the required `conclusion`, assembles the site as the Docs
  workflow publishes it and runs lychee 0.24.2, pinned by version and
  SHA-256, with `--offline --include-fragments` over every page, and over
  `README.md` against the repository tree, so a link to a page or an anchor
  that does not exist fails the change and no request leaves the runner.
  `scripts/checks/site-links.sh` runs the same check locally, with a
  self-test, and `scripts/site/assemble.sh` takes `SITE_ROADMAP=off` to leave
  the roadmap block unrendered. `docs/VERSIONS.md` pins lychee, and the
  versions and pin-freshness guards hold the pin.
- The errors page names the requests that answer `501 not-implemented`
  (#378; §7a.1, N32): every path under `{base}/v1/`, and `OPTIONS {base}/`,
  without a registry; the DEMOGRAPHIC API unless its endpoint is declared;
  the ADMIN API; stored-query execution without the stored-query registry; a
  path ITS-REST does not define or a method it does not declare; and
  `OPTIONS` on an unserved path. The errors test now sends each of them.
- The quickstart's `compose.yaml` runs only the published
  `ghcr.io/ferrohealth/ferrofed` image: its `build:` block is gone, so
  `docker compose up --build` no longer builds the gateway, and
  `scripts/checks/release-compose.sh` fails when any compose file in the
  repository carries `build:` (#385; no specification governs packaging: our
  own design). The gateway service now has a 20-second stop grace period,
  longer than its 10-second drain, so `docker compose down` no longer cuts a
  drain off at Docker's default 10 seconds.
- The README, the landing page and the book are rewritten against what the
  gateway does on `main` (#363; no specification governs the website). The
  book's claims page lists what v0.0.3, v0.0.6 and v0.0.7 carry and what
  v0.0.8 and v0.0.9 plan, by issue; the operate pages state that the gateway
  authenticates no client yet and which onward credentials it sends, that no
  session-scoped resolution binding is held until client authentication
  (#80), and that a node's consent refusal is reported `node-error` until
  #83. Two pages are new: Identity resolution, covering `[pixm]`, `[dev]`,
  `profile` and `federation.default_namespace`, and Health probes, moved out
  of the container page. `Prefer: wait` is documented beside the budgets.
  The landing page's quickstart runs the four-node compose stack with its
  seed script, and its cards name what each release shipped.
- The ask-all probe is timed (#366; no specification governs metrics): each
  member's probe now carries its latency, so
  `ferrofed_node_request_duration_seconds` times it beside its count in
  `ferrofed_node_requests_total`, and a probe still waiting when the overall
  budget runs out is a `time-out` timed to that moment. The book's Metrics
  page lists which calls each `outcome` and the histogram cover.
- The book's client contract and configuration pages are split along their
  own sections (#348): the client contract continues on Follow-ups,
  Templates, definitions and demographics, and Stored queries, and the
  configuration on The registry, and Queries and API areas. The file-length
  guard now holds the book's Markdown pages to the same limits as the Rust
  sources.
- A node's error in `meta.federation.endpoints[]` follows one rule on every
  path that reports a member: a federated query, the fan-out template upload
  and the stored-query distribution and drift check (#343; §9.5, §11.1,
  §12.6 item 2, N40). A `node-error` carries the node's HTTP status, then
  an excerpt of the node's own message: at most 512 characters, control
  characters and invisible format marks turned into spaces, and the patient
  identifier the query resolved on replaced by `[withheld]` wherever the
  node echoed it, the whole message when the node echoed it percent-encoded
  (§5.4.1, N33). The HTTP client's reason an `offline` or `time-out` node
  was not reached is held to the same rule. The two fan-outs carried the
  node's status alone before,
  and a query copied the node's message with no cleaning and no masking.
- A recombined `AVG` over integers answers an integer (#309; AQL 1.1.0
  §3.9.1.5, "it will also determine the return type"). When every node's
  `SUM` is an integer, the gateway returns the integer nearest the exact
  quotient of the federation's sum and count, a tie going to the even one,
  so `5 / 2` is `2` and `7 / 2` is `4`; it rounds once, after adding every
  node's sum and count, never per node. AQL states no rounding rule, so the
  rounding is FerroFED's own. When a node's `SUM` is a real, `AVG` is the
  decimal mean written as the nearest JSON number, as before.
  `openehr-federation` is 0.0.33.
- The `openehr-*` family moves to 0.0.81 (FerroEHR #3548, #3551, #3552), and
  `openehr-federation` to 0.0.35 with it (#329).
- A body routed to one node without a `Content-Type` travels with the first
  media type the operation lists, `application/json` wherever ITS-REST lists
  several, where it was a `415` for an operation whose body is declared in
  more than one (#329). ITS-REST 1.1.0 makes `Content-Type` optional with no
  default, so the first listed is the gateway's own choice, as for a missing
  `Accept`. The `Content-Type` of a body is held to the media types the
  operation's body is declared in, and a `CONTRIBUTION` is read for its
  preceding versions by the request body reader of `openehr-its`.
- The compose quickstart runs four FerroEHR nodes, `ferroehr-a` to
  `ferroehr-d` on ports 8081 to 8084, each with its own `system_id` (#322).
  They share one FerroEHR PostgreSQL container, `ferroehr-postgres`, with a
  database per node owned by its own role; the image's init script runs once
  per node through `docker/postgres/20-ferrofed-node-databases.sh`, so the
  per-node `ferroehr-a-postgres` and `ferroehr-b-postgres` services and their
  volumes are gone (`docker compose down -v` removes the old ones). Each node
  database admits only its own node's role: `CONNECT` is revoked from
  `PUBLIC`, so one node's role is refused on another node's database. The
  gateway's quickstart configuration is a development profile whose static
  cross-reference maps four synthetic patients in `urn:oid:2.999.1.1`: one at
  all four nodes, one at two, one at one and one at none, and
  `scripts/quickstart/seed.sh` creates exactly those EHRs and a composition in
  each over ITS-REST. A patient query now resolves in the quickstart instead
  of failing closed with `424`. The book's container page walks through a full
  federated query, a patient missing at some nodes and a query directed at one
  node, and states the measured memory use: 238 MiB after
  `docker compose up --wait`, 425 MiB after the seed and the queries.
- The end-to-end harness starts one FerroEHR PostgreSQL container per topology
  with a database per node, through the same init script, instead of one
  container per node (#322, #320). CI keeps two nodes.
- `scripts/conformance/obligations.sh` and `scripts/checks/obligations.sh`
  are executable, as every other script is (#320).
- The query string of a stored-query `PUT {base}/v1/definition/query/{name}/{version}`
  is decoded by the generated `openehr-its` parameters (#292, FerroEHR
  #3540), so a `+` is a literal plus. A `query_type` given twice, or a pair
  that does not percent-decode to UTF-8 text, is now `400 body-invalid` and
  nothing is stored; an undeclared parameter stays
  `400 query-parameter-refused`.
- The `openehr-*` family moves to 0.0.80 (FerroEHR #3539 to #3541, #3543), and
  `openehr-federation` to 0.0.31 with it. The fuzz crate is now held to the
  family pin by `scripts/checks/versions.sh`.
- An `ehr_id` that the session's resolution bindings or the `ehr_id` index
  hold at two members is now refused `409 ehr-id-collision` listing the
  claimants, on a write as on a read (#63; §12.5.2, N42). A read no longer
  goes on to the ask-all probe, and a write is no longer `400
  target-required`. Naming the node in `openEHR-federation-endpoint` still
  routes the request to it (§12.5.1 step 1, N41). The integrity incidents of
  the `creating_system_id` map are logged under `ferrofed::integrity` too.
- The `409 controlling-system-unreachable` message now names the controlling
  system in every case (#66; §10.3 `copy-write-reject`, N36, CP-29). Where
  the registry routes the version's `creating_system_id` to another member,
  it names that `creating_system_id` as the registry spells it, with the
  member's node and endpoint. Where no member is known to control the
  version, it points at the place in the request that names the version
  (`If-Match`, the path, or a `CONTRIBUTION` version by position), and still
  quotes nothing of the request. CP-29 is scored end to end: a write against
  a row that `version-identity` dedup kept, sent through the copy's `ehr_id`,
  is refused `409` whether the creating node is down or up, the copy's node
  is sent nothing and the creating node is never tried, and a write through
  the creating node's own `ehr_id` reaches it alone and its answer names no
  copy.
- A body routed to one node always travels with a `Content-Type` its
  ITS-REST operation declares (#298; §12.6, N43, CP-34). The gateway reads
  each operation's request-body media types from `openehr-its`
  (`request_media`): a client `Content-Type` for an operation that declares
  no `Content-Type` parameter, such as the versioned stored-query `PUT`, is
  composed as the listed media type it names, or refused `415
  media-type-unsupported`; a body sent without one travels with the one
  media type the operation's body is declared in, `text/plain` for a stored
  query and `application/json` for a commit, and an operation whose body is
  declared in several is `415`. A forwarded body no longer reaches a node
  without a `Content-Type`.
- Each path identifier of a routed request is parsed as the identifier class
  `openehr-its` states for it, and the follow-up routing table reads a
  version from the path parameters of that class, with no name list of the
  gateway's own (#291; §5.4.1, §12.2, N33, CP-26). The `uid_based_id` of a
  `DELETE` is an `OBJECT_VERSION_ID`, as ITS-REST declares: a DEMOGRAPHIC
  delete addressing a `HIER_OBJECT_ID` is `400 parameter-value-invalid`,
  and a composition delete stays `400 preceding-version-invalid`, with
  nothing sent.

### Fixed

- Every link in `llms.txt` is now an absolute URL (#392). The file is
  served at the site root, where its repository-relative links resolved to
  nothing.

- A fan-out template upload, a stored-query distribution or repair, and a
  stored-query drift check fail with a `500` when the task for one member
  panics (#380; no specification governs the metrics or health probes). Such
  a task was reported as abandoned by the member: counted as a `time-out` in
  `ferrofed_node_requests_total`, marked `down` on
  `GET /health/dependencies`, and listed as `time-out` in `meta.federation`,
  blaming the node for a defect in the gateway. The probe and the federated
  query already failed this way, and none of these calls records a member
  for a request it fails.
- The admission check reports an EHR call whose deadline passed before it
  left the gateway as a call never sent (#379; §11.5), where it said the
  node did not answer before the deadline.
- A request whose deadline passed before it left the gateway is no longer
  counted as a node `time-out` (#374; §11.5; no specification governs the
  metrics or health probes). A request routed to one node, an ask-all probe,
  a fan-out template upload, a stored-query call and a federated query member
  the budget overtook before sending are now neither counted in
  `ferrofed_node_requests_total` nor timed in
  `ferrofed_node_request_duration_seconds`, and leave the member's state on
  `GET /health/dependencies` as it was, where they counted as `time-out` and
  set the member `down`. A fan-out whose overall budget had already run out
  when it started reported every member as asked and silent; it now reports
  none as asked. What the client receives is unchanged: the routed request
  and the probe answer `504` (`node-timeout`), and the member record in
  `meta.federation` says `time-out`.
- `GET {base}/health/dependencies` records each member a fan-out template
  upload, a stored-query distribution or repair, or a stored-query drift
  check asked (#366; no specification governs health probes), where a member
  those calls found down kept the state an earlier request left. One rule
  now holds for every call: the state is the member's reachability and
  health, never whether the request was valid. Any answer below `500` is
  `up`, a `5xx` is `failing`, and no answer is `down`, read from the node's
  own HTTP status whatever the §11.1 record says. A query member that
  answered `400`, a refused store, a node that refused the gateway's onward
  credentials, and a drift check whose copy differs or is missing are `up`
  where some of them were `failing`; a request that never left the gateway
  changes nothing.
- `ferrofed_node_requests_total` and `ferrofed_node_request_duration_seconds`
  count only requests that left the gateway (#366). A member a fan-out
  template upload or a stored-query call could not send a request to stays
  `offline` in `meta.federation`, which §11.1 leaves no other status for,
  and is no longer counted and timed as an `offline` node request; nor is a
  federated query member whose deadline passed before its request left.
- A request the gateway routes or answers under `{base}/v1/` is logged under
  the path template of the ITS-REST operation it addresses, such as
  `/v1/ehr/{ehr_id}/composition/{uid_based_id}`, where its request line named
  `<unmatched>` (#219). The template comes from `openehr-its`'s route table,
  so the line never carries the `ehr_id` or version uid of the path, and the
  configured base path appears in front of it once. An `OPTIONS` request is
  logged under the template of the resource it describes. A path that names
  no route, or any other method its operation does not declare, is still
  logged as `<unmatched>`.
- `POST {base}/v1/query/aql` and the stored-query `POST {base}/v1/query/{name}`
  hold their `Content-Type` to `application/json`, the media type ITS-REST
  1.1.0 lists for both (#269). Another media type, or a parameter other than
  `charset=utf-8`, is `415 media-type-unsupported`, and no node is asked
  (RFC 9110 §15.5.16); a body sent as `text/plain` used to be read as JSON.
  A body sent without a `Content-Type` is read as JSON, the first listed
  media type, as for a missing `Accept`, since ITS-REST and RFC 9110 §8.3
  name no default. The stored-query definition
  `PUT {base}/v1/definition/query/{name}/{version}` the registry answers
  holds its `Content-Type` to `text/plain` under the same rule: another media
  type is `415 media-type-unsupported` and nothing is stored.
- The `query-parameter-refused` security event says "a request carried a
  query parameter the gateway does not admit for its operation, and was
  refused", which is true of a routed request and of the stored-query
  definition `PUT` that raises it too (#269).
- `OPTIONS {base}/` declares the EHR area as the gateway routes it (#290;
  §7a.2, N30, CP-23). `its_rest.ehr` now says that a new EHR, by
  `POST {base}/v1/ehr` or by `PUT {base}/v1/ehr/{ehr_id}`, goes only to the
  endpoint the targeting headers name, and that the `PUT` is refused when
  another member holds its `ehr_id`. It no longer names the session's
  resolution binding as a routing step, because without client
  authentication no request has a session. Each of the four `its_rest`
  declarations is now tested against the behaviour of its area in every
  configuration mode.
- A query dispatched to a single node, directed at one endpoint by the
  `openEHR-federation-endpoint` header or `FROM ENDPOINT`, or undirected with
  the patient resolving at one member alone, answers with
  `openEHR-federation-endpoint` and `openEHR-federation-system-id` naming that
  endpoint and its node's `system_id`, as N31 requires of a request
  dispatched to a single node (#319; §7a.3, CP-24). Zero rows, a `424` or a
  `504` from that node carries them too; a query that asked no node carries
  neither.
- A request routed by its target alone (`POST {base}/v1/ehr` and a definition
  request) checks its declared header and query values before its target, as
  the EHR route does: a malformed value is `400 parameter-value-invalid`, and
  an `Accept` or `Content-Type` the operation does not list is `406` or `415`,
  before `400 target-required` (§5.4.1, N33).
- A `[credentials."<endpoint id>"]` key is held to the registry's endpoint id
  rule when the configuration resolves, with or without a registry (#272).
  Configuration used to accept 1 to 128 printable ASCII characters and left
  the registry's rule to the federation load, so a key such as `node:a`,
  `-node` or a 65-character id passed when no registry was configured. The
  one rule is now the registry's: 1 to 64 ASCII letters, digits, `.`, `-`
  and `_`, starting with a letter or digit, and the refusal (exit 78) names
  the key. The CI container job now runs the `ENDPOINT`
  attribute test against both FerroEHR nodes, and a new tier-1 guard,
  `scripts/checks/e2e-placement.sh`, refuses a container test placed where
  that job never selects it.

### Security

- A request routed to one node no longer forwards client text in a path
  identifier, a structured query value or a negotiation header (#261;
  §5.4.1, N33, CP-26). The routed path resolves no patient, so the outbound
  gate had nothing to compare a client's `version_at_time`, `Accept` or
  path `version_uid` against, and a patient identifier written there
  reached the node. The gateway now works from what the ITS-REST operation
  declares in `openehr-its`'s parameter table. It composes `Accept`,
  `Content-Type` and `Prefer` itself, so the node receives a value the
  operation lists, in its own spelling: `Accept` is negotiated as an
  RFC 9110 §12.5.1 media-range list, `*/*` or no `Accept` sending the first
  listed type, and one that admits no listed type is refused with the new
  code `media-type-not-acceptable` (`406`); `Content-Type` must name a
  listed type, with `charset=utf-8` accepted and dropped, or it is refused
  with the new code `media-type-unsupported` (`415`); `Prefer` keeps only
  the listed preferences and ignores the rest (RFC 7240 §2). Each path
  identifier parses as the openEHR identifier it names with `openehr-base`
  (`version_uid` an `OBJECT_VERSION_ID`, a text `uid_based_id` a
  `UID_BASED_ID`, a `uuid`-format parameter a canonical UUID), and the path
  then travels byte for byte (N22). Each query value matches its declared
  kind: a date-time is an extended ISO 8601 date-time read by
  `openehr-base`'s `Iso8601_date_time` (ITS-REST Overview, "Datetime
  format"). A malformed value is refused with the new code
  `parameter-value-invalid` (`400`), naming the header, or the parameter by
  position and declared name, never the value; no node is asked, the
  ask-all probe included, and the refusal is logged as the security event
  `parameter-value-refused`. A composition update must address the
  versioned object's UUID, as ITS-REST declares. The free text the table
  states no kind for (`If-Match`, `openehr-audit-details`,
  `openehr-item-tag`, `openehr-template-id`, `openehr-version`,
  `openehr-version-item-tag`, the item-tag `key`, `path`, `tag_key`,
  `tag_value`, `tag_target_path`) still travels as sent; whether N33 covers
  a forwarded client value is recorded on #212. Track 10 gains the
  identifier in a date-time parameter, an `Accept` parameter and a path
  uid. `ferrofed-engine` adds the `declared` module, which takes `mime` for
  the media types, and `ForwardError::Value`.
- The `Debug` output of the configuration no longer prints an inline
  credential (#364). An inline `bearer_token` or `password` of a
  `[credentials]` section or of a PIX Manager was a plain string, so any
  `{:?}` of the configuration, in a log line, a panic message or a test
  failure, showed it in clear, and a PIX Manager URL or a registry endpoint
  URL showed the user name and password of its userinfo. Every credential
  is now held in one of two types of the new `ferrofed_registry::secret`
  module: a `Secret`, which `Debug`, `Display` and `Serialize` render as
  `***`, or a `SecretUrl`, which they render with the userinfo and the query
  replaced by `***`, and as `***` whole for a libpq key/value connection
  string. The stored-query store's PostgreSQL `url` and `url_file` and
  `metrics.otlp_endpoint` are `SecretUrl`s too. A `_file` sibling is read
  straight into the same type, and the text read is zeroed once it is
  trimmed, so the resolved settings, the composed PIXm configuration and
  the registry document carry them as well. What reaches a node, a PIX
  Manager, the store or the collector is unchanged. `ferrofed-identity`
  adds `PixmConfigError::BaseUrl` for a Manager base URL that does not
  parse.
- A PIX Manager `url` that carries a user name or a password is refused by
  `config check` and at start, naming `pixm.manager[N].url` and never
  quoting it, as an endpoint URL in the registry document already is (#364).
  Its credentials go in `[pixm.manager.credentials]`. A configuration that
  put them in the URL must move them there.
- The `Debug` output of `ihe-iti`'s types no longer prints a credential
  (#370). `PixmClient` and `PdqmClient` derived `Debug`, so a base URL that
  carried a user name and password showed both in clear, and the HTTP
  client they hold showed its default headers, an `Authorization` header
  among them. Each client now shows its URLs with the userinfo and the
  query replaced by `***`, and leaves the HTTP client out. mCSD's
  `DirectoryOrganization` and `DirectoryEndpoint` show their `fullUrl` and
  the endpoint `address` the same way, and the endpoint leaves out the rest
  of its resource, whose `header` list may hold a credential. The crate
  carries this redaction itself and depends on nothing in FerroFED. Every
  redacted value in `ihe-iti` now shows the family's `***`, where the
  identifiers, the matched Patients and the page links showed `[REDACTED]`,
  and so do `ferrofed-identity`'s `PatientRef` and the admission probe's
  synthetic subject.
  `ihe-iti` is 0.0.9.
- A URL whose password holds an unencoded `/`, `?` or `#`, such as
  `https://user:pa/ss@host`, no longer shows that password in the rendering
  of a `ferrofed_registry::secret::SecretUrl` or of an `ihe-iti` type
  (#370). The userinfo was taken to end at the first `/`, `?` or `#`, so no
  `@` was found and the text showed as written. Text that parses as a URL
  is now read as the URL parser reads it, so an `@` in a path or a query
  is not userinfo, and text that does not parse is redacted up to its last
  `@`.

## [0.0.6] - 2026-10-03

The v0.0.4, v0.0.5 and v0.0.6 milestones in one release (no v0.0.4 or v0.0.5
tag was cut). The federated query now shapes its answer across nodes the way
a single CDR would: `ORDER BY` with `LIMIT` re-applied at the Tier, bounded
`OFFSET` pages, `DISTINCT`, decomposable aggregates and opt-in
version-identity dedup, each refused with a reason where it cannot be exact.
Every failure carries its §11.2 status and a stable error code. A request
under `{base}/v1/ehr/{ehr_id}` is routed to the one node that holds that EHR,
byte-identical, and every answer teaches the gateway which system created
each version. A client names nodes through the `FROM ENDPOINT` /
`ORGANISATION` directive or the targeting headers and selects their ENDPOINT
attributes, the registry also loads as FHIR `Endpoint` and `Organization`
resources, stored queries are held at the gateway as immutable versions, and
`OPTIONS {base}/` declares all of it. The identifier-hygiene property is now
held by the track 10 leakage suite against both nodes: no client-chosen
request id, targeting header or path segment carries a patient identifier to
a node or into the log.

### Added

- The follow-up routing table learns from every answer (#64; §12.2, N21,
  CP-13). Each version uid in a federated query's rows, and each one a
  routed read names or answers with in its `ETag`, teaches a route for a
  `creating_system_id` the registry document does not map; one seen at two
  nodes raises the `LearnedCreatingSystemConflict` integrity incident and is
  routed on by neither. CP-13 is now scored. A follow-up read of a version
  under `{base}/v1/ehr/{ehr_id}/…` routes by the path `ehr_id` in the order
  of N41, never by the version's `creating_system_id` (§12a.1, §12.5.1, N22,
  N42a), and reaches the node byte-identical.
- The federated stored-query registry (#77; §12.7, N44, N33, CP-40,
  CP-28). With `[stored_queries] path` set, the gateway holds stored queries
  itself in an embedded `redb` file that survives a restart, and
  `OPTIONS {base}/` declares `definition.stored_query_registry: true`.
  `PUT {base}/v1/definition/query/{name}/{version}` stores the AQL on
  ITS-REST's semver segment: the name is `[{namespace}::]{query-name}`, the
  version `major.minor.patch`, and the text is analysed as an inline query
  would be, with each `$parameter` standing in for a bound value, and held
  as its canonical print. A definition that names the patient by a literal
  is refused `400 subject-literal`, so no patient identifier is held at
  rest; a second `PUT` of a held name and version is refused
  `409 stored-query-held` and the held text stands, across a restart too.
  `GET` on the same path reads the ITS-REST `StoredQuery` back, and
  `GET {base}/v1/definition/query/{pattern}` lists the versions of every
  name the pattern starts. `POST {base}/v1/query/{name}[/{version}]` runs
  the stored query over every member exactly as if its text were sent
  inline, with the client's `offset`, `fetch` and `query_parameters`, the
  targeting, completeness and dedup headers and the budget; without a
  version the highest runs, and a `{major}` or `{major}.{minor}` prefix runs
  the highest it matches. The answer carries ITS-REST's `name` naming the
  gateway's definition. New codes: `query-name-invalid`,
  `query-version-invalid`, `query-version-required`, `query-type-unsupported`,
  `subject-literal` (each `400`), `stored-query-held` (`409`) and
  `stored-query-unknown` (`404`). Without `[stored_queries]` the definition
  routes still answer `501`. `openehr-federation` is 0.0.30 and adds
  `aql::definition::Definition`, the admission analysis of a stored
  definition. Definition fan-out to the nodes is #78.

- Routing a path `ehr_id` in the order of §12.5.1 (#62; §12.5, N41,
  CP-33). A request to `{base}/v1/ehr/{ehr_id}` or below it goes to the node
  the targeting headers name, then the node a resolution binding of the
  client session names, then the node the new `ehr_id` index names, and for
  a read only, the one member an ask-all probe finds. The probe sends
  `GET {base}/v1/ehr/{ehr_id}` to every member at once within the per-node
  timeout and the overall budget (§11.5); a read of the EHR itself is
  answered from the owner's probe answer. A read no member holds is
  `404 no-destination`, one two members hold is `409 ehr-id-collision`
  listing the claimants, and one a member did not answer for is `504`
  (`node-timeout`, `node-unreachable`) or `424` (`node-refused`, and the new
  code `node-error`), naming the member: a member that gave no answer may
  hold the `ehr_id`, so the owner is unknown. A read that names no node no
  longer answers `501`. A write none of the first three steps routes stays
  `400 target-required`, and nothing is probed. A step that names two
  members names none, and no later step picks one of them. The path
  `ehr_id` must be an openEHR `HIER_OBJECT_ID`, and any other value is `400`
  with the new code
  `ehr-id-invalid` before any routing. The index learns from resolutions and
  from members' successful answers, holds `ehr_id`s and member ids only, in
  memory, and forgets the least recently used `ehr_id` past
  `federation.ehr_index_capacity` (default 100000; 0 is refused). Session
  bindings answer once client authentication lands (#80); the integrity
  incident of a collision is #63.

- `OPTIONS {base}/`, the gateway's self-description (#73; §7a.2, N30,
  CP-23). The body validates against the vendored `options-root.schema.json`
  and is built from the running configuration: the federation id, the
  `major.minor` of the pinned specification, `aql.fan_out` from the node
  selection, the dedup default, modes and request header, the per-node and
  overall budgets, all-or-nothing with the best-effort opt-in when it is
  offered, the `OFFSET` strategy (`bounded` with its `max_window`, or
  `reject`, never a cursor), the decomposable aggregates (an empty list when
  none), nothing offered in the definition area, `localization.on_failure:
  "closed"`, the ITS-REST areas, and every registry endpoint with its
  organisation, membership status, node, `system_id`, and product and
  version where the registry holds them. It declares no targeting mechanism,
  no resolution carrier, no asynchronous queries and no JWKS location, and it
  needs no patient identifier. `OPTIONS` on a path under `{base}/v1/`
  answers `204` with the methods served there in `Allow`, without asking a
  node, and `501` where the gateway serves nothing.

- ENDPOINT attributes in rows (#72; §9.2, §9.3, §9.4, N12, N17, N18,
  CP-35, CP-37). A directed query that selects `p/id` or `p/endpoint_id`,
  `p/organisation` or `p/organization_id`, `p/system_id` or `p/url` through
  the `FROM ENDPOINT` variable gets each value in every row, as a string, from
  the registry entry of the endpoint the row came from: the values
  `meta.federation.endpoints[]` reports for it. No node is asked for an
  attribute, and a query that selects none keeps the row shape of a single
  CDR. The §9.4 example answers the §9.4 columns and rows, with `columns[]`
  paths in the ITS-REST form (`/id`, `/system_id`, `/uid/value`). An alias on
  an attribute keeps it apart from an EHR-derived column of the same name;
  giving both the same name is refused `400` with the new refusal
  `endpoint-name-collision`, and a path through the variable that is no §9.3
  attribute with `endpoint-attribute-unknown`. Under `DISTINCT` the
  attributes take part in which rows are equal. An attribute beside an
  aggregate recombined across endpoints is refused `400`
  `indecomposable-aggregate`, and ordering on an attribute stays refused `400`
  `endpoint-variable`. The `501 not-implemented` answer for these queries is
  gone. `openehr-federation` 0.0.27 adds the `attribute` module with
  `EndpointAttribute`, `ColumnSource::Endpoint(EndpointAttribute)`,
  `Analysis::attributes`, `NodeAnswer::with_attributes` and
  `Merged::attributes`; the engine adds `Plan::annotating` and
  `FederatedAnswer::attributes`. CP-37 is covered.
- Track 10, the adversarial identifier-leakage suite, judged on node-side
  wire capture (#90; §16.3 track 10, §5.4, N33, N34, N5, CP-26). One
  synthetic patient identifier is supplied in each of the four positions the
  track names: an `EHR_STATUS.subject.external_ref` predicate, a
  `PARTY_IDENTIFIED`/`DV_IDENTIFIER` predicate, a `SELECT` projection, and
  the client's query string or headers (`X-Request-Id`, `Authorization`,
  `Prefer`, the targeting and completeness headers, and a free-form header).
  Each runs on the fan-out, on a query directed at one node, and on the
  single-node route, and the journal of the capturing proxy in front of each
  node must hold no occurrence of the identifier, its namespace or any of its
  fragments in the path, the query string, a header or the dispatched AQL,
  raw or percent-decoded; a refusal must be the `400` the specification
  names, with no node asked. Every dispatched request must locate its node by
  the node's own `ehr_id` alone, and a projected subject column must be the
  re-injected identifier. The converse check commits a `COMPOSITION` with
  the identifier in a `DV_IDENTIFIER` and finds it byte-identical at the
  node, compared by bytes and by digest. The gateway's own log (the request
  lines, the security events and the panic line) is searched too and must
  hold none of it. The suite runs against two mock nodes in the normal suite
  and against the two FerroEHR nodes of the harness behind `FERROFED_E2E`,
  and the conformance matrix records track 10 as covered for the gateway's
  half; CP-27, the node's own obligation, stays with the node profile (#93).
  `ferrofed-testkit` adds the `leak` module, the search over the proxy
  journal.
- The `openEHR-federation-endpoint` and `openEHR-federation-organisation`
  request headers, the targeting mechanism beside the AQL (#71; §8.4,
  §8.4.1, N35, CP-28). Each carries a comma-separated list of registry ids,
  over one field line or several, and selects nodes exactly as the
  `FROM ENDPOINT` and `ORGANISATION` directive does: a listed endpoint where
  the patient is not known is `not-resolved`, every other endpoint is
  `excluded`, an identifier the registry does not know, or a header with no
  identifier in it, is a `400` (`endpoint-unknown`, `organisation-unknown`),
  and a selection of no endpoint is `404 no-destination`. A query directed at
  one endpoint by the header may be an aggregate, as one directed by the AQL
  may. The headers apply to the federated query and to a request routed to
  one node, where together they select exactly one endpoint. When the
  directive and a header, or the two headers, appear in one request, the
  same node set proceeds and different sets are refused `400` with the new
  code `targeting-conflict`, whose message names both sets by their registry
  endpoint ids; the gateway never merges them and never picks one. No query
  parameter targets anything: `?endpoint=` and `?organisation=` have no
  effect on the federated query, and on a routed request they are refused
  `400 query-parameter-refused` like any parameter the operation does not
  declare. Neither header reaches a node: both join `Authorization` and
  `X-Request-Id` in the set a routed request always withholds.
- The registry document in FHIR form (#74; N19, N20, N21, §15.2, CP-13,
  CP-20). With `registry.format = "fhir"`, `registry.document` names a FHIR R4
  JSON `Bundle` of `Organization` and `Endpoint` resources, the shape an mCSD
  directory delivers, which loads into the same members and routes
  identically to the native TOML form. The registry's ids travel as
  identifiers in FerroFED's systems under `https://ferrofed.eu/fhir/sid/`,
  and an endpoint's `connectionType` is `openehr-rest-query` in
  `https://ferrofed.eu/fhir/CodeSystem/connection-type`. `config check`
  refuses, naming the resource, an endpoint whose `connectionType` is
  `hl7-fhir-rest`, an informal string or any other code, an endpoint whose
  managing organisation is missing or not in the Bundle, an endpoint no
  organisation or two organisations operate, an id that is missing or
  repeated, and a node its endpoints disagree on. `ihe-iti` 0.0.8 reads the
  directory content under its `mcsd` feature, resolving references inside the
  Bundle as FHIR R4 §2.36.4.1 does.

- The `FROM ENDPOINT` and `ORGANISATION` directive in AQL (#70; §8.1,
  §8.4.1, N11, N19, N20, CP-6). `FROM ENDPOINT p ["node-a-pub", …]` asks
  exactly the listed endpoints, and `FROM ORGANISATION ["org-a"]` asks every
  endpoint the registry lists as managed by each organisation. The
  identifiers are registry ids, never URLs. The directive does not replace
  resolution: each listed endpoint is still asked about its own `ehr_id`, a
  listed one where the patient is not known is `not-resolved` and fails
  nothing, and every endpoint the directive does not list is `excluded`. The
  directive is parsed by `openehr-query`'s `federation` feature and never
  reaches a node: each node receives the same standard AQL as for the
  undirected query. An identifier the registry does not know is refused `400`
  with the new codes `endpoint-unknown` and `organisation-unknown`, which
  locate it by its place in the list and never quote it, and an organisation
  that manages no endpoint answers `404 no-destination`. A query directed at
  one endpoint dispatches an aggregate or a function AQL does not define
  unchanged (N14, §11.6.3). The directive's variable may only be selected, as
  an ENDPOINT attribute, which #72 adds to the rows, and any other use is
  refused `400` with the new refusal `endpoint-variable`. `openehr-federation` adds
  `aql::directive::FacadeQuery`, `Context::with_targeting`,
  `Analysis::sources` and `ColumnSource::Endpoint`. Golden case 02 now
  passes, and CP-6 is covered. The endpoint and organisation headers of §8.4
  are #71.

- The registry maps every observed `creating_system_id` (#67; §12.2, N21,
  the mapping half of CP-13): a `[[creating_system]]` entry in the registry
  document maps a `creating_system_id` that is no member's own `system_id` to
  an endpoint, and a member's own `system_id` maps to that member without one.
  The document is refused, by `config check` too and naming the
  `creating_system_id`, when a mapping names an undeclared endpoint, maps one
  id twice (ASCII case aside), or maps a member's own `system_id`.
  `ferrofed-registry` adds the learned map: the first sighting of an id the
  document does not route learns a read route to the endpoint it was seen
  at, and a learned mapping never overrides the document. A sighting at a
  second node, or a learned mapping the document contradicts, withdraws it
  and raises an integrity incident, logged at `ERROR` with a stable kind and
  routing ids only. An id nothing routes is a typed miss, never a default
  endpoint. The follow-up read routing that consumes the table is #64.

- Opt-in version-identity dedup (#56; §10, N15, N36, CP-9, CP-29): a request
  that sends `openEHR-federation-dedup: version-identity` gets one copy of a
  version held at several endpoints, keyed on the full `OBJECT_VERSION_ID`
  of the row's `COMPOSITION`, else `VERSION`, uid as `openehr-base` reads
  it. The copy kept is the one from the endpoint whose registry `system_id`
  is the version's `creating_system_id`, else the one from the lowest
  endpoint id. Two versions of one object are two rows, and a row with no
  version uid is never suppressed. `meta.federation.dedup` records the mode
  on every answer, `none` and failing `424` and `504` envelopes included,
  and beside the rows `suppressed_rows` and `suppressed_endpoints[]`, counted
  before `DISTINCT`, `OFFSET` and `LIMIT`. Every node is asked the version
  uid, as a hidden column when the client does not select it (under
  `DISTINCT` only a selected uid is the key), and a query with a `LIMIT` and
  no `ORDER BY` is ordered on it. Under the mode a tie on the `ORDER BY`
  keys breaks on the uid before `endpoint_id`, which keeps a per-node
  `LIMIT n` (or `k + n` for a page) exact after suppression (§11.6.1). The
  default stays `none` (§10.1), which `none` states explicitly; any other
  value, or a repeated header, is `400` `dedup-invalid`. A node whose version
  uid is not an `OBJECT_VERSION_ID` is `node-error`, and the query fails
  `424` under all-or-nothing. A recombined aggregate under the mode is `400`
  `indecomposable-aggregate` (§11.6.3). `openehr-federation` 0.0.20 adds
  `dedup::DedupMode`, `aql::Context::with_dedup`,
  `order::ResultOrder::with_version_key`, `merge::NodeAnswer::with_system_id`,
  `merge::Suppressed`, `merge::Disagreement::VersionId` and
  `aql::refusal::Indecomposable::Dedup`.

- `SELECT DISTINCT` at the Tier (#55; N13, CP-8, CP-32): a row two nodes
  return is answered once, compared on the columns the client selected under
  the Tier comparator (`2` and `2.0` are one value, two spellings of one
  instant are two), and the duplicates are removed before the `LIMIT` and
  the `OFFSET` (AQL 1.1.0 §LIMIT), so a duplicate no longer takes two slots
  of a `LIMIT n` answer or of a bounded `OFFSET` page. The copy kept is the
  first under `ORDER BY`, then `endpoint_id`. A re-injected subject column
  and a column the gateway adds never make two rows distinct. Each
  endpoint's `row_count` stays what it contributed (§9.5). A node that
  returned its full `LIMIT` with two rows the Tier holds equal is
  `node-error`, because a distinct row can lie past its cut, and the query
  fails `424` under all-or-nothing. `openehr-federation` 0.0.18 adds
  `order::ResultOrder::with_distinct` and `distinct`, and
  `merge::Disagreement::Distinct`.

- Single-node routing for the EHR area (#61; §7a.1, §7a.3, §9.6, §11.2, N22,
  N31, N33, CP-24): every ITS-REST operation under `{base}/v1/ehr/{ehr_id}`,
  named by `openehr-its`'s `routes::lookup`, is forwarded once to the node
  the `openEHR-federation-endpoint` header names, through `Client::forward`.
  The body reaches the node byte for byte, a `DV_IDENTIFIER` in a committed
  `COMPOSITION` included, and the node's status, body, `Location` and `ETag`
  come back unmodified, a `404` or `500` included. Every routed answer names
  the acting endpoint and its node's `system_id` in
  `openEHR-federation-endpoint` and `openEHR-federation-system-id`. Only the
  request headers and query parameters the matched ITS-REST operation
  declares travel, read from `openehr-its`'s per-operation parameter table
  (FerroEHR #3530): `If-Match` reaches the node on a `PUT` and never on a
  `GET`. The client's `Authorization` and `X-Request-Id` never travel, and
  the node receives the request's minted `X-Request-Id`. An undeclared query
  parameter, and `subject_id` or `subject_namespace` anywhere, is refused
  before dispatch. New codes: `target-required` (a write that names
  no node, `400`), `endpoint-unknown` and `endpoint-several` (`400`),
  `query-parameter-refused` (`400`), `node-timeout` and `node-unreachable`
  (`504`) and `node-refused` (`424`, a node that refused the gateway's onward
  credentials). A read that names no node still answers `501` until #62
  routes it. A node's `3xx` is its answer, passed on with its `Location`
  unmodified, and no request is re-sent to a host outside the registry.
- Decomposable aggregates (#54; §11.6.3, N14, N39, CP-10, CP-32): an
  undirected `COUNT`, `SUM`, `MIN`, `MAX` or `AVG` is sent to every node and
  answered with one recombined row in the client's columns, never one row
  per node. Counts and sums add exactly, reals in decimal arithmetic; `MIN`
  and `MAX` are re-applied over numbers and complete date-times; `AVG` is
  asked of each node as its `SUM` and `COUNT`. A node value the
  recombination cannot use is `node-error`, so the query fails `424` with no
  value. `DISTINCT`, `COUNT(DISTINCT …)` and a plain column beside an
  aggregate are refused `400` (`indecomposable-aggregate`), and a `partial`
  request for a recombined aggregate is refused `400` (`partial-aggregate`).
  The new `[federation] decomposable_aggregates` list (all five by default,
  `[]` for none) declares the functions, and an undeclared one is still
  `undirected-aggregate`. A directed single-node aggregate is sent unchanged.
  `openehr-federation` 0.0.13 adds the `aggregate` module,
  `aql::Context::with_decomposable_aggregates` and `merge::combine`, and
  takes `rust_decimal` 1.43.0, already in the tree through `openehr-rm`.

- The full per-endpoint report (#49; §9.5, §11.1, N16, N40, CP-11, CP-31):
  every registry member appears in `meta.federation.endpoints[]`, an endpoint
  a directed request did not name as `excluded` with the reason, which stays
  out of scope, so it neither clears `complete` nor fails the query. A
  `[[node]]` of the registry document may record its CDR `product` and
  `version`, which the report carries only from there and omits when the
  registry does not say. `latency_ms` appears exactly for the endpoints the
  gateway dispatched to, and `row_count` counts what each node contributed
  before any federation-level `DISTINCT`, dedup or `LIMIT`.
- Completeness (#50; §11.2 to §11.4, N6, N37, CP-30): all-or-nothing stays
  the default, and a request opts into best-effort with
  `openEHR-federation-completeness: partial`. Under best-effort the gateway
  answers `200` with the rows of the nodes that answered, names every other
  node with its status, and sets `complete: false`. A cross-reference that
  cannot answer is reported there and is not a `424`. `all` is accepted
  explicitly. Any other value, a repeated header, or `partial` where
  `federation.best_effort = false` withdraws the mode is a `400` that asks no
  node and never quotes the value. The `not-resolved` and `consent-denied`
  carve-outs and the scope rule hold in both modes. `complete` is always
  derived from the statuses.
- Timeouts (#51; §11.5, N38, CP-31; RFC 7240): a client shortens the budget
  with `Prefer: wait=<seconds>`, and a longer wait leaves the configured
  budget in force. The effective budget is the one reported in
  `meta.federation.timeout`, and a wait that set it is echoed in
  `Preference-Applied`. Only the first `wait` counts; a malformed one is
  ignored and never refused. `wait=0` asks no node and reports each one
  `time-out`. The overall budget runs from the request's arrival, so the
  patient resolution and the fan-out share it and the gateway answers inside
  it, and abandoning one node never aborts a request in flight to another.
- `ORDER BY` with `LIMIT` re-applied at the Tier, with a deterministic
  tie-break (#52; §11.6.1, N9, N13, N39, CP-8, CP-32). Every node is sent the
  client's `LIMIT n` unchanged, with the row's uid appended as the last
  `ORDER BY` key; an `ORDER BY` path the query does not select travels as a
  hidden column the client never sees. The gateway merges the node answers
  under one total order (null greatest, numbers exactly, complete date-times
  by instant, strings by code point, `DV_ORDERED` values through the openEHR
  RM's own comparison), breaks ties on the endpoint id and then the uid, and
  cuts the result at `n`. A node that returned `n` rows out of that order, or
  more than `n`, is reported `node-error`, so the query fails `424` under
  all-or-nothing. `TOP n` is read as `LIMIT n`; `TOP n BACKWARD`, a `TOP` beside a
  `LIMIT` clause, and a `DISTINCT` query ordered on a path it does not
  select are refused `400`. A query with `LIMIT` and no `ORDER BY` now returns
  at most `n` rows across all nodes.
- `OFFSET` paging across a fan-out (#53; §11.6.2, N9, N39, CP-32). `OFFSET`
  never reaches a node. Under the default `federation.offset_strategy =
  "bounded"`, `ORDER BY … LIMIT n OFFSET k` asks each node for `LIMIT k + n`,
  checks each node's visible order as for `LIMIT n`, merges in the federation
  order and returns rows `k` to `k + n`. A page whose `k + n` is past
  `federation.max_offset_window` (1000 rows per node by default; 0 refuses to
  boot), an `OFFSET` with no `LIMIT`, and an `OFFSET` with no `ORDER BY` are
  refused `400`, the first naming the bound. `offset_strategy = "reject"`
  refuses every `OFFSET` past zero `400`. The ITS-REST `offset` and `fetch`
  members follow the same strategy.
- The `comment-style` guard checks citations (#173). It fails on a Rust
  comment, doc comment or lint `reason` that cites `docs/architecture.md`, a
  path under `.claude/` or a rule file by name, or that names a
  decision-register entry such as `decision A17`. It applies the same check to
  the full-line `#` comments of the shell scripts under `scripts/` and of every
  `Cargo.toml`. `comment-style.sh --self-test` proves each refused form and
  its near misses, and CI runs it before the full-tree pass. Every comment that
  cited one of these now cites the specification section it rests on, or says
  that no specification governs it.
- The error vocabulary (#57; §11.2, N32, N36, N37, N42, CP-12, CP-30): every
  failure the gateway reports on its own behalf answers the ITS-REST `Error`
  body with a stable `code` and the `request_id`, and its status follows one
  table. A refused query is a `400` named by its refusal (`not-aql`,
  `unreducible`, `offset-unsupported`, and the rest), an unknown path a `404`
  `not-found`, an unexposed ITS-REST area a `501` `not-implemented`, and the
  gateway's own fault a `500` `internal`, and a request with no member in
  scope a `404` `no-destination`; `ehr-id-collision` and
  `controlling-system-unreachable` (`409`) are fixed for follow-up routing.
  The codes are API and are only ever added. The book's "Errors and status
  codes" page lists them all, and a test holds the page to the gateway's
  table.
- The `versions` guard holds two hand-typed version facts to their sources
  (#181). The landing page's release note and status panel must name the
  newest `## [x.y.z]` release of `CHANGELOG.md`, so a release cut that forgets
  the page fails. Each specification row of `docs/VERSIONS.md` must name a
  crate constant (`FEDERATION_SPEC`, `ITS_REST`, `AQL`) that carries the
  version the row pins. `versions.sh --self-test` proves both checks, and CI
  runs it before the full pass.
- Generated conformance badges on the README (#175), as shields.io endpoint
  files under `conformance/badges/`: the Gateway points of §17 covered out of
  the Gateway total, the Node and Operator point counts, each labelled with the
  pinned specification version and linked to the book's conformance page, and
  the AQL golden cases passed out of the vendored corpus, linked to its pass
  list. `scripts/conformance/matrix.sh --badges-write` writes the files and the
  README block between `conformance:begin` and `conformance:end`, and the
  `conformance-matrix` guard fails when either drifts from the matrix or the
  pass list. The golden test fails when a case in
  `conformance/aql-golden/pass-list.txt` stops passing or an unlisted case
  passes, and rewrites the list when `FERROFED_CONFORMANCE_UPDATE` is `1`.
  Case 02 (`FROM ENDPOINT`) is refused until #70 and is not counted as
  passing. The static badge row gains the image-pulls badge for
  `ghcr.io/ferrohealth/ferrofed`.
- The `versions` guard holds the book's "Pinned versions" page to
  `docs/VERSIONS.md` (#198). Each row of its pin table names its matrix rows
  by their exact names, and every pin it states, a version, a package, an
  `edition 2024` or an abbreviated `commit`, must be the one those rows pin;
  a pin the guard cannot read fails rather than passing unread. The page now
  lists all five `openehr-*` crates, the vendored PIXm and PDQm packages and
  the specification's source commit as rows of their own. `versions.sh
  --self-test` proves each drift.
- Tests that pin the two optional facilities FerroFED does not offer (#59,
  #60; §11.6.4, §11.7, CP-31, CP-32). A query sent with `Prefer:
  respond-async`, alone or beside `Prefer: wait`, gets the ordinary
  synchronous answer under the same budget: never a `202` or a
  `Content-Location`, and `Preference-Applied` never names `respond-async`
  (RFC 7240 §2, §3). A bounded `OFFSET` page carries no cursor handle or
  expiry in `meta.federation` and runs the fan-out on every request.

### Changed

- A gateway with `registry.document` set now refuses to boot without
  `federation.id`, the federation's own identifier that `OPTIONS {base}/`
  names (#73; §7a.2, N30). Add `id = "<your federation>"` to `[federation]`;
  an empty id is refused.

- The `openehr-*` family moves from 0.0.78 to 0.0.79 (#234). The
  `openehr-rm` attribute model now holds the BASE primitives, the `Ordered`
  marker and the target class of a reference-typed attribute (FerroEHR
  #3537), which the AQL rewrite reads to decide which selected paths a node
  can order under `DISTINCT`. `openehr-federation` is 0.0.28.
- The `openehr-*` family moves from 0.0.77 to 0.0.78 (#237), and an onward
  credential is checked at configuration load by the node client's own
  `Authorization` composition (FerroEHR #3535), for each endpoint and each
  PIX Manager alike. A bearer token must now be the `b64token` of RFC 6750
  §2.1 (letters, digits, `-`, `.`, `_`, `~`, `+` and `/`, then any `=`
  padding), so a token holding a space, a quote or any other character
  outside that set is refused by `config check` and at boot with exit code
  78, naming its key and never its value; it used to pass and then fail at
  the node. The basic rules of RFC 7617 §2 are unchanged.
  `openehr-federation` is 0.0.26.
- The `openehr-*` family moves from 0.0.76 to 0.0.77 (#195). `openehr-query`
  now classifies every AQL function call (FerroEHR #3529): a string, numeric,
  or date and time function of AQL 1.1.0 is a built-in, and any other name is
  not. The rewrite reads that classification where it kept a list of the 16
  function names of its own, so an undirected query that calls a function
  AQL does not define is refused exactly as before (`undefined-function`;
  §11.6.3, N14), and the `CONCAT`, `CONCAT_WS` and `SUBSTRING` folding and
  the `DISTINCT` cut check read the crate's classification too. Every
  `openehr-its` client is built with redirects off (FerroEHR #3531), so a
  node's `3xx` answer is never followed: the node is `node-error`.
- Incompleteness is carried by `meta.federation.complete` alone, and the
  gateway emits no FHIR `OperationOutcome` (#58; §9.1, §11.4, N17, CP-12).
  §11.4, CP-12 and track 4 ask for the resource only for a FHIR-facing
  consumer, and the answer is an ITS-REST `RESULT_SET` whose federation
  additions live under `meta.federation`. The conformance matrix gives CP-12
  that reason beside its scored status codes, a test asserts that a
  best-effort `200` and an all-or-nothing `424` each carry `complete: false`,
  validate against `federated-result-set.schema.json` and hold no
  `OperationOutcome`, and the client contract in the book says so. Where the
  resource would travel for a gateway with FHIR-facing consumers is recorded
  as an upstream report (#204).
- The conformance record's loose ends (#196). The `comment-style` citation
  checks read the conformance tables under `conformance/`: their `#` comment
  lines and their `reason` column, which the book renders. The tables now
  cite §16.2, §16.3, §16.4 and §17 where they cited an internal file or a
  decision-register entry, or say that no specification governs the choice.
  The CP-10 and CP-32 rows name #187. The book page links the specification
  site at the version `docs/VERSIONS.md` pins, with no second copy of it in
  `scripts/conformance/matrix.sh`. `openehr-federation` 0.0.16 marks its
  local list of AQL function names for removal (#195).
- The `comment-style` citation checks cover the rest of the tree (#178): the
  full-line comments of the workflow and composite-action YAML under
  `.github/`, the `echo` and `printf` text of a workflow `run:` block,
  `clippy.toml` with its `reason` strings, and the quickstart TOML under
  `docker/`. The per-edit hook runs the guard on every file kind CI checks.
  Every comment, lint reason and printed line there that cited an internal
  file or a decision-register entry now cites the specification section or
  official documentation it rests on, or says that no specification governs
  it. The six vendor scripts no longer write an internal path into their
  `PROVENANCE.md`.
- The `comment-style` citation checks refuse a citation of any markdown file
  under `docs/` outside the vendored `docs/specs/` tree (#184): a path that
  opens a parenthetical or is followed by `section` or `§`. Naming
  `docs/VERSIONS.md` as the file a script or test reads still passes. The
  checks also read every YAML `description:` scalar and the trailing `#`
  comment after a YAML or TOML value, outside quoted strings and block
  scalars, and the guard runs the same under mawk. The citations this
  catches cite the GitHub documentation or say that no specification
  governs them, among them `fuzz.yml`, the `setup-rust` action
  description, the version and corpus scripts and the CI rule. The crate
  manifests' comment on the 0.0.0 name reservation changes with them, so
  `openehr-federation` moves to 0.0.15, `ihe-iti` to 0.0.7 and
  `nl-generic-functions` to 0.0.4 with no change to their code.
- The `comment-style` citation checks also refuse a citation of a
  `README.md` of the tree, at the root or in a member, outside the vendored
  `docs/specs/` and `vendor/` trees (#198), in the same citation form. Naming
  a README as a file a script reads still passes. `fuzz.yml` cites the
  libFuzzer documentation for how a crashing input is kept, and the scripts
  that cited a README no longer do.
- The release lane fails a tag whose tree has no root `Cargo.toml` (#198),
  with a message saying the tag cannot be checked against the workspace
  version. It used to skip that check.
- The release lane has one path (#201): the binaries and the image build for
  every tag `plan` accepts, and `finalize-release` publishes only when every
  build leg succeeded. The branches for a tag with no workspace, which `plan`
  now refuses, are gone. The book's claims page links to the pinned-versions
  page for the `openehr-*` pin instead of restating the number, and
  `docs/ci-cd.md` names all four self-tests of the `tracker-helpers` job,
  `rel.sh` among them.
- `scripts/gh/fields.sh` checks its arguments before any `gh` call (#198).
  No argument, an unknown command, a wrong operand count or `--help` prints
  the usage to stderr and exits 2, with no network call and no token needed.
  `fields.sh --self-test` proves it against a stub that records every call.
- The per-edit `comment-style` hook also reads the `conformance/*.tsv`
  tables (#198), as CI does, and stays a quiet pass for any other file.
  Conformance track 5 (Dedup + DISTINCT) names #187 beside its issues, as its
  CP-10 and CP-32 rows do, and the book's conformance page is re-rendered.
- Error bodies (#57): the gateway's own refusals (`404`, `501`, a caught
  panic's `500`) answer the ITS-REST `Error` shape with `code` and
  `request_id`, where they named the code in an `error` member, and the codes
  are kebab-case (`not-found`, `not-implemented`). No error body quotes the
  query, a parameter value or a header value (§5.4.3); a `424` or `504` under
  all-or-nothing stays the §11.4 result set and echoes the client's own `q`
  (N17). A node row shorter than the dispatched query selects is now that
  node's `node-error` (`424`, or reported under `partial`), where the whole
  query answered `502` (§11.1, §11.2).
- The `openehr-*` family moves from 0.0.74 to 0.0.76 (#57). 0.0.76 keeps the
  members an open ITS-REST schema admits in an `additional_properties` map
  (FerroEHR #3526), so every error body is the generated ITS-REST `Error`
  with `code` and `request_id` in that map, and no FerroFED-side error type
  remains. The generated request and result-set types carry the same map.
- The site, the README and the book describe the released gateway (#176).
  The landing page opens with the FerroFED mark and the current release, and
  shows the quickstart, the release binaries and the image
  `ghcr.io/ferrohealth/ferrofed`, with what each specification property has
  in v0.0.3 and what is planned. The README's install section names the
  release assets and their verification, and its quickstart runs the
  published image with `docker compose up --wait`. The book's introduction,
  claims, deployment, container and contribution pages, the governance and
  contribution guides, and the working rules no longer describe a design
  phase or a project with nothing to install.
- The quickstart and the end-to-end harness run two FerroEHR nodes (#155,
  decision A44). EHRbase left both, because it refuses a `.` in
  `PARTY_REF.namespace`, which openEHR BASE admits (#118). In `compose.yaml`
  the services are `ferroehr-a` and `ferroehr-b`, each on its own database
  and with its own `system_id` (`node-a.quickstart.local`,
  `node-b.quickstart.local`, which the quickstart registry declares), and
  node B moved from port 8091 to 8082 (`FERROEHR_B_PORT`; node A's
  `FERROEHR_PORT` is now `FERROEHR_A_PORT`). Every end-to-end case seeds the
  patient's subject on both nodes, and the `e2e (containers)` CI job now runs
  the server's container tests as well as the testkit's.
- The tracker records the kind, the urgency and the size of an issue in
  GitHub's native issue type (Bug, Feature, Task) and the organisation's
  Priority and Effort fields (#154). The `bug`, `enhancement` and `P0` to
  `P3` labels are retired, the bug and feature forms set the issue type, and
  `scripts/gh/fields.sh` and `scripts/gh/migrate-fields.sh` carry the model
  and the migration.

### Fixed

- The local crate-bump hook judges a commit or a push against `origin/main`,
  the base CI uses, and leaves finding the merge base to
  `scripts/checks/crate-version-guard.sh` (#243). A branch that bumps a crate
  to the version main already took is now blocked locally, as CI refuses it,
  where it used to pass. The book's checks page lists the `tracker-helpers`
  job, the self-tests of the `scripts/gh` helpers, among the tier-1 checks.
- The outbound gate no longer reads the host and port of a registry
  endpoint URL (#232; §5.4.1, N33, CP-26). An endpoint whose host or port
  contained a withheld identifier, such as a short local identifier that
  matched a port number, refused every query for that patient. The
  authority comes from the operator's registry and never from a request,
  and §5.4.1 names the request path, the query string and the headers. The
  gate still reads the path, the query and the fragment, raw and
  percent-decoded, with the AQL text, the paging and the headers the
  gateway adds.
- A `SELECT DISTINCT` query with `ORDER BY` and `LIMIT` (or a bounded
  `OFFSET` page) that selects a path whose value AQL defines no order for is
  refused `400` (`incomparable-distinct-key`), where a node could cut among
  distinct rows differently on each repeat (#234; §11.6.1, N13, CP-8,
  CP-32). AQL orders on data "available to primitives and `Ordered` types"
  (AQL master03-syntax §ORDER BY), so a whole RM object (`SELECT DISTINCT
  c`), a data value that is not a `DV_ORDERED` (`c/name`, a `DV_TEXT`), the
  `DATA_VALUE` of an `ELEMENT`, a collection and a path the RM does not
  resolve are not keys. The rewrite decides it by walking the path through
  `openehr-rm`'s static model from the class `FROM` binds, asking the
  model's `is_primitive` and `conforms_to_ordered` lookups and following a
  reference to the target class the model names (FerroEHR #3537). Under
  `DISTINCT` only the selected paths to a primitive or a `DV_ORDERED` are
  pushed as `ORDER BY` keys, so the same query without a `LIMIT` is answered
  and the other paths are compared at the Tier alone. A path to a primitive
  value, such as `c/name/value` or `…/value/magnitude`, and an ordered data
  value, such as `c/context/start_time`, are pinned as before. The `aql`
  feature of `openehr-federation` now depends on `openehr-rm`; the crate is
  0.0.28 and adds `Refusal::IncomparableDistinctKey`.
- A query with `ORDER BY` and `LIMIT` (or `TOP`, the `fetch` member, or a
  bounded `OFFSET` page) ordered on a path whose value AQL defines no order
  for is refused `400` with the new code `incomparable-order-key`, where it
  used to be sent to every node and re-ordered at the Tier unchecked (#242;
  §11.6.1, N39, CP-32). AQL ordering "assumes that data identified by the
  path … are comparable", the primitives and `Ordered` types (AQL
  master03-syntax §ORDER BY), and §11.6.1 holds the federated first `n`
  rows in each node's first `n` only "under a total order", so a node's cut
  on `ORDER BY c/name LIMIT 10`, a `DV_TEXT`, could drop a row of the
  answer. The rewrite uses the comparability rule of #234, and the
  refusal points at the key by position. The same order with no `LIMIT` is
  answered: every row reaches the Tier, which orders them all under its own
  total order, the same on every repeat. Under `DISTINCT` the refusal stays
  `incomparable-distinct-key`. `openehr-federation` is 0.0.29 and adds
  `Refusal::IncomparableOrderKey`.
- Under version-identity dedup, `SELECT DISTINCT` compares the version uid
  without regard to case, as the dedup and the Tier order already do (#234;
  §10.2, CP-8, CP-9; BASE `master05-identification_package.adoc`
  §"Composite Identifiers and Case"; AQL master03-syntax §DISTINCT). Two
  rows of one node whose uids differ only in case are one distinct row, the
  first in the Tier order, with its text as the node sent it; a node cut at
  its `LIMIT` that returns both is refused as `node-error`. Outside dedup a
  uid compares as sent, in step with the Tier order.
- A `SELECT DISTINCT` query with `ORDER BY` and `LIMIT` (or a bounded
  `OFFSET` page) whose selected function column the selected paths do not
  fix is refused `400` (`unordered-distinct-cut`), where a node could cut
  among distinct rows tied on every path differently on each repeat (#210;
  §11.6.1, CP-8, CP-32). AQL orders only on paths (AQL master03-syntax
  §ORDER BY), and under `DISTINCT` the gateway adds no column, so the
  selected paths are a node's only keys. A call to a single-row function AQL
  defines whose arguments are literals, parameters and selected paths, such
  as `LENGTH(c/name/value)` beside `c/name/value`, is answered as before; a
  call that reads a path the query does not select, a clock function such as
  `NOW()`, and `TERMINOLOGY` are refused. The same query without a `LIMIT`
  is answered. `openehr-federation` 0.0.22 adds
  `Refusal::UnorderedDistinctCut`.
- Under version-identity dedup, the Tier orders the version uid without
  regard to case (#229; §10.2, §11.6.1, §11.6.2, CP-9, CP-32; BASE
  `master05-identification_package.adoc` §"Composite Identifiers and
  Case"). Two copies of one version whose ids differ only in case rank as
  one in the merged order and in the check of each node's order, as an
  `ORDER BY` key and as the tie-break, so an `ORDER BY` with `LIMIT` and a
  page under dedup stay exact for them. Every row keeps its uid as its node
  sent it. The comparison is `openehr-base`'s `composite_id_key`. A node cut
  at its `LIMIT` that orders uids byte for byte and returns them in an order
  the Tier's disagrees with is refused as `node-error`, since the Tier
  cannot change how a node sorts. Outside dedup the uid orders as before, by
  code point. `openehr-federation` is 0.0.23.
- An onward credential the `Authorization` header cannot carry is refused at
  configuration load, by `config check` and at boot with exit code 78, in
  place of failing every query to its endpoint with a `500` (#231). A bearer
  token, inline or read from `bearer_token_file`, is checked as the
  `Bearer <token>` value the node client sends, by the same `http` parse the
  client applies, and a basic user or password holding a control character,
  or a basic user holding a colon, is refused as RFC 7617 §2 requires. The
  refusal names the key the value came from
  (`credentials.<endpoint>.bearer_token`, its `_file` sibling, `.user` or
  `.password`) and never the value. A PIX Manager's
  `[pixm.manager.credentials]` is held to the same rule.
- Version-identity dedup compares identifiers without regard to case (#225;
  §10.2, CP-9; BASE `master05-identification_package.adoc` §"Composite
  Identifiers and Case"). Two copies whose version ids differ only in case
  are one version, and the node whose `system_id` differs from the version's
  `creating_system_id` only in case keeps the originating copy. Before, both
  were compared byte for byte. A kept row still comes back as its node sent
  it. The comparison is `openehr-base`'s `composite_ids_equal` and
  `composite_id_key`, which the registry identifiers already use, and the
  session-scoped resolution bindings now key on `EhrId` in place of their
  own case fold. `openehr-federation` is 0.0.21.
- A patient query is no longer refused at random when its withheld
  identifier is a short hexadecimal value that occurs inside the minted
  `X-Request-Id` (#227; §5.4.1, N33, CP-26). The outbound gate reads the AQL
  text, the paging, the URL and every other header the gateway adds, and
  skips the minted id, which `OutboundId::mint` makes from no client input.
  A request a panic, the request timeout or the body ceiling answers now has
  its request-log line, with the status it answered (`500`, `408`, `413`)
  under the gateway's id. The "the federated query failed" line carries the
  same `request_id` as the request line. The hygiene assertions of the tests
  compare the raw bytes a mock node received, so a header value or a body
  that is not UTF-8 is searched too, and the CP-26 row of the conformance
  matrix names #217.
- CP-29 is `planned` again in the conformance matrix and the gateway badge
  (#221): only its visibility half, the dedup record of §10.2 and §10.3, is
  built, and its write-routing and `409` half is #66. The dedup tests carry
  CP-9 alone. `scripts/checks/crate-version-guard.sh` run without its base
  ref prints its usage on stderr and exits 2.
- The book's "What FerroFED claims" page lists decomposable aggregates (#54)
  among what has merged since v0.0.3, and no longer as planned; de-duplication
  is what remains planned for v0.0.4 (#211). The `merge` module doc of
  `openehr-federation` is wrapped like the rest of the crate's docs, and
  `openehr-federation` moves to 0.0.19 with no change to its code.
- An `ORDER BY` with `LIMIT` query whose rows carry no uid, such as one that
  selects from `EHR` alone, returns the same rows on every repeat (#157;
  §11.6.1, §11.6.2, N39, CP-32). Each node is asked to order on a row key
  after the client's keys: the uid of the first `COMPOSITION` or `VERSION`
  as before, else `<ehr>/ehr_id/value`, else the uid of an `EHR_STATUS` or
  `EHR_ACCESS`; the key travels as a hidden column and never reaches
  `columns[]`. A node cut at `LIMIT n`, or at the `LIMIT k + n` of a bounded
  `OFFSET` page, then keeps the same tied rows each time, so two pages agree
  on the rows tied across their edge. A patient query, scoped to one
  `ehr_id` per node, gets no `EHR` key; `FOLDER` is never a key; a
  `DISTINCT` query keeps its selected columns as the tie-break and gains no
  column. A query with no row key is still answered: the Tier orders the
  rows it receives, and which tied rows a node returns is the node's.
  `openehr-federation` 0.0.17 carries the change.
- A query that calls a function AQL 1.1.0 does not define, such as a
  product-specific `MEDIAN(x)`, in `SELECT` or `WHERE` is refused `400`
  (`undefined-function`) when it would reach more than one node, where it was
  sent to every node and answered one row per node (#187; §11.6.3, N14, N39,
  CP-10, CP-32). The gateway cannot tell whether such a function aggregates,
  and per-node aggregate rows are the answer §11.6.3 forbids. Directed to one
  endpoint, the query is sent unchanged. The single-row functions AQL defines
  (AQL §Functions: the string, numeric, and date and time functions, and
  `TERMINOLOGY`) are still sent to every node as written, and the five
  aggregates keep the decomposition rules. `openehr-federation` 0.0.14 adds
  `aql::refusal::Refusal::UndefinedFunction`.
- A query that uses `TOP` together with a `LIMIT` clause is refused `400`
  (`top-with-limit`), whether or not the two counts agree, because AQL
  forbids the pair (#162; AQL §TOP, §LIMIT). A `TOP` query sent with the
  ITS-REST `fetch` member is refused `400` (`top-with-fetch`), because
  ITS-REST says `fetch` "cannot be combined with AQL-top". `TOP n` alone is
  still read as `LIMIT n`.
- A federated query that leaves no registry member in scope, because every
  endpoint is `excluded` (suspended by the operator, for example), answers
  `404` and asks no node, where it answered `200` with empty `rows` (#166;
  §11.1, §11.2, §11.3). A candidate set that localization left empty is not
  that case: every member is `not-localized`, and the answer stays `200`
  with `complete: true` (§14.1). A patient who is `not-resolved` at every
  member in scope still answers `200`.
- A gateway that federates refuses to boot, and `config check` refuses the
  file, unless `server.request_timeout_ms` exceeds
  `federation.overall_timeout_ms` by more than one second, the margin kept
  for combining the answers (#167; §11.5). The refusal names both keys.
  Before, a request timeout between the two let the server's `408`, with no
  body, cut a slow fan-out instead of the `504` that carries
  `meta.federation`.
- The documentation describes the gateway that exists (#163). The
  `openehr-federation` README names both subject carriers as resolution
  input. The book's client contract and configuration pages name
  `POST /v1/query/aql` as served, and the `_file` secrets paragraph sits
  under the configuration file it describes. Code and doc comments cite the
  specification sections a decision rests on, or say that no specification
  governs it.
- The landing page no longer scrolls sideways on a phone narrower than about
  380 px (#182). The audience cards and the other card grids take a column
  minimum that shrinks to the page width, so a 320 px viewport shows one
  full-width column in the light and dark themes.
- `llms.txt` describes follow-up routing to the owning CDR as planned for
  v0.0.5, where it described it as built (#180). The CI log, the guard
  scripts, the analyzer configuration and the `research` label no longer
  describe the repository as being in a design phase.
- `scripts/gh/labels.sh` prints its usage and exits `2` on any argument it
  does not know, `--help` included, before it calls `gh` (#180). It ignored
  such an argument and wrote the label taxonomy to the repository. Its
  `--self-test` proves the refusal.
- The `ferrofed-engine` crate description names what it holds (dispatch,
  fan-out, the budgets, the completeness decision and the outbound
  identifier-hygiene gate) and lists follow-up routing on the creating
  system id as planned, where it described routing as built (#191). The
  workflows, the release checklist, the version pages of the book and the
  repository, and the working rules no longer describe the Cargo workspace
  as not yet existing, and the book names the `openehr-*` pin as 0.0.76 and
  the PIXm and PDQm packages as vendored.
- `scripts/gh/rel.sh` prints its usage and exits `2` before it calls `gh` on
  a usage error: no argument, an unknown command, a wrong operand count, a
  flag other than `--replace`, or `--help` (#191). It exited `1` with no
  argument, `0` on `--help`, and resolved the repository through `gh`
  first. Its new `--self-test`, run by the CI `tracker-helpers` job, proves
  each write's endpoint and each refusal.

### Security

- The ask-all probe no longer sends a path `ehr_id` that is not a bare UUID
  to every member (#259; §5.4.1, §12.5.1, N33, CP-26). The path `ehr_id` was
  checked only as a `HIER_OBJECT_ID`, a form that admits a bare national
  number as a one-arc ISO OID, so a patient identifier in the `ehr_id` slot
  of a read nothing routed was broadcast to the whole federation. A read
  whose `ehr_id` is an ISO OID, an internet id or a UUID with an extension,
  and that no targeting header, resolution binding or `ehr_id` index entry
  routes, is now refused with the new code `probe-requires-uuid` (`400`),
  and no member is asked. The same `ehr_id` with the endpoint header is
  forwarded to the named node alone, and the index then routes it, since a
  member may mint `ehr_id`s in a scheme other than UUID (§12b.2, N42a).
  The refusal is logged as the security event `ehr-id-probe-refused`, which
  never names the `ehr_id`. Track 10 gains the identifier in the `ehr_id`
  slot. `ferrofed-registry` adds `EhrId::is_uuid`, and `ferrofed-engine`'s
  `probe::Probe` carries a `ProbedEhrId`, which only a bare UUID converts
  to, in place of the client's path segment.
- A panic no longer prints its message to stderr (#260; §5.4.3, CP-26).
  `ferrofed serve` used Rust's default panic hook, which printed the
  message, and any value formatted into it, to stderr, part of a
  deployment's log stream. The binary now installs its own hook before it
  serves. The hook writes one fixed line through `tracing`, "a thread
  panicked", with the source location and, inside a request, the gateway's
  `request_id`, and never the message. It writes nothing to stderr. A
  panicking handler still answers `500`, and its "the request handler
  panicked" line is unchanged. Track 10's panic case now runs in a child
  process and asserts that its stderr is empty. `ferrofed-server` adds
  `panic::install_hook` and `request_id::serving`.
- A request routed to one node no longer logs the client's `X-Request-Id`
  (#62; §5.4.1, §5.4.3, N33, CP-26). The security events of a refused query
  parameter or a withheld request, and the failure events of the routed path
  and the ask-all probe, named the client's own id, free text that can carry
  a patient identifier. Every event of the routed path now names the
  gateway's own id, the one its request line records; the client's id stays
  in the response and its error body only.
- A client's `X-Request-Id` no longer reaches any node (#217; §5.4.1, N33,
  CP-26). A legal client value was sent to every node of the fan-out as it
  came, so a patient identifier written into it passed the outbound gate,
  which checks only the identifiers the gateway resolved on. The gateway now
  mints its own id, a version 4 UUID, for every request and sends only that,
  the same id to every node of one request. The response header and the
  error bodies still name the client's own id. The request log, the security
  events and the panic line record the gateway's id and never the client's,
  with a `client_named` flag on the request line saying whether the client
  sent one; a client that sends none gets the gateway's id back. The book
  lists every header a node request carries and where its value comes from.
  `ferrofed-engine` adds `outbound_id::OutboundId`, which only
  `OutboundId::mint` makes, and `DispatchOptions::with_request_id` and
  `fanout::fan_out` take one in place of a string.

## [0.0.3] - 2026-10-02

The first two federated milestones in one release (v0.0.2 and v0.0.3; no
v0.0.2 tag was cut). `POST {base}/v1/query/aql` answers one ITS-REST
`RESULT_SET` over two openEHR CDRs, with the patient resolved outside AQL
through a PIXm PIX Manager (or the development cross-reference) on either
patient carrier, and no directly identifying identifier sent to a node: the
rewrite refuses it, and an outbound gate re-checks every request before it
leaves. Also the Cargo workspace and its spec-named crates, the PIXm and PDQm
clients, the container and compose quickstart, the end-to-end harness with a
PIX Manager, the fuzz lane, and the release lane at SLSA Build Level 3,
rehearsed as `v0.0.2-rc.1`.

### Added

- The PDQm ITI-78 patient demographics query client in `ihe-iti`, feature
  `pdqm` (#119; PDQm 3.2.0, ITI TF-2 §2:3.78). `pdqm::PdqmClient` posts a
  `PatientQuery` to `[base]/Patient/_search` as a form body, so no URL carries
  a demographic value, and reads the `searchset` into the matching Patients
  (FHIR R4 `Patient` from `fhir-types` `resources`), the `total`, each match's
  score and `match-grade`, the `OperationOutcome` warnings and the `next` page
  link, which it follows only on the Supplier's origin. A `404` with a
  `not-found` issue for a query that names an identifier domain is the
  profile's unrecognised-domain answer; every other failure is a typed error
  that carries no value, URL or Supplier text. The ITI-78 artefacts of the PDQm
  3.2.0 package are vendored under `docs/specs/ihe-pdqm/` by
  `scripts/vendor/ihe-pdqm.sh`. The `OperationOutcome` issue type moves to
  `ihe_iti::outcome::IssueType`, shared by PIXm and PDQm; `ihe-iti` is 0.0.5.
- The identity-lifecycle hook for track 8 (#48): `ResolutionBindings::identity_changed`
  drops every resolution binding a merge or split at the identity source could
  have made stale, by `ehr_id` in every session or all of them for an
  unscoped change, and `Federation::identity_changed` is the entry point a PMIR
  subscription calls. The binding lifetime, `federation.binding_ttl_ms`, is the
  bound until one exists. Track 8 stays provisional and is not claimed.
- The ask-all node selection of a deployment with no localizer (#46; §4.3
  Variant B, N4, N10). `federation.node_selection = "ask-all"` declares it,
  and a gateway that federates refuses to boot without the declaration
  (`NodeSelectionUndeclared`), so the choice is never a silent default. Every
  active member's cross-reference is asked, the query reaches only the members
  that return an `ehr_id`, and the others are `not-resolved` without failing
  the query (N6, N8). The selection is named in the startup log; the
  `OPTIONS` self-description follows with #73. The quickstart and the
  configuration page declare it.
- A PIX Manager in the test harness, seeded by ITI-104 (#47). The testkit's
  `pix::PixManager` is a test device, not a PIXm implementation: an in-process
  loopback server that takes the PIXm 3.1.0 Patient Identity Feed FHIR
  (ITI-104, a conditional `PUT Patient?identifier=` held to the
  `IHE.PIXm.Patient` minimums, plus the Remove Patient Option's conditional
  `DELETE`) and answers ITI-83 `$ihe-pix` from what was fed, with each case of
  ITI TF-2 §3.83.4.2.2. The seed builder feeds it a synthetic patient in the
  `urn:oid:2.999` arc with one `ehr_id` per node domain (`seed::feed`), and a
  capturing proxy in front of it journals and faults its traffic. The server's
  e2e suite now resolves through it, at one member and at both, while the
  failure classes stay on stubbed Managers. The IG's ITI-104 artefacts (the
  Source `CapabilityStatement`, the `Patient` profiles and the example
  Patients) are added to the vendored PIXm corpus.
- The outbound identifier-hygiene gate and the security events of the
  analysis guard (#45, §5.4, N33, CP-26). Right before a request leaves for a
  node, the engine re-reads its AQL text (raw and as the printer escapes a
  literal), its paging members, its URL (raw and percent-decoded) and the
  headers the gateway adds, against the identifiers resolution consumed, and
  refuses to send a request that still carries one: nothing reaches the node,
  the query fails closed, and the refusal names the part of the request and
  never the value. The gate reads past the `ehr_id` literal the rewrite
  scoped the query to, so a short identifier that occurs inside the node's
  own `ehr_id` is answered. Every patient predicate the rewrite strips, every refused
  query and every gate stop is a security event under `ferrofed::security`,
  located by byte range and carrying no identifier. A clinician or facility
  predicate (composer, care facility, performer, committer) is dispatched
  unchanged; the same path compared with the patient identifier is refused.
  `openehr-federation` 0.0.5 adds `Refusal::kind`, `Refusal::at` and
  `PatientQuery::stripped` for those events.
- The resolution step through a PIX Manager (#43). A `[pixm]` table selects
  the PIXm resolver: each `[[pixm.manager]]` names a PIX Manager's FHIR base,
  its credentials, and the `ehr_id` domain of every member it resolves, and
  `[pixm.namespaces]` maps a client's issuing namespace to a PIX assigning
  authority. One ITI-83 call per Manager resolves the patient at its members;
  only a member that knows the patient is asked its node query, a member that
  does not is `not-resolved` in `meta.federation`, a patient known nowhere is
  a `200` with no rows and `complete: false`, and a Manager that cannot answer
  fails the query `424` with no node asked. The patient identifier and its
  namespace reach the PIX Manager only. Boot is refused when a member has no
  Manager, when a member has two, or when `[dev]` and `[pixm]` are both set.
  The `{node, ehr_id}` set of each resolution is held in memory as the client
  session's resolution bindings, bounded by `federation.binding_ttl_ms`
  (default 15 minutes), and nothing derived from a patient identifier is
  stored or logged.
- The PIXm ITI-83 client (#42): `ihe-iti` 0.0.3, feature `pixm`, asks a PIX
  Manager's `Patient/$ihe-pix` for the identifiers other domains hold for a
  patient, held to the vendored PIXm 3.1.0 `OperationDefinition` and read into
  a cross-reference, the profile's not-found answer, or a typed error (an
  unknown source or target domain, a rejection with its issue types, a
  timeout, a transport failure, a malformed answer). A `404` without a
  `not-found` issue never reads as "patient unknown". Identifier values are
  redacted in `Debug` and carried by no error. The IHE PIXm 3.1.0 package
  artefacts of ITI-83 are vendored under `docs/specs/ihe-pixm/` by
  `scripts/vendor/ihe-pixm.sh`, pinned by version and tarball sha256, and the
  FHIR R4 model comes from `fhir-types` 0.1.107.
- The fuzz lane (#134): four `cargo fuzz` targets over the untrusted inputs
  (the AQL rewrite over arbitrary text and parameters, the ITS-REST
  `AdhocQueryExecute` body through the façade's intake, a federated
  `RESULT_SET` with its `meta.federation`, and the `OPTIONS {base}/` body),
  with seeds generated from the vendored golden cases and the specification's
  JSON examples by `scripts/fuzz/seeds.sh`. The rewrite target asserts the
  identifier-hygiene property of §5.4.1 (N33) on every query it accepts,
  string-function reconstructions included. `fuzz.yml` runs the targets
  weekly, on dispatch and on pull requests touching the code they read.
- Both patient-identifier carriers resolve (#44; §5.4.3, N33, CP-38): an
  `ENTRY`-level `subject` predicate, `…/subject/identifiers/id`, is resolution
  input on equal terms with `EHR_STATUS.subject.external_ref`, with the
  `DV_IDENTIFIER` `issuer` or `type` as its issuing namespace. It is consumed
  and stripped exactly as `external_ref` is, so the same patient query through
  either carrier sends the same node query and returns the same rows. The same
  value in both carriers is consumed once; a second, different value, and
  qualifiers that name two namespaces, are refused with a `400`, as are the
  `assigner`, a predicate on the identifier list, an `ENTRY`-level
  `external_ref`, and the carrier selected, ordered on or inside a function.
  `openehr-federation` is 0.0.4, and the interim `Refusal::EntrySubject` is
  gone.
- The release lane at SLSA Build Level 3 (#31): the reusable
  `release-build.yml` builds `ferrofed` per target with `cargo auditable`,
  writes a CycloneDX and a syft SBOM, and attests the tarball's provenance and
  both SBOMs; the reusable `release-image.yml` builds the `linux/amd64` and
  `linux/arm64` image from the attested musl binaries, pushes it to
  `ghcr.io/ferrohealth/ferrofed` by digest, and attests the index and each
  platform manifest. A release is published only when the draft carries all
  eight assets of every target, and a pre-release is never marked latest.
- The first federated query (#38): `POST {base}/v1/query/aql` answers one
  ITS-REST `RESULT_SET` over every member of the registry, with no federation
  syntax needed (N1, CP-1).
  - The gateway types the `query_parameters`, analyses the query with
    `openehr-federation`'s `aql` rewrite, resolves the patient at every member
    through the configured cross-reference, and sends each member that knows
    the patient standard AQL keyed on its own `ehr_id` (N2, N7, CP-2, CP-4).
  - It fans out under the budget and re-injects the selected subject column
    as the resolution input (N5, CP-7). `meta.federation` names every endpoint
    (N16): a member that does not know the patient is `not-resolved`, a
    suspended endpoint and a second endpoint of one member are `excluded`.
  - A refused query is a `400` ITS-REST error that locates the fault and
    quotes nothing (§5.4.3). Without a cross-reference a patient query fails
    closed with `424` (decision A17), and without a registry the route stays
    `501`.
  - The configuration gains `profile`, `[registry] document`, `[federation]`
    (`per_node_timeout_ms`, `overall_timeout_ms` below the request timeout,
    `default_namespace`) and the `[[dev.crossref]]` rows of the development
    profile. `config check` loads the registry too.
  - The compose quickstart mounts `docker/quickstart/registry.toml` and
    `ferrofed.toml`, and the README shows one federated query returning the
    EHR of both nodes. The end-to-end test runs it against FerroEHR and
    EHRbase behind their capturing proxies, and no node request carries the
    patient identifier in any carrier (N33).
- The fan-out engine, first increment (#37): `ferrofed-engine`'s `fanout`
  module sends one request per in-scope node at once, each under a per-node
  deadline cut to the overall budget, with no retry and no hedging (§11.5,
  N38). A node still outstanding when the overall budget runs out is abandoned
  and reported `time-out`, without touching any other request, and its late
  answer contributes nothing. `meta.federation` is built from every outcome
  before the decision, so a failing answer carries it, with the effective
  `timeout` budget and `complete: false`. Under the all-or-nothing default an
  `offline` or `time-out` node fails the query `504`, a `node-error` fails it
  `424`, `504` taking precedence; `not-resolved` and `consent-denied` fail
  nothing, so a patient found nowhere is a `200` with empty rows (§11.3, §11.4,
  N6, N37). Every endpoint record carries its node, `system_id`, managing
  organisation, base URL and `latency_ms` (§9.5, N40), and the rows of the
  `active` nodes are concatenated in endpoint id order until the merge (#52).
- The crates.io lane behind the workspace `publish` switch (#32):
  `publish-crates.yml` runs on every `v*` tag, reads the publishable set from
  `cargo metadata`, packages it and publishes it in dependency order through
  crates.io Trusted Publishing, and is a successful no-op while the root
  `[workspace.package] publish` is `false`. The `publish-dry-run` job packages
  every library crate on every pull request, and
  `scripts/release/publish-crates.sh` is the shared implementation.
- Node dispatch (#34): `ferrofed-engine`'s `dispatch` module builds one
  `openehr-its` client per registry endpoint, rooted at the endpoint's base URL
  as the registry holds it plus the ITS-REST `v1` segment (N28), and sends each
  node query as the generated `POST {base}/v1/query/aql`, once, with the
  per-node deadline and the gateway's request id. The answer maps to exactly
  one §11.1 status: a result set to `active`, a refused connection or broken
  stream to `offline` with its reason, a passed deadline to `time-out`, and a
  documented error, an undocumented status or a body that is not a result set
  to `node-error` carrying the node's own status and message (§11.2, N16,
  N40). A credential the provider cannot produce, or a request the client
  runtime refuses to compose, is a typed dispatch error, never an endpoint
  status. The engine names no HTTP engine directly, and a test holds it.
- The AQL rewrite, first increment (#35): the `aql` feature of
  `openehr-federation` 0.0.3 parses a façade query with `openehr-query`, binds
  its `query_parameters` before analysis, and consumes the
  `EHR_STATUS.subject.external_ref` patient predicate with its namespace into a
  redacted `Subject` (§5.2). Each node receives the query scoped to its own
  `ehr_id` (§7.1, N7, N29); a selected subject or namespace column is
  re-injected after the merge (N5); and `columns[]` is rendered from the façade
  query alone (N17). The rewrite refuses with a typed `400`, never quoting the
  identifier: a predicate outside the top-level `AND` chain or under another
  operator (§7.1), an integer identifier (decision A6), two different patients
  (A7), an unqualified identifier with no default namespace (A5), a query with
  no patient where a localizer decides the node set (A8), paging members that
  disagree with the query (A10), cross-node `OFFSET` (§11.6.2), an undirected
  aggregate (N14), and the identifier anywhere else in the query (§5.4.1),
  including rebuilt by `CONCAT`, `CONCAT_WS` or `SUBSTRING` over split
  literals, which the rewrite folds before the value test; a string function
  over a literal that cannot be folded is refused on an identifier-bearing path
  (decision A4). The reference implementation's 17 golden cases run as a
  corpus, each adjudicated. CI's test lane runs every feature.
- The static registry (#36): `ferrofed-registry` 0.0.1 loads the federation's
  membership from a reviewed TOML bootstrap document (`[[organisation]]`,
  `[[node]]` with its `system_id` and `[[node.identifier]]`, `[[endpoint]]`)
  with `deny_unknown_fields` throughout into an immutable `RegistrySnapshot`.
  A document with a dangling reference, a duplicate id, a `system_id` shared by
  two nodes (compared without ASCII case), an endpoint without exactly one
  managing organisation, a connection type other than `openehr-rest-query`, an
  unusable base URL or a node without an endpoint refuses to load (N19, N20,
  N21, §12b.2). `NodeId`, `EndpointId` and `SystemId` are distinct types with
  no conversion between them (N32, §12a.1).
- The resolver seam and the development cross-reference (#36):
  `ferrofed-identity` 0.0.1 carries `PatientRef`, the patient identifier
  redacted in every rendering and never serialized (§5.4, N33), the `Resolver`
  trait (N3, §5.2), and `StaticResolver`, a fixed table from a synthetic
  identifier to each member's `ehr_id` that a configuration can enable only
  under `profile = "development"`. It is FerroFED's own testing device, not an
  identity binding.
- The container image: `docker/Dockerfile` puts the `ferrofed` musl binary on
  distroless static, digest-pinned, as the numeric non-root user `65532`, with
  no shell and a read-only root; `scripts/release/stage-dist.sh` stages the
  binaries of a published release after checking their checksums. `compose.yaml`
  starts the gateway beside two member CDRs, FerroEHR 4.3.1 and EHRbase 2.36.0,
  every image pinned by digest in `docs/VERSIONS.md` and held there by the
  versions guard (#30).
- The test harness in `tools/ferrofed-testkit` (#39): FerroEHR 4.3.1 and
  EHRbase 2.36.0 as two nodes on their own documented databases, pinned by
  digest and started only behind the `FERROFED_E2E` gate; a capturing and
  fault proxy in front of each node, whose journal records every request
  (method, path, query, headers, body) and which refuses, delays or answers
  with a chosen status per node; and a synthetic seed builder that writes
  EHRs, the template and compositions over ITS-REST alone, with patient
  identifiers only inside the `urn:oid:2.999` example arc. CI runs the
  container suite in its own `e2e (containers)` job, and the versions guard
  holds the image pins to `docs/VERSIONS.md`.

### Fixed

- `ihe-iti` 0.0.4 names PMIR's transactions as ITI-93 and ITI-94 in its
  description, feature list and README. ITI-104, which it attributed to PMIR,
  is the PIXm Patient Identity Feed FHIR (#47).

### Changed

- The repository moved to the FerroHEALTH organization,
  <https://github.com/FerroHEALTH/FerroFED>, with the roadmap board as
  <https://github.com/orgs/FerroHEALTH/projects/1>; every link, the image name
  `ghcr.io/ferrohealth/ferrofed` and the crates' `repository` follow it (#123).
- The crate layout names every crate that may be published after the
  specification it implements, one crate per specification with a feature per
  layer or profile (#106): `openehr-federation` 0.0.1 (the Federation Tier
  wire types, formerly `ferrofed-wire`, with the `aql` and `merge` features),
  `ihe-iti` 0.0.1 (features `pixm`, `pdqm`, `mcsd`, `pmir`, `xcpd`) and
  `nl-generic-functions` 0.0.1 (features `nvi`, `mitz`, `lrza`, `nuts-auth`).
  The names are held on crates.io by 0.0.0 placeholders. `ferrofed-registry`,
  `ferrofed-identity` and `ferrofed-engine` move under `app/` and are never
  published. A new CI job lints every feature of the published crates on its
  own, and the architecture test also fails when a binding crate depends on
  FerroFED.
- The `openehr-*` family moves to 0.0.74, the lockstep release with the AST
  visitor, spans, parameter binding and the federation directive in
  `openehr-query`, and the router builder, operation matcher, credentials
  provider and per-call options in `openehr-its` (FerroEHR #3505 to #3514).

### Security

- A configuration that does not parse no longer quotes its source line
  (#133). The TOML reader's error printed the offending line, so a malformed
  `[dev]` row could put its patient identifier, and a mistyped inline secret
  its value, into the `config check` output and the boot failure. The refusal
  now names the file, the line and column, the key path (`dev` alone for any
  line of the `[dev]` table) and the kind of fault, and none of the text.

## [0.0.1] - 2026-10-01

The first release: the repository setup, the documentation site on
<https://ferrofed.eu/>, the architecture of record, the Cargo workspace and the
server binary's shape (configuration, telemetry, health, readiness and
shutdown). The `ferrofed` binary serves health and readiness only; the
federated query follows from v0.0.2.

### Added

- The Cargo workspace (#28): the root manifest with the family lint set, the
  release profile and the `publish = false` switch every library crate
  inherits; the `openehr-*` family declared as one lockstep pin group at 0.0.72,
  with the versions guard failing when one member moves alone; `deny.toml`; the
  `serde_json::Value` ban in `clippy.toml`; and every crate of the architecture's
  crate map as a documented placeholder, with the `ferrofed` binary over a thin
  library run path and the testkit's pin-matrix reader asserting each crate's
  specification constant against `docs/VERSIONS.md`.
- The server binary shape (#29): `ferrofed serve` and `ferrofed config check`;
  configuration from a TOML file and `FERROFED__` environment overrides with
  unknown keys refused, `_file` siblings for every secret, outbound
  credentials per endpoint id, and a typed refusal (exit 78) on any bad value,
  a broken log filter included; `auto`, `json` and `pretty` console formats;
  `GET /`, `GET /health` and `GET /health/readiness` over an indicator
  registry; the request id, the panic catch (a `500` that carries neither the
  panic message nor anything the client sent), the request timeout and the
  body ceiling; a bounded drain on `SIGTERM`; and `501` for every path of the
  ITS-REST surface until the façade lands. The request log records the
  method, the matched route, the status, the latency and the request id, and
  never a body, the AQL text, a header value, an unmatched path, or a query
  value other than the digit-only `offset` and `fetch`, so a façade query's
  patient identifier reaches no log line at any level (§5.4.3).
- The federation wire types in `ferrofed-wire` (#33): `meta.federation` with
  `complete` derived from the endpoint statuses (§11.4), the per-endpoint
  record whose shape carries the N40 `error` and `latency_ms` obligations of
  each §11.1 status, the `OPTIONS {base}/` self-description with its two
  schema conditionals (§7a.2), the federation header names, and the one seam
  into the ITS-REST `ResultSetMetadata`, which refuses the flat and
  `_`-prefixed members §9.1 forbids (CP-35). Unknown members of every open
  object round-trip. The tests validate every emitted body against the
  vendored schemas, the specification's §9.4 and §7a.2 examples included,
  fail on drift between the schemas and the types, and pin the rules no
  schema states.

- The conformance matrix (#41): `conformance/matrix.tsv` with every
  conformance point of section 17, `conformance/tracks.tsv` with the section
  16.3 tracks (track 8 deferred as provisional) and
  `conformance/requirements.tsv` with the reachability of all 46 requirements,
  derived from the vendored specification by `scripts/conformance/matrix.sh`;
  the `// conformance:` test marker; the tier-1 `conformance-matrix` guard; and
  the matrix rendered into the book's Evaluate part.
- The clinical-path storage rule (#40): an engine test reads the crate graph
  and fails when a library crate reaches a storage implementation or the
  application crate.

### Fixed

- `scripts/checks/crate-version-guard.sh` no longer exits early on the change
  that adds the root `Cargo.toml`, where the base has none.

## [0.0.1-rc.1] - 2026-10-01

A pre-release that rehearses the release lane (#14): the repository setup,
the documentation site and the architecture of record, with no binaries.

### Added

- The architecture of record, `docs/architecture.md`, from the first research
  pass (#16, evidence on #18 to #27): the published `openehr-*` crates as the
  openEHR surface at the planned 0.0.74 pin, the request pipeline, the AQL
  rewrite and identifier hygiene on `openehr-query`'s AST, the façade split
  over the ITS-REST route tables, the identity seams with each binding (PIXm,
  mCSD, PMIR, XCPD, the Dutch Generic Functions) in its own crate, the
  security handoff, the registry and its storage, the fan-out, completeness
  and cross-node merge with the `LIMIT` agreement check, the hand-written wire
  types, the crate map with the `publish` switch, the conformance instrument,
  the test topology, the milestone map, and the decision register, every
  entry decided by the owner on 2026-10-01.
- The documentation site on ferrofed.eu: a landing page at `/` and an mdBook
  under `/docs/` organised by reader intent (Evaluate, Operate, Integrate,
  Contribute), built on every pull request and deployed from `main` by
  `docs.yml`, with the pinned docs toolchain, the vendored mermaid assets, the
  favicon set and its `favicon-sync` guard, and the README badge block (#12).
- The release lane, `.github/workflows/release.yml`: a signed `v*` tag is
  checked against `CITATION.cff` and the product row of `docs/VERSIONS.md`,
  the version's `CHANGELOG.md` section becomes the release notes, and the
  release is created as a draft and published once the asset set it promises
  (none before there is code) is verified. `docs/release.md` is the cut (#14).
- The Business Source License 1.1 with Vernum Projecten B.V. as the Licensor
  and copyright holder (#1), and the contribution-licence terms, the
  pull-request checkbox and the `contribution-licence-guard` check (#3).
- The working discipline under `.claude/`: the rules, the hooks (no AI
  attribution, dangerous-command guard, format and comment-style on edit,
  the SessionStart tracker summary), the issue-loop skills, the
  `spec-researcher` and `implementer` agents, and the tracked project memory.
  `CLAUDE.md` states the product, the design-phase status and the hard rules
  (#6), with the identifier-hygiene rule for everything the gateway composes
  for a node.
- The vendored specification corpora under `docs/specs/`, each with a
  `PROVENANCE.md` and a fetch script under `scripts/vendor/`: the Federation
  Tier with AQL specification at v0.9.0 (release candidate, CC0 1.0), its
  reference implementation (Apache-2.0) as evidence, the openEHR ITS-REST
  1.1.0 OpenAPI documents and the openEHR AQL 1.1.0 source with its grammar
  (#7).
- One pin matrix, `docs/VERSIONS.md`, and `scripts/checks/versions.sh`, the
  guard that fails on drift between the matrix and every file repeating a pin
  (#8).
- The GitHub setup: issue and pull-request templates, CODEOWNERS, Dependabot,
  the label set and tracker helpers under `scripts/gh/`, and the CI workflows
  (the tier-1 guards with the Rust tier gated until a workspace exists, one
  `conclusion` check, CodeQL, Scorecard and SonarQube Cloud), and
  `docs/ci-cd.md`, the design of the workflows and the repository settings
  (#9, #10, #11).
- The community and governance set: `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`,
  `SUPPORT.md`, `AI_STATEMENT.md`, `CITATION.cff`, `llms.txt`, and the root
  toolchain, format and lint configuration (#15).

[Unreleased]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.13...HEAD
[0.0.13]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.12...v0.0.13
[0.0.12]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.11...v0.0.12
[0.0.11]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.10...v0.0.11
[0.0.10]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.9...v0.0.10
[0.0.9]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.8...v0.0.9
[0.0.8]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.7...v0.0.8
[0.0.7]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.3...v0.0.6
[0.0.3]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1...v0.0.3
[0.0.2-rc.1]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1...v0.0.2-rc.1
[0.0.1]: https://github.com/FerroHEALTH/FerroFED/compare/v0.0.1-rc.1...v0.0.1
[0.0.1-rc.1]: https://github.com/FerroHEALTH/FerroFED/releases/tag/v0.0.1-rc.1
