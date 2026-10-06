#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-ckm.sh
#
# Vendors the openEHR International Patient Summary template from the openEHR
# international Clinical Knowledge Manager (CKM): the template source (OET),
# the operational template CKM generates from it (OPT), and CKM's record of
# the template, which states its status and asset version. The patient
# summary's stored section queries select compositions by the archetypes this
# template names for each section, and their test reads the OET.
#
# CKM serves each artefact over its REST API with no git commit to pin, so
# each is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh) and
# committed verbatim. The openEHR Foundation licenses the clinical models CKM
# hosts, archetypes and templates, under CC BY-SA
# (https://openehr.org/governance/#licensing); the template's own licence
# field is empty.
# scripts/checks/pin-freshness.sh lists this script as unwatched: CKM
# publishes no release feed for a template.
#
# Usage:
#   scripts/vendor/openehr-ckm.sh
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

template=https://ckm.openehr.org/ckm/rest/v1/templates/1013.26.376
licence='CC BY-SA: the licence the openEHR Foundation gives the clinical models CKM hosts, archetypes and templates (https://openehr.org/governance/#licensing); the template'"'"'s own licence field is empty'
version='CKM cid 1013.26.376, asset version 1, DRAFT (2020-08-18)'
read_by=$'#776 (the stored section queries of the patient summary,\n  app/ferrofed-eehrxf)'

pin commit international-patient-summary.oet "$version" "$template/oet" \
  e293bc003fc787e95318be0e18f32ae7ca8903ef8475ed7644b86258a59e5a34 "$licence" \
  'The template source as CKM serves it.'
pin commit international-patient-summary.opt "$version" "$template/opt" \
  981c66aa2b30e7ef686ab02859f65da773c1d97fbe97adc64935e15ebf1b08ad "$licence" \
  'The operational template CKM generates from the source.'
pin commit international-patient-summary.ckm.xml "$version" "$template" \
  83f22fcd2a2cd71e0097ec14f300bd4c5bec2eb139249f9aac9498942e32e8bc "$licence" \
  'CKM'"'"'s record of the template: its id, status, asset version and project.'
pinned_corpus openehr-ckm-ips "openEHR CKM International Patient Summary template" \
  "the openEHR International Patient Summary template" \
  "The International Patient Summary template of the openEHR international
Clinical Knowledge Manager, <https://ckm.openehr.org/ckm/>, template
\`937fca6c-ec24-4c0f-8986-623843b6ebca\`, CKM cid \`1013.26.376\`, at asset
version 1 and status DRAFT. Its template source names one
\`openEHR-EHR-SECTION.adhoc.v1\` per International Patient Summary section and,
in each, the archetypes that section is recorded in. The openEHR Foundation
licenses archetypes and templates hosted in CKM under CC BY-SA
(<https://openehr.org/governance/#licensing>, \"Clinical models\"), and each
archetype the template names declares the Creative Commons
Attribution-ShareAlike 4.0 International License in its own \`licence\`
field; the template's own \`licence\` field is empty. Committed verbatim with
attribution, as CC BY-SA permits." "" "$read_by"

say "done"
