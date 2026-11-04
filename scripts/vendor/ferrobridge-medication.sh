#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferrobridge-medication.sh
#
# Vendors FerroBRIDGE's ADL 2 medication template, the template the HL7
# Europe MPD project the interoperability component ships maps over, with
# the five published openEHR CKM archetypes it specialises that
# docs/specs/openehr-ckm-medication/ does not hold. The map mode of the
# prescription face compiles that project against the operational template
# built from them, and its tests read them.
#
# The template comes from FerroHEALTH/FerroBRIDGE and the archetypes from
# the openEHR CKM mirror, each at a commit, pinned by URL and sha256
# (scripts/vendor/lib/pinned.sh) and committed verbatim. The template is
# the Licensor's own (BUSL-1.1); each archetype states the Creative Commons
# Attribution-ShareAlike 4.0 International License in its own licence field.
# scripts/checks/pin-freshness.sh lists this script as unwatched: neither
# file has a release feed.
#
# Usage:
#   scripts/vendor/ferrobridge-medication.sh
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

bridge=https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/94f2f95381e96790a7f55ca44a1ca7c150e4638d
summary=https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/ea09df2cc37910437748c00615392df52b3c28d1
mirror=https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094/local/archetypes
own='BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file'"'"'s own SPDX header)'
licence='"This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype'"'"'s licence field)'
read_by=$'#1560 (the map mode of the prescription face) and #1551 (the\n  patient summary'"'"'s placement by template section), app/ferrofed-eehrxf'

pin commit ferrobridge_medication.adlt 'FerroHEALTH/FerroBRIDGE commit 94f2f95, template openEHR-EHR-COMPOSITION.ferrobridge_medication.v1.0.0' \
  "$bridge/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_medication.adlt" \
  7d2224c9cf5b92093096c15286ff56594e88270fc23705caefceeb71a7929f75 "$own" \
  'The ADL 2 template with its template overlays.'
pin commit ferrobridge_patient_summary.adlt 'FerroHEALTH/FerroBRIDGE tag v0.0.17, template openEHR-EHR-COMPOSITION.ferrobridge_patient_summary.v1.0.0' \
  "$summary/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_patient_summary.adlt" \
  9ae82fcf4813d26aacbeec381a059f75fad9848d8737a0bab58ee4a56a1765ee "$own" \
  'The ADL 2 patient summary template, whose sections place the component'"'"'s medication entries.'
pin commit openEHR-EHR-COMPOSITION.health_summary.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.3, PUBLISHED' \
  "$mirror/composition/openEHR-EHR-COMPOSITION.health_summary.v1.adl" \
  ee0a386326ef43d549751da6201885fd027da655963ff8292f277eda1c5ebfc9 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-SECTION.adhoc.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.12, PUBLISHED' \
  "$mirror/section/openEHR-EHR-SECTION.adhoc.v1.adl" \
  a9feab12d11919aceb73465afdf67604427668f76d458f6172b5ecb3cd2d2299 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-SECTION.immunisation_list.v0.adl 'CKM mirror commit c3fdc43, revision 0.0.1-alpha, in development' \
  "$mirror/section/openEHR-EHR-SECTION.immunisation_list.v0.adl" \
  3a38a177aff6c290019e0a4c22fb00408ca44f3cd8dc54d29a138f625c875b3e "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-COMPOSITION.encounter.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.12, PUBLISHED' \
  "$mirror/composition/openEHR-EHR-COMPOSITION.encounter.v1.adl" \
  f334e2d3b7238c634cf753d19ab6e7900e8ffc4baa4b0c9614be4d55168d866c "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-CLUSTER.medication.v2.adl 'CKM mirror commit c3fdc43, revision 2.0.6, PUBLISHED' \
  "$mirror/cluster/openEHR-EHR-CLUSTER.medication.v2.adl" \
  8ea80b21e16b8f7dbbd7d9215b55619bf5c3670813ffa3cd07198745cb9a5bb9 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl 'CKM mirror commit c3fdc43, revision 1.4.2, PUBLISHED' \
  "$mirror/cluster/openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl" \
  d507ee1d0458c6b3089c9108c66a20e33dc78eb437f002e6bf69855ce406e86a "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-CLUSTER.dosage.v2.adl 'CKM mirror commit c3fdc43, revision 2.0.3, PUBLISHED' \
  "$mirror/cluster/openEHR-EHR-CLUSTER.dosage.v2.adl" \
  5b497db6b93eceffe4589d01c8a2771c6dd00fa3a3768b11e2a4a94d99982db9 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-CLUSTER.timing_daily.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.2, PUBLISHED' \
  "$mirror/cluster/openEHR-EHR-CLUSTER.timing_daily.v1.adl" \
  b2ac788c50c76dcf641f59c8dcf75874557df8640712631fcc988197ba39605b "$licence" \
  'The archetype in ADL 1.4.'
pinned_corpus ferrobridge-medication "FerroBRIDGE medication template" \
  "FerroBRIDGE's medication template" \
  "FerroBRIDGE's ADL 2 medication template,
\`openEHR-EHR-COMPOSITION.ferrobridge_medication.v1.0.0\`: an encounter
\`COMPOSITION\` holding one \`openEHR-EHR-INSTRUCTION.medication_order.v3\`
and one \`openEHR-EHR-ACTION.medication.v1\`, whose template overlays fill
the medication, therapeutic direction, dosage and daily timing slots. The
HL7 Europe MPD project the \`eehrxf\` crate ships (\`mappings::MPD\`) maps
over this template. Beside it is FerroBRIDGE's ADL 2 patient summary template,
\`openEHR-EHR-COMPOSITION.ferrobridge_patient_summary.v1.0.0\`, whose ad hoc
medication summary section holds the medication order and whose vaccination
list section holds the medication action, the placement \`eehrxf\` reads in
\`crosswalk::patient_summary::SOURCES\`, and the eight published or
in-development archetypes of the openEHR international Clinical Knowledge
Manager the two specialise that \`docs/specs/openehr-ckm-medication/\` does
not hold, from the CKM mirror <https://github.com/openEHR/CKM-mirror>; the
other five come from there. The templates are the Licensor's own, under
BUSL-1.1. Each archetype declares
the Creative Commons Attribution-ShareAlike 4.0 International License in its
own \`licence\` field, with the openEHR Foundation as custodian, and is
committed verbatim with attribution, as CC BY-SA permits." "" "$read_by"

say "done"
