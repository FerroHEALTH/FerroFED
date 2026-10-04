<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Swedish RIV-TA service contracts and Inera documentation

Vendored by `scripts/vendor/se.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `32e7de7b0fced3297889794a12b7a5f2026199254935135b95339d4f2e286cf8`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 5 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `dd79567003eaaa286917fd8de9d01cbd3157874307d7b706e871b55408920c29`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

The service contracts of the engagement index (the national record
locator), blocking, patient consent and the HSA organisation directory, the
RIV-TA Basic Profile 2.1 and the engagement index FAQ. The Basic Profile is
published under CC BY-SA 2.5 SE and is committed. The archives mix
Apache-2.0 schema files with documents and test data that state no licence,
so they stay in the cache with the FAQ.

## Artefacts

### `se-riv.itintegration.engagementindex-1.0.10.tar.gz` (cache only, not redistributed)

- Source: <https://bitbucket.org/rivta-domains/riv.itintegration.engagementindex/get/1.0.10.tar.gz>
- Version: tag 1.0.10, commit feef3879647a
- sha256: `4b9e05932874860ec706619c515fa520e120adea346806a58415d527f424a611`
- Licence: Mixed: most XSD and WSDL files carry the header "Licensed under the Apache License, Version 2.0"; the .docx contract descriptions, the test suites and the remaining files state no licence (7 of 8 schema files)
- Not redistributed: the script fetches it into `.vendor-cache/se-inera/`, which git
  ignores; this repository carries none of its content.
- Note: A Bitbucket archive generated per request and pinned by its sha256; a re-generated archive fails the pin loudly.

### `se-riv.ehr.blocking-3.2.2.tar.gz` (cache only, not redistributed)

- Source: <https://bitbucket.org/rivta-domains/riv.ehr.blocking/get/ehr_blocking_3.2.2.tar.gz>
- Version: tag ehr_blocking_3.2.2, commit 558d8574eefc
- sha256: `f1304316630d9408f13d0094372fd188466f716c1184ddba2ccb4a6f05dba0f6`
- Licence: Mixed: most XSD and WSDL files carry the header "Licensed under the Apache License, Version 2.0"; the .docx contract descriptions, the test suites and the remaining files state no licence (34 of 36 schema files)
- Not redistributed: the script fetches it into `.vendor-cache/se-inera/`, which git
  ignores; this repository carries none of its content.
- Note: A Bitbucket archive generated per request and pinned by its sha256; a re-generated archive fails the pin loudly.

### `se-riv.ehr.patientconsent-1.0.1_RC1.tar.gz` (cache only, not redistributed)

- Source: <https://bitbucket.org/rivta-domains/riv.ehr.patientconsent/get/ehr_patientconsent_1.0.1_RC1.tar.gz>
- Version: tag ehr_patientconsent_1.0.1_RC1, commit 70440f0fcd08; only a release-candidate tag exists
- sha256: `2756ad9c72cf0d46671d64a0a2e7e586b36eef6cbcb24cedb974a80c6d56a3b8`
- Licence: Mixed: most XSD and WSDL files carry the header "Licensed under the Apache License, Version 2.0"; the .docx contract descriptions, the test suites and the remaining files state no licence (15 of 16 schema files)
- Not redistributed: the script fetches it into `.vendor-cache/se-inera/`, which git
  ignores; this repository carries none of its content.
- Note: A Bitbucket archive generated per request and pinned by its sha256; a re-generated archive fails the pin loudly.

### `se-riv.infrastructure.directory.organization-5.0.1.tar.gz` (cache only, not redistributed)

- Source: <https://bitbucket.org/rivta-domains/riv.infrastructure.directory.organization/get/5.0.1.tar.gz>
- Version: tag 5.0.1, commit 6dfc2dbac0da
- sha256: `59d1aad6fcac539df7ed4216f4f1573ee3639efdb0264c447f6519e3070a6a0c`
- Licence: Mixed: most XSD and WSDL files carry the header "Licensed under the Apache License, Version 2.0"; the .docx contract descriptions, the test suites and the remaining files state no licence (11 of 16 schema files)
- Not redistributed: the script fetches it into `.vendor-cache/se-inera/`, which git
  ignores; this repository carries none of its content.
- Note: A Bitbucket archive generated per request and pinned by its sha256; a re-generated archive fails the pin loudly.

### `se-rivta-bp21.json` (committed)

- Source: <https://inera.atlassian.net/wiki/rest/api/content/3632875?expand=body.storage,version>
- Version: Version 3.1, ARK_0002, 2026-06-08 (Confluence page version 26)
- sha256: `b306473e17b041922db6c0663b39ea0063e8e9531fc985a0a27a5432cd9b3ff0`
- Licence: CC BY-SA 2.5 SE, section 1.3 of the document: "Detta dokument är publicerat under licensen Creative Commons CC-BY-SA (http://creativecommons.org/licenses/by-sa/2.5/se/)", attributed to Sveriges Kommuner och Regioner
- Note: A Confluence REST export of a live page; each new page version moves the hash.

### `se-ei-faq.json` (cache only, not redistributed)

- Source: <https://inera.atlassian.net/wiki/rest/api/content/5565513926?expand=body.storage,version>
- Version: page version 1 (2026-04-01)
- sha256: `9ee599f2004bcfd39f89e0b83c48805fd9895666942603279dd749703771f84f`
- Licence: No licence statement on the page
- Not redistributed: the script fetches it into `.vendor-cache/se-inera/`, which git
  ignores; this repository carries none of its content.
- Note: A Confluence REST export of a live page; each new page version moves the hash.
