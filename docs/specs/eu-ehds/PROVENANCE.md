<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the European Health Data Space Regulation and eHealth Network guidelines

Vendored by `scripts/vendor/eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `35a38a6ca7a1f6026bca1c412af9c8fbabd05a8efadb2111ab75b93aba0a0fd8`
- Fetched: 2026-10-04, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 4 committed, 0 cache only, 1 needing
  manual retrieval
- Files in this directory: 4 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b4cb3f491f848c2e7361b0e627420904221c8e8f7805f035eece53fdfc0c684a`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes)

Regulation (EU) 2025/327 on the European Health Data Space (the research
cites Articles 8 to 12, 14 to 16, 23 and 105), the eHealth Network
guidelines on the Patient Summary and on ePrescription and eDispensation,
and the Commission legal notice that licenses them. The Regulation is an
official EU legal act; the reuse of Commission documents is governed by
Commission Decision 2011/833/EU of 12 December 2011 on the reuse of
Commission documents, which the legal notice names. The eHDSI
interoperability specifications are on a wiki behind EU Login and are
recorded for manual retrieval.

## Artefacts

### `reg-eu-2025-327-en.xhtml` (committed)

- Source: <https://publications.europa.eu/resource/cellar/531b8c37-f962-11ef-b7db-01aa75ed71a1.0006.03/DOC_1>
- Version: OJ L, 2025/327, 5.3.2025 (CELEX 32025R0327), English
- sha256: `bf331ac48264118fb0461f793123a2726da88195a11d4bccb2075fd6326909da`
- Licence: An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents
- Note: Pinned at the Publications Office (Cellar) manifestation, the XHTML EUR-Lex serves at https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=OJ:L_202500327. The EUR-Lex copy adds a monitoring script tag with per-request ids, so its sha256 differs on every fetch.

### `ehn-guidelines-patientsummary.pdf` (committed)

- Source: <https://health.ec.europa.eu/document/download/e020f311-c35b-45ae-ba3d-03212b57fa65_en?filename=ehn_guidelines_patientsummary_en.pdf>
- Version: Release 3.4, November 2024
- sha256: `9daaab30ef8e8cb5f8ab2be1480d80869b17702d118ab3d13268324184267f5d`
- Licence: CC BY 4.0 under the European Commission legal notice (https://commission.europa.eu/legal-notice_en): "Unless otherwise indicated (e.g. in individual copyright notices), content owned by the EU on this website is licensed under the Creative Commons Attribution 4.0 International (CC BY 4.0) licence". The PDF states no licence of its own

### `ehn-guidelines-eprescription.pdf` (committed)

- Source: <https://health.ec.europa.eu/document/download/b744f30b-a05e-4b9c-9630-ad96ebd0b2f0_en?filename=ehn_guidelines_eprescriptions_en.pdf>
- Version: as downloaded on 2026-10-04
- sha256: `e0dcb7671e3f5a92707e3bc9f67c38304addb20952da60fea0733c35ae80bbca`
- Licence: CC BY 4.0 under the European Commission legal notice (https://commission.europa.eu/legal-notice_en): "Unless otherwise indicated (e.g. in individual copyright notices), content owned by the EU on this website is licensed under the Creative Commons Attribution 4.0 International (CC BY 4.0) licence". The PDF states no licence of its own

### `ec-legal-notice.html` (committed)

- Source: <https://commission.europa.eu/legal-notice_en>
- Version: as served on 2026-10-04
- sha256: `9a6071ebac9918891f1daf8968ce86d6c13120db8a6d9e6a580789a1f96f8a48`
- Licence: CC BY 4.0: the page is itself content of a Commission website under the licence it states
- Note: A live page kept as the licence evidence for the two guidelines; its bytes change with every edit, so a re-run fails until the pin is renewed.

### `ehdsi-interoperability-specifications` (needs manual retrieval)

- Source: <https://webgate.ec.europa.eu/fpfis/wikis/display/EHDSI/2.+eHDSI+INTEROPERABILITY+SPECIFICATIONS%2C+Requirements+and+Frameworks>
- Version: not retrieved
- sha256: none, nothing is fetched
- Licence: Unknown: the page could not be read
- Needs manual retrieval: the source answers an automated client with a
  login or a bot challenge, so the script fetches nothing and this
  repository carries none of its content.
- Note: Behind EU Login; the normative eHDSI XCPD, XCA and XUA profiles live here.
