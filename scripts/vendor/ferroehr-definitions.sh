#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferroehr-definitions.sh
#
# Vendors FerroEHR's professional PERSON and ROLE definitions, the two ADL 2
# archetypes a FerroEHR node of release 5.7.0 or later loads with
# `ferroehr definitions load` before `serve`, so that its party mapping finds
# its templates at boot and binds every person who signs in. The quickstart
# inlines them for its four nodes (compose.yaml) and the e2e harness loads
# them into every node it starts with authentication on.
#
# Both come from FerroHEALTH/FerroEHR at the release tag v5.7.0, with the
# CC BY-SA 3.0 licence text beside them, pinned by URL and sha256
# (scripts/vendor/lib/pinned.sh) and committed verbatim. FerroEHR adapted them
# from openEHR CKM archetypes and states that they carry the CKM sources'
# Creative Commons Attribution-ShareAlike 3.0 licence and their copyright line.
# scripts/checks/pin-freshness.sh lists this script as unwatched: it moves
# with the FerroEHR node image pin.
#
# Usage:
#   scripts/vendor/ferroehr-definitions.sh
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

ehr=https://raw.githubusercontent.com/FerroHEALTH/FerroEHR/7d85f0a1d2b86ca5bf2d43025090e6a102d99f74
licence='Creative Commons Attribution-ShareAlike 3.0, © openEHR Foundation (FerroEHR'"'"'s deploy/definitions/professional/PROVENANCE.md)'
read_by=$'#1615 (the quickstart and the e2e nodes reached under the caller),\n  compose.yaml and tools/ferrofed-testkit'

pin commit openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls 'FerroHEALTH/FerroEHR tag v5.7.0' \
  "$ehr/deploy/definitions/professional/openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls" \
  22e0004006afb36e903841afecaa2541c7624f3f1d6e03dfeb5c9354ee370c7f "$licence" \
  'The professional PERSON, loaded first.'
pin commit openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls 'FerroHEALTH/FerroEHR tag v5.7.0' \
  "$ehr/deploy/definitions/professional/openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls" \
  a9cb003e2238dd4ba8d69b5184fc917102b538b198344d9507f303215b9a3fec "$licence" \
  'The professional ROLE, loaded second.'
pin commit LICENSE-CC-BY-SA-3.0 'FerroHEALTH/FerroEHR tag v5.7.0' \
  "$ehr/LICENSE-CC-BY-SA-3.0" \
  3f941b3b89cf7b8370ceb83cc76d2120d471b58735d8ca60238a751a48d7f72f 'Creative Commons Attribution-ShareAlike 3.0 Unported (the licence text)' \
  'The licence the two definitions carry.'
pinned_corpus ferroehr-definitions "FerroEHR professional definitions" \
  "FerroEHR's professional PERSON and ROLE definitions" \
  "FerroEHR's professional PERSON and ROLE definitions,
\`openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0\` and
\`openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0\`: the test and
quickstart pair a FerroEHR node of release 5.7.0 or later loads with
\`ferroehr definitions load\` before \`serve\`, so that its party mapping
(\`[authz.party_binding.person]\` and \`[authz.party_binding.professional_role]\`)
finds its templates at boot and every person who signs in binds to a
\`PERSON\`. FerroEHR wrote them by hand as adaptations of openEHR CKM
archetypes and states that they carry the Creative Commons
Attribution-ShareAlike 3.0 licence and the copyright line of their CKM
sources (© openEHR Foundation); they are committed verbatim with that
attribution and the licence text, as CC BY-SA permits." "" "$read_by"

say "done"
