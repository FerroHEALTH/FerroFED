<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Cybersecurity risk assessment

This page is the cybersecurity risk assessment Cadasto B.V. keeps for
FerroFED under Regulation (EU) 2024/2847, the Cyber Resilience Act (CRA).
Art 13(2) has the manufacturer "undertake an assessment of the cybersecurity
risks associated with a product with digital elements" and take its outcome
into account "during the planning, design, development, production, delivery
and maintenance phases". Art 13(3) says what it holds: "an analysis of
cybersecurity risks based on the intended purpose and reasonably foreseeable
use, as well as the conditions of use", whether and how each point of Annex
I, Part I, point (2), applies and is implemented, and how Part I, point (1),
and the vulnerability handling of Part II are applied. Annex VII, point 3,
makes it part of the technical documentation.

The manufacturer's side of the CRA is the same for every FerroHEALTH
product and is in the family book: who the manufacturer is
([The manufacturer](https://ferrohealth.eu/docs/manufacturer.html)), what
applies from when and how conformity is assessed
([The Cyber Resilience Act](https://ferrohealth.eu/docs/cra.html)), and the
vulnerability-handling policy
([Security](https://ferrohealth.eu/docs/security.html)). This page is
FerroFED's own product-level assessment.

Every quotation of the CRA is from the Official Journal text, vendored at
[`docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml)
(<http://data.europa.eu/eli/reg/2024/2847/oj>), and Art 104 of Regulation
(EU) 2025/327 (the EHDS Regulation) is read from
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
No conformity assessment has been carried out. A row below describes the
evidence the repository holds, never conformity. This page is not legal
advice.

## Date and revision

This assessment was made on **2026-10-15**, against the code of FerroFED
0.0.10 and the `main` branch after it, and last reviewed on **2026-10-25**,
against FerroFED 0.0.13. It is updated with each release: the
version-bump pull request reviews every row against what the release
changes and adds a line to [Revisions](#revisions)
([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
Before the tag, step 5). A vulnerability report, a security advisory or a
change to the [threat model](threat-model.md) reopens the rows it touches
between releases (Art 13(7)). The assessment that applies to a release is
this page at that release's tag.

## One assessment, read under two acts

FerroFED is an EHR system under the EHDS Regulation
([Classification](regulatory-status.md#classification-an-ehr-system)). EHDS
Art 104(1) replaces CRA Art 13(4), so that for a product of CRA Art 32(5a)
"the cybersecurity risk assessment may be part of the risk assessment
required by those Union legal acts". This page uses that: it builds on two
records that already exist, and copies neither.

- **The [threat model](threat-model.md)** names the assets, the trust
  boundaries, the attacker positions, and per boundary each threat with the
  setting and the test that answers it. It covers the whole gateway.
- **The [clinical safety risk file](clinical-safety.md)** is the EHDS-side
  risk assessment of the two harmonised software components, with each
  hazard rated before and after its control.

This page reads both against Annex I of the CRA and states what neither
says on its own: whether each requirement applies, and the justification
where it does not.

## Scope

- **The product.** Each tagged release is one product with digital
  elements, in the forms on
  [Forms placed on the market](technical-documentation.md#forms-placed-on-the-market):
  the source at the tag, the `ferrofed` tarballs, the gateway image and the
  operator console image. The library crates reach a user only inside
  those forms, so they are assessed with them.
- **When.** Annex I binds the releases placed on the market from 11
  December 2027 (Art 71(2), Art 69(2)). The assessment runs now so that a
  gap is known before that date.
- **Intended purpose.** As stated on
  [Regulatory status](regulatory-status.md#intended-purpose): a federation
  gateway used by healthcare providers, through their client applications,
  to read and write a patient's openEHR records across the clinical data
  repositories (CDRs) of a federation. It holds no clinical data of its
  own.
- **Reasonably foreseeable use and misuse.** The misuse the clinical safety
  file names ([Scope](clinical-safety.md#scope)): a wrong configuration, a
  client that ignores `meta.federation`, a member that breaks the
  conditions of membership. For security, also: the development profile run
  near real data, a listener reached without the TLS hop the deployment is
  meant to supply, and a release run after a newer one is published, which
  receives no fix ([Hardening](../operate/hardening.md)).
- **Operating environment.** Linux on `x86_64` or `aarch64`, as the image
  or the binary, behind a reverse proxy, with the services it consumes:
  the member CDRs, the callers' issuers, the identity services and an Audit
  Record Repository
  ([The services a gateway consumes](../operate/deployment-shape.md#the-services-a-gateway-consumes)).
- **Time in use.** Art 13(3) asks the assessment to take into account "the
  length of time the product is expected to be in use". Security fixes are
  published, in new releases, for five years after each release date, and
  only the newest release is supported
  ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)),
  set on the expected time in use that
  [The basis of the five years](regulatory-status.md#the-basis-of-the-five-years)
  records, and the rows below hold for that period.
- **Assets.** The eight assets of the threat model
  ([Assets](threat-model.md#assets)). The patient identifier and the
  routing state weigh most: a leak of the first is a privacy breach, and a
  wrong route sends a request to another patient's record.

## How the risks were analysed

The threat model applies STRIDE per trust boundary
([Trust boundaries](threat-model.md#trust-boundaries)). The clinical safety
file rates severity and occurrence after the method of EN ISO 14971:2019
([Method](clinical-safety.md#method)). The CRA names no method, so no
specification governs the method: our own design. This assessment applies
no harmonised standard, common specification or European cybersecurity
certification scheme (Annex VII, point 5).

## Annex I, Part I, point (1)

"Products with digital elements shall be designed, developed and produced
in such a way that they ensure an appropriate level of cybersecurity based
on the risks." The level is the one the threat model sets: every threat it
names has a control and a test, or is named as a risk the deployment
carries, with the control the operator supplies
([Risks the deployment carries](threat-model.md#risks-the-deployment-carries)).

## Annex I, Part I, point (2), point by point

| Point | Applies | How FerroFED implements it | Where |
|---|---|---|---|
| (a) no known exploitable vulnerabilities | yes | `cargo deny check` reads the RustSec advisory database and fails on any advisory or yanked crate; it is part of the required check on every change to `main`, from which a release is tagged. One exception in `deny.toml` is an unmaintained compile-time macro of the console with no runtime code (RUSTSEC-2024-0436, `paste`), and its OpenVEX statement under `security/vex/` gives the argument. Each release scans both images and the binaries with Trivy at every severity, fixed or not, and is refused while a finding has no OpenVEX statement that judges it; each image is scanned for fixable HIGH and CRITICAL findings before it is tagged, and the published images are scanned every week. Dependabot proposes updates; fuzz targets run over the untrusted inputs; each binary embeds its dependency list (`cargo auditable`); the console's site build runs its WebAssembly tools only at pinned releases checked against their SHA-256 | [Checks and gates](../contribute/checks-and-gates.md); [`deny.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/deny.toml) |
| (b) secure by default, and reset | yes | `profile` defaults to `production`; the development profile must be set, takes a restart and prints a red notice. There is no unauthenticated mode; the binary listens on loopback; the admin listener is off; a credential or a patient identifier travels only over `https`; the audit destination must be chosen once a registry is configured. See [Reset](#reset-to-the-original-state) | [Configuration](../operate/configuration.md); [What must travel encrypted](../operate/configuration.md#what-must-travel-encrypted); [B8](threat-model.md#b8-configuration-and-secrets) |
| (c) security updates | yes, automatic installation excepted | a security fix ships as a patch release of the newest release, the only one supported, with the fix alone, or with the reason a functional change ships with it stated in the notes; users hear of it from the advisory and the release feed; the gateway names its support end date and says when it has passed. See [Automatic security updates](#automatic-security-updates) | [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions); [Upgrading](../operate/upgrading.md) |
| (d) protection from unauthorised access, and reporting it | yes | every request is authenticated before the façade reads it; scopes per route; the operator surface and the admin write actions need the operator scope; the ADMIN API is refused. Every refusal is logged as `caller-refused` under `ferrofed::security` | [B1](threat-model.md#b1-client-to-gateway), [B5](threat-model.md#b5-the-admin-listener); [Client authentication](../operate/authentication.md) |
| (e) confidentiality | yes | in transit: `https` for every credential and patient identifier outside development, mutual TLS and DPoP where configured, TLS on the listener as an option, and the OTLP export of traces and metrics to an `https://` collector, with mutual TLS where configured. At rest: no clinical data is written to disk; secrets are read from files, redacted and zeroised, and a secret the operator console sets is written to a new file of mode `0600` and held in a configuration revision only as its reference; the audit spool is `0700` and is encrypted by the volume, which the deployment supplies | [B2](threat-model.md#b2-gateway-to-member-node), [B6](threat-model.md#b6-audit-and-telemetry-sinks); [Hardening](../operate/hardening.md#tls) |
| (f) integrity, and reporting corruptions | yes | each release is built in an isolated lane with signed provenance, verified with `gh attestation verify`; the image runs read-only; `config check` refuses a defective configuration and a refused reload keeps the running one; two members claiming one `ehr_id` is `409` and an integrity incident, counted and alerted | [Integrity incidents](../operate/registry.md#integrity-incidents); [The image](../operate/container.md#the-image) |
| (g) data minimisation | yes | the patient identifier is consumed at the gateway and never reaches a node (§5.4, N33); clinical content stays in memory for one request; logs, metrics and spans carry no body, query text, header value or patient identifier | [Where the patient identifier stops](../how-it-works/identifier-hygiene.md); [Processing inventory](data-protection.md#processing-inventory) |
| (h) availability, also after an incident | yes | a concurrency limit, a per-caller rate, a body limit and a request timeout; per-node and overall budgets; several replicas and a bounded drain on stop; after a restart the routing state is learned again from the next resolutions and answers, and the spool delivers what it holds | [Overload protection](../operate/overload.md); [Running several replicas](../operate/deployment-shape.md#running-several-replicas); [Health probes](../operate/health.md#stopping-without-dropping-a-request) |
| (i) minimal impact on other devices and networks | yes | one federated query is a request to every member, so the gateway caps requests in flight per member, bounds each answer it reads, and reads an issuer's key set again at most once per `auth.jwks_refresh_interval_seconds` | [The per-member cap](../operate/overload.md#the-per-member-cap); [The answer bound](../operate/overload.md#the-answer-bound); [B7](threat-model.md#b7-the-callers-issuers) |
| (j) limited attack surface | yes | an ITS-REST path the gateway does not serve answers `501`; only the health family, `GET {base}/` and the JWK Set are open; the admin listener is off unless set and on loopback unless allowed; the image has no shell; a build can leave out a regional binding | [B1](threat-model.md#b1-client-to-gateway); [The bindings and their features](../operate/configuration.md#the-bindings-and-their-features) |
| (k) exploitation mitigation | yes | Rust with `unsafe` code forbidden in every crate and overflow checks in the release build; the image runs as a non-root user with every capability dropped; the outbound gate stops a finished request that carries a consumed identifier and fails closed | [The container](../operate/hardening.md#the-container); [What the outbound gate reads](../how-it-works/identifier-hygiene.md#what-the-outbound-gate-reads) |
| (l) recording and monitoring, with an opt-out | yes, the opt-out in part | every access to patient data is recorded with the verified caller; IHE transactions are audited over ITI-20; refusals and admin write actions are logged under `ferrofed::security`; metrics and traces are exported when turned on. See [The opt-out](#the-opt-out-for-recording) | [The access log](../operate/access-log.md); [The audit trail](../operate/audit.md); [Metrics](../operate/metrics.md) |
| (m) secure removal of data and settings, and secure transfer | yes | the gateway keeps its configuration, registry, secrets, the stored-query store, the audit spools and, under `config.ui = "managed"`, the configuration revisions; each is a file or a database schema the operator removes. The learned routes are exported for a replacement product | [What the gateway keeps](../operate/deployment-shape.md#what-the-gateway-keeps); [Exporting the identity state](../operate/identity-state-export.md); [Decommissioning](cra-user-information.md#8d-secure-decommissioning) |

### Reset to the original state

Point (b) asks for "the possibility to reset the product to its original
state". The binary and the image do not change while they run: the image's
root filesystem is read-only. The gateway's state is what the operator gives
it and what it keeps: the configuration and registry files, the secrets,
the stored-query store and the audit spools. The resolution bindings, the
`ehr_id` index and the learned routes are held in memory and lost on a
restart ([What the gateway keeps](../operate/deployment-shape.md#what-the-gateway-keeps)).
Starting the release with the example configuration it attaches, an empty
stored-query store and empty spools returns it to the state it shipped in.
Empty a spool only once the dependency report shows `audit_repository` and
`audit_feed` as `up`, or its records are lost
([Losing a spool](../operate/container.md#losing-a-spool)).

### Automatic security updates

Point (c) asks for automatic security updates "where applicable", and Annex
I, Part II, point (7), asks for automatic distribution "where applicable
for security updates". They do not apply to FerroFED, and the gateway
installs no update and asks no server of the manufacturer for one:

- An upgrade can need a configuration change: a key made mandatory or a
  value now refused, listed in the release's upgrade notes, which
  `config check` verifies before the new release runs
  ([Before you upgrade](../operate/upgrading.md#before-you-upgrade)).
- An upgrade can migrate the stored-query store, which an older release
  then refuses, so the operator backs it up first.
- The gateway sits on the path of every clinical query of a federation. An
  update on the manufacturer's schedule would restart it at a moment no
  operator chose, and a channel from the manufacturer into the deployment
  would be a new inbound path.

The operator installs each update, and is told of it by the channels of
[Hearing of an update](https://ferrohealth.eu/docs/security.html#hearing-of-an-update)
and by the gateway itself once the end date of its release has passed.

### The opt-out for recording

Point (l) asks for recording "with an opt-out mechanism for the user". The
log level, the metrics surface and the trace export are the operator's to
set, and the metrics and the traces are off by default
([Metrics](../operate/metrics.md#turning-it-on),
[Tracing](../operate/tracing.md#turning-it-on)). The record of each access
to patient data has no opt-out outside the development profile: EHDS
Annex II, point 3.2, has the European logging software component "record at
least the following information on every access event or group of events",
and the gateway refuses an `[audit] destination` of `off` or `log` outside
development once a registry is configured
([`[audit]`](../operate/audit.md#audit)). Where the two acts meet, the EHDS
requirement decides, and no opt-out is offered for those records.

## Annex I, Part II: vulnerability handling

Part II sets the manufacturer's processes, and the family book's
[Security](https://ferrohealth.eu/docs/security.html) page states them for
every product. What is FerroFED's own:

- **Point (1), the SBOM.** Each release attaches a CycloneDX SBOM of the
  source graph and an SPDX SBOM of the shipped binary per tarball, and
  attests an SPDX SBOM per platform of each image
  ([The release binaries](../operate/container.md#the-release-binaries)).
- **Point (2), remediation.** Point (2) asks that "where technically
  feasible, new security updates shall be provided separately from
  functionality updates". A security fix ships as a patch release of the
  newest release, the only one supported, with the fix alone; when a
  functional change has to ship with it, the release notes open its
  Security section with the reason
  ([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)).
- **Point (3), tests and reviews.** The required checks run on every
  change; the leakage track 10 and the identifier-integrity track 11 run
  against the gateway ([Conformance matrix](conformance.md)); CodeQL and
  the fuzz targets run beside them.
- **Points (4) to (6) and (8).** The advisories, the disclosure policy and
  the reporting address are the family book's, applied through
  [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md).
- **Point (7), secure distribution.** Each release is built in a reusable
  workflow from a cold checkout, signed through Sigstore, and verifiable by
  digest; a published release cannot be changed
  ([The image](../operate/container.md#the-image)).

## Residual risk

The risks the gateway cannot close alone, each with the control the
deployment supplies or the issue that tracks it, are the threat model's
[Risks the deployment carries](threat-model.md#risks-the-deployment-carries).
The residual risk of the two harmonised components is the clinical safety
file's [Overall residual risk](clinical-safety.md#overall-residual-risk).
The operator reads both before a deployment goes live, and the
[instructions for use](../operate/instructions-for-use.md#limitations) name
the ones a professional user needs to know.

## Revisions

| Revision | Date | Release | Change |
|---|---|---|---|
| 1 | 2026-10-15 | 0.0.10, and `main` before 0.0.11 | first assessment |
| 2 | 2026-10-15 | `main` before 0.0.11 | Annex I, Part II, point (2): a security fix ships as its own patch release on each supported line, with no feature changes in it |
| 3 | 2026-10-16 | 0.0.11 | every row reviewed against the release: point (a) adds the pinned and checked tools of the console's site build; point (e) the write-only secrets of configuration revisions; point (m) the revision store among what the gateway keeps. Configuration revisions and the console's admin actions run on the admin listener behind the operator scope, as point (d) already says, and a configuration or registry refusal names no credential carried in a URL. No point changes whether it applies |
| 4 | 2026-10-19 | `main` before 0.0.12 | Annex I, Part II, point (2), and the time in use: only the newest release is supported, and a security fix ships as a patch release of it, with no feature changes in it |
| 5 | 2026-10-23 | `main` before 0.0.12 | Annex I, Part II, point (2): a security fix ships alone where technically feasible, and a release that ships one beside functional changes opens its Security section with the reason; every Security entry names its GitHub security advisory or VEX statement, and the release lane refuses a section that does not |
| 6 | 2026-10-24 | 0.0.12 | every row reviewed against the release: point (a) adds the release's full image and binary scans with the OpenVEX record, the scan before an image is tagged and the weekly scan, and the console's site build needs no CSS tool; point (d) holds with client authentication on `ferrohealth-smart`, every token that fails a check answered with one `invalid_token` challenge and every static bearer token removed, so a machine authenticates with HTTP Basic or the OAuth 2.0 client credentials grant; point (e) holds with the Argon2id hashes of the Basic users' passwords; the gateway and console images no longer trust the compromised emSign intermediates (GHSA-w3hc-w475-673c) and `ctutils` 0.4.4 fixes RUSTSEC-2026-0385 (GHSA-rv83-4vc9-vqxg). No point changes whether it applies |
| 7 | 2026-10-25 | 0.0.13 | every row reviewed against the release: point (e) adds the OTLP export over TLS, with private trust roots and mutual TLS, so a remote collector needs no forwarder beside the gateway; point (d) holds with the running configuration the admin listener answers behind the operator scope, which names a secret by its file and never by its value; point (j) holds with the Kubernetes package's NetworkPolicy, which admits only the reverse proxy, the scraper and the console to the gateway; Annex I, Part II, points (4) to (6) hold with the response times `SECURITY.md` now states, those of every product of the family. No point changes whether it applies |
