<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CRA information to the user

Regulation (EU) 2024/2847, the Cyber Resilience Act (CRA), has each product
with digital elements "accompanied by the information and instructions to
the user set out in Annex II, in paper or electronic form" (Art 13(18)).
This page gives that information for FerroFED, point by point in the order
of Annex II. Each point quotes what Annex II asks and answers it, or links
the page that does.

The product is each tagged FerroFED release, in the forms on
[Forms placed on the market](technical-documentation.md#forms-placed-on-the-market).
Points 1 and 2, and the first half of point 7, are the same for every
FerroHEALTH product, and the family book is their source
([The information that accompanies each release](https://ferrohealth.eu/docs/cra.html#the-information-that-accompanies-each-release)).
The [Annex II checklist](annex-ii.md) of this book is a different list: the
essential requirements of Annex II of Regulation (EU) 2025/327, the EHDS
Regulation.

Every quotation is from the Official Journal text, vendored at
[`docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml`](https://github.com/FerroHEALTH/FerroFED/blob/main/docs/specs/eu-cra/reg-eu-2024-2847-en.xhtml)
(<http://data.europa.eu/eli/reg/2024/2847/oj>). No conformity assessment
has been carried out and no EU declaration of conformity exists. This page
gives the information Annex II lists; it does not say that FerroFED, or a
deployment of it, meets the Regulation.

## Which copy to read

The information for a release is this page at that release's tag, from
FerroFED 0.0.11 on. Art 13(18) has information provided online kept
"available online for at least 10 years after the product with digital
elements has been placed on the market or for the support period, whichever
is longer". A `v*` tag cannot be moved or deleted, so the page's source at
each tag stays as it was
([Keeping it for ten years](technical-documentation.md#keeping-it-for-ten-years)).
The page is written in English; a market surveillance authority that needs
a translation asks the single point of contact of point 2.

## 1. The manufacturer

"the name, registered trade name or registered trademark of the
manufacturer, and the postal address, the email address or other digital
contact as well as, where available, the website at which the manufacturer
can be contacted;"

Cadasto B.V. The details are on the family book's
[The manufacturer](https://ferrohealth.eu/docs/manufacturer.html#contact-details)
page, and the [information sheet](information-sheet.md#a-the-manufacturer)
repeats them. The running gateway names them in `GET {base}/`,
`OPTIONS {base}/`, its startup banner and `ferrofed --version`.

## 2. The single point of contact for vulnerabilities

"the single point of contact where information about vulnerabilities of
the product with digital elements can be reported and received, and where
the manufacturer's policy on coordinated vulnerability disclosure can be
found;"

The policy and the routes are on the family book's
[Security](https://ferrohealth.eu/docs/security.html#reporting-a-vulnerability)
page. A vulnerability in FerroFED is reported through
[GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroFED/security/advisories/new),
as [`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#reporting-a-vulnerability)
says, and the advisories are published on the repository's
[security advisories](https://github.com/FerroHEALTH/FerroFED/security/advisories).

## 3. Identification

"name and type and any additional information enabling the unique
identification of the product with digital elements;"

- **Name:** FerroFED.
- **Type:** software, a federation gateway for openEHR clinical data
  repositories, run as a server process.
- **Unique identification:** the version, `X.Y.Z`, with the signed tag
  `vX.Y.Z`. `ferrofed --version`, the startup banner and `GET {base}/` name
  the version a gateway runs. An image is identified by its digest, and a
  tarball by its `.sha256sum` and its attestations
  ([(b) Name, version and date of release](information-sheet.md#b-name-version-and-date-of-release)).

## 4. Intended purpose, security environment and properties

"the intended purpose of the product with digital elements, including the
security environment provided by the manufacturer, as well as the product's
essential functionalities and information about the security properties;"

- **Intended purpose:** [Intended purpose](regulatory-status.md#intended-purpose).
- **Essential functionalities:** the federated query, follow-up routing and
  definitions ([How it works](../how-it-works/overview.md)).
- **The security environment:** the reverse proxy, the issuers, the
  identity services and the Audit Record Repository a deployment supplies
  ([What FerroFED runs beside](../operate/deployment-shape.md)), and the
  trust relationships between them ([Trust and keys](../how-it-works/trust-and-keys.md)).
- **Security properties:** every caller is authenticated; no directly
  identifying patient identifier reaches a node (§5.4, N33,
  [Where the patient identifier stops](../how-it-works/identifier-hygiene.md));
  each node is told who asks and decides for itself; every access to
  patient data is recorded. The [threat model](threat-model.md) names each
  property with the test that holds it, and the
  [cybersecurity risk assessment](cra-risk-assessment.md) reads them against
  Annex I.

## 5. Circumstances that may lead to significant cybersecurity risks

"any known or foreseeable circumstance, related to the use of the product
with digital elements in accordance with its intended purpose or under
conditions of reasonably foreseeable misuse, which may lead to significant
cybersecurity risks;"

The full list, each with its control, is the threat model's
[Risks the deployment carries](threat-model.md#risks-the-deployment-carries).
The circumstances that lead to the most severe ones:

- running `profile = "development"` near real data
  ([Hardening](../operate/hardening.md#the-container));
- leaving the hop between the proxy and the gateway in cleartext on a
  shared network
  ([TLS on the listeners](../operate/configuration.md#tls-on-the-listeners));
- trusting an issuer or an identity service that is compromised, or a
  member that ignores the caller the gateway conveys
  ([Admitting a node](../operate/admission.md));
- keeping the audit spool on an unencrypted volume
  ([The audit spool](../operate/hardening.md#the-audit-spool));
- running a release past the end date of its support period (point 7).

## 6. The EU declaration of conformity

"where applicable, the internet address at which the EU declaration of
conformity can be accessed;"

None has been issued. Once the manufacturer issues one, it is attached to
each release it covers at
`https://github.com/FerroHEALTH/FerroFED/releases/download/vX.Y.Z/ferrofed-vX.Y.Z-eu-declaration-of-conformity.md`
([The EU declaration of conformity](technical-documentation.md#the-eu-declaration-of-conformity)).

## 7. Technical security support and the support period

"the type of technical security support offered by the manufacturer and
the end-date of the support period during which users can expect
vulnerabilities to be handled and to receive security updates;"

The type of support is the manufacturer's, described on the family book's
[How a fix reaches you](https://ferrohealth.eu/docs/security.html#how-a-fix-reaches-you).
For FerroFED, a security fix ships in a new release, and a user of an older
release within its support period moves to the latest release to get it
([`SECURITY.md`](https://github.com/FerroHEALTH/FerroFED/blob/main/SECURITY.md#supported-versions)).

Each release is supported for five years from its release date. The end
dates, as `SECURITY.md` lists them:

| Release | Released | Supported until |
|---|---|---|
| v0.0.1 | 2026-10-01 | 2031-10-01 |
| v0.0.3 | 2026-10-02 | 2031-10-02 |
| v0.0.6 | 2026-10-03 | 2031-10-03 |
| v0.0.7 | 2026-10-03 | 2031-10-03 |
| v0.0.8 | 2026-10-04 | 2031-10-04 |
| v0.0.9 | 2026-10-05 | 2031-10-05 |
| v0.0.10 | 2026-10-15 | 2031-10-15 |

`SECURITY.md` is the record. The release cut adds each new row to it and
copies the row here, and a guard, `scripts/checks/support-periods.sh`, fails
every change and every release while the two tables differ. A gateway names its own end date in the startup banner and in
`OPTIONS {base}/`, and says so once the date has passed.

## 8. Instructions

"detailed instructions or an internet address referring to such detailed
instructions and information on:"

### 8(a) Secure use from commissioning on

"the necessary measures during initial commissioning and throughout the
lifetime of the product with digital elements to ensure its secure use;"

Commission a deployment with [A production deployment](../operate/production.md)
and check it against the [Hardening](../operate/hardening.md) checklist. The
[instructions for use](../operate/instructions-for-use.md) give the order of
the steps, and their
[maintenance table](../operate/instructions-for-use.md#maintenance-and-its-frequency)
lists what keeps it secure through its lifetime: upgrades, `config check`,
key rotation, certificate renewal and the alerts.

### 8(b) How changes affect the security of data

"how changes to the product with digital elements can affect the security
of data;"

- **Configuration.** The configuration decides whom the gateway trusts and
  where it sends data. The development profile admits a static
  cross-reference and cleartext credentials; an issuer added to `[auth]` is
  trusted for every caller it vouches for; a member added to the registry
  receives queries once admitted. Run `ferrofed config check` before every
  start, reload and upgrade
  ([Configuration](../operate/configuration.md#running-it),
  [Admitting a node](../operate/admission.md)).
- **Upgrades.** A release's upgrade notes list every key that became
  mandatory or a value now refused
  ([Upgrading](../operate/upgrading.md#before-you-upgrade)).
- **Stored queries.** A stored query an operator publishes runs against
  every member it reaches
  ([Stored queries](../integrate/stored-queries.md)).
- **Changes to the source.** A binary or image built from modified source
  is not a Cadasto B.V. release: its provenance does not verify, and the
  [cybersecurity risk assessment](cra-risk-assessment.md) does not describe
  it.

### 8(c) Installing security updates

"how security-relevant updates can be installed;"

Hear of a release from the release feed,
`https://github.com/FerroHEALTH/FerroFED/releases.atom`, and of a
vulnerability from the repository's security advisories
([Hearing of an update](https://ferrohealth.eu/docs/security.html#hearing-of-an-update)).
Verify the release with `gh attestation verify`
([Kubernetes](../operate/container.md#kubernetes),
[The release binaries](../operate/container.md#the-release-binaries)), then
follow [Upgrading](../operate/upgrading.md); [Rollback](../operate/rollback.md)
says how to go back.

### 8(d) Secure decommissioning

"the secure decommissioning of the product with digital elements, including
information on how user data can be securely removed;"

[Decommissioning](../operate/decommissioning.md) takes a deployment out of
service step by step: it lists what a deployment holds and where, moves what
a replacement needs, lets the audit spools deliver, stops the replicas,
keeps the access records for their retention period, retires the signing
keys and the credentials, and removes the stores and the volumes.

### 8(e) Turning off automatic security updates

"how the default setting enabling the automatic installation of security
updates, as required by Part I, point (2)(c), of Annex I, can be turned
off;"

FerroFED installs no update by itself, so there is no such setting; the
risk assessment says why
([Automatic security updates](cra-risk-assessment.md#automatic-security-updates)).

### 8(f) Information for an integrator

"where the product with digital elements is intended for integration into
other products with digital elements, the information necessary for the
integrator to comply with the essential cybersecurity requirements set out
in Annex I and the documentation requirements set out in Annex VII."

This point does not apply: FerroFED is not intended for integration into
another product, and its library crates reach a user only inside the
release forms. Client applications connect to a running gateway over
ITS-REST ([The client contract](../integrate/client-contract.md)).

## 9. The software bill of materials

"If the manufacturer decides to make available the software bill of
materials to the user, information on where the software bill of materials
can be accessed."

Cadasto B.V. makes them available. Each release tarball on
[the releases page](https://github.com/FerroHEALTH/FerroFED/releases) comes
with a CycloneDX SBOM of the source graph (`.cdx.json`) and an SPDX SBOM of
the shipped binary (`.spdx.json`), each attested
([The release binaries](../operate/container.md#the-release-binaries)). The
gateway and console images carry an SPDX SBOM per platform as an attestation
in the registry, which `gh attestation verify` reads
([Kubernetes](../operate/container.md#kubernetes)).
