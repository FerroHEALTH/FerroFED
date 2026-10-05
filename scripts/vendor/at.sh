#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/at.sh
#
# Vendors the Austrian sources of the #488 country research, one corpus
# each: the Gesundheitstelematikgesetz 2012 (docs/specs/at-gtelg/), the ELGA
# Berechtigungssystem developer pages (docs/specs/at-elga-bes/), the ELGA
# overview and the Digital Health Standards Catalogue (docs/specs/at-elga/)
# and the HL7 Austria core profiles (docs/specs/at-hl7-core/).
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. The
# ELGA pages are fetched into the git-ignored .vendor-cache/ and never
# committed; the developer portal refuses a non-browser User-Agent.
#
# Usage:
#   scripts/vendor/at.sh
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

rendered='Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.'

pin commit GTelG2012-20261004.pdf "Fassung vom 04.10.2026" \
  'https://www.ris.bka.gv.at/GeltendeFassung/Bundesnormen/20008120/GTelG%202012%2c%20Fassung%20vom%2004.10.2026.pdf' \
  8973bc92c2ad2b509c212b7c91f9a482b9717deced3f36f4768c666bb0ecd10f \
  'Official work without copyright protection: Austrian UrhG §7(1)' \
  'The RIS renders the consolidated PDF per date, so the URL names the date of this version.'
pinned_corpus at-gtelg "Austrian GTelG 2012" \
  "the Gesundheitstelematikgesetz 2012" \
  "The consolidated GTelG 2012 as the Rechtsinformationssystem des Bundes
renders it for 4 October 2026: §9 (eHVD), §15 (opt-out), §18 (Z-PI), §19
(GDA-Index) and §21 (Berechtigungssystem). A statute is a free work under
§7(1) of the Austrian Urheberrechtsgesetz: \"Gesetze, Verordnungen,
amtliche Erlässe, Bekanntmachungen und Entscheidungen sowie ausschließlich
oder vorwiegend zum amtlichen Gebrauch hergestellte amtliche Werke der im
§ 2 Z 1 oder 3 bezeichneten Art genießen keinen urheberrechtlichen Schutz\"
(<https://www.ris.bka.gv.at/NormDokument.wxe?Abfrage=Bundesnormen&Gesetzesnummer=10001848&Paragraf=7>)."

elga='"Copyright © 2026 ELGA GmbH" (page footer), with the portal'"'"'s AGB at https://developer.elga.gv.at/agb'
bes='https://developer.elga.gv.at/apis/BeS/v5.5'
pin cache elga-bes-v5.5-bes_public_aufbau_des_berechtigungssystems.html "BeS v5.5" \
  "$bes/bes/public/aufbau_des_berechtigungssystems/" \
  9ea022f4d571f7e693e4709994e340c39f3493dea96bdec0a63dbb8f2ba42ba0 "$elga" "$rendered"
pin cache elga-bes-v5.5-bes_public_schnittstellen_des_berechtigungssystems.html "BeS v5.5" \
  "$bes/bes/public/schnittstellen_des_berechtigungssystems/" \
  95c22b15c4ff9d343cf23a0ae67cf29109a4352b994556f9e3c3e9c292407c00 "$elga" "$rendered"
pin cache elga-bes-v5.5-bes_public_saml_assertion_uebersicht.html "BeS v5.5" \
  "$bes/bes/public/saml_assertion_uebersicht/" \
  bec13638636cc289d6570cc9da9a2042e8adb53a476bb330bb0fafdbbdf802f9 "$elga" "$rendered"
pin cache elga-bes-v5.5-bes_public_elga_policies.html "BeS v5.5" \
  "$bes/bes/public/elga_policies/" \
  6680b00182b36a02209bb46c5c53597c7c14d9917954d1b166bd147f80e7aeee "$elga" "$rendered"
pin cache elga-bes-v5.5-ac_public_patientenidentifikation.html "BeS v5.5" \
  "$bes/ac/public/patientenidentifikation/" \
  99bba37c9848ec21557699a887359c09cc7f1ed2db15433bc5f7dba1e8cb2a04 "$elga" "$rendered"
pin cache elga-bes-v5.5-ac_public_kontaktbestaetigungen.html "BeS v5.5" \
  "$bes/ac/public/kontaktbestaetigungen/" \
  d4bc5eb6c75d3aefbfb4d4e8f11a12969d8f6f847771f55d10e62c325c1ca9a9 "$elga" "$rendered"
pinned_corpus at-elga-bes "Austrian ELGA Berechtigungssystem" \
  "the ELGA Berechtigungssystem v5.5 developer pages" \
  "Six pages of the ELGA developer portal on the Berechtigungssystem (ETS,
KBS, PAP, ZGF), its interfaces and SAML assertions, and on patient
identification and contact confirmations for application containers. The
portal reserves its content to ELGA GmbH, so every page stays in the cache
and this directory holds the provenance alone." browser

pin cache elga-technischer-aufbau.html "as served on 2026-10-04" \
  https://www.elga.gv.at/technischer-hintergrund/technischer-aufbau-im-ueberblick/ \
  ccefcf215408b6bb390d4150ca8c8b801df16178dc053872159dd8f7096eaf65 \
  'No licence statement: the page names no licence, and its only copyright line is that of the TYPO3 software it is served with' \
  "$rendered"
pin cache Digital_Health_Standards_Catalogue_Austria_2026.pdf "2026" \
  https://www.elga.gv.at/fileadmin/user_upload/Dokumente_PDF_MP4/CDA/Digital_Health_Standards_Catalogue_Austria_2026.pdf \
  3e1f752ac5145786b35df5ea3dfbed67aa581091bfb7efb0a7f2e7e9cd2a9409 \
  'No licence statement in the file'
pinned_corpus at-elga "Austrian ELGA overview" \
  "the ELGA overview and the Digital Health Standards Catalogue Austria" \
  "The ELGA page on its technical structure (the ELGA-Bereiche as XCA
communities, the Z-PI and the GDA-Index as central services) and the Digital
Health Standards Catalogue Austria 2026 of ELGA GmbH. Neither states a
licence, so both stay in the cache and this directory holds the provenance
alone." browser

pin commit hl7.at.fhir.core.r4-2.0.0.tgz "2.0.0" \
  https://packages.simplifier.net/hl7.at.fhir.core.r4/2.0.0 \
  847d6f859615eeacef19a42d24c61696f62d3363ac02865f55521687fbc2b8b4 \
  'CC0-1.0: the license of the package manifest' \
  'The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
pinned_corpus at-hl7-core "Austrian core profiles (HL7 Austria)" \
  "the HL7 Austria core profiles hl7.at.fhir.core.r4" \
  "The package whose Patient profile slices \`identifier\` into the social
security number (\`1.2.40.0.10.1.4.3.1\`) and the bPK
(\`1.2.40.0.10.2.1.1.149\`), committed under CC0-1.0."

say "done"
