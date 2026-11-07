#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferrobridge-imaging.sh
#
# Vendors FerroBRIDGE's ADL 2 imaging report template, the template the HL7
# Europe Imaging Report project the interoperability component ships maps
# over, with the two published openEHR CKM archetypes it specialises or
# holds and the CKM mirror's LICENSE. The FHIR face compiles that project
# against the operational template built from them, and its tests read
# them.
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
#   scripts/vendor/ferrobridge-imaging.sh
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
read_by=$'#1406 (the imaging reports the FHIR face writes from a member\'s\n  record), app/ferrofed-eehrxf and app/ferrofed-server'

pin commit ferrobridge_imaging_report.adlt 'FerroHEALTH/FerroBRIDGE tag v0.0.22, template openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0' \
  "$bridge/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_imaging_report.adlt" \
  1479ca7061d15cf690db731b345b5f15109e918b722fe5c83a2dfa2b730c1f78 "$own" \
  'The ADL 2 template with its template overlays.'
pin commit openEHR-EHR-COMPOSITION.report-result.v1.adl 'CKM mirror commit c3fdc43, revision 1.1.0, PUBLISHED' \
  "$mirror/local/archetypes/composition/openEHR-EHR-COMPOSITION.report-result.v1.adl" \
  c68e587256f0cb46022f5a4ccc5c74099d41a75d143a20824dc1007b19aaddf2 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl 'CKM mirror commit c3fdc43, revision 1.1.3, PUBLISHED' \
  "$mirror/local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl" \
  a273990123f9f9bb213cab4fe91ec98b57c7127791520895550b087863e710d5 "$licence" \
  'The archetype in ADL 1.4.'
pin commit LICENSE 'CKM mirror commit c3fdc43' \
  "$mirror/LICENSE" \
  57af6f95937b451248252ebb22a129d3b65136b0c34340815b60f41b87afb6c6 'Creative Commons Attribution-ShareAlike 3.0 Unported (the mirror'"'"'s LICENSE)' \
  'The licence of the CKM mirror.'
pinned_corpus ferrobridge-imaging "FerroBRIDGE imaging report template" \
  "FerroBRIDGE's imaging report template" \
  "FerroBRIDGE's ADL 2 imaging report template,
\`openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0\`: a
\`COMPOSITION.report-result.v1\` holding one
\`openEHR-EHR-OBSERVATION.imaging_exam_result.v1\`. The HL7 Europe Imaging
Report project the \`eehrxf\` crate ships (\`mappings::IMAGING\`) maps over
this template. Beside it are the two published archetypes of the openEHR
international Clinical Knowledge Manager it specialises or holds, from the
CKM mirror <https://github.com/openEHR/CKM-mirror>, and the mirror's
LICENSE. The template is the Licensor's own, under BUSL-1.1. Each archetype
declares the Creative Commons Attribution-ShareAlike 4.0 licence in its own
\`licence\` field, with the openEHR Foundation as custodian, and is
committed verbatim with attribution, as CC BY-SA permits." "" "$read_by"

say "done"
