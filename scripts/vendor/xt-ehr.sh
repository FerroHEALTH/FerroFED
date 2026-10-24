#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/xt-ehr.sh
#
# Vendors the Xt-EHR deliverables that are the closest published text of the
# specifications Regulation (EU) 2025/327 Article 15(1) leaves to an
# implementing act, while the Commission has published no draft of it (Have
# your say initiative 17512 lists the draft act as upcoming, with no
# attachment): D6.1 (patient summary), D6.2 (electronic prescription and
# dispensation) and D5.2 (exchange-format metadata). Each is pinned by URL and
# sha256 (scripts/vendor/lib/pinned.sh). They state no licence, so they are
# kept in the git-ignored cache only and never committed; PROVENANCE.md under
# docs/specs/xt-ehr-deliverables/ records them.
# scripts/checks/pin-freshness.sh lists this script as unwatched: the joint
# action publishes no release feed for its deliverables.
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

xtehr=https://www.xt-ehr.eu/wp-content/uploads/2026/05
no_licence='No licence statement in the document or on the deliverables page'
read_by_xtehr=$'#1265 (the re-check of decisions A67 to A72 for the national\n  contact point against the closest published exchange-format text)'
pin cache Xt-EHR-D6.1.pdf "D6.1, as published on xt-ehr.eu in 2026-05" "$xtehr/D6.1-2.pdf" \
  65da226ed74e549e4e3d3611b9a458382a147f8059bd7903b6e1d2bc6fe0374c "$no_licence" \
  'Patient Summary: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems; its section 5 names the MyHealth@EU Requirements Catalogue 9.0.0 (2025-03-24) as an EC resource available only with authorisation.'
pin cache Xt-EHR-D6.2.pdf "D6.2, as published on xt-ehr.eu in 2026-05" "$xtehr/D6.2-2.pdf" \
  1d2478a4c75e056756bf1ebd0093ec263ac5ea0c594b75a860dd4842358bed7c "$no_licence" \
  'Electronic prescription and electronic dispensation: implementation guides on the EEHRxF, functional and technical requirements and specifications for EHR systems.'
pin cache Xt-EHR-D5.2.pdf "D5.2, as published on xt-ehr.eu in 2026-05" "$xtehr/Xt-EHR-D5.2.pdf" \
  5d2bb693af07646a506f831854166fcd4d6a3ebf4f092b6937d2a9976410333a "$no_licence" \
  'Technical requirements for EEHRxF metadata.'
pinned_corpus xt-ehr-deliverables "Xt-EHR exchange-format deliverables" \
  "the Xt-EHR deliverables on the European electronic health record exchange format" \
  "Three deliverables of the Xt-EHR joint action (grant 101128085), published
at <https://www.xt-ehr.eu/deliverables/>: D6.1 (Patient Summary), D6.2
(electronic prescription and electronic dispensation) and D5.2 (EEHRxF
metadata). They are the closest published text of the specifications
Regulation (EU) 2025/327 Article 15(1) leaves to an implementing act that
the Commission has not adopted, and no draft of that act is published for
feedback; FerroFED reads them as a draft. They carry no licence statement,
so each is fetched into the cache only and this repository carries none of
their content." "" "$read_by_xtehr"

say "done"
