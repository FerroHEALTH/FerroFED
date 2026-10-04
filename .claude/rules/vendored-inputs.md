---
paths: ["scripts/vendor/*.sh", "**/vendor/**", "docs/specs/**"]
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Vendored inputs

External material enters this repository one way only: a committed fetch
script, vendored verbatim, stamped with provenance. The specification corpora
live under `docs/specs/`, one directory per corpus, each fetched by its own
`scripts/vendor/*.sh` from the pins in `docs/VERSIONS.md`:

- `docs/specs/federation-spec/`: the Federation Tier with AQL specification
  source (CC0 1.0), whole tree, including its two JSON schemas.
- `docs/specs/federation-ref/`: the reference implementation (Apache License
  2.0), whole tree, as evidence and a test corpus. Its code is never copied
  into this repository (`spec-adherence.md`).
- `docs/specs/its-rest/`: the openEHR ITS-REST 1.1.0 OpenAPI documents
  (content CC-BY-ND 3.0).
- `docs/specs/aql/`: the openEHR AQL 1.1.0 specification source, its examples
  and the grammar `.g4` files (CC-BY-SA 3.0).
- `docs/specs/ihe-pixm/`: the ITI-83 artefacts of the IHE PIXm 3.1.0 FHIR
  package (CC-BY-4.0): the `$ihe-pix` OperationDefinition, the Query
  Parameters profiles, the capability statements and the IG's examples,
  pinned by package version and tarball sha256.
- `docs/specs/ihe-pdqm/`: the ITI-78 artefacts of the IHE PDQm 3.2.0 FHIR
  package (CC-BY-4.0): the Consumer and Supplier capability statements, the
  Query Patient Resource Response Message and Patient profiles and the IG's
  examples, pinned by package version and tarball sha256.
- `docs/specs/ihe-mcsd/`: the ITI-90 and ITI-91 artefacts of the IHE mCSD
  4.0.0 FHIR package (CC-BY-4.0): the Directory, Query Client and Update
  Client capability statements, the Organization, Endpoint and Location
  profiles, the endpoint type code system and value sets and the IG's
  Organization and Endpoint examples, pinned by package version and tarball
  sha256.
- `docs/specs/nl-gf/`: the source of the Netherlands Generic Functions IG
  `fhir.nl.gf` 0.3.0 (EUPL-1.2): the localization, consent, care services,
  identification and authentication pages (with the six GFI transactions),
  their FSH profiles, capability statements and examples, pinned by tag and
  commit, since the IG has no package on the FHIR package registry.
- The country research corpora of #488, each artefact pinned by URL and
  sha256 through `scripts/vendor/lib/pinned.sh`, one script per country
  (`de.sh`, `at.sh`, `ch.sh`, `eu.sh`, `be.sh`, `fr.sh`, `dk.sh`, `se.sh`,
  `no.sh`, `fi.sh`) plus `ihe-iti-tf.sh`. Material that may not be
  redistributed, or whose licence is unclear, is cache only: fetched into
  `.vendor-cache/` and recorded in the corpus's `PROVENANCE.md` alone.
  - `docs/specs/de-gematik-epa/`: the ePA-Basic OpenAPI documents
    (Apache-2.0); the gematik ePA specification pages are cache only.
  - `docs/specs/de-gematik-vzd/`: the VZD FHIR package and connectionType
    code system (Apache-2.0 from their source repository); the VZD
    specification page is cache only.
  - `docs/specs/de-gematik-zeta/`: gemSpec_ZETA, cache only.
  - `docs/specs/de-hl7-basisprofil/`: `de.basisprofil.r4`, cache only (no
    licence stated).
  - `docs/specs/de-gematik-isik/`: `de.gematik.isik`, cache only (no
    licence stated).
  - `docs/specs/de-mii-consent/`: the MII consent package, cache only (no
    package licence stated).
  - `docs/specs/de-sgb5/`: SGB V §§290, 339 and 342, official texts (UrhG
    §5).
  - `docs/specs/at-gtelg/`: the GTelG 2012, an official text (Austrian UrhG
    §7).
  - `docs/specs/at-elga-bes/`: the ELGA Berechtigungssystem pages, cache
    only.
  - `docs/specs/at-elga/`: the ELGA overview and standards catalogue, cache
    only.
  - `docs/specs/at-hl7-core/`: `hl7.at.fhir.core.r4` (CC0-1.0).
  - `docs/specs/ch-fedlex-epr/`: the EPR acts, ordinances, annexes and the
    EGDG draft, official texts (URG Art. 5).
  - `docs/specs/ch-epr-fhir/`: `ch.fhir.ig.ch-epr-fhir` (CC0-1.0).
  - `docs/specs/ch-ehs-central-services/`: the EPR central services
    interface pack, cache only.
  - `docs/specs/ihe-pixm-ch/`, `docs/specs/ihe-pdqm-ch/`,
    `docs/specs/ihe-iua-ch/`: IHE PIXm 3.0.4, PDQm 3.1.0 and IUA 2.3, the
    revisions Swiss Annex 5 pins (CC-BY-4.0).
  - `docs/specs/eu-ehds/`: Regulation (EU) 2025/327 (an official EU act),
    the eHealth Network guidelines and the Commission legal notice (CC BY
    4.0); the eHDSI wiki needs manual retrieval.
  - `docs/specs/ehdsi/`: the NCPeH API package and page (CC0-1.0); the
    guide index and OpenNCP (evidence only) are cache only.
  - `docs/specs/ihe-iti-tf/`: ITI TF Volume 1 chapters 13, 18 and 27
    (General Introduction §9).
  - `docs/specs/be-ehealth/`: the eHealth platform cookbooks, cache only
    until the owner settles their licence.
  - `docs/specs/be-fhir/`: `hl7.fhir.be.core` (CC0-1.0).
  - `docs/specs/fr-ans/`: the ANS FHIR packages and guide page (CC0-1.0)
    and two ANS repositories (MIT); the INSi pages need manual retrieval.
  - `docs/specs/dk-nsp/`: the NSPOP pages, cache only.
  - `docs/specs/se-inera/`: RIV-TA Basic Profile 2.1 (CC BY-SA 2.5 SE); the
    service contract archives and the FAQ are cache only.
  - `docs/specs/no-nhn/`: the NHN developer portal pages, cache only.
  - `docs/specs/fi-kanta/`: the Kanta documents and packages, cache only.
  - `docs/specs/fi-hl7/`: `hl7.fhir.fi.base` (CC0-1.0).
