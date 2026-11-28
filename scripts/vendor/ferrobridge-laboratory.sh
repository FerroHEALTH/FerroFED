#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ferrobridge-laboratory.sh
#
# Vendors FerroBRIDGE's laboratory report operational template,
# `ferrobridge_laborbericht.v1.0.0`, the template the laboratory report
# mapping `eehrxf` ships (its `mappings::LABORATORY` project) maps over, with
# the Apache License 2.0 text beside it. The receive tests of a laboratory
# report write the mapping against this template, as a deployment does with
# its own.
#
# The template is the copy the eehrxf crate ships, taken from
# FerroHEALTH/FerroBRIDGE at commit 33ca7e1, pinned by URL and sha256
# (scripts/vendor/lib/pinned.sh) and committed verbatim. It is a modified
# copy of the published KDS_Laborbericht operational template, which adds the
# Comment of the report composition, and keeps that template's Apache License
# 2.0. scripts/checks/pin-freshness.sh lists this script as unwatched: no file
# has a release feed.
#
# Usage:
#   scripts/vendor/ferrobridge-laboratory.sh
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

bridge=https://raw.githubusercontent.com/FerroHEALTH/FerroBRIDGE/33ca7e19dc55af6114988e89ed58a79a09f16b6a/crates/eehrxf/mappings/laboratory
licence='Apache-2.0: a modified copy of the KDS_Laborbericht operational template openFHIR/openfhir publishes under that licence (the PROVENANCE.md beside it in FerroBRIDGE)'
read_by=$'#1404 (the receive tests of a laboratory report,\n  tools/ferrofed-testkit/src/laboratory.rs)'

pin commit ferrobridge_laborbericht.opt 'FerroHEALTH/FerroBRIDGE commit 33ca7e1 (eehrxf 0.52.0), template ferrobridge_laborbericht.v1.0.0' \
  "$bridge/ferrobridge_laborbericht.opt" \
  aa45515ae9962e71316b37acbd1ac36b9b310a9b291f58035c5dc2ae65ad3f6b "$licence" \
  'The operational template the eehrxf crate ships.'
pin commit LICENSE-Apache-2.0 'FerroHEALTH/FerroBRIDGE commit 33ca7e1' \
  "$bridge/LICENSE-Apache-2.0" \
  c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4 "$licence" \
  'The Apache License 2.0 text shipped beside the template.'
pinned_corpus ferrobridge-laboratory "FerroBRIDGE laboratory report template" \
  "FerroBRIDGE's laboratory report template" \
  "FerroBRIDGE's laboratory report operational template,
\`ferrobridge_laborbericht.v1.0.0\`: the published \`KDS_Laborbericht\`
operational template with the Comment (\`at0007\`) of
\`openEHR-EHR-COMPOSITION.report-result.v1\` revision 1.1.0 kept in the
composition's \`other_context\`, where a received report names the elements
its mapping does not carry. The HL7 Europe Laboratory Report project the
\`eehrxf\` crate ships (\`mappings::LABORATORY\`) maps over this template, and
the crate ships the same file. It is a modified copy of the template the
openFHIR repository <https://github.com/openFHIR/openfhir> publishes, under
that repository's Apache License 2.0, whose text is committed beside it. It
is read by the test suites alone." "" "$read_by"

say "done"
