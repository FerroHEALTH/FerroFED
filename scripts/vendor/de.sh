#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/de.sh
#
# Vendors the German sources of the #488 country research, one corpus each:
# the gematik ePA für alle specifications and OpenAPI documents
# (docs/specs/de-gematik-epa/), the VZD FHIR-Directory
# (docs/specs/de-gematik-vzd/), ZETA (docs/specs/de-gematik-zeta/), the
# German base profiles (docs/specs/de-hl7-basisprofil/), ISiK
# (docs/specs/de-gematik-isik/), the MII consent module
# (docs/specs/de-mii-consent/) and SGB V §§290, 339 and 342
# (docs/specs/de-sgb5/).
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. The
# gematik specification pages ("Alle Rechte vorbehalten") and the FHIR
# packages that state no licence are fetched into the git-ignored
# .vendor-cache/ and never committed.
#
# Usage:
#   scripts/vendor/de.sh
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

gematik_reserved='"© 2026 gematik Alle Rechte vorbehalten" (page footer)'
rendered='Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.'
epa_basic='tag ePA-3.1.3, commit 3186ad27a1a865bc3b9ccbe6a4d4c697883903e6'
epa_raw='https://raw.githubusercontent.com/gematik/ePA-Basic/3186ad27a1a865bc3b9ccbe6a4d4c697883903e6'
epa_apache='Apache-2.0: the document'"'"'s info.license is "Apache 2.0" (https://www.apache.org/licenses/LICENSE-2.0), and the repository LICENSE is vendored beside it as ePA-Basic-LICENSE'

pin cache gemILF_PS_ePA_V3.8.1.html "3.8.1 (24.03.2026)" \
  https://gemspec.gematik.de/docs/gemILF/gemILF_PS_ePA/gemILF_PS_ePA_V3.8.1/ \
  c03d39145f08a4eaffade5cd3a0c033d732efc1aba6d27fb62674393849388d1 \
  "$gematik_reserved" "$rendered"
pin cache gemSpec_Aktensystem_ePAfueralle_V1.8.2.html "1.8.2 (14.07.2026)" \
  https://gemspec.gematik.de/docs/gemSpec/gemSpec_Aktensystem_ePAfueralle/gemSpec_Aktensystem_ePAfueralle_V1.8.2/ \
  9443b45f4a8242174002fb3572d2e0ae8c924b60e39f09642ec089f6c5bd04b1 \
  "$gematik_reserved" "$rendered"
pin commit I_Information_Service-ePA-3.1.3.yaml "$epa_basic, API 1.5.1" \
  "$epa_raw/src/openapi/I_Information_Service.yaml" \
  4b140f37b58d3d1819c6aaf2398b949e54217207685a87ae5a7114409dc52373 "$epa_apache"
pin commit I_Entitlement_Management-ePA-3.1.3.yaml "$epa_basic, API 1.8.0" \
  "$epa_raw/src/openapi/I_Entitlement_Management.yaml" \
  8422a18ff5af3a0c812dcc7e560eaeaee644a6ce4db92fed980eaf009b3b7406 "$epa_apache"
pin commit I_Consent_Decision_Management-ePA-3.1.3.yaml "$epa_basic, API 1.7.1" \
  "$epa_raw/src/openapi/I_Consent_Decision_Management.yaml" \
  cc6fba385c2e33b2dfbf434f695d850b785418e53e5dd8464b82a807ebc2ef59 "$epa_apache"
pin commit I_Authorization_Service-ePA-3.1.3.yaml "$epa_basic, API 1.9.1" \
  "$epa_raw/src/openapi/I_Authorization_Service.yaml" \
  82307d87de0a9a662c951663cbd584eff5dae8bea1820b7ff0a2129f5de0ff9b "$epa_apache"
pin commit ePA-Basic-LICENSE "$epa_basic" "$epa_raw/LICENSE" \
  40c467d22ec8924e0388c6936baf74a039f5f1f9be4bcd0d42b2ce3c0619765f \
  'The repository licence: "Apache License Version 2.0, January 2004"'
pinned_corpus de-gematik-epa "German ePA für alle (gematik)" \
  "the gematik ePA für alle specifications and OpenAPI documents" \
  "The ePA für alle record system and the primary-system integration guide,
