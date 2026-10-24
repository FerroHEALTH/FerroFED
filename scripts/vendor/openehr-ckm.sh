#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/openehr-ckm.sh
#
# Vendors two corpora from the openEHR international Clinical Knowledge
# Manager (CKM). The first is the International Patient Summary template: the
# template source (OET), the operational template CKM generates from it
# (OPT), and CKM's record of the template, which states its status and asset
# version. The patient summary's stored section queries select compositions
# by the archetypes this template names for each section, and their test
# reads the OET. The second is the medication archetypes an openEHR mapping
# of prescriptions and dispensations reads,
# openEHR-EHR-INSTRUCTION.medication_order.v3 and
# openEHR-EHR-ACTION.medication.v1, with the three clusters the order's
# slots admit for its authorisation, its status and its supply amount,
# openEHR-EHR-CLUSTER.medication_authorisation.v0,
# openEHR-EHR-CLUSTER.medication_order_summary.v0 and
# openEHR-EHR-CLUSTER.medication_supply_amount.v0: each in ADL 1.4, in the
# archetype XML CKM generates from it, and CKM's record of it.
#
# CKM serves each artefact over its REST API with no git commit to pin, so
# each is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh) and
# committed verbatim. The openEHR Foundation licenses the clinical models CKM
# hosts, archetypes and templates, under CC BY-SA
# (https://openehr.org/governance/#licensing); the template's own licence
# field is empty; each archetype states CC BY-SA 4.0 or 3.0 in its own
# licence field.
# CKM ignores a version query on these URLs, so a pin holds the asset version
# CKM served when it was taken, and a new one fails the sha256 check.
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

order=https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.5946
action=https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.123
licence='"This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/4.0/." (the archetype'"'"'s licence field)'
order_version='CKM cid 1013.1.5946, revision 3.2.1, asset version 7, PUBLISHED'
action_version='CKM cid 1013.1.123, revision 1.6.0, asset version 44, PUBLISHED'

pin commit openEHR-EHR-INSTRUCTION.medication_order.v3.adl "$order_version" "$order/adl" \
  206c2a07fa1a7197ebc9ed7a29a418ecfc9cb6b63fa2aaf811e780ee69df9e9b "$licence" \
  'The archetype in ADL 1.4 as CKM serves it.'
pin commit openEHR-EHR-INSTRUCTION.medication_order.v3.xml "$order_version" "$order/xml" \
  e9bc195b525d4dccc772b27c69c910687c967d147f34f092606cc82124526cc6 "$licence" \
  'The archetype XML CKM generates from the ADL.'
pin commit openEHR-EHR-INSTRUCTION.medication_order.v3.ckm.xml "$order_version" "$order" \
  c5dbf4f8d89d8da4c1cba0321801d5db345c5293b3e9483db1602bbefb6abb00 "$licence" \
  'CKM'"'"'s record of the archetype: its id, status, revision and asset version.'
pin commit openEHR-EHR-ACTION.medication.v1.adl "$action_version" "$action/adl" \
  7e2484d4aec5ad8c5ceb5edb466a82b43b45396fbe3abaa4ee4a298b8e6437bc "$licence" \
  'The archetype in ADL 1.4 as CKM serves it.'
pin commit openEHR-EHR-ACTION.medication.v1.xml "$action_version" "$action/xml" \
  68f3f8f7f656052a9573ee790abde1a999bf25956bd84271585b2bb0d23b3ff2 "$licence" \
  'The archetype XML CKM generates from the ADL.'
pin commit openEHR-EHR-ACTION.medication.v1.ckm.xml "$action_version" "$action" \
  8013573068a7a3f44fa6787c1c1a2242e774042d5bfcc20d8d78e326fee25761 "$licence" \
  'CKM'"'"'s record of the archetype: its id, status, revision and asset version.'
authorisation=https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2300
summary=https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2306
supply=https://ckm.openehr.org/ckm/rest/v1/archetypes/1013.1.2453
licence_3='"This work is licensed under the Creative Commons Attribution-ShareAlike 3.0 License. To view a copy of this license, visit http://creativecommons.org/licenses/by-sa/3.0/." (the archetype'"'"'s licence field)'
authorisation_version='CKM cid 1013.1.2300, revision 0.0.1-alpha, asset version 3, DRAFT'
summary_version='CKM cid 1013.1.2306, revision 0.0.1-alpha, asset version 6, DRAFT'
supply_version='CKM cid 1013.1.2453, revision 0.0.1-alpha, asset version 5, DRAFT'
read_by=$'#980 and #1043 (the openEHR mapping of ePrescriptions and\n  eDispensations, #523)'

