#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferrobridge-imaging.sh
#
# Vendors FerroBRIDGE's ADL 2 imaging report template, the template the HL7
# Europe Imaging Report project the interoperability component ships maps
# over, with the three published openEHR CKM archetypes it specialises,
# holds or slots and the CKM mirror's LICENSE. The FHIR face compiles that
# project against the operational template built from them, and its tests
# read them.
#
# The template comes from FerroHEALTH/FerroBRIDGE at the release tag v0.0.24
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

bridge=https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/bc1c3e555b5418daff29c01dc4e11fa24f5d1ebe
mirror=https://raw.githubusercontent.com/openEHR/CKM-mirror/c3fdc43dcf0eb7ef0cfe60f548fcdebc24c70094
own='BUSL-1.1, the licence of the Licensor, Cadasto B.V., which publishes FerroBRIDGE (the file'"'"'s own SPDX header)'
licence='Creative Commons Attribution-ShareAlike 4.0 (the archetype'"'"'s licence field)'
read_by=$'#1406 (the imaging reports the FHIR face writes from a member\'s\n  record), app/ferrofed-eehrxf and app/ferrofed-server'

pin commit ferrobridge_imaging_report.adlt 'FerroHEALTH/FerroBRIDGE tag v0.0.24, template openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0' \
  "$bridge/tools/ferrobridge-testkit/fixtures/adl2/ferrobridge_imaging_report.adlt" \
  f12c9a5b386b6fd8bb063a2f36ffd0041d2b3a3d082f917b1238c25e83e495a7 "$own" \
  'The ADL 2 template with its template overlays.'
pin commit openEHR-EHR-COMPOSITION.report-result.v1.adl 'CKM mirror commit c3fdc43, revision 1.1.0, PUBLISHED' \
  "$mirror/local/archetypes/composition/openEHR-EHR-COMPOSITION.report-result.v1.adl" \
  c68e587256f0cb46022f5a4ccc5c74099d41a75d143a20824dc1007b19aaddf2 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl 'CKM mirror commit c3fdc43, revision 1.1.3, PUBLISHED' \
  "$mirror/local/archetypes/entry/observation/openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl" \
  a273990123f9f9bb213cab4fe91ec98b57c7127791520895550b087863e710d5 "$licence" \
  'The archetype in ADL 1.4.'
pin commit openEHR-EHR-CLUSTER.organisation.v1.adl 'CKM mirror commit c3fdc43, revision 1.0.4, PUBLISHED' \
  "$mirror/local/archetypes/cluster/openEHR-EHR-CLUSTER.organisation.v1.adl" \
  ed947e4085b72940e10aab3bc19d68ff8e677bd9494667a3732e4966db8cab8e "$licence" \
  'The archetype in ADL 1.4, in the Imaging service slot of the observation.'
pin commit LICENSE 'CKM mirror commit c3fdc43' \
  "$mirror/LICENSE" \
  57af6f95937b451248252ebb22a129d3b65136b0c34340815b60f41b87afb6c6 'Creative Commons Attribution-ShareAlike 3.0 Unported (the mirror'"'"'s LICENSE)' \
  'The licence of the CKM mirror.'
pinned_corpus ferrobridge-imaging "FerroBRIDGE imaging report template" \
  "FerroBRIDGE's imaging report template" \
  "FerroBRIDGE's ADL 2 imaging report template,
\`openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0\`: a
\`COMPOSITION.report-result.v1\` holding one
\`openEHR-EHR-OBSERVATION.imaging_exam_result.v1\`, whose Imaging service
slot holds the \`openEHR-EHR-CLUSTER.organisation.v1\` that performed the
examination. The HL7 Europe Imaging
Report project the \`eehrxf\` crate ships (\`mappings::IMAGING\`) maps over
this template. Beside it are the three published archetypes of the openEHR
international Clinical Knowledge Manager it specialises, holds or slots, from
the CKM mirror <https://github.com/openEHR/CKM-mirror>, and the mirror's
LICENSE. The template is the Licensor's own, under BUSL-1.1. Each archetype
declares the Creative Commons Attribution-ShareAlike 4.0 licence in its own
\`licence\` field, with the openEHR Foundation as custodian, and is
committed verbatim with attribution, as CC BY-SA permits." "" "$read_by"

say "done"