with the four ePA-Basic OpenAPI documents the research reads: the Information
Service (\`getRecordStatus\`, the \`x-insurantid\` header), Entitlement
Management, Consent Decision Management and the Authorization Service. The
OpenAPI documents and the repository licence are committed under
Apache-2.0. The two specification pages carry gematik's \"Alle Rechte
vorbehalten\" and stay in the cache."

pin cache gemSpec_VZD_FHIR_Directory_V1.7.0.html "1.7.0 (17.02.2026)" \
  https://gemspec.gematik.de/docs/gemSpec/gemSpec_VZD_FHIR_Directory/gemSpec_VZD_FHIR_Directory_V1.7.0/ \
  bd8d56eb36eeff9ee65ce5b4a5562e9914a79466dae1b0a4d2c6b53b4f0b54dd \
  "$gematik_reserved" "$rendered"
pin commit de.gematik.fhir.directory-1.3.0.tgz "1.3.0" \
  https://packages.simplifier.net/de.gematik.fhir.directory/1.3.0 \
  c70118a218663f685524c9888bb8ecc3bd4aa05127cee59f13ceadc0898856bb \
  'Apache-2.0, from its source: the package.json has no license field; the package is built from gematik/api-vzd, whose LICENSE is Apache-2.0 (vendored beside it as api-vzd-LICENSE)' \
  'The source link: src/fhir/sushi-config.yaml at api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113 declares canonical https://gematik.de/fhir/directory and version 1.3.0, the canonical and version of this package.'
pin commit EndpointDirectoryConnectionType.fsh "gematik/api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113" \
  https://raw.githubusercontent.com/gematik/api-vzd/6377cd2ad15f6bbd7f5220f92a0a1222e6429113/src/fhir/input/fsh/codesystems/EndpointDirectoryConnectionType.fsh \
  0efa5a159ec606ea7f84d135e6ab851200b768181153dc5db3f344bb6a553694 \
  'Apache-2.0: the repository LICENSE, vendored beside it as api-vzd-LICENSE'
pin commit api-vzd-LICENSE "gematik/api-vzd commit 6377cd2ad15f6bbd7f5220f92a0a1222e6429113" \
  https://raw.githubusercontent.com/gematik/api-vzd/6377cd2ad15f6bbd7f5220f92a0a1222e6429113/LICENSE \
  6a09605aea14e79bdacff887bd42d5cb80ae82dbed2756c29e408589811e3f5b \
  'The repository licence: "Apache License Version 2.0, January 2004"'
pinned_corpus de-gematik-vzd "German VZD FHIR-Directory (gematik)" \
  "the gematik VZD FHIR-Directory" \
  "The national directory of the Telematikinfrastruktur: its specification
page, its FHIR package \`de.gematik.fhir.directory\` 1.3.0 (canonical
\`https://gematik.de/fhir/directory\`, FHIR 4.0.1) and the TI
\`connectionType\` code system. The package and the code system are committed
under the Apache-2.0 licence of their source repository; the specification
page stays in the cache."

pin cache gemSpec_ZETA_V1.3.2.html "1.3.2 (26.06.2026)" \
  https://gemspec.gematik.de/docs/gemSpec/gemSpec_ZETA/gemSpec_ZETA_V1.3.2/ \
  463c1cbda7f832d2bb2705ef19927f2957854a5579463829ce95081429ba2347 \
  "$gematik_reserved" "$rendered"
pinned_corpus de-gematik-zeta "German ZETA (gematik)" \
  "the gematik Zero Trust Access specification" \
  "The ZETA specification: OAuth 2.0 Token Exchange with an SMC-B subject
token, RFC 7523 client authentication and DPoP. It carries gematik's \"Alle
Rechte vorbehalten\" and stays in the cache, so this directory holds the
provenance alone."

pin cache de.basisprofil.r4-1.6.0.tgz "1.6.0" \
  https://packages.simplifier.net/de.basisprofil.r4/1.6.0 \
  e0f4c05f0750afeac24e6283f98dc7711fb4d3f1f352ac1b7c88f65c1a531784 \
  'No licence statement: package.json has no license field, its ImplementationGuide resource has no license or copyright, and the Simplifier package page states none; 53 resources carry copyright "HL7 Deutschland e.V.", 3 "GKV-Spitzenverband" and 1 "Kassenärztliche Bundesvereinigung (KBV)"'
pinned_corpus de-hl7-basisprofil "German base profiles (HL7 Deutschland)" \
  "the German base profiles de.basisprofil.r4" \
  "The package that defines the KVNR identifier systems
(\`http://fhir.de/sid/gkv/kvid-10\`, \`identifier-pkv-kvid-10\`). It states no
licence, so it stays in the cache and this directory holds the provenance
alone."

pin cache de.gematik.isik-6.0.0.tgz "6.0.0" \
  https://packages.simplifier.net/de.gematik.isik/6.0.0 \
  dc4850239599565b67f75a1daf3ee242f288f991d38b8e65c5edeb5299ee9b92 \
  'No licence statement: package.json has no license field (author "gematik GmbH"), the package has no ImplementationGuide resource, no resource states a licence of its own, and the Simplifier package page states none'
pinned_corpus de-gematik-isik "German ISiK (gematik)" \
  "the gematik ISiK package de.gematik.isik" \
  "ISiK 6.0.0, whose \`ISiKPatient\` carries the KVNR. The package states no
licence; the ISiK Basismodul source repository is Apache-2.0, but nothing
links it to this package, so the package stays in the cache and this
directory holds the provenance alone."

pin cache de.medizininformatikinitiative.kerndatensatz.consent-2026.0.0.tgz "2026.0.0" \
  https://packages.simplifier.net/de.medizininformatikinitiative.kerndatensatz.consent/2026.0.0 \
  3402dffbabee2788dd9dd07c16b4077ed2f7743e7464812138a09e587a24733a \
  'No package licence: package.json has no license field and its ImplementationGuide resource has no license or copyright. One resource, the CodeSystem mii-cs-consent-version-modules, states "© 2019+ TMF e. V., Charlottenstraße 42, 10117 Berlin" and "Diese Arbeit ist lizensiert unter der Creative Commons Attribution 4.0 International License", which covers that resource alone'
pinned_corpus de-mii-consent "German MII consent module" \
  "the Medizininformatik-Initiative consent module" \
  "The MII consent profiles, for secondary-use consent outside the
Telematikinfrastruktur; the research mentions them without reading them.
The package states no licence of its own, so it stays in the cache and this
directory holds the provenance alone."

sgb='Official work without copyright protection: German UrhG §5(1)'
pin commit sgb5-290.html "consolidated text as served on 2026-10-04" \
  https://www.gesetze-im-internet.de/sgb_5/__290.html \
  3dfd69b3c4c926169e78c0a1ae8d0daaa37a0a0e2dc7d67a9c5406dd8d9c5ba9 "$sgb" "$rendered"
pin commit sgb5-339.html "consolidated text as served on 2026-10-04" \
  https://www.gesetze-im-internet.de/sgb_5/__339.html \
  c14449da138f409fb3232b3fdd3c4e6b7553df4b9fe961e48b7b27e451a96c03 "$sgb" "$rendered"
pin commit sgb5-342.html "consolidated text as served on 2026-10-04" \
  https://www.gesetze-im-internet.de/sgb_5/__342.html \
  cc42b90d9ef7ed9b352aa2b043c057d448d6b64d82eeadc71ae4c4db9764a198 "$sgb" "$rendered"
pinned_corpus de-sgb5 "German SGB V" \
  "Sozialgesetzbuch V, §§290, 339 and 342" \
  "§290 (the Krankenversichertennummer), §339 (access to the
Telematikinfrastruktur, HBA and SMC-B) and §342 (the ePA opt-out), as
gesetze-im-internet.de serves them. A statute is an official work without
copyright protection under §5(1) of the German Urheberrechtsgesetz:
\"Gesetze, Verordnungen, amtliche Erlasse und Bekanntmachungen sowie
Entscheidungen und amtlich verfaßte Leitsätze zu Entscheidungen genießen
keinen urheberrechtlichen Schutz\"
(<https://www.gesetze-im-internet.de/urhg/__5.html>)."

say "done"