pin commit openEHR-EHR-CLUSTER.medication_authorisation.v0.adl "$authorisation_version" "$authorisation/adl" \
  920f4769badd87df62af444846560e6bed940c10ed9b358c026c3c22f025567b "$licence_3" \
  'The archetype in ADL 1.4 as CKM serves it.'
pin commit openEHR-EHR-CLUSTER.medication_authorisation.v0.xml "$authorisation_version" "$authorisation/xml" \
  fcae6569fc7b1a075166586140f5c190d5cb4e551ee6cbea6c8ced42c90c9849 "$licence_3" \
  'The archetype XML CKM generates from the ADL.'
pin commit openEHR-EHR-CLUSTER.medication_authorisation.v0.ckm.xml "$authorisation_version" "$authorisation" \
  6980e1246daf62dd538354056ac4748c465964c1c56497109a8c1bbc09534d59 "$licence_3" \
  'CKM'"'"'s record of the archetype: its id, status, revision and asset version.'
pin commit openEHR-EHR-CLUSTER.medication_order_summary.v0.adl "$summary_version" "$summary/adl" \
  d83990ffb5fd4f2b76ab30b6b18364e1157df742abd481cbfc166b1e01c9caec "$licence_3" \
  'The archetype in ADL 1.4 as CKM serves it.'
pin commit openEHR-EHR-CLUSTER.medication_order_summary.v0.xml "$summary_version" "$summary/xml" \
  10d496d61bb358f2a0f914148f0202ec5ca5370b98f589c94552539a098b3044 "$licence_3" \
  'The archetype XML CKM generates from the ADL.'
pin commit openEHR-EHR-CLUSTER.medication_order_summary.v0.ckm.xml "$summary_version" "$summary" \
  fb3cb807b0df29d0c79e24d60702182136a7f0f37a2c354ca5002fdddfdda98f "$licence_3" \
  'CKM'"'"'s record of the archetype: its id, status, revision and asset version.'
pin commit openEHR-EHR-CLUSTER.medication_supply_amount.v0.adl "$supply_version" "$supply/adl" \
  a5dd6492402310e3b58fa392ef2af68bd09589bc77c9b6bc21011e3232e33df4 "$licence" \
  'The archetype in ADL 1.4 as CKM serves it.'
pin commit openEHR-EHR-CLUSTER.medication_supply_amount.v0.xml "$supply_version" "$supply/xml" \
  3f15e88fb2c3db5be31a6cbd3ac5347585b43e338ccbdc88aacbba06e00c4841 "$licence" \
  'The archetype XML CKM generates from the ADL.'
pin commit openEHR-EHR-CLUSTER.medication_supply_amount.v0.ckm.xml "$supply_version" "$supply" \
  f43c204be43b35dc866062087fc275b99a2036ff844d8e145ee66849b46e5643 "$licence" \
  'CKM'"'"'s record of the archetype: its id, status, revision and asset version.'
pinned_corpus openehr-ckm-medication "openEHR CKM medication archetypes" \
  "the openEHR CKM medication archetypes" \
  "Five archetypes of the openEHR international Clinical Knowledge Manager,
<https://ckm.openehr.org/ckm/>, all in the project \`Medication family of
archetypes\` (CKM cid \`1013.30.27\`). Two are PUBLISHED:
\`openEHR-EHR-INSTRUCTION.medication_order.v3\` (Medication order, CKM cid
\`1013.1.5946\`, revision 3.2.1, asset version 7) and
\`openEHR-EHR-ACTION.medication.v1\` (Medication management, CKM cid
\`1013.1.123\`, revision 1.6.0, asset version 44). Three are DRAFT, each at
revision 0.0.1-alpha: \`openEHR-EHR-CLUSTER.medication_authorisation.v0\`
(Medication authorisation, CKM cid \`1013.1.2300\`, asset version 3),
\`openEHR-EHR-CLUSTER.medication_order_summary.v0\` (CKM cid
\`1013.1.2306\`, asset version 6) and
\`openEHR-EHR-CLUSTER.medication_supply_amount.v0\` (CKM cid \`1013.1.2453\`,
asset version 5). Each is committed in ADL 1.4, in the archetype XML CKM
generates from it, and as CKM's record of it. CKM serves each at an
unversioned URL and ignores a version query, so the pins hold the asset
versions above and a later one fails the sha256 check. Each archetype
declares a Creative Commons Attribution-ShareAlike licence in its own
\`licence\` field, version 4.0 International or, for the authorisation and
order summary clusters, 3.0, with the openEHR Foundation as copyright
holder, and the openEHR Foundation licenses the clinical models CKM hosts
under CC BY-SA (<https://openehr.org/governance/#licensing>, \"Clinical
models\"). Committed verbatim with attribution, as CC BY-SA permits." "" "$read_by"

say "done"
