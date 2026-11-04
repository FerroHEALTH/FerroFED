#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/kds-laborbericht.sh
#
# Vendors the KDS_Laborbericht operational template, the template the
# laboratory report mapping `eehrxf` ships (its `mappings::LABORATORY`
# project) compiles against, with the LICENSE of the repository that
# publishes it. The receive tests of a laboratory report write the mapping
# against this template, as a deployment does with its own.
#
# The template is one file of the openFHIR test resources on GitHub, pinned
# by its raw URL at one commit and the sha256 of its bytes
# (scripts/vendor/lib/pinned.sh), and committed verbatim under the Apache
# License 2.0 of that repository: the template itself carries an empty
# copyright detail and no licence detail.
#
# Usage:
#   scripts/vendor/kds-laborbericht.sh
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

raw=https://raw.githubusercontent.com/openFHIR/openfhir/5e4d68007518fcddae1907b333a86f495791aa53
version='openFHIR/openfhir commit 5e4d68007518fcddae1907b333a86f495791aa53'
licence='Apache-2.0: the LICENSE of openFHIR/openfhir at the same commit; the template carries an empty copyright detail and no licence detail'
read_by=$'#1404 (the receive tests of a laboratory report,\n  tools/ferrofed-testkit/src/laboratory.rs)'

pin commit KDS_Laborbericht.opt "$version" \
  "$raw/core/src/test/resources/kds/laborbericht/KDS_Laborbericht.opt" \
  6c00892f282d192c79010e9e6f0ee7bf07cc2c2923ca78a93f3ef05fd301ca97 "$licence" \
  'The operational template, at core/src/test/resources/kds/laborbericht/.'
pin commit LICENSE "$version" "$raw/LICENSE" \
  c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4 "$licence" \
  'The licence of the repository the template is published in.'
pinned_corpus kds-laborbericht "KDS Laborbericht operational template" \
  "the KDS_Laborbericht operational template" \
  "The \`KDS_Laborbericht\` operational template as the openFHIR repository
<https://github.com/openFHIR/openfhir> carries it among its test resources,
with that repository's LICENSE at the same commit. The template carries an
empty copyright detail and no licence detail, so it is used on the Apache
License 2.0 the repository redistributes it under. It is read by the test
suites alone and never shipped." "" "$read_by"

say "done"
