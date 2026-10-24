<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the MyHealth@EU Patient Summary FHIR IG myhealth.eu.fhir.eps

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `0dc5b4f7bc3bc4770c7e2673420eeb945366524a2324abd205c16a851c457c85`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 2 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 2 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `b15e19860e6a40597cbd4cb5a64c51907ce3020729a9de9c7d61250b622472b6`
- Read by: #1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against
  the MyHealth@EU guides), and the testkit

The proof of concept of the MyHealth@EU Patient Summary service on FHIR,
derived from the HL7 Europe Patient Summary, canonical
`http://fhir.ehdsi.eu/eps`, with its rendered home page, committed under CC-BY-4.0, which permits
verbatim redistribution with attribution: the attribution is the copyright
the ImplementationGuide resource carries, kept verbatim in the package.

## Artefacts

### `myhealth.eu.fhir.eps-1.0.0-poc.tgz` (committed)

- Source: <https://packages.fhir.org/myhealth.eu.fhir.eps/1.0.0-poc>
- Version: 1.0.0-poc, QA preview (FHIR 4.0.1, 2026-07-16)
- sha256: `34c49509890f6b41f5ffcb788f04cbeefb5379ba92c2a1f486f4295ee58fdb62`
- Licence: CC-BY-4.0: the license of the package manifest and of the ImplementationGuide resource, whose copyright reads "Copyright European Union, represented by the European Commission - DG SANTE. Licenced under CC-BY-4.0."
- Note: The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin. https://fhir.ehdsi.eu/eps/package.tgz serves a rebuild of 1.0.0-poc with other bytes (read 2026-10-24).

### `myhealth.eu.fhir.eps-index.html` (committed)

- Source: <https://fhir.ehdsi.eu/eps/index.html>
- Version: 1.0.0-poc build
- sha256: `20605f271ac465fc28695adc88702a71c632deb89f29b1dbfeb847888f7a7302`
- Licence: CC-BY-4.0: a page of the implementation guide whose ImplementationGuide resource declares license CC-BY-4.0 and whose footer carries the same copyright
- Note: The rendered home page, whose scope says the Patient Summary service of MyHealth@EU runs on HL7 CDA and that the guide is a proof of concept of the same service on FHIR; a new build fails the pin loudly.
