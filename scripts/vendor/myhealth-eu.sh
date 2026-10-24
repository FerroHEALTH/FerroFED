#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/myhealth-eu.sh
#
# Vendors the public MyHealth@EU and eHDSI material the decisions on the
# national contact point, the exchange format and the priority categories
# read, one corpus each under docs/specs/: the MyHealth@EU FHIR
# implementation guides that https://fhir.ehdsi.eu/ indexes (core, Patient
# Summary, ePrescription and eDispensation, Laboratory Report, Hospital
# Discharge Report, Medical Imaging Report and Vaccination), the eHDSI CDA
# reference test documents of the eHDSI general repository, and the
# specification files of the OpenNCP Transformation Manager (the CDA
# schematrons and the coded element list). The NCPeH API guide and OpenNCP
# itself are vendored by scripts/vendor/eu.sh, the Master Value Sets
# Catalogue by scripts/vendor/eehrxf.sh.
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. A guide
# on the FHIR package registry is pinned at the registry tarball, whose bytes
# never change for a version; a guide only the MyHealth@EU site serves is
# pinned at that site's package URL, which serves the current build, so a new
# build fails the pin loudly. Each package's licence is the one its manifest
# and its ImplementationGuide resource declare (CC0-1.0 or CC-BY-4.0, both
# committed). The CDA test documents state no licence and are cache only;
# the Transformation Manager files are committed under the Apache-2.0 licence
# of the OpenNCP repository, with its LICENSE and NOTICE beside them.
# scripts/checks/pin-freshness.sh reads each package's registry entry or
# build URL and each repository's branch or tags.
#
# Usage:
#   scripts/vendor/myhealth-eu.sh
#
# Requires: curl, shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require curl shasum awk
pinned_begin

registry=https://packages.fhir.org
site=https://fhir.ehdsi.eu
cc0='CC0-1.0: the license of the package manifest and of the ImplementationGuide resource'
ccby='CC-BY-4.0: the license of the package manifest and of the ImplementationGuide resource, whose copyright reads "Copyright European Union, represented by the European Commission - DG SANTE. Licenced under CC-BY-4.0."'
whole='The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
build='The package the MyHealth@EU site serves for its current build (package.json says notForPublication: true); not on packages.fhir.org, so a new build fails the pin loudly.'
read_by=$'#1290 (the re-check of decisions A51, A60 to A72, A81 and A102 against\n  the MyHealth@EU guides), and the testkit'

pin commit myhealth.eu.fhir.core-1.0.0.tgz "1.0.0, Wave 9 (FHIR 4.0.1, 2026-05-04)" \
  "$registry/myhealth.eu.fhir.core/1.0.0" \
  d40b9979dd971a76fc39452aee4091a5f79a9d46192ece7c28e70b4d429a9270 "$cc0" \
  "$whole $site/core/package.tgz serves a rebuild of 1.0.0 of 2026-05-04 with other bytes, and https://build-fhir.ehdsi.eu/core/ a 1.0.1 build of 2026-10-16 (read 2026-10-24)."
pinned_corpus ehdsi-core "MyHealth@EU Core" \
  "the MyHealth@EU Core FHIR IG myhealth.eu.fhir.core" \
  "The shared foundation of the MyHealth@EU FHIR guides, canonical
