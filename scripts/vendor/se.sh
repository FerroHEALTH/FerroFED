#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/se.sh
#
# Vendors the Swedish sources of the #488 country research into
# docs/specs/se-inera/: four RIV-TA service contract archives (the
# engagement index, blocking, patient consent and the HSA organisation
# directory), the RIV Tekniska Anvisningar Basic Profile 2.1 and the
# engagement index FAQ, the last two as the Confluence REST API exports
# them.
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and the corpus row of docs/VERSIONS.md carries the pin-set digest. Only
# the Basic Profile states a licence for the whole document; the archives
# carry an Apache-2.0 header on most schema files and none on their
# documents and test data, so they and the FAQ are fetched into the
# git-ignored .vendor-cache/ and never committed.
#
# Usage:
#   scripts/vendor/se.sh
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

bb='https://bitbucket.org/rivta-domains'
mixed='Mixed: most XSD and WSDL files carry the header "Licensed under the Apache License, Version 2.0"; the .docx contract descriptions, the test suites and the remaining files state no licence'
archive='A Bitbucket archive generated per request and pinned by its sha256; a re-generated archive fails the pin loudly.'
live='A Confluence REST export of a live page; each new page version moves the hash.'

pin cache se-riv.itintegration.engagementindex-1.0.10.tar.gz "tag 1.0.10, commit feef3879647a" \
  "$bb/riv.itintegration.engagementindex/get/1.0.10.tar.gz" \
  4b9e05932874860ec706619c515fa520e120adea346806a58415d527f424a611 \
  "$mixed (7 of 8 schema files)" "$archive"
pin cache se-riv.ehr.blocking-3.2.2.tar.gz "tag ehr_blocking_3.2.2, commit 558d8574eefc" \
  "$bb/riv.ehr.blocking/get/ehr_blocking_3.2.2.tar.gz" \
  f1304316630d9408f13d0094372fd188466f716c1184ddba2ccb4a6f05dba0f6 \
  "$mixed (34 of 36 schema files)" "$archive"
pin cache se-riv.ehr.patientconsent-1.0.1_RC1.tar.gz "tag ehr_patientconsent_1.0.1_RC1, commit 70440f0fcd08; only a release-candidate tag exists" \
  "$bb/riv.ehr.patientconsent/get/ehr_patientconsent_1.0.1_RC1.tar.gz" \
  2756ad9c72cf0d46671d64a0a2e7e586b36eef6cbcb24cedb974a80c6d56a3b8 \
  "$mixed (15 of 16 schema files)" "$archive"
pin cache se-riv.infrastructure.directory.organization-5.0.1.tar.gz "tag 5.0.1, commit 6dfc2dbac0da" \
  "$bb/riv.infrastructure.directory.organization/get/5.0.1.tar.gz" \
  59d1aad6fcac539df7ed4216f4f1573ee3639efdb0264c447f6519e3070a6a0c \
  "$mixed (11 of 16 schema files)" "$archive"
pin commit se-rivta-bp21.json "Version 3.1, ARK_0002, 2026-06-08 (Confluence page version 26)" \
  'https://inera.atlassian.net/wiki/rest/api/content/3632875?expand=body.storage,version' \
  b306473e17b041922db6c0663b39ea0063e8e9531fc985a0a27a5432cd9b3ff0 \
  'CC BY-SA 2.5 SE, section 1.3 of the document: "Detta dokument är publicerat under licensen Creative Commons CC-BY-SA (http://creativecommons.org/licenses/by-sa/2.5/se/)", attributed to Sveriges Kommuner och Regioner' \
  "$live"
pin cache se-ei-faq.json "page version 1 (2026-04-01)" \
  'https://inera.atlassian.net/wiki/rest/api/content/5565513926?expand=body.storage,version' \
  9ee599f2004bcfd39f89e0b83c48805fd9895666942603279dd749703771f84f \
  'No licence statement on the page' "$live"
pinned_corpus se-inera "Swedish RIV-TA and Inera documentation" \
  "the Swedish RIV-TA service contracts and Inera documentation" \
  "The service contracts of the engagement index (the national record
locator), blocking, patient consent and the HSA organisation directory, the
RIV-TA Basic Profile 2.1 and the engagement index FAQ. The Basic Profile is
published under CC BY-SA 2.5 SE and is committed. The archives mix
Apache-2.0 schema files with documents and test data that state no licence,
so they stay in the cache with the FAQ."

say "done"
