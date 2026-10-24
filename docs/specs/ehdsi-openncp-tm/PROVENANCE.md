<!-- This file describes vendored third-party material; the bytes beside it
     keep their upstream licence, not the licence of this repository. -->

# Provenance: the OpenNCP Transformation Manager resources

Vendored by `scripts/vendor/myhealth-eu.sh`, each artefact pinned by its
URL and the sha256 of its bytes. Never edit a file here: change the pins in
the script and the pin-set digest in docs/VERSIONS.md, and re-run the script.

- Pin-set digest (sha256 over the sorted `mode  file  url  sha256` lines of
  the pins): `5de8e98013948ea4433573a33c99e2b069129fe4d1e6bbcbe5d27d88c518526a`
- Fetched: 2026-10-24, with the User-Agent `ferrofed-vendor (scripts/vendor)`
- Artefacts: 8 committed, 0 cache only, 0 needing
  manual retrieval
- Files in this directory: 8 besides this one, each verbatim as the
  publisher serves it
- Tree digest (sha256 over the sorted per-file `sha256  path` listing,
  `PROVENANCE.md` excluded): `97c985b02bfe4a6b992f4aea7e7f21e00818ed4f76176a66d777852aad801172`
- Read by: #1290 (the re-check of decision A72, the NCP-level serialisation), and
  the testkit

The specification files of the OpenNCP Transformation Manager, the
reference NCPeH's component that validates, transcodes and translates the
eHDSI CDA documents: the Patient Summary, ePrescription and eDispensation
Schematrons, the friendly Patient Summary Schematron and the eHDSI coded
element list, each a raw file of the OpenNCP repository at tag v10.1.0,
committed under its Apache-2.0 licence with its LICENSE and NOTICE. The CDA
Pharma schema set beside them carries HL7's own per-file terms and is read
from the OpenNCP archive cached under docs/specs/ehdsi/; the rest of
OpenNCP is application source and stays evidence only. The other public
repositories of the ehdsi group were read on 2026-10-24 for specification
material: ehdsi-fhir-ig is the earlier source of the guides above,
superseded by the hl7-fhir-ig group; openatna and ehealth-portal carry
copies of the same Transformation Manager resources and application
configuration; nothing else in them is a specification or a test corpus.

## Artefacts

### `PatientSummary.sch` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-docker/openncp-configuration/TM_resources/schematron/PatientSummary.sch>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `2189cafe711585c013f782c3d8ccd8b1599bbd929051b69f949e1bef2712966f`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: The Schematron the Transformation Manager validates an epSOS Patient Summary CDA document against.

### `ePrescription.sch` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-docker/openncp-configuration/TM_resources/schematron/ePrescription.sch>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `5800ed284b7a924f5270da6196b07be359177c7252005a406d97f224a159959b`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: The Schematron the Transformation Manager validates an ePrescription CDA document against.

### `eDispensation.sch` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-docker/openncp-configuration/TM_resources/schematron/eDispensation.sch>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `a8bbcfdf07f9034897f9906bf88075e80f0d525bd4e72981a6761ed9f3f291d9`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: The Schematron the Transformation Manager validates an eDispensation CDA document against.

### `epSOS-PatientSummary-Friendly.sch` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-docker/openncp-configuration/TM_resources/schematron/epSOS-PatientSummary-Friendly.sch>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `10e7c02d36cdf28d30c9a96b323641c9dab562dfb79097a671c67bffeb1853df`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: The friendly (pivot) Patient Summary Schematron of the Transformation Manager.

### `coded_element_list_ehdsi.xml` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-core/openncp-core-transformation/src/main/resources/coded_element_list_ehdsi.xml>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `5d8c68610234b5610bc71858e0c44b273189267bca92f2c1fc5591402fa2b8f8`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: The eHDSI coded element list: per CDA element path, its usage in each document type and the eHDSI value set the Transformation Manager transcodes it against ("Created based on eHDSI Semantic Implementation Guides and eHDSI PS and eP display documents", its first comment).

### `TM_resources-README.md` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/openncp-docker/openncp-configuration/TM_resources/README.md>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `adf206814a86cfd753f6f90cf7b62c9e8f150e258bf2c61f3e28b96f28523b2f`
- Licence: Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file
- Note: What each Transformation Manager resource is for, renamed from README.md so it is not taken for this directory's own.

### `LICENSE` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/LICENSE>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `bdd2d88c939d6132587468fa1503ade2b0b8661e7c08aa7fe6674bf1e4afdbe9`
- Licence: Apache-2.0: the licence text itself
- Note: The Apache License 2.0 the OpenNCP repository is under.

### `NOTICE` (committed)

- Source: <https://code.europa.eu/ehdsi/ehealth/-/raw/bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f/NOTICE>
- Version: tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
- sha256: `294461ca3e847d6cfbf28568b19978634918ff6c536f2bab4669c22c1c08b039`
- Licence: Apache-2.0: the NOTICE file Apache-2.0 section 4(d) requires to be kept
- Note: The OpenNCP NOTICE: Copyright 2017 European Commission - DG Health and Food Safety, and the third-party files, none of them vendored here.
