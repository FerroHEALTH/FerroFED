#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fi.sh
#
# Vendors the Finnish sources of the #488 country research, one corpus each:
# the Kanta documents and FHIR packages (docs/specs/fi-kanta/) and the HL7
# Finland base profiles (docs/specs/fi-hl7/).
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. The
# Kanta documents and packages state no licence and are fetched into the
# git-ignored .vendor-cache/, never committed.
#
# Usage:
#   scripts/vendor/fi.sh
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

pdf='No licence statement in the file'
pkg='No licence statement: package.json has no license field, the package has no ImplementationGuide resource, no resource states a licence, and the Simplifier package page states none'

pin cache fi-luovutustenhallinnan-yleiskuvaus-v2.1.pdf "v2.1" \
  https://www.kanta.fi/documents/d/guest/asiakas-ja-potilastietojen-luovutustenhallinnan-yleiskuvaus-v-2-1 \
  c40e5166d350c7d53353b22f666ec282a74fc515ef1d3585ff18e3ead80c2e63 "$pdf"
pin cache fi-tekniset-liittymismallit-20240104.pdf "20240104" \
  'https://www.kanta.fi/documents/20143/106828/KANTA+Tekniset+liittymismallit+20240104.pdf/7b24507a-4c40-a6be-14a8-153e31eefda3?t=1704378407873' \
  ac17a5fb2ea0842f688d803e5e4ee440d1f8f286b124816b3c69b10c78442d1a "$pdf"
pin cache fi-kanta-jwt-v1.4.0.pdf "v1.4.0 (dated 1.10.2025)" \
  https://www.kanta.fi/documents/d/guest/kanta-json-web-token-maarittely-v1-4-0 \
  2175f33b2780cfe6299c2102c478bff02c50dfd305e2b6aee9918747e3da4173 "$pdf"
pin cache fi.kela.kanta.kvp.r4-1.0.0.tgz "1.0.0" \
  https://packages.simplifier.net/fi.kela.kanta.kvp.r4/1.0.0 \
  6f8d48e26629dc4e02d0ee6f8ed16efddfe748e38e066a5d21712c5d0ae7e665 "$pkg"
pin cache fi.kanta.gen.r4-0.9.2.tgz "0.9.2" \
  https://packages.simplifier.net/fi.kanta.gen.r4/0.9.2 \
  b158f183117fc6e571e2a4b882c8b9e2b0666d8bcc29e9445546029cd6cf843a "$pkg"
pinned_corpus fi-kanta "Finnish Kanta documents and packages" \
  "the Kanta services documents and FHIR packages" \
  "The overview of disclosure management (consents and prohibitions), the
Kanta connection models, the Kanta JSON Web Token specification, and the
FHIR packages of the query and relay service and of the common Kanta
definitions. None states a licence, so all stay in the cache and this
directory holds the provenance alone."

pin commit hl7.fhir.fi.base-2.0.0.tgz "2.0.0" \
  https://packages.simplifier.net/hl7.fhir.fi.base/2.0.0 \
  2c5b711bc04087a7d438aef90fd543c95a81bc8f595c28f68ce571563eb5174b \
  'CC0-1.0: the license of the package manifest' \
  'The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
pinned_corpus fi-hl7 "Finnish base profiles (HL7 Finland)" \
  "the HL7 Finland base profiles hl7.fhir.fi.base" \
  "The Finnish base profiles 2.0.0, committed under CC0-1.0."

say "done"
