#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/xt-ehr.sh
#
# Vendors the published output of the Xt-EHR joint action: every deliverable
# and task outcome its deliverables page lists, among them the closest
# published text of the specifications Regulation (EU) 2025/327 Article
# 15(1) leaves to an implementing act, while the Commission has published no
# draft of it (Have your say initiative 17512 lists the draft act as
# upcoming, with no attachment), and the Xt-EHR Metadata Foundation guide.
# Each is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh). The
# deliverables state no licence and the site reserves all rights, so they
# are kept in the git-ignored cache only and never committed; PROVENANCE.md
# under docs/specs/xt-ehr-deliverables/ records them. The metadata guide's
# package declares CC0-1.0 and is committed under docs/specs/eu-xtehr-metadata/.
# The Xt-EHR logical models are vendored by scripts/vendor/eehrxf.sh.
# scripts/checks/pin-freshness.sh reads the version the metadata guide's
# build URL serves; it lists this script as unwatched for the deliverables,
# for which the joint action publishes no release feed.
#
# Usage:
#   scripts/vendor/xt-ehr.sh
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

no_licence='No licence statement in the document; the deliverables page footer reads "© 2025 Xt-EHR. All rights reserved."'
read_by_xtehr=$'#1265 and #1290 (the re-checks of decisions A51, A60 to A72, A81 and\n  A102 against the closest published exchange-format text)'
pin cache Xt-EHR-D1.1.pdf "D1.1, no version statement in the text, PDF of 2024-07-03" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D1.1.pdf' \
  60f8ccef12d343545cb847e8a7397afe2ea19c78d7607febeb1c593354c9901d "$no_licence" \
  'D1.1: Project Management Plan and overall requirement analysis.'
pin cache Xt-EHR-D2.1.pdf "D2.1, version 3.0 (its cover), PDF of 2024-07-04" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D2.1.pdf' \
  cb72a36611f76d644e1d7c3a28c2d000b6ced0aca468fc9088134714706e539c "$no_licence" \
  'D2.1: Dissemination & Communication plan.'
pin cache Xt-EHR-D2.2.pdf "D2.2, no version statement in the text, PDF of 2026-04-30" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D2.2.pdf' \
  8d524c7f8c5da705e00985e79d9924de0f0baa04da16bd486ebb702ef96779b0 "$no_licence" \
  'D2.2: Dissemination & Events report.'
pin cache Xt-EHR-D2.3.pdf "D2.3, history to 1.0 (final review), last change 30/4/2025, PDF of 2025-04-30" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D2.3.pdf' \
  c5d1ad528b19dc52a8f7476263601cd30d2731013d0a6c2b9e52c78b02cc3d17 "$no_licence" \
  'D2.3: Project educational and dissemination platform: User Requirements & Specifications.'
pin cache Xt-EHR-D3.1.pdf "D3.1, version 1.1 (its cover), PDF of 2024-07-04" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D3.1.pdf' \
  e6f1f1cdaa27b8c713be006918a488aa91b6a6898e49ec993c64c78d064380f5 "$no_licence" \
  'D3.1: Project Evaluation Plan.'
pin cache Xt-EHR-D3.2.pdf "D3.2, no version statement in the text, PDF of 2026-04-23" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D3.2.pdf' \
  d6fd7d1384ee6ce9e77f1e13ddecd5e21c1f7022c68dfae9630fe0eaab4e1555 "$no_licence" \
  'D3.2: Overall Evaluation Report.'
pin cache Xt-EHR-D3.3.pdf "D3.3, status as per June 2026 (its text), PDF of 2026-06-12" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/06/D3.3_List-of-action-level-indicators-1.pdf' \
  348f3a828446952b3d151ba34a170938ee1a4383522043f81612494629c0f27a "$no_licence" \
  'D3.3: List of Action Level Indicators.'
pin cache Xt-EHR-D4.1.pdf "D4.1, no version statement in the text, PDF of 2024-07-03" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D4.1.pdf' \
  d516790db5414e1d36c5ee4751aae9ed5afc03c301e1f15a7a6239084c051062 "$no_licence" \
  'D4.1: Current status of play in EHR and primary use of data.'
