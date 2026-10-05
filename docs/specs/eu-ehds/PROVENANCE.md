<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the European Health Data Space Regulation, its adopted implementing acts and eHealth Network guidelines

Vendored by `scripts/vendor/eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `7c2e0fa5d9ca2bdceadc96250f90cb72a1be2b0f3663e87cc24fd67d0d6ffdc1`
- Fetched: 2026-10-05, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 7 committed, 0 cache only, 1 needing
  manual retrieval
- Files in this directory: 7 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `3f5394412ac55241582bd27aa0ba7012576b05deb6ac82f305f4f547b0940712`
- Read by: #488 (the country research into identity resolution,
  localization, consent, addressing and authentication to nodes) and
  #519 (EHDS readiness)

Regulation (EU) 2025/327 on the European Health Data Space (the research
cites Articles 8 to 12, 14 to 16, 23 and 105), the two implementing acts
adopted under it that reach the cross-border exchange (Commission
Implementing Regulation (EU) 2026/2083 on MyHealth@EU and (EU) 2026/2099 on
cross-border identification and authentication), Commission Recommendation
(EU) 2019/243 on a European Electronic Health Record exchange format, the
eHealth Network guidelines on the Patient Summary and on ePrescription and
eDispensation, and the Commission legal notice that licenses them. The
Regulation, the implementing acts and the Recommendation are official EU
legal acts published in the Official Journal; their reuse is governed by
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

### `impl-reg-eu-2026-2083-en.xhtml` (committed)

- Source: <https://publications.europa.eu/resource/cellar/5a79d49d-b554-11f1-81de-01aa75ed71a1.0006.03/DOC_1>
- Version: OJ L, 2026/2083, 21.9.2026 (CELEX 32026R2083), English
- sha256: `51340e65db3e85bc892d723a630663172ce89001f376dea0b870d74ab2231c58`
- Licence: An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents
- Note: Commission Implementing Regulation (EU) 2026/2083 of 18 September 2026 on MyHealth@EU, adopted under Article 23(4) and (8) of Regulation (EU) 2025/327; applies from 26 March 2027 (its Article 19). Pinned at the Cellar manifestation the CELEX resource resolves to.

### `impl-reg-eu-2026-2099-en.xhtml` (committed)

- Source: <https://publications.europa.eu/resource/cellar/dff777d4-b61d-11f1-81de-01aa75ed71a1.0006.03/DOC_1>
- Version: OJ L, 2026/2099, 22.9.2026 (CELEX 32026R2099), English
- sha256: `6389d047750ed8fca879d53ebeda22af444d1f1619ed3bf6f13560097aa887d0`
- Licence: An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents
- Note: Commission Implementing Regulation (EU) 2026/2099 of 21 September 2026 on the cross-border identification and authentication mechanism, adopted under Article 16(2) of Regulation (EU) 2025/327; applies from 26 March 2027, its Article 3(3) and Article 5(2) from 26 March 2029 (its Article 9). Pinned at the Cellar manifestation the CELEX resource resolves to.

### `rec-eu-2019-243-en.xhtml` (committed)

- Source: <https://publications.europa.eu/resource/cellar/cf529e8a-2dcb-11e9-8d04-01aa75ed71a1.0006.03/DOC_1>
- Version: OJ L 39, 11.2.2019, p. 18 (CELEX 32019H0243), English
- sha256: `d6c8d817271b376e836744d81c2ddfe5d40d8a9f5f0b24eb100c1bc507183480`
- Licence: An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents
- Note: Commission Recommendation (EU) 2019/243 of 6 February 2019 on a European Electronic Health Record exchange format, which recital 26 of Regulation (EU) 2025/327 names as the foundation of the exchange format. Pinned at the Cellar manifestation the CELEX resource resolves to.

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
- Version: as served on 2026-10-05
- sha256: `fd6687f313b4b016675c5d74595e3bc343f45a34b430a7ff13a35e2b5cf03a75`
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
