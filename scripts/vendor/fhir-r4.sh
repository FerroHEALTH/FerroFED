#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhir-r4.sh
#
# Vendors the pages of the FHIR R4 core specification (4.0.1) that govern an
# definition FerroFED publishes: the extensibility page (2.5.0), the defining
# extensions page (2.5.1), the StructureDefinition resource page with its
# invariants (sdf-1 to sdf-23), the CodeSystem resource page (csd-0, csd-1),
# the NamingSystem resource page (nsd-0 to nsd-2), and the licence page that
# states the terms of the others. The exchange format is FHIR R4 4.0.1, the version every HL7
# Europe package scripts/vendor/eehrxf.sh vendors declares.
#
# Each page is pinned by its versioned URL under hl7.org/fhir/R4/ and the
# sha256 of its bytes (scripts/vendor/lib/pinned.sh), and the corpus row of
# docs/VERSIONS.md carries the pin-set digest. The license page states the
# specification is licensed under CC0.
#
# Usage:
#   scripts/vendor/fhir-r4.sh
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

r4=https://hl7.org/fhir/R4
cc0='CC0-1.0: FHIR R4 2.20.2 (license.html) licenses the specification under Creative Commons No Rights Reserved (CC0)'
read_by=$'#1053 (the StructureDefinition of the remaining-dispensations\n  extension, crates/eehrxf/src/medication/) and #1072 (the definitions of the\n  registry\'s FHIR form, app/ferrofed-identity/src/ihe/mcsd/definition/ and\n  app/ferrofed-registry/src/, and of the access log origin system,\n  app/ferrofed-server/src/access/)'

pin commit extensibility.html "4.0.1, 2.5.0 Extensibility" "$r4/extensibility.html" \
  3c1df2fd19b10d5f7f132976dea0691e42addfaafd0bd10295ccf8ec51269eb3 "$cc0"
pin commit defining-extensions.html "4.0.1, 2.5.1 Defining Extensions" "$r4/defining-extensions.html" \
  dfdfa215d56375ced5f69d10ec752a33ad3088f2c4885823f01b8e18e7a21021 "$cc0"
pin commit structuredefinition.html "4.0.1, the StructureDefinition resource" "$r4/structuredefinition.html" \
  35cd9ecf45464d7f0adbc189ddd014e8641b860e4db91d1eebacc971be43c596 "$cc0"
pin commit codesystem.html "4.0.1, the CodeSystem resource" "$r4/codesystem.html" \
  6bc77ed13750d1a1fe65bdf274dfcaf7ce87c07fa23bde57ad2cdb7cdcd6b83b "$cc0"
pin commit namingsystem.html "4.0.1, the NamingSystem resource" "$r4/namingsystem.html" \
  96b3281f6f320aa0c1b74578366296719a358151fbd9552d3a9a1b66294132f9 "$cc0"
pin commit license.html "4.0.1, 2.20 License and Legal Terms" "$r4/license.html" \
  b2aeb02247ca2aa855ed1fce29f62600aeeb8dcc8257ec1e0b40ceaf4fd60a15 "$cc0" \
  "The licence statement of the pages beside it, vendored as their LICENSE."
pinned_corpus fhir-r4 "FHIR R4 core specification pages" \
  "the FHIR R4 core specification pages" \
  "The Extensibility (2.5.0), Defining Extensions (2.5.1), StructureDefinition,
CodeSystem, NamingSystem and License and Legal Terms (2.20) pages of HL7 FHIR Release 4, version
4.0.1, as \`hl7.org/fhir/R4/\` serves them, committed under CC0-1.0, which
the license page states for the specification." "" "$read_by"

say "done"
