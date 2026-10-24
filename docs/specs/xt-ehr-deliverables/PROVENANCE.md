<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the Xt-EHR deliverables on the European electronic health record exchange format

Vendored by `scripts/vendor/xt-ehr.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `7ec2ef864d11f9a1fc1d7c3fb8865b7c5b1ab052b7f0eabb0020b887ccca8a90`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 3 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #1265 (the re-check of decisions A67 to A72 for the national
  contact point against the closest published exchange-format text)

Three deliverables of the Xt-EHR joint action (grant 101128085), published
at <https://www.xt-ehr.eu/deliverables/>: D6.1 (Patient Summary), D6.2
(electronic prescription and electronic dispensation) and D5.2 (EEHRxF
metadata). They are the closest published text of the specifications
Regulation (EU) 2025/327 Article 15(1) leaves to an implementing act that
the Commission has not adopted, and no draft of that act is published for
feedback; FerroFED reads them as a draft. They carry no licence statement,
so each is fetched into the cache only and this repository carries none of
their content.

## Artefacts

### `Xt-EHR-D6.1.pdf` (cache only, not redistributed)

- Source: <https://www.xt-ehr.eu/wp-content/uploads/2026/05/D6.1-2.pdf>
- Version: D6.1, as published on xt-ehr.eu in 2026-05
- sha256: `65da226ed74e549e4e3d3611b9a458382a147f8059bd7903b6e1d2bc6fe0374c`
- Licence: No licence statement in the document or on the deliverables page
- Not redistributed: the script fetches it into `.vendor-cache/xt-ehr-deliverables/`, which git
  ignores; this repository carries none of its content.
- Note: Patient Summary: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems; its section 5 names the MyHealth@EU Requirements Catalogue 9.0.0 (2025-03-24) as an EC resource available only with authorisation.

### `Xt-EHR-D6.2.pdf` (cache only, not redistributed)

- Source: <https://www.xt-ehr.eu/wp-content/uploads/2026/05/D6.2-2.pdf>
- Version: D6.2, as published on xt-ehr.eu in 2026-05
- sha256: `1d2478a4c75e056756bf1ebd0093ec263ac5ea0c594b75a860dd4842358bed7c`
- Licence: No licence statement in the document or on the deliverables page
- Not redistributed: the script fetches it into `.vendor-cache/xt-ehr-deliverables/`, which git
  ignores; this repository carries none of its content.
- Note: Electronic prescription and electronic dispensation: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems.

### `Xt-EHR-D5.2.pdf` (cache only, not redistributed)

- Source: <https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D5.2.pdf>
- Version: D5.2, as published on xt-ehr.eu in 2026-05
- sha256: `5d2bb693af07646a506f831854166fcd4d6a3ebf4f092b6937d2a9976410333a`
- Licence: No licence statement in the document or on the deliverables page
- Not redistributed: the script fetches it into `.vendor-cache/xt-ehr-deliverables/`, which git
  ignores; this repository carries none of its content.
- Note: Technical requirements for EEHRxF metadata.
