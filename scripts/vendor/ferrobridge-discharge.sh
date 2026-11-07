#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferrobridge-discharge.sh
#
# Vendors FerroBRIDGE's ADL 2 discharge report template, the template the
# HL7 Europe Hospital Discharge Report project the interoperability
# component ships maps over, with the three published openEHR CKM archetypes
# it specialises or holds that docs/specs/ferrobridge-medication/ and
# docs/specs/openehr-ckm-medication/ do not hold, and the CKM mirror's
# LICENSE. The FHIR face compiles that project against the operational
# template built from them, and its tests read them.
#
# The template comes from FerroHEALTH/FerroBRIDGE at the release tag v0.0.22
# and the archetypes from the openEHR CKM mirror, each at a commit, pinned by
# URL and sha256 (scripts/vendor/lib/pinned.sh) and committed verbatim. The
# template is the Licensor's own (BUSL-1.1); each archetype states the
# Creative Commons Attribution-ShareAlike 4.0 licence in its own licence
# field, and the mirror's LICENSE states the CC BY-SA 3.0 Unported licence of
# the mirror. scripts/checks/pin-freshness.sh lists this script as
# unwatched: no file has a release feed.
#
# Usage:
#   scripts/vendor/ferrobridge-discharge.sh
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

bridge=https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/7498b0a22f11d9c67040ecf95f35001342c774fb
mirror=https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094
own='BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file'"'"'s own SPDX header)'
licence='Creative Commons Attribution-ShareAlike 4.0 (the archetype'"'"'s licence field)'
read_by=$'#1405 (the discharge reports the FHIR face writes from a member\'s\n  record), app/ferrofed-eehrxf and app/ferrofed-server'

pin commit ferrobridge_discharge_report.adlt 'FerroHEALTH/FerroBRIDGE tag v0.0.22, template openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.0.0' \
  "$bridge/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_discharge_report.adlt" \
  aaf58074d380e1c869425aefb31257cc432f8f80c2a010932b2ce9a344064622 "$own" \
  'The ADL 2 template with its template overlays.'
pin commit openEHR-EHR-COMPOSITION.transfer_summary.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.2, PUBLISHED' \
  "$mirror/local/archetypes/composition/openEHR-EHR-COMPOSITION.transfer_summary.v1.adl" \
  8cfb8ec37925d85c683ad5e2803e16c14bbb51a6d6cec606308c9f8b6a8856ad "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl 'CKM mirror commit c3fdc43, revision 1.7.4, PUBLISHED' \
  "$mirror/local/archetypes/entry/evaluation/openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl" \
  8462a3b811a440c2b01451f0da48c7d297d53b5dc4d45c4722b13321e645ce1f "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-ACTION.procedure.v1.adl 'CKM mirror commit c3fdc43, revision 1.5.3, PUBLISHED' \
  "$mirror/local/archetypes/entry/action/openEHR-EHR-ACTION.procedure.v1.adl" \
  a913b4ef4121bfd0e2c4fa975205fd588c01322cb1526eae128eced1bb1ddf11 "$licence" \
  'The archetype in ADL 1.4.'
pin commit LICENSE 'CKM mirror commit c3fdc43' \
  "$mirror/LICENSE" \
  57af6f95937b451248252ebb22a129d3b65136b0c34340815b60f41b87afb6c6 'Creative Commons Attribution-ShareAlike 3.0 Unported (the mirror'"'"'s LICENSE)' \
  'The licence of the CKM mirror.'
pinned_corpus ferrobridge-discharge "FerroBRIDGE discharge report template" \
  "FerroBRIDGE's discharge report template" \
  "FerroBRIDGE's ADL 2 discharge report template,
\`openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.0.0\`: a transfer
of care summary \`COMPOSITION\` whose context is the hospital stay, holding
three ad hoc \`SECTION\`s, a diagnostic summary with
\`openEHR-EHR-EVALUATION.problem_diagnosis.v1\`, significant procedures with
\`openEHR-EHR-ACTION.procedure.v1\` and discharge medications with
\`openEHR-EHR-INSTRUCTION.medication_order.v3\`. The HL7 Europe Hospital
Discharge Report project the \`eehrxf\` crate ships (\`mappings::HDR\`) maps
over this template. Beside it are the three published archetypes of the
openEHR international Clinical Knowledge Manager it adds to the medication
archetypes of \`docs/specs/ferrobridge-medication/\` and
\`docs/specs/openehr-ckm-medication/\`, from the CKM mirror
<https://github.com/openEHR/CKM-mirror>, and the mirror's LICENSE. The
template is the Licensor's own, under BUSL-1.1. Each archetype declares the
Creative Commons Attribution-ShareAlike 4.0 licence in its own \`licence\`
field, with the openEHR Foundation as custodian, and is committed verbatim
with attribution, as CC BY-SA permits." "" "$read_by"

say "done"
