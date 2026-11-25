<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Laboratory Report FHIR IG myhealth.eu.fhir.laboratory

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `751fe398e5b9fd3f6668745d4e731f77ba9fe01d40e572e62ec81b76a2977740`
- Fetched: 2026-11-25, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 1 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 1 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `d12eb4122925a51329059ca86734154f6278e3cf64a8eb979c953de57a51a589`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The MyHealth@EU Laboratory Result Report service on FHIR, derived
from the HL7 Europe Laboratory Report, canonical
`http://fhir.ehdsi.eu/laboratory`, committed under CC0-1.0.

## Artefacts

### `myhealth.eu.fhir.laboratory-9.2.0.tgz` (committed)

- Source: <https://build-fhir.ehdsi.eu/laboratory/package.tgz>
- Version: 9.2.0, CI build (FHIR 4.0.1, 2026-11-24)
- sha256: `e258e974945caa6f0f41a0816231d617638f56c3f16bf6382760ecb2ca3dd0ce`
- Licence: CC0-1.0: the license of the package manifest and of the ImplementationGuide resource
- Note: The CI build https://build-fhir.ehdsi.eu/ serves, the line the family validates against by owner ruling of 2026-11-24 (#1951); not on packages.fhir.org, so a new build fails the pin loudly and scripts/checks/pin-freshness.sh reports it.