pin cache Xt-EHR-D4.2.pdf "D4.2, no version statement in the text, PDF of 2026-03-13" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D4.2.pdf' \
  f2a00fa8446e244ec8be548634b4a495ef1c82b0e0bccc9891d36edbe4758f41 "$no_licence" \
  'D4.2: Sustainability Plan and Mapping of Proxy Services.'
pin cache Xt-EHR-D5.1.pdf "D5.1, no version statement in the text, PDF of 2026-05-18" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D5.1.pdf' \
  dc3f96e0cc6cc47b390e91614550f8b2b30645dbbe619909850b79204b3084b9 "$no_licence" \
  'D5.1: Technical Requirements for EHRs and key system interfaces.'
pin cache Xt-EHR-D5.2.pdf "D5.2, no version statement in the text, PDF of 2026-04-09" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D5.2.pdf' \
  5d2bb693af07646a506f831854166fcd4d6a3ebf4f092b6937d2a9976410333a "$no_licence" \
  'D5.2: Technical Requirements for EEHRxF metadata.'
pin cache Xt-EHR-T5.2-Gen2EHR-Workshop2.pdf "T5.2 outcome, no version statement in the text, PDF of 2026-05-25" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Gen2EHR_Workshop2_Integrating_Genomic_Data_into_EHR_Xt-EHR-Deliverable.pdf' \
  e6f33679a34e0f13bc3a1cd452fd94d802656b6e1eef489fa7969558a459d76a "$no_licence" \
  'T5.2 outcome: Gen2EHR Workshop2 (September 2025): Integrating Genomic Data into EHR.'
pin cache Xt-EHR-MS17-T5.3.docx "T5.3 outcome (MS17), no version statement in the text, document modified 2025-11-03" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/MS17-Task-5.3-Identification-and-Authentication-of-Patients-and-Healthcare-Professionals-in-Europe.docx' \
  5a0c06f30ab57d8e2440537803230e37f61731f5f20d2cbebcda3393ae557e3e "$no_licence" \
  'T5.3 outcome (MS17): Identification and Authentication of Patients and Healthcare Professionals in Europe.'
pin cache Xt-EHR-D6.1.pdf "D6.1, no version statement in the text, PDF of 2026-05-19" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D6.1-2.pdf' \
  65da226ed74e549e4e3d3611b9a458382a147f8059bd7903b6e1d2bc6fe0374c "$no_licence" \
  'D6.1: Patient Summary: Implementation guides on EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D6.2.pdf "D6.2, no version statement in the text, PDF of 2026-05-04" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D6.2-2.pdf' \
  1d2478a4c75e056756bf1ebd0093ec263ac5ea0c594b75a860dd4842358bed7c "$no_licence" \
  'D6.2: Electronic prescription and electronic dispensation: Implementation guides on EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D7.1.pdf "D7.1, no version statement in the text, PDF of 2026-05-04" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D7.1-2.pdf' \
  e74823c6bfd94899f0fd9329a1ecf62bf4ad9988d25b5d4df17da7f4a34c779f "$no_licence" \
  'D7.1: Laboratory results and reports: Implementation guides on EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D7.2.pdf "D7.2, no version statement in the text, PDF of 2026-05-19" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D7.2-2.pdf' \
  f4c865d52549eb281f4109888739f0c061aa1efd710551270c9e65db30c95cff "$no_licence" \
  'D7.2: Medical images and reports: Implementation guides on EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D7.3.pdf "D7.3, no version statement in the text, PDF of 2026-05-19" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D7.3.pdf' \
  c202f974acab433b5dc47207626d6e6ea1d1100fc9851fbeaeeecce20f8f83cc "$no_licence" \
  'D7.3: Discharge reports: Implementation guides on EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D8.1.pdf "D8.1, no version statement in the text, PDF of 2026-05-13" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/D8.1-2.pdf' \
  2e8d952db7bdc50c0f24beb2f22a47d0f4c984fac2df6bfa731fe57db2f0af68 "$no_licence" \
  'D8.1: Classification and functional profiles of EHR systems guidelines.'
