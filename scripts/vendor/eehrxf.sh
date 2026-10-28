#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/eehrxf.sh
#
# Vendors the published proxies for the European electronic health record
# exchange format of Regulation (EU) 2025/327 Article 15(1), until its
# implementing act is adopted: the Xt-EHR EHDS Logical Information Models and
# the HL7 Europe FHIR implementation guides (Base and Core, Patient Summary,
# Medication Prescription and Dispense, Laboratory Report), with the HL7
# Europe Extensions, the International Patient Summary and the IHE Pharmacy
# Medication Prescription and Dispense profile at the versions those guides
# depend on. Beside them, the packages the categories of the access record
# are read against: the HL7 Europe EU Health Data API, whose code system
# codes the priority categories of Article 14(1), the HL7 Europe Imaging
# Report and Hospital Discharge Report guides, and the MyHealth@EU Master
# Value Sets Catalogue, and the IHE MHD profile the MyHealth@EU NCPeH API
# depends on. One corpus per package, each under docs/specs/.
# Each package is the FHIR package registry's tarball, pinned by URL and
# sha256 (scripts/vendor/lib/pinned.sh) and committed whole, so its sha256
# stays the pin; each corpus row of docs/VERSIONS.md carries its pin-set
# digest. Every package declares CC0-1.0 in its manifest, except the IHE
# Pharmacy package, which declares CC-BY-SA-4.0, and IHE MHD, which declares
# CC-BY-4.0.
# The interoperability component itself, eehrxf, is published from
# FerroHEALTH/FerroBRIDGE (#813), which vendors the versions its own tests
# read; the gateway keeps every package its testkit, its validator lane and
# its decisions read, the dependency closure of the MyHealth@EU guides
# included.
# scripts/checks/pin-freshness.sh reads each package's registry entry.
#
# Usage:
#   scripts/vendor/eehrxf.sh
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
cc0='CC0-1.0: the license of the package manifest and of the ImplementationGuide resource'
whole='The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
read_by=$'#683 (the exchange-format proxies the gateway\'s FHIR face and its\n  testkit are held to; the component itself moved to FerroHEALTH/FerroBRIDGE\n  with #813)'

pin commit xtehr.eu.ehds.models-1.0.0.tgz "1.0.0 (FHIR 5.0.0, 2026-04-13)" \
  "$registry/xtehr.eu.ehds.models/1.0.0" \
  a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8 "$cc0" "$whole"
pin commit xtehr.eu.ehds.models-0.3.0.tgz "0.3.0 (FHIR 5.0.0, 2026-02-06)" \
  "$registry/xtehr.eu.ehds.models/0.3.0" \
  f2cb6b0b5b35cab33e9b618b38c54797ece61f5e2dda59982c952ecd9398865b "$cc0" \
  "$whole The version every MyHealth@EU guide of scripts/vendor/myhealth-eu.sh depends on."
pinned_corpus eu-xtehr-models "Xt-EHR EHDS Logical Information Models" \
  "the Xt-EHR EHDS Logical Information Models xtehr.eu.ehds.models" \
  "The logical models of the Xt-EHR joint action for the priority categories
