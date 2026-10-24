<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the eHDSI CDA reference test documents

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `5f358affe656663017a0f806d54791dce590497aa3db325429cbfb956b2e9147`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 0 committed, 1 cache only, 0 needing
  manual retrieval
- Files in this directory: 0 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
- Read by: #1290 (the re-check of decision A72, the NCP-level serialisation)

The CDA documents the eHDSI general repository publishes as reference test
data for the MyHealth@EU CDA services, pinned by commit. The repository
states no licence, so the archive is fetched into the cache only and this
repository carries none of its content.

## Artefacts

### `ehdsi-general-repository-e521266708aac6e47a1e78f243c178bc0eb7d3b2.tar.gz` (cache only, not redistributed)

- Source: <https://code.europa.eu/ehdsi/ehdsi-general-repository/-/archive/e521266708aac6e47a1e78f243c178bc0eb7d3b2/ehdsi-general-repository-e521266708aac6e47a1e78f243c178bc0eb7d3b2.tar.gz>
- Version: branch main, commit e521266708aac6e47a1e78f243c178bc0eb7d3b2 (2023-10-19)
- sha256: `e82d8ce55d11e3bcb510a516c9bc2e817c47680d3c939556b0b5c19f17586c28`
- Licence: No licence statement: the repository has no licence file and its README is the GitLab template
- Not redistributed: the script fetches it into `.vendor-cache/ehdsi-cda-test-documents/`, which git
  ignores; this repository carries none of its content.
- Note: The repository archive at the commit: cda documents/W5, W6 and W7 (Patient Summary, ePrescription, eDispensation and original clinical documents of three eHDSI waves, with PASS and FAIL cases named by issue key) and unicom (three ePrescriptions), 63 XML files and the README.
