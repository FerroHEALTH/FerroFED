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
# depend on. One corpus per package, each under docs/specs/.
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

say "done"