\`http://fhir.ehdsi.eu/core\`: logical models refining the Xt-EHR EHDS
models with MyHealth@EU obligations, profiles derived from HL7 Europe Base
and Core, and value sets, committed under CC0-1.0." "" "$read_by"

pin commit myhealth.eu.fhir.eps-1.0.0-poc.tgz "1.0.0-poc, QA preview (FHIR 4.0.1, 2026-07-16)" \
  "$registry/myhealth.eu.fhir.eps/1.0.0-poc" \
  34c49509890f6b41f5ffcb788f04cbeefb5379ba92c2a1f486f4295ee58fdb62 "$ccby" \
  "$whole $site/eps/package.tgz serves a rebuild of 1.0.0-poc with other bytes (read 2026-10-24)."
pin commit myhealth.eu.fhir.eps-index.html "1.0.0-poc build" \
  "$site/eps/index.html" \
  20605f271ac465fc28695adc88702a71c632deb89f29b1dbfeb847888f7a7302 \
  'CC-BY-4.0: a page of the implementation guide whose ImplementationGuide resource declares license CC-BY-4.0 and whose footer carries the same copyright' \
  'The rendered home page, whose scope says the Patient Summary service of MyHealth@EU runs on HL7 CDA and that the guide is a proof of concept of the same service on FHIR; a new build fails the pin loudly.'
pinned_corpus ehdsi-eps "MyHealth@EU Patient Summary" \
  "the MyHealth@EU Patient Summary FHIR IG myhealth.eu.fhir.eps" \
  "The proof of concept of the MyHealth@EU Patient Summary service on FHIR,
derived from the HL7 Europe Patient Summary, canonical
\`http://fhir.ehdsi.eu/eps\`, with its rendered home page, committed under CC-BY-4.0, which permits
verbatim redistribution with attribution: the attribution is the copyright
the ImplementationGuide resource carries, kept verbatim in the package." "" "$read_by"

pin commit myhealth.eu.fhir.epd-1.0.0-poc.tgz "1.0.0-poc, QA preview (FHIR 4.0.1, 2026-07-16)" \
  "$registry/myhealth.eu.fhir.epd/1.0.0-poc" \
  5e9e01171e70cb62d523c64c7e47238584bdde3c43875dd09fd2dbb5eaed1d5e "$ccby" \
  "$whole $site/epd/package.tgz serves a rebuild of 1.0.0-poc with other bytes (read 2026-10-24)."
pin commit myhealth.eu.fhir.epd-index.html "1.0.0-poc build" \
  "$site/epd/index.html" \
  f3f922ce9535978fba76fc080c3b209e012f274ed26302861189bae633479b16 \
  'CC-BY-4.0: a page of the implementation guide whose ImplementationGuide resource declares license CC-BY-4.0 and whose footer carries the same copyright' \
  'The rendered home page, whose scope says the ePrescription and eDispensation service of MyHealth@EU runs on HL7 CDA and that the guide is a proof of concept of the same service on FHIR; a new build fails the pin loudly.'
pinned_corpus ehdsi-epd "MyHealth@EU ePrescription and eDispensation" \
  "the MyHealth@EU Electronic Prescription and Dispense FHIR IG myhealth.eu.fhir.epd" \
  "The proof of concept of the MyHealth@EU ePrescription and eDispensation
service on FHIR, derived from the HL7 Europe Medication Prescription and
Dispense guide, canonical \`http://fhir.ehdsi.eu/epd\`, with its rendered
home page, committed under CC-BY-4.0, with the copyright the ImplementationGuide resource carries kept
verbatim in the package." "" "$read_by"

pin commit myhealth.eu.fhir.laboratory-9.1.1.tgz "9.1.1, Wave 9 (FHIR 4.0.1, 2026-05-05)" \
  "$registry/myhealth.eu.fhir.laboratory/9.1.1" \
  3e3bd638231997fd99799a40a7d8934541ca5ba65304f3a4b3886a7e675b0b56 "$cc0" \
  "$whole The same bytes $site/laboratory/package.tgz serves; https://build-fhir.ehdsi.eu/laboratory/ serves a 9.2.0 build of 2026-10-22 (read 2026-10-24)."
pinned_corpus ehdsi-laboratory "MyHealth@EU Laboratory Report" \
  "the MyHealth@EU Laboratory Report FHIR IG myhealth.eu.fhir.laboratory" \
  "The MyHealth@EU Laboratory Result Report service on FHIR (Wave 8), derived
from the HL7 Europe Laboratory Report, canonical
\`http://fhir.ehdsi.eu/laboratory\`, committed under CC0-1.0." "" "$read_by"

pin commit myhealth.eu.fhir.hdr-9.1.1.tgz "9.1.1, draft build (FHIR 4.0.1, 2026-05-05)" \
  "$site/hdr/package.tgz" \
  38eb62092c18aa269c72cdceac01d706b293a5e5389186c682c50467bf84b38c "$cc0" "$build"
pinned_corpus ehdsi-hdr "MyHealth@EU Hospital Discharge Report" \
  "the MyHealth@EU Hospital Discharge Report FHIR IG myhealth.eu.fhir.hdr" \
  "The MyHealth@EU Hospital Discharge Report service on FHIR (Wave 9),
derived from the HL7 Europe Hospital Discharge Report, canonical
\`http://fhir.ehdsi.eu/hdr\`, committed under CC0-1.0." "" "$read_by"

pin commit myhealth.eu.fhir.imaging-9.1.1.tgz "9.1.1, draft build (FHIR 4.0.1, 2026-05-05)" \
  "$site/imaging/package.tgz" \
  286eebc30bc4be9c95b2478152e1dce95a6f23bc31f42f4587fd846ae04a982d "$cc0" \
  "$build Its home page calls it a preliminary draft, not intended for implementation."
pinned_corpus ehdsi-imaging "MyHealth@EU Medical Imaging Report" \
  "the MyHealth@EU Medical Imaging Report FHIR IG myhealth.eu.fhir.imaging" \
  "The MyHealth@EU Medical Imaging Report service on FHIR (Wave 9), a
preliminary draft derived from the HL7 Europe Imaging Report, canonical
\`http://fhir.ehdsi.eu/imaging\`, committed under CC0-1.0." "" "$read_by"

pin commit myhealth.eu.fhir.euvac-10.0.0-preview.tgz "10.0.0-preview, QA preview (FHIR 4.0.1, 2026-07-16)" \
  "$registry/myhealth.eu.fhir.euvac/10.0.0-preview" \
  96bf16bc13cf01b1c9a7642aef20704cb210c4900d83ae4ec6511081535fe3f6 "$ccby" \
  "$whole $site/euvac/package.tgz serves a rebuild of 10.0.0-preview with other bytes (read 2026-10-24)."
pinned_corpus ehdsi-euvac "MyHealth@EU Vaccination" \
  "the MyHealth@EU Vaccination FHIR IG myhealth.eu.fhir.euvac" \
  "The MyHealth@EU vaccination history on FHIR, canonical
\`http://fhir.ehdsi.eu/euvac\`, committed under CC-BY-4.0, with the
copyright the ImplementationGuide resource carries kept verbatim in the
package." "" "$read_by"

general=e521266708aac6e47a1e78f243c178bc0eb7d3b2
pin cache "ehdsi-general-repository-$general.tar.gz" "branch main, commit $general (2023-10-19)" \
  "https://code.europa.eu/ehdsi/ehdsi-general-repository/-/archive/$general/ehdsi-general-repository-$general.tar.gz" \
  e82d8ce55d11e3bcb510a516c9bc2e817c47680d3c939556b0b5c19f17586c28 \
  'No licence statement: the repository has no licence file and its README is the GitLab template' \
  'The repository archive at the commit: cda documents/W5, W6 and W7 (Patient Summary, ePrescription, eDispensation and original clinical documents of three eHDSI waves, with PASS and FAIL cases named by issue key) and unicom (three ePrescriptions), 63 XML files and the README.'
pinned_corpus ehdsi-cda-test-documents "eHDSI CDA reference test documents" \
  "the eHDSI CDA reference test documents" \
  "The CDA documents the eHDSI general repository publishes as reference test
data for the MyHealth@EU CDA services, pinned by commit. The repository
states no licence, so the archive is fetched into the cache only and this
repository carries none of its content." "" \
  $'#1290 (the re-check of decision A72, the NCP-level serialisation)'

openncp=bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f
raw="https://code.europa.eu/ehdsi/ehealth/-/raw/$openncp"
apache='Apache-2.0: the LICENSE and NOTICE files of the OpenNCP repository, vendored here; NOTICE names no third party for this file'
pin commit PatientSummary.sch "tag v10.1.0, commit $openncp" \
  "$raw/openncp-docker/openncp-configuration/TM_resources/schematron/PatientSummary.sch" \
  2189cafe711585c013f782c3d8ccd8b1599bbd929051b69f949e1bef2712966f "$apache" \
  'The Schematron the Transformation Manager validates an epSOS Patient Summary CDA document against.'
pin commit ePrescription.sch "tag v10.1.0, commit $openncp" \
  "$raw/openncp-docker/openncp-configuration/TM_resources/schematron/ePrescription.sch" \
  5800ed284b7a924f5270da6196b07be359177c7252005a406d97f224a159959b "$apache" \
  'The Schematron the Transformation Manager validates an ePrescription CDA document against.'
pin commit eDispensation.sch "tag v10.1.0, commit $openncp" \
  "$raw/openncp-docker/openncp-configuration/TM_resources/schematron/eDispensation.sch" \
  a8bbcfdf07f9034897f9906bf88075e80f0d525bd4e72981a6761ed9f3f291d9 "$apache" \
  'The Schematron the Transformation Manager validates an eDispensation CDA document against.'
pin commit epSOS-PatientSummary-Friendly.sch "tag v10.1.0, commit $openncp" \
  "$raw/openncp-docker/openncp-configuration/TM_resources/schematron/epSOS-PatientSummary-Friendly.sch" \
  10e7c02d36cdf28d30c9a96b323641c9dab562dfb79097a671c67bffeb1853df "$apache" \
  'The friendly (pivot) Patient Summary Schematron of the Transformation Manager.'
pin commit coded_element_list_ehdsi.xml "tag v10.1.0, commit $openncp" \
  "$raw/openncp-core/openncp-core-transformation/src/main/resources/coded_element_list_ehdsi.xml" \
  5d8c68610234b5610bc71858e0c44b273189267bca92f2c1fc5591402fa2b8f8 "$apache" \
  'The eHDSI coded element list: per CDA element path, its usage in each document type and the eHDSI value set the Transformation Manager transcodes it against ("Created based on eHDSI Semantic Implementation Guides and eHDSI PS and eP display documents", its first comment).'
pin commit TM_resources-README.md "tag v10.1.0, commit $openncp" \
  "$raw/openncp-docker/openncp-configuration/TM_resources/README.md" \
  adf206814a86cfd753f6f90cf7b62c9e8f150e258bf2c61f3e28b96f28523b2f "$apache" \
  'What each Transformation Manager resource is for, renamed from README.md so it is not taken for this directory'"'"'s own.'
pin commit LICENSE "tag v10.1.0, commit $openncp" "$raw/LICENSE" \
  bdd2d88c939d6132587468fa1503ade2b0b8661e7c08aa7fe6674bf1e4afdbe9 \
  'Apache-2.0: the licence text itself' 'The Apache License 2.0 the OpenNCP repository is under.'
pin commit NOTICE "tag v10.1.0, commit $openncp" "$raw/NOTICE" \
  294461ca3e847d6cfbf28568b19978634918ff6c536f2bab4669c22c1c08b039 \
  'Apache-2.0: the NOTICE file Apache-2.0 section 4(d) requires to be kept' \
  'The OpenNCP NOTICE: Copyright 2017 European Commission - DG Health and Food Safety, and the third-party files, none of them vendored here.'
pinned_corpus ehdsi-openncp-tm "OpenNCP Transformation Manager resources" \
  "the OpenNCP Transformation Manager resources" \
  "The specification files of the OpenNCP Transformation Manager, the
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
configuration; nothing else in them is a specification or a test corpus." "" \
  $'#1290 (the re-check of decision A72, the NCP-level serialisation), and\n  the testkit'

say "done"
