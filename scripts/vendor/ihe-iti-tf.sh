#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ihe-iti-tf.sh
#
# Vendors the three IHE ITI Technical Framework Volume 1 chapters the #488
# country research cites into docs/specs/ihe-iti-tf/: XUA (ch. 13), XCA
# (ch. 18) and XCPD (ch. 27), as profiles.ihe.net renders them.
#
# Each page is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh), and
# the corpus row of docs/VERSIONS.md carries the pin-set digest.
#
# Usage:
#   scripts/vendor/ihe-iti-tf.sh
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

ihe='IHE Technical Frameworks General Introduction §9: IHE International grants "to any other user of these documents, an irrevocable, worldwide, perpetual, royalty-free, nontransferable, nonexclusive, non-sublicensable license under its copyrights in any IHE profiles and Technical Framework documents [...] to reproduce and distribute" them (page footer "© 2000 — IHE International")'
rendered='Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.'

pin commit ihe-iti-xua-ch13.html "ITI TF Revision 20.2, page as served on 2026-10-04" \
  https://profiles.ihe.net/ITI/TF/Volume1/ch-13.html \
  e9624b8007480f653103a641a0db3c8a2a3d6efdb44c33396e5fb70a10851108 "$ihe" "$rendered"
pin commit ihe-iti-xca-ch18.html "ITI TF Revision 20.2, page as served on 2026-10-04" \
  https://profiles.ihe.net/ITI/TF/Volume1/ch-18.html \
  2bd1b6569e15923f6ded425be7abe045ee159ec2f874abd9e1afcb40e79ce6d1 "$ihe" "$rendered"
pin commit ihe-iti-xcpd-ch27.html "ITI TF Revision 20.2, page as served on 2026-10-04" \
  https://profiles.ihe.net/ITI/TF/Volume1/ch-27.html \
  fc88613b05a3fb08d40980f36ead00bc38e962ae811378225ac7c016c61966ad "$ihe" "$rendered"
pinned_corpus ihe-iti-tf "IHE ITI Technical Framework Volume 1 pages" \
  "IHE ITI Technical Framework Volume 1, chapters 13, 18 and 27" \
  "The profile overviews of Cross-Enterprise User Assertion (XUA),
Cross-Community Access (XCA) and Cross-Community Patient Discovery (XCPD),
which MyHealth@EU and the Norwegian and Swiss record networks build on. They
are IHE's own text, which the General Introduction §9
(<https://profiles.ihe.net/GeneralIntro/ch-9.html>) licenses for
reproduction and distribution; that licence does not reach base-standard
material, and these Volume 1 chapters reproduce no HL7 table. The Volume 2
transaction text of ITI-55, which reproduces HL7 v3 tables, is not vendored."

say "done"
