<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Technical documentation

Art 30(1)(c) of Regulation (EU) 2025/327 on the European Health Data Space
(the EHDS Regulation) has the manufacturer "draw up the technical
documentation of their EHR systems in accordance with Article 37 before
placing those EHR systems on the market, and subsequently keep it up to
date". Art 37(2) says what it is for: it "shall demonstrate that the EHR
system complies with the essential requirements laid down in Annex II and
provide market surveillance authorities with all the necessary information
to assess the conformity of the EHR system with those requirements", and
"shall contain, as a minimum, the elements set out in Annex III and a
reference to the results obtained from a European digital testing
environment referred to in Article 40".

This page is the index of FerroFED's technical documentation. Each element
of Annex III is documented on the page or file the index names, and the
technical documentation of a release is those pages and files as they stand
at the release's tag. The index is kept as
[`conformance/technical-documentation.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/technical-documentation.tsv),
and `scripts/checks/technical-documentation.sh` fails a change that leaves
an element without a row, a row whose file or heading does not exist, or an
element phrase the vendored Regulation does not hold. CI runs it on every
change and the release lane runs it before a release is created.

Every quotation of the Regulation is from the Official Journal text, vendored
at
[`docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-ehds/reg-eu-2025-327-en.xhtml).
The dates the obligations apply from are on the
[regulatory status](regulatory-status.md#the-obligations-and-their-dates)
page. This page is not legal advice.

## The Annex III index

Annex III asks for these elements "as applicable to the harmonised software
components of an EHR system in the relevant EHR system". A status of
partial or planned names the issue that completes it.

| Item | What Annex III asks | Where FerroFED documents it | Status |
|---|---|---|---|
| 1(a) | "its intended purpose, and the date and version of the EHR system" | [Intended purpose](regulatory-status.md#intended-purpose); the information sheet, [(b) Name, version and date of release](information-sheet.md#b-name-version-and-date-of-release) | documented |
| 1(b) | "the categories of personal electronic health data that the EHR system has been designed to process" | the information sheet, [(d) The categories of data](information-sheet.md#d-the-categories-of-data) | documented |
| 1(c) | how it "interacts or can be used to interact with hardware or software that is not part of the EHR system itself" | [The services a gateway consumes](../operate/deployment-shape.md#the-services-a-gateway-consumes); [the client contract](../integrate/client-contract.md); [the patient summary over FHIR](../integrate/patient-summary.md); [prescriptions and dispensations over FHIR](../integrate/prescriptions.md) | documented |
| 1(d) | "the versions of relevant software or firmware and any requirement related to version update" | [Pinned versions](versions.md) and [`docs/VERSIONS.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/VERSIONS.md); [the compatibility policy](../operate/upgrading.md#the-compatibility-policy) | documented |
| 1(e) | "the description of all forms in which the EHR system is placed on the market or put into service" | [Forms placed on the market](#forms-placed-on-the-market) | documented |
| 1(f) | "the description of hardware on which the EHR system is intended to run" | [The hardware it runs on](#the-hardware-it-runs-on); [Supported platforms](../operate/deployment-shape.md#supported-platforms) | documented |
| 1(g) | "a description of the system architecture explaining how software components build on or feed into each other", with labelled diagrams | [The parts of the gateway](../how-it-works/overview.md#the-parts-of-the-gateway), with its diagrams; the design of record, [`docs/architecture.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/architecture.md) | documented |
| 1(h) | the technical specifications, "including a detailed description of the data structures, storage and input/output of data" | the standards, [(e) Standards, formats and specifications](information-sheet.md#e-standards-formats-and-specifications); [what a client gets back](../integrate/client-contract.md#what-a-client-gets-back); [what the gateway keeps](../operate/deployment-shape.md#what-the-gateway-keeps); [Configuration](../operate/configuration.md); [what a revision holds](../operate/configuration-revisions.md#what-a-revision-holds); [Sizing](../operate/instructions-for-use.md#sizing) | documented |
| 1(i) | "a description of any change made to the system throughout its lifecycle" | the [changelog](https://github.com/FerroHEALTH/FerroFED/blob/main/CHANGELOG.md); [Changes to the harmonised components](#changes-to-the-harmonised-components); the clinical safety [review table](clinical-safety.md#review-and-change) | documented |
| 1(j) | "the instructions for use for the user and, where applicable, installation instructions" | [Instructions for use](../operate/instructions-for-use.md); [A production deployment](../operate/production.md) | documented |
| 2 | "A detailed description of the system in place to evaluate the EHR system performance, where applicable" | [Metrics](../operate/metrics.md); [Scoring a deployment](../operate/conformance-run.md); the [clinical safety method](clinical-safety.md#method) | documented |
| 3 | "The references to any common specification used in accordance with Article 36" | none is adopted yet ([The obligations and their dates](regulatory-status.md#the-obligations-and-their-dates)) | planned, [#526](https://github.com/FerroHEALTH/FerroFED/issues/526) |
| 4 | "The results and critical analyses of all verifications and validation tests" | the [Annex II checklist](annex-ii.md), the [conformance statement](conformance-statement.md), the [conformance matrix](conformance.md), the [obligations checklist](obligations.md) and the [clinical safety risk file](clinical-safety.md); the Annex II checklist has points still partial, each with its issue ([What is missing](annex-ii.md#what-is-missing)) | partial, [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) |
| 5 | "A copy of the information sheet referred to in Article 38" | the [information sheet](information-sheet.md), attached to each release | documented |
| 6 | "A copy of the EU declaration of conformity" | [The EU declaration of conformity](#the-eu-declaration-of-conformity), attached to each release as `ferrofed-vX.Y.Z-eu-declaration-of-conformity.md` once the manufacturer issues it; the generator [`scripts/release/declaration-of-conformity.sh`](https://github.com/FerroHEALTH/FerroFED/blob/main/scripts/release/declaration-of-conformity.sh) | partial, [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) |
| 37(2) | "a reference to the results obtained from a European digital testing environment referred to in Article 40" | [The European digital testing environment](#the-european-digital-testing-environment) | planned, [#675](https://github.com/FerroHEALTH/FerroFED/issues/675) |

## The Cyber Resilience Act's part

FerroFED is also a product with digital elements under Regulation (EU)
2024/2847, the Cyber Resilience Act
([The Cyber Resilience Act](regulatory-status.md#the-cyber-resilience-act)).
Art 104(2) of the EHDS Regulation replaces that Act's Art 31(3), so for an
EHR system "a single set of technical documentation shall be drawn up
containing the information referred to in Annex VII and the information
required by those Union legal acts". The index below is that Act's part of
this set. It is kept as
[`conformance/cra-technical-documentation.tsv`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/cra-technical-documentation.tsv),
and `scripts/checks/technical-documentation.sh` holds it as it holds the
Annex III index, against the vendored Act at
[`docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml).
Annex VII asks for each point "as applicable to the relevant product with
digital elements". A status of partial names the issue that completes it.

| Annex VII | What it asks | Where FerroFED documents it | Status |
|---|---|---|---|
| point 1(a) | "its intended purpose" | [Intended purpose](regulatory-status.md#intended-purpose) | documented |
| point 1(b) | "versions of software affecting compliance with essential cybersecurity requirements" | [Pinned versions](versions.md) and [`docs/VERSIONS.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/VERSIONS.md) | documented |
| point 1(c) | photographs or illustrations, "where the product with digital elements is a hardware product" | FerroFED is software ([3. Identification](cra-user-information.md#3-identification)) | not applicable |
| point 1(d) | "user information and instructions as set out in Annex II" | [CRA information to the user](cra-user-information.md) | documented |
| point 2(a) | "necessary information on the design and development of the product with digital elements", with the system architecture | [The parts of the gateway](../how-it-works/overview.md#the-parts-of-the-gateway); the design of record, [`docs/architecture.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/architecture.md) | documented |
| point 2(b) | "necessary information and specifications of the vulnerability handling processes put in place by the manufacturer", with the SBOM, the disclosure policy, the contact address and the secure distribution of updates | [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md), with the family book's [Security](https://ferrohealth.eu/docs/security.html); [The release binaries](../operate/container.md#the-release-binaries); [The image](../operate/container.md#the-image); the CRA Art 14 procedure of [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md) | documented |
| point 2(c) | "necessary information and specifications of the production and monitoring processes of the product with digital elements and the validation of those processes" | the release lane, [`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md); the required checks, [`docs/ci-cd.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/ci-cd.md); post-market monitoring, [`docs/post-market.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/post-market.md) | documented |
| point 3 | "an assessment of the cybersecurity risks against which the product with digital elements is designed, developed, produced, delivered and maintained pursuant to Article 13" | [Cybersecurity risk assessment](cra-risk-assessment.md) | documented |
| point 4 | "relevant information that was taken into account to determine the support period pursuant to Article 13(8)" | [The support period](regulatory-status.md#the-support-period) and [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions) state the period, and [The basis of the five years](regulatory-status.md#the-basis-of-the-five-years) answers each matter Art 13(8) names | documented |
| point 5 | "a list of the harmonised standards applied in full or in part", or the solutions adopted where none is applied | the solutions, point by point, in the [cybersecurity risk assessment](cra-risk-assessment.md#annex-i-part-i-point-2-point-by-point); the specifications applied, [(e) Standards, formats and specifications](information-sheet.md#e-standards-formats-and-specifications); no list of harmonised standards | partial, [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) |
| point 6 | "reports of the tests carried out to verify the conformity of the product with digital elements and of the vulnerability handling processes" | the [conformance statement](conformance-statement.md), the [conformance matrix](conformance.md) and the tests the [threat model](threat-model.md) names; no report against Annex I | partial, [#525](https://github.com/FerroHEALTH/FerroFED/issues/525) |
| point 7 | "a copy of the EU declaration of conformity" | [The EU declaration of conformity](#the-eu-declaration-of-conformity); none is issued yet | partial, [#940](https://github.com/FerroHEALTH/FerroFED/issues/940) |
| point 8 | "where applicable, the software bill of materials", on a reasoned request of a market surveillance authority | [The release binaries](../operate/container.md#the-release-binaries): every release attaches its SBOMs | documented |

The single set of technical documentation for both acts is planned under
[#525](https://github.com/FerroHEALTH/FerroFED/issues/525).

## Forms placed on the market

Each release is placed on the market by Cadasto B.V. in these forms, all
from the same signed tag `vX.Y.Z` on
[the releases page](https://github.com/FerroHEALTH/FerroFED/releases):

- **The source code** at the tag, under the Business Source License 1.1
  ([Licensing](licensing.md)).
- **The gateway binary** `ferrofed`, as one tarball per target:
  `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-gnu` and `aarch64-unknown-linux-musl`. Each carries
  its checksum, two SBOMs and its signed provenance
  ([The release binaries](../operate/container.md#the-release-binaries)).
- **The gateway image** `ghcr.io/ferrohealth/ferrofed`, for `linux/amd64`
  and `linux/arm64`, tagged with the version and, for a release that is not
  a pre-release, `<major>.<minor>` and `latest`
  ([The image](../operate/container.md#the-image)).
- **The operator console image** `ghcr.io/ferrohealth/ferrofed-viewer`, for
  the same two platforms ([The operator console](../operate/operator-console.md)).
  The console holds neither harmonised component.
- **The files attached to the release:** the gateway compose file with its
  example `ferrofed.toml` and `registry.toml`, the nginx reverse proxy
  configuration, the conformance seed data, the Grafana dashboard and the
  Prometheus alert rules, the copies of the information sheet and the
  instructions for use, and the EU declaration of conformity once the
  manufacturer issues it.

The library crates under `crates/`, `ehds-logging` and `eehrxf` among them,
are not published to crates.io; each reaches a user only inside the forms
above.

An EHR system is also "put into service" when someone else hosts FerroFED
as a service or a health institution builds and runs it for itself (Art
26(2)); [A deployment a health institution runs for
itself](regulatory-status.md#a-deployment-a-health-institution-runs-for-itself)
says what that means for the release it was built from.

## The hardware it runs on

FerroFED runs on Linux on `x86_64` and `aarch64` processors, as the binary
on glibc or musl, or as the container image under a container runtime such
as Docker or Kubernetes. It runs on Unix only, and nothing is built or
tested for Windows ([Supported platforms](../operate/deployment-shape.md#supported-platforms)).
Its signing keys and secrets are files, and it writes to local disk only
the embedded stored-query file and the audit spools, where a deployment
configures them ([The audit spool](../operate/container.md#the-audit-spool)).

The release sets no minimum processor or memory. The
[measured footprint](../operate/container.md#memory-footprint) of the
quickstart is a few MiB of memory for the gateway, and the
[sizing guidance](../operate/instructions-for-use.md#sizing) bounds the
concurrent work from what you measure. Several replicas that accept stored
queries share one PostgreSQL database
([Running several replicas](../operate/deployment-shape.md#running-several-replicas)).

## Changes to the harmonised components

Art 30(2) has the manufacturer "ensure that procedures are in place to
ensure that the design, development and deployment of the harmonised
software components of an EHR system continue to comply with the essential
requirements laid down in Annex II", and says that "changes in EHR system
design or characteristics with regard to the harmonised software components
of an EHR system shall be adequately taken into account and reflected in the
technical documentation".

The harmonised software components (Art 25(1)) and the code that serves them
are these paths:

- `crates/ehds-logging`, the European logging software component;
- `app/ferrofed-server/src/access`, which builds its access record;
- `crates/eehrxf`, the European interoperability software component;
- `app/ferrofed-eehrxf`, its stored section queries and its documents;
- `app/ferrofed-server/src/fhir`, the FHIR face that serves it.

A release that changes any of them since the previous release cannot be
cut until it is reviewed. `scripts/checks/technical-documentation.sh`
compares the version being built with the newest release tag below it, and
when a file under these paths differs, it fails unless the
[clinical safety review table](clinical-safety.md#review-and-change) has a
row whose release is that version and the table below has a row for it. A
routine pull request keeps the version of the last release, whose rows
exist, so it is not held up; the version-bump pull request and the `plan`
job of the release lane are where the check stops a release
([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
Before the tag).

| Release | Date | What changed in the harmonised components | Review |
|---|---|---|---|
| 0.0.11 | 2026-10-16 | the access record's labels per category and origin, the patients a population query read, the version a write and the EHR a create left, insertions by the person or a representative, and the log of each semantic transformation; the prescription and dispensation datasets and their search, read, version read and receipt on the FHIR face; a person's own patient summary; archetype-local codes and the preferred code systems; the category a received document takes from its template | the clinical safety review of 2026-10-16 and the [claims review](claims-review.md) findings of 2026-10-16 |
| 0.0.10 | 2026-10-15 | first released: the logging component and the access log, the interoperability component with the patient summary on the FHIR face, the document list, the receipt of a document, the section queries and the terminology rules | the clinical safety review of 2026-10-15 and the [claims review](claims-review.md) findings of 2026-10-15 |

## Keeping it for ten years

Art 30(3) has the manufacturer "keep the technical documentation referred to
in Article 37 and the EU declaration of conformity referred to in Article 39
for 10 years after the EHR system covered by the EU declaration of
conformity has been placed on the market". FerroFED keeps every release from
0.0.10 on this way, from the date of the release in the changelog.

The repository keeps most of it:

- **The tag.** The `release-tags` ruleset blocks the deletion and the moving
  of a `v*` tag, so the source, this book's pages, `docs/`, `conformance/`
  and `CHANGELOG.md` stay as they were at the release.
- **The release.** GitHub's immutable-releases setting freezes a published
  release's notes and assets, the information sheet and instructions for
  use among them, and the lane never reopens one
  ([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
  What is immutable).
- **The images,** by digest in the GitHub Container Registry, with their
  attestations. A withdrawal moves tags and deletes no image.

Two parts are not covered by those settings. The CI run of the release
commit holds the conformance report and the FHIR validation results as
workflow artifacts, which GitHub deletes when the repository's artifact
retention period ends. And no setting of the repository guarantees that
GitHub serves it for ten years. So at each release Cadasto B.V. keeps an
archive copy outside GitHub, for ten years from the release date:

```sh
git bundle create ferrofed-vX.Y.Z.bundle vX.Y.Z
gh release download vX.Y.Z --repo FerroHEALTH/FerroFED --dir ferrofed-vX.Y.Z
gh run list --repo FerroHEALTH/FerroFED --workflow ci.yml \
  --commit "$(git rev-list -n1 vX.Y.Z)" --event push
gh run download <run id> --repo FerroHEALTH/FerroFED \
  --name conformance-report --name eehrxf-fhir-validation --dir ferrofed-vX.Y.Z/ci
```

The release download also holds the release's EU declaration of
conformity when the release carries one. Where Cadasto B.V. publishes the
archived declaration again, if the repository stops serving a release
within those ten years, is the manufacturer's decision and is still open
([Decisions open to the manufacturer](#decisions-open-to-the-manufacturer)).

Art 30(3) also asks the manufacturer to make the source code available to
the relevant authorities "upon a reasoned request". The source of every
release is public at its tag, and the archive keeps it.

## The EU declaration of conformity

Art 39(4): "Where an EU declaration of conformity is drawn up in a digital
format, it shall be made accessible online for the expected lifetime of the
EHR system and, in any event, for at least 10 years from the placing on the
market or the putting into service of the EHR system."

Art 39(1) has the declaration "state that the manufacturer of an EHR system
has demonstrated that the essential requirements laid down in Annex II have
been fulfilled", and Art 39(3) has it "contain the information set out in
Annex IV". FerroFED's declaration is drawn up per release, as a Markdown
document attached to that release beside the information sheet:

```text
https://github.com/FerroHEALTH/FerroFED/releases/download/vX.Y.Z/ferrofed-vX.Y.Z-eu-declaration-of-conformity.md
```

The immutable-releases setting keeps each attached declaration unchanged
and online, and the archive copy of [Keeping it for ten
years](#keeping-it-for-ten-years) holds it too. Annex III, point 6, has
its copy there.

`scripts/release/declaration-of-conformity.sh` writes it at the release's
tag, with every item of Annex IV in its order. The release lane's `plan`
job writes it before the release exists, `github-release` attaches it, and
`finalize-release` then publishes no release that misses it
([`docs/release.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/release.md),
Before the tag, step 7). Each item comes from a record of the repository at
the tag, or from the manufacturer:

| Annex IV | What it asks | Where the value comes from |
|---|---|---|
| 1 | "The name of the EHR system, version and any additional unambiguous reference allowing identification of the EHR system" | the name and version of `CITATION.cff`, held to `docs/VERSIONS.md`; the tag and its commit; the versions of the two harmonised components, `ehds-logging` and `eehrxf` |
| 2 | "Name and address of the manufacturer or, where applicable, its authorised representative" | the manufacturer the running gateway names ([Complaints and incidents](post-market.md#the-manufacturer)); the authorised representative row of the [information sheet](information-sheet.md#a-the-manufacturer) |
| 3 | "A statement that the EU declaration of conformity is issued under the sole responsibility of the manufacturer" | the statement, as Annex IV words it |
| 4 | the statement of conformity with Chapter III and other relevant Union law, "complemented by the result from the testing environment mentioned in Article 40" | the statement; the other Union acts (Art 39(2)) and the testing environment's result from the manufacturer |
| 5 | "References to any relevant harmonised standards used and in relation to which conformity is declared" | the manufacturer |
| 6 | "References to any common specifications used and in relation to which conformity is declared" | the Annex III, point 3 row of the index above: none is adopted yet |
| 7 | "Place and date of issue of the declaration, signature plus name and function of the person who signed and, if applicable, an indication of the person on whose behalf it was signed" | the date of the release in `CHANGELOG.md`; the place, the signatory, the signature and on whose behalf from the manufacturer |
| 8 | "Where applicable, additional information" | the technical documentation at the tag, the attached information sheet and instructions for use, and the standards table of the [information sheet](information-sheet.md#e-standards-formats-and-specifications) |

The manufacturer supplies its values in
[`conformance/eu-declaration-of-conformity.toml`](https://github.com/FerroHEALTH/FerroFED/blob/main/conformance/eu-declaration-of-conformity.toml).
It also says whether the manufacturer issues a declaration at all. While it
says it does not, a release carries none and the release lane warns so.
Once it says it does, every value must be filled: the script refuses a
declaration that still holds an item marked for the manufacturer, and the
`plan` job then fails before the release exists, so no release carries a
declaration with a blank.

No EU declaration of conformity has been issued yet, and no release carries
one: the manufacturer's file says `issue = false` and its values are empty.

### Decisions open to the manufacturer

- **Where the declaration is published again.** Art 39(4) asks for the
  declaration to stay "accessible online for the expected lifetime of the
  EHR system and, in any event, for at least 10 years". The releases page
  serves it while the repository does. Where Cadasto B.V. publishes the
  archived declaration if the repository stops serving a release within
  those ten years is not decided yet
  ([#940](https://github.com/FerroHEALTH/FerroFED/issues/940)).
- **The values of the manufacturer's file**, listed above.
- **The languages.** Art 39(3) has the declaration "translated into one or
  more official Union languages determined by the Member States in which
  the EHR system is made available". The declaration is drawn up in
  English; which translations a Member State asks for is not recorded yet.

## The European digital testing environment

Art 40(1) has the Commission develop "a European digital testing
environment for the assessment of harmonised software components of EHR
systems". FerroFED has not run it yet;
[#675](https://github.com/FerroHEALTH/FerroFED/issues/675) plans the run,
and its results are referenced here when it is done.

## Language and translation on request

Art 37(3): the technical documentation "shall be drawn up in an official
language of the Member State concerned or a language that is easily
understandable in that Member State", and "following a reasoned request
from the market surveillance authority of a Member State, the manufacturer
shall provide a translation of the relevant parts of the technical
documentation into an official language of that Member State".

FerroFED's technical documentation is written in English. A market
surveillance authority that needs it, or a translation of parts of it,
sends its reasoned request to the single point of contact on the
[Complaints and incidents](post-market.md#the-manufacturer) page, naming
the release, the parts and the language. Art 37(4) sets the deadline:
Cadasto B.V. provides the documentation or the translation "within 30 days
of the date of the request, unless a shorter deadline is justified because
of a serious and immediate risk", and then within the deadline the request
sets. Cadasto B.V. records the date of each request and of its answer, so
the deadline can be shown to have been met. The translation is of the
documentation at the tag of the release the request names.