- `docs/specs/nuts-rfc/`: Nuts RFC003, RFC021 and RFC022 (CC BY-SA 4.0,
  stated in each document), pinned by commit: the authorization server, the
  VP Token Grant Type and the Discovery Service of the Annex B §B.4 track.
- `docs/specs/ietf-oauth/`: RFC 6749, 7519, 7521, 7523, 7662, 8414, 9126,
  9396 and 9449 as the RFC Editor publishes them (IETF Trust Legal Provisions
  §3.c, in full and unmodified), each pinned by URL and sha256.
- `docs/specs/w3c-did-vc/`: VC Data Model 1.1, DID 1.0, DID Resolution and
  Bitstring Status List 1.0 (W3C Software and Document License), each pinned
  by its dated URL and sha256, and the did:web method report, pinned by
  commit.
- `docs/specs/dif-pe/`: DIF Presentation Exchange 2.0.0, its text and JSON
  Schemas, with the claim format designations of the DIF Claim Format
  Registry they reference (Apache-2.0), each pinned by commit.
- `docs/specs/openid/`: the provenance alone of the FAPI 2.0 Security
  Profile, OpenID4VCI 1.0 and OpenID4VP draft 18, pinned by URL and sha256
  and fetched under the ignored `.vendor-cache/openid/`, because the OpenID
  Foundation licenses them only for developing and implementing the
  specifications.
- `docs/specs/mitz/`: the Mitz documents that define the closed
  authorization question of Annex B §B.6 (the VZVZ implementation guide
  Open en gesloten autorisatievraag 3.8.2, the PvE AMC and the message
  authentication guide 3.8.1.ad1). No licence is stated, so only
  `PROVENANCE.md` is committed: `scripts/vendor/mitz.sh` pins each document
  by its Confluence attachment version and sha256 and fetches it into the
  git-ignored `docs/specs/mitz/cache/`.
- `website/book/vendor/mermaid/`: the mermaid browser bundle and the
  mdbook-mermaid init script the book loads, fetched by
  `scripts/vendor/mdbook-mermaid-assets.sh` (MIT and MPL 2.0).

A new corpus (an IHE profile's published artefacts, the Dutch
Generic Functions IG) gets its own directory, script, pin and `PROVENANCE.md`
in the change that first needs it, and this list grows with it.

## The rule

Every external corpus (a specification's machine-readable artifacts, a
schema, a test corpus, a grammar) is:

- **Fetched by a committed `scripts/vendor/*.sh` script.** Never hand-download
  into the tree, never hand-edit a vendored file, and never paste material in
  from a chat transcript. To refresh or extend a corpus, change the script,
  re-run it, and commit the result. A hand-edit of a vendored file is a defect
  to revert.
- **Vendored verbatim**, byte for byte as the publisher ships it. Reformatting,
  pretty-printing, or trimming a vendored file destroys the property that makes
  it checkable against its source.
- **Stamped with a `PROVENANCE.md`** in its own directory, recording the
  upstream source (the URL or registry), the exact version or commit pin, the
  fetch date, and the upstream licence, with the upstream `LICENSE` vendored
  alongside. A vendored tree with no provenance is unusable: nobody can tell
  what it is or whether it may be redistributed.
- **Exercised.** A vendored input is not done until something reads it: a
  codegen drift check, a schema-validation test, or a corpus test. An unread
  corpus is dead weight that rots without anyone noticing.
- **Marked in `.gitattributes`** as `linguist-vendored`, and `-text` where the
  bytes must not be line-ending normalized, so the tree matches the upstream
  archive exactly.

## Licensing

Vendored material keeps its upstream terms, and those terms are recorded in the
`PROVENANCE.md` rather than assumed. The project's own code and text are
under the Business Source License 1.1 (`CLAUDE.md` §Licence); a vendored tree
is not, and the two are never conflated. If a corpus's licence does not permit redistribution, it is
not vendored: the fetch script pulls it into an ignored directory at build time
and the repository ships none of it. That directory is `.vendor-cache/<corpus>/`,
and the same holds for material whose licence is unclear: the committed
`PROVENANCE.md` records its URL, version, sha256, fetch date and licence
statement, and no excerpt, quote or converted copy of its content.

## Never commit clinical or patient data

No patient data, no identifiable health information, and no extract from a
production system belongs in this repository, in a fixture, or in a vendored
tree. Test fixtures are synthetic content invented for the test
(`testing.md`). A patient identifier in a fixture is synthetic too: never a
real national identifier such as a BSN, even one that looks like a test
value. A licence-gated code system or clinical corpus is a deployment
input, never a committed artifact, and `.gitignore` refuses its shapes so a
copy dropped into a working tree cannot be committed by accident. Widen
`.gitignore` in the same change when a genuinely new shape appears.
