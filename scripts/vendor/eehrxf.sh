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
# Value Sets Catalogue. One corpus per package, each under docs/specs/.
# Last, the Xt-EHR deliverables that are the closest published text of the
# Article 15(1) specifications while the Commission has published no draft
# (Have your say initiative 17512 lists the draft act as upcoming, with no
# attachment): D6.1 (patient summary), D6.2 (electronic prescription and
# dispensation) and D5.2 (exchange-format metadata). They state no licence,
# so they are kept in the cache only and never committed.
#
# Each package is the FHIR package registry's tarball, pinned by URL and
# sha256 (scripts/vendor/lib/pinned.sh) and committed whole, so its sha256
# stays the pin; each corpus row of docs/VERSIONS.md carries its pin-set
# digest. Every package declares CC0-1.0 in its manifest, except the IHE
# Pharmacy package, which declares CC-BY-SA-4.0.
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
read_by=$'#683 (the exchange-format proxies the interoperability\n  component of #519, EHDS readiness, is designed against)'

pin commit xtehr.eu.ehds.models-1.0.0.tgz "1.0.0 (FHIR 5.0.0, 2026-04-13)" \
  "$registry/xtehr.eu.ehds.models/1.0.0" \
  a4853e58a869468464847e2eafa50fb07349e6da1e99456a0d93aae27848e1f8 "$cc0" "$whole"
pinned_corpus eu-xtehr-models "Xt-EHR EHDS Logical Information Models" \
  "the Xt-EHR EHDS Logical Information Models xtehr.eu.ehds.models" \
  "The logical models of the Xt-EHR joint action for the priority categories