pin cache Xt-EHR-D8.2.pdf "D8.2, no version statement in the text, PDF of 2026-05-19" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D8.2.pdf' \
  a8cd93d994bceb1c7de952b006384d2c638a038907bba48325985ed7d851fbf4 "$no_licence" \
  'D8.2: EHR Conformity Assessment Scheme: Assertion Document and Checklists.'
pin cache Xt-EHR-D8.3.pdf "D8.3, no version statement in the text, PDF of 2026-02-02" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D8.3.pdf' \
  49e57c555e1690b07d8bb90f1d5e2882f0717fb8bf84ba33c3259fe344f1a5cf "$no_licence" \
  'D8.3: Wellness application labelling guidelines.'
pin cache Xt-EHR-D9.1.pdf "D9.1, version 2.0 of 30/01/2025, the submitted version (its history), PDF of 2025-01-30" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D9.1.pdf' \
  0e77e7434c477eac9b6b609322a1bc6e0c3a7196a9b0160a0f8aa020108d011b "$no_licence" \
  'D9.1: Requirements and use cases on the availability of health data in crossborder telemedicine services.'
pin cache Xt-EHR-D9.2.pdf "D9.2, version 2.0 of 20/03/2026 (its history), PDF of 2026-04-02" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D9.2.pdf' \
  f454948ea62372677ac3799253b6dd02f35ed5537d8c2b89c8fefb3a565e954d "$no_licence" \
  'D9.2: Technical specifications on the availability of health data in cross-border telemedicine services.'
pin cache Xt-EHR-D9.3.pdf "D9.3, version 2.1 of 09/02/2026 (its history), PDF of 2026-03-03" \
  'https://www.xt-ehr.eu/wp-content/uploads/2026/05/Xt-EHR-D9.3.pdf' \
  29c7396f1211e6e0790d97f914382fb6602389312aa8688e8143e9cd0b5857ed "$no_licence" \
  'D9.3: Requirements for Large-Scale Uptake of Telemedicine Service.'
pinned_corpus xt-ehr-deliverables "Xt-EHR deliverables" \
  "the Xt-EHR deliverables" \
  "Every deliverable and task outcome the Xt-EHR joint action (grant
101128085) lists at <https://www.xt-ehr.eu/deliverables/> (read
2026-10-24): D1.1 to D9.3, twenty-two deliverables, with the T5.2 and T5.3
outcomes. Each note gives the number and title the page gives it, and each
version the version and date its text states, or the date of the file when
it states none. No deliverable the page lists is unpublished, and D3.3 says
all 22 deliverables are complete, one pending approval by the Commission,
without naming it. D5.1, D5.2, D6.1, D6.2, D7.1 to D7.3 and D8.1 are the
closest published text of the specifications Regulation (EU) 2025/327
Article 15(1) leaves to an implementing act the Commission has not adopted,
and no draft of that act is published for feedback; FerroFED reads them as
a draft. They carry no licence statement and the site reserves all rights,
so each is fetched into the cache only and this repository carries none of
their content." "" "$read_by_xtehr"

pin commit hl7.eu.fhir.xt-ehr.metadata-0.1.0.tgz "0.1.0, ci-build (FHIR 5.0.0, 2025-09-10)" \
  https://xt-ehr.github.io/xt-ehr-metadata/package.tgz \
  50066947a06a59f819b44024650a2ebada71dea40479c239db770b0893713a4a \
  'CC0-1.0: the license of the package manifest and of the ImplementationGuide resource' \
  'The continuous build of https://github.com/Xt-EHR/xt-ehr-metadata, the only build the README names; not on packages.fhir.org, so a new build fails the pin loudly.'
pinned_corpus eu-xtehr-metadata "Xt-EHR Metadata Foundation" \
  "the Xt-EHR Metadata Foundation FHIR IG hl7.eu.fhir.xt-ehr.metadata" \
  "The Xt-EHR metadata guide behind D5.2, canonical
\`https://www.xt-ehr.eu/specifications/fhir\`, committed under CC0-1.0." "" \
  $'#1290 (the re-check of decision A81 against the exchange-format\n  metadata), and the testkit'

say "done"