of Regulation (EU) 2025/327 Article 14(1), canonical
\`http://www.xt-ehr.eu/fhir/models\`, at the release and at the version the
MyHealth@EU guides depend on, committed under CC0-1.0. The continuous build
of the source repository (https://xt-ehr.github.io/xt-ehr-common/) serves
0.4.0 of 2026-02-26, older than the 1.0.0 release (read 2026-10-24)." "" "$read_by"

pin commit hl7.fhir.eu.base-2.0.1.tgz "2.0.1 (FHIR 4.0.1, 2026-09-13)" \
  "$registry/hl7.fhir.eu.base/2.0.1" \
  3fb23b64d70656ea809e1ad66d84d5d008400e18424266debf176cb46fba0b4a "$cc0" "$whole"
pinned_corpus eu-hl7-base "HL7 Europe Base and Core" \
  "the HL7 Europe Base and Core FHIR IG hl7.fhir.eu.base" \
  "The European base and core profiles the other HL7 Europe guides build on,
canonical \`http://hl7.eu/fhir/base\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.eps-1.0.0-ballot.tgz "1.0.0-ballot (FHIR 4.0.1, 2026-06-06)" \
  "$registry/hl7.fhir.eu.eps/1.0.0-ballot" \
  e0fff1fb20d3daf75609259faa6b860131ebf4fc8890bb68f0da882307afc33c "$cc0" \
  "$whole The only version on the registry; scripts/checks/pin-freshness.sh reports its first release."
pinned_corpus eu-hl7-eps "HL7 Europe Patient Summary" \
  "the HL7 Europe Patient Summary hl7.fhir.eu.eps" \
  "The European Patient Summary guide in its ballot version, canonical
\`http://hl7.eu/fhir/eps\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.mpd-1.0.0.tgz "1.0.0 (FHIR 4.0.1, 2026-05-11)" \
  "$registry/hl7.fhir.eu.mpd/1.0.0" \
  f1bc1084efc93e16ae4b45c1bb71340385d314a72031949c4a7e0d97aea5fd6b "$cc0" "$whole"
pinned_corpus eu-hl7-mpd "HL7 Europe Medication Prescription and Dispense" \
  "the HL7 Europe Medication Prescription and Dispense guide hl7.fhir.eu.mpd" \
  "The European ePrescription and eDispensation guide, canonical
\`http://hl7.eu/fhir/mpd\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.laboratory-2.0.0.tgz "2.0.0 (FHIR 4.0.1, 2026-05-05)" \
  "$registry/hl7.fhir.eu.laboratory/2.0.0" \
  097aa45c6efcdbc3f25ead8b7d448fee5011b5e7ecf264aa400c5e2c69c5c488 "$cc0" "$whole"
pin commit hl7.fhir.eu.laboratory-2.1.0.tgz "2.1.0 (FHIR 4.0.1, 2026-10-16)" \
  "$registry/hl7.fhir.eu.laboratory/2.1.0" \
  87019c396f4cff934ec5230ce1f97d76b79b40591be08161e694ad661129379c "$cc0" \
  "$whole The current release; 2.0.0 is the version MyHealth@EU Laboratory 9.1.1 and Hospital Discharge Report 9.1.1 depend on, and the one the interoperability component reads."
pinned_corpus eu-hl7-laboratory "HL7 Europe Laboratory Report" \
  "the HL7 Europe Laboratory Report guide hl7.fhir.eu.laboratory" \
  "The European laboratory report guide, canonical
\`http://hl7.eu/fhir/laboratory\`, at its current release and at the
version the MyHealth@EU guides and the interoperability component read,
committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.extensions.r4-1.3.0.tgz "1.3.0 (FHIR 4.0.1, 2026-03-27)" \
  "$registry/hl7.fhir.eu.extensions.r4/1.3.0" \
  8510e930856961d550c04d43f243ef0bdcb27089431fe183ab11c7f805f0c2f1 "$cc0" \
  "$whole The version hl7.fhir.eu.eps 1.0.0-ballot and hl7.fhir.eu.laboratory 2.0.0 depend on."
pin commit hl7.fhir.eu.extensions.r4-1.3.1.tgz "1.3.1 (FHIR 4.0.1, 2026-09-12)" \
  "$registry/hl7.fhir.eu.extensions.r4/1.3.1" \
  37f3ee7ae7a2312e71a4e855e6c36f45d2bc3b13f995bcf4a9fb499ca014cff6 "$cc0" \
  "$whole The version hl7.fhir.eu.base 2.0.1 depends on."
pin commit hl7.fhir.eu.extensions-0.1.0.tgz "0.1.0 (FHIR 4.0.1, 2024-02-20)" \
  "$registry/hl7.fhir.eu.extensions/0.1.0" \
  f30da1e061c6ce5917970a05f2517b11ebd37300ca5c9327d9caa8d4b789ad4e "$cc0" \
  "$whole The package published before the .r4 name, at the version myhealth.eu.fhir.hdr 9.1.1 depends on."
pinned_corpus eu-hl7-extensions "HL7 Europe Extensions" \
  "the HL7 Europe Extensions hl7.fhir.eu.extensions.r4 and hl7.fhir.eu.extensions" \
  "The extensions the HL7 Europe guides share, canonical
\`http://hl7.eu/fhir/extensions\`, at both versions the pinned HL7 Europe
guides depend on, and as \`hl7.fhir.eu.extensions\` 0.1.0, the package name
before the \`.r4\` one, at the version MyHealth@EU Hospital Discharge Report
9.1.1 depends on, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.uv.ips-2.0.0.tgz "2.0.0, STU2 (FHIR 4.0.1, 2025-10-03)" \
  "$registry/hl7.fhir.uv.ips/2.0.0" \
  b3964eba08ee699bc121b905c4290641e54dd34f2cf3b5cd3edeb08a40a66979 "$cc0" \
  "$whole The version hl7.fhir.eu.eps 1.0.0-ballot depends on."
pin commit hl7.fhir.uv.ips-2.0.1.tgz "2.0.1, STU2 (FHIR 4.0.1, 2026-06-19)" \
  "$registry/hl7.fhir.uv.ips/2.0.1" \
  7183242b70fb2a9058aa3701fb607517a3c2fd0e3100d1d8c538d744c2adf799 "$cc0" \
  "$whole The version hl7.fhir.eu.laboratory 2.1.0 and hl7.fhir.eu.hdr 1.0.0 depend on."
pinned_corpus hl7-ips "HL7 International Patient Summary" \
  "the International Patient Summary Implementation Guide hl7.fhir.uv.ips" \
  "The International Patient Summary, canonical \`http://hl7.org/fhir/uv/ips\`,
at the version the HL7 Europe Patient Summary depends on and at the version
HL7 Europe Laboratory Report 2.1.0 and Hospital Discharge Report 1.0.0
depend on, committed under CC0-1.0." "" "$read_by"

pin commit ihe.pharm.mpd.r4-1.0.0-comment-2.tgz "1.0.0-comment-2 (FHIR 4.0.1, 2025-05-27)" \
  "$registry/ihe.pharm.mpd.r4/1.0.0-comment-2" \
  05eda0d1871d2980cb13023d18a5a9618c9945c3a85cf721c14c5e989e0d3776 \
  'CC-BY-SA-4.0: the license of the package manifest and of the ImplementationGuide resource' \
  "$whole The version hl7.fhir.eu.base 2.0.1, hl7.fhir.eu.eps 1.0.0-ballot and hl7.fhir.eu.mpd 1.0.0 depend on."
pinned_corpus ihe-pharm-mpd "IHE Pharmacy Medication Prescription and Dispense" \
  "the IHE Pharmacy Medication Prescription and Dispense profile ihe.pharm.mpd.r4" \
  "The IHE Pharmacy profile for medication prescription and dispense,
canonical \`https://profiles.ihe.net/PHARM/MPD\`, in the public-comment
version the HL7 Europe Base and Core, Patient Summary and Medication
Prescription and Dispense guides depend on, committed under CC-BY-SA-4.0,
which permits verbatim redistribution with attribution." "" "$read_by"

pin commit ihe.iti.mhd-4.2.2.tgz "4.2.2 (FHIR 4.0.1, 2024-05-23)" \
  "$registry/ihe.iti.mhd/4.2.2" \
  824061df179f6ade65c29bd79dfc35f00075748fe13054c8505a6e6f3d1cd578 \
  'CC-BY-4.0: the license of the package manifest and of the ImplementationGuide resource' \
  "$whole The version myhealth.eu.fhir.ncp-api 9.1.1-ci depends on."
pinned_corpus ihe-mhd "IHE MHD FHIR package" \
  "the IHE Mobile access to Health Documents profile ihe.iti.mhd" \
  "The IHE ITI Mobile access to Health Documents profile, canonical
\`https://profiles.ihe.net/ITI/MHD\`, at the version the MyHealth@EU NCPeH
API depends on, committed under CC-BY-4.0, which permits verbatim
redistribution with attribution." "" "$read_by"

read_by_categories=$'#824 (the wire form of the Article 14(1) categories, decision A81,\n  which the access record of ehds-logging writes from FerroHEALTH/FerroAUDIT\n  since #1176), and the eehrxf tests and the testkit'

pin commit hl7.fhir.eu.health-data-api-1.0.0-ballot.tgz "1.0.0-ballot (FHIR 4.0.1, 2026-03-13)" \
  "$registry/hl7.fhir.eu.health-data-api/1.0.0-ballot" \
  5204af60bf7f74f301a627e641a34683d392014f53d5df8ba3651c964e92977b "$cc0" \
  "$whole It defines EEHRxFDocumentPriorityCategoryCS, the code system of the six priority categories."
pinned_corpus eu-hl7-health-data-api "HL7 Europe EU Health Data API" \
  "the HL7 Europe EU Health Data API hl7.fhir.eu.health-data-api" \
  "The European health data API guide in its ballot version, canonical
\`http://hl7.eu/fhir/health-data-api\`, whose
\`CodeSystem/eehrxf-document-priority-category-cs\` codes the priority
categories of Regulation (EU) 2025/327 Article 14(1), committed under
CC0-1.0." "" "$read_by_categories"

pin commit hl7.fhir.eu.imaging-1.0.0-ballot.tgz "1.0.0-ballot (FHIR 4.0.1, 2026-03-16)" \
  "$registry/hl7.fhir.eu.imaging/1.0.0-ballot" \
  bb6012bd7a019ac82f5e939dd6e892e6db6b15fd2931dcf7cc445c1ad92c3d70 "$cc0" \
  "$whole Its Composition and DiagnosticReport profiles require the priority category code Medical-Imaging."
pin commit hl7.fhir.eu.imaging-1.0.0-alpha.tgz "1.0.0-alpha (FHIR 4.0.1, 2026-02-27)" \
  "https://hl7.eu/fhir/imaging/xtehr/package.tgz" \
  bbc369b9aadabd96b223f4944643a9adff0c40479c61970153661a50bba73e89 "$cc0" \
  "The tarball HL7 Europe serves at the path its package-list.json gives for 1.0.0-alpha, which the FHIR package registry does not list; the version myhealth.eu.fhir.imaging 9.1.1 depends on."
pinned_corpus eu-hl7-imaging "HL7 Europe Imaging Report" \
  "the HL7 Europe Imaging Report R4 guide hl7.fhir.eu.imaging" \
  "The European imaging report guide in its ballot version and in the
1.0.0-alpha preview MyHealth@EU Medical Imaging Report 9.1.1 depends on,
canonical \`http://hl7.eu/fhir/imaging\`, committed under CC0-1.0." "" "$read_by_categories"

pin commit hl7.fhir.eu.hdr-0.1.0-ballot.tgz "0.1.0-ballot (FHIR 4.0.1, 2025-06-03)" \
  "$registry/hl7.fhir.eu.hdr/0.1.0-ballot" \
  458b8ba0edcf22d75d455d6149c9264d486bb8ec052251a90afd8654410482ee "$cc0" \
  "$whole It carries no priority category code."
pin commit hl7.fhir.eu.hdr-1.0.0.tgz "1.0.0 (FHIR 4.0.1, 2026-10-16)" \
  "$registry/hl7.fhir.eu.hdr/1.0.0" \
  eedf9af9f66b68cd66f7d6c678a36dfba86b0920df4ef15e8d2da900c4134879 "$cc0" \
  "$whole The first release; MyHealth@EU Hospital Discharge Report 9.1.1 derives from 0.1.0-ballot."
pinned_corpus eu-hl7-hdr "HL7 Europe Hospital Discharge Report" \
  "the HL7 Europe Hospital Discharge Report guide hl7.fhir.eu.hdr" \
  "The European hospital discharge report guide at its first release and in
the ballot version the access record's categories were read against,
canonical \`http://hl7.eu/fhir/hdr\`, committed under CC0-1.0." "" "$read_by_categories"

pin commit myhealth.eu.fhir.mvc-package-9.1.0.tgz "9.1.0 (FHIR 4.0.1, 2026-05-08)" \
  "$registry/myhealth.eu.fhir.mvc-package/9.1.0" \
  3f99350f36a2a1ea17b7fde37c39ff766bd5c3bc9a29c6183af1acfd07b78ce1 "$cc0" \
  "$whole Its document type value sets code each MyHealth@EU service by LOINC, with no category code system."
pinned_corpus ehdsi-mvc "MyHealth@EU Master Value Sets Catalogue" \
  "the MyHealth@EU Master Value Sets Catalogue myhealth.eu.fhir.mvc-package" \
  "The value sets MyHealth@EU exchanges are coded with, canonical
\`http://fhir.ehdsi.eu/mvc-package\`, committed under CC0-1.0." "" "$read_by_categories"

say "done"