of Regulation (EU) 2025/327 Article 14(1), canonical
\`http://www.xt-ehr.eu/fhir/models\`, committed under CC0-1.0." "" "$read_by"

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
pinned_corpus eu-hl7-laboratory "HL7 Europe Laboratory Report" \
  "the HL7 Europe Laboratory Report guide hl7.fhir.eu.laboratory" \
  "The European laboratory report guide, canonical
\`http://hl7.eu/fhir/laboratory\`, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.eu.extensions.r4-1.3.0.tgz "1.3.0 (FHIR 4.0.1, 2026-03-27)" \
  "$registry/hl7.fhir.eu.extensions.r4/1.3.0" \
  8510e930856961d550c04d43f243ef0bdcb27089431fe183ab11c7f805f0c2f1 "$cc0" \
  "$whole The version hl7.fhir.eu.eps 1.0.0-ballot and hl7.fhir.eu.laboratory 2.0.0 depend on."
pin commit hl7.fhir.eu.extensions.r4-1.3.1.tgz "1.3.1 (FHIR 4.0.1, 2026-09-12)" \
  "$registry/hl7.fhir.eu.extensions.r4/1.3.1" \
  37f3ee7ae7a2312e71a4e855e6c36f45d2bc3b13f995bcf4a9fb499ca014cff6 "$cc0" \
  "$whole The version hl7.fhir.eu.base 2.0.1 depends on."
pinned_corpus eu-hl7-extensions "HL7 Europe Extensions" \
  "the HL7 Europe Extensions hl7.fhir.eu.extensions.r4" \
  "The extensions the HL7 Europe guides share, canonical
\`http://hl7.eu/fhir/extensions\`, at both versions the pinned guides depend
on, committed under CC0-1.0." "" "$read_by"

pin commit hl7.fhir.uv.ips-2.0.0.tgz "2.0.0, STU2 (FHIR 4.0.1, 2025-10-03)" \
  "$registry/hl7.fhir.uv.ips/2.0.0" \
  b3964eba08ee699bc121b905c4290641e54dd34f2cf3b5cd3edeb08a40a66979 "$cc0" \
  "$whole The version hl7.fhir.eu.eps 1.0.0-ballot depends on."
pinned_corpus hl7-ips "HL7 International Patient Summary" \
  "the International Patient Summary Implementation Guide hl7.fhir.uv.ips" \
  "The International Patient Summary, canonical \`http://hl7.org/fhir/uv/ips\`,
at the version the HL7 Europe Patient Summary depends on, committed under
CC0-1.0." "" "$read_by"

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
pinned_corpus eu-hl7-imaging "HL7 Europe Imaging Report" \
  "the HL7 Europe Imaging Report R4 guide hl7.fhir.eu.imaging" \
  "The European imaging report guide in its ballot version, canonical
\`http://hl7.eu/fhir/imaging\`, committed under CC0-1.0." "" "$read_by_categories"

pin commit hl7.fhir.eu.hdr-0.1.0-ballot.tgz "0.1.0-ballot (FHIR 4.0.1, 2025-06-03)" \
  "$registry/hl7.fhir.eu.hdr/0.1.0-ballot" \
  458b8ba0edcf22d75d455d6149c9264d486bb8ec052251a90afd8654410482ee "$cc0" \
  "$whole It carries no priority category code."
pinned_corpus eu-hl7-hdr "HL7 Europe Hospital Discharge Report" \
  "the HL7 Europe Hospital Discharge Report guide hl7.fhir.eu.hdr" \
  "The European hospital discharge report guide in its ballot version,
canonical \`http://hl7.eu/fhir/hdr\`, committed under CC0-1.0." "" "$read_by_categories"

pin commit myhealth.eu.fhir.mvc-package-9.1.0.tgz "9.1.0 (FHIR 4.0.1, 2026-05-08)" \
  "$registry/myhealth.eu.fhir.mvc-package/9.1.0" \
  3f99350f36a2a1ea17b7fde37c39ff766bd5c3bc9a29c6183af1acfd07b78ce1 "$cc0" \
  "$whole Its document type value sets code each MyHealth@EU service by LOINC, with no category code system."
pinned_corpus ehdsi-mvc "MyHealth@EU Master Value Sets Catalogue" \
  "the MyHealth@EU Master Value Sets Catalogue myhealth.eu.fhir.mvc-package" \
  "The value sets MyHealth@EU exchanges are coded with, canonical
\`http://fhir.ehdsi.eu/mvc-package\`, committed under CC0-1.0." "" "$read_by_categories"

xtehr=https://www.xt-ehr.eu/wp-content/uploads/2026/05
no_licence='No licence statement in the document or on the deliverables page'
read_by_xtehr=$'#1265 (the re-check of decisions A67 to A72 for the national\n  contact point against the closest published exchange-format text)'
pin cache Xt-EHR-D6.1.pdf "D6.1, as published on xt-ehr.eu in 2026-05" "$xtehr/D6.1-2.pdf" \
  65da226ed74e549e4e3d3611b9a458382a147f8059bd7903b6e1d2bc6fe0374c "$no_licence" \
  'Patient Summary: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems; its section 5 names the MyHealth@EU Requirements Catalogue 9.0.0 (2025-03-24) as an EC resource available only with authorisation.'
pin cache Xt-EHR-D6.2.pdf "D6.2, as published on xt-ehr.eu in 2026-05" "$xtehr/D6.2-2.pdf" \
  1d2478a4c75e056756bf1ebd0093ec263ac5ea0c594b75a860dd4842358bed7c "$no_licence" \
  'Electronic prescription and electronic dispensation: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D5.2.pdf "D5.2, as published on xt-ehr.eu in 2026-05" "$xtehr/Xt-EHR-D5.2.pdf" \
  5d2bb693af07646a506f831854166fcd4d6a3ebf4f092b6937d2a9976410333a "$no_licence" \
  'Technical requirements for EEHRxF metadata.'
pinned_corpus xt-ehr-deliverables "Xt-EHR exchange-format deliverables" \
  "the Xt-EHR deliverables on the European electronic health record exchange format" \
  "Three deliverables of the Xt-EHR joint action (grant 101128085), published
at <https://www.xt-ehr.eu/deliverables/>: D6.1 (Patient Summary), D6.2
(electronic prescription and electronic dispensation) and D5.2 (EEHRxF
metadata). They are the closest published text of the specifications
Regulation (EU) 2025/327 Article 15(1) leaves to an implementing act that
the Commission has not adopted, and no draft of that act is published for
feedback; FerroFED reads them as a draft. They carry no licence statement,
so each is fetched into the cache only and this repository carries none of
their content." "" "$read_by_xtehr"

say "done"
