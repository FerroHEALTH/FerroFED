#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ch.sh
#
# Vendors the Swiss sources of the #488 country research, one corpus each:
# the electronic patient record acts, ordinances and annexes from Fedlex with
# the EGDG draft (docs/specs/ch-fedlex-epr/), the CH EPR FHIR implementation
# guide (docs/specs/ch-epr-fhir/), the FOPH central services interface pack
# (docs/specs/ch-ehs-central-services/). The IHE PIXm, PDQm and IUA
# revisions EPDV-EDI Annex 5 pins moved with the identity crates to
# FerroHEALTH/FerroPIX (#367), which vendors them by its own ch.sh.
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. The
# interface pack states no licence and is fetched into the git-ignored
# .vendor-cache/, never committed.
#
# Usage:
#   scripts/vendor/ch.sh
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

store='https://fedlex.data.admin.ch/filestore/fedlex.data.admin.ch/eli'
act='Official work without copyright protection: Swiss URG Art. 5(1)(a)'
pin commit EPDG-SR816.1-20241001-de.html "SR 816.1, consolidation of 2024-10-01" \
  "$store/cc/2017/203/20241001/de/html/fedlex-data-admin-ch-eli-cc-2017-203-20241001-de-html-2.html" \
  9b6340a346814c4f91fdfecaab85f91fdac2847ab8adf042fad8667514a8abb0 "$act"
pin commit EPDV-SR816.11-20241001-de.html "SR 816.11, consolidation of 2024-10-01" \
  "$store/cc/2017/204/20241001/de/html/fedlex-data-admin-ch-eli-cc-2017-204-20241001-de-html-2.html" \
  096f72e6cb47f6694d6eb9787605c786581bbd9a021de4bb3a500dd032b5f21f "$act"
pin commit EPDV-EDI-SR816.111-20260601-de.html "SR 816.111, consolidation of 2026-06-01 (AS 2026 202)" \
  "$store/cc/2017/205/20260601/de/html/fedlex-data-admin-ch-eli-cc-2017-205-20260601-de-html.html" \
  89ee5fa8a9cea20f817e91c6d451a3f8b8ad459b102850812953fc712636a495 "$act"
pin commit EPDV-EDI-Anhang2-oce-2026-49-de.pdf "Annex 2, AS 2026 202 (in force from 2026-06-01)" \
  "$store/oce/2026/49/de/pdf-a/fedlex-data-admin-ch-eli-oce-2026-49-de-pdf-a.pdf" \
  6cb03ca401f9988c18fb77c90f8de75e648aa67f6619ee56192c4f4f36c7fbf2 "$act" \
  'An annex of SR 816.111 published by reference in the official compilation.'
pin commit EPDV-EDI-Anhang5-Erg1-oce-2026-51-de.pdf "Supplement 1 to Annex 5, edition 9 (30.04.2026)" \
  "$store/oce/2026/51/de/pdf-a/fedlex-data-admin-ch-eli-oce-2026-51-de-pdf-a.pdf" \
  a2efd92d8f6389fb4db50653522512ef480ab3993209a3146f37733147a0eb0c "$act" \
  'An annex of SR 816.111 published by reference in the official compilation.'
pin commit EPDV-EDI-Anhang5-Erg2.1-oce-2026-52-de.pdf "Supplement 2.1 to Annex 5, edition 8 (30.04.2026)" \
  "$store/oce/2026/52/de/pdf-a/fedlex-data-admin-ch-eli-oce-2026-52-de-pdf-a.pdf" \
  a1de19ac30bb96871f3048ed2e2c6625a7be764f5e8447ac60694c08aa3ad062 "$act" \
  'An annex of SR 816.111 published by reference in the official compilation.'
pin commit EPDV-EDI-Anhang5-Erg2.3-oce-2026-53-de.pdf "Supplement 2.3 to Annex 5, edition 8 (30.04.2026)" \
  "$store/oce/2026/53/de/pdf-a/fedlex-data-admin-ch-eli-oce-2026-53-de-pdf-a.pdf" \
  988d77f50e0b414d62a22b10910c7c601be011e2aad91251193fb8c742675d9e "$act" \
  'An annex of SR 816.111 published by reference in the official compilation.'
pin commit EPDV-EDI-Anhang8-oce-2026-54-de.pdf "Annex 8, AS 2026 202 (in force from 2026-06-01)" \
  "$store/oce/2026/54/de/pdf-a/fedlex-data-admin-ch-eli-oce-2026-54-de-pdf-a.pdf" \
  5a864eb01789001e53b2db285f14e910b3d6ac283624335be246db972400f62d "$act" \
  'An annex of SR 816.111 published by reference in the official compilation.'
pin commit EGDG-Entwurf-fga-2025-3399-de.html "draft, published in the Bundesblatt on 2025-11-28 (BBl 2025 3399)" \
  "$store/fga/2025/3399/de/html/fedlex-data-admin-ch-eli-fga-2025-3399-de-html.html" \
  1141f636de549dab579e5a31027c5935956c777e39aaa42de926fe9d15b6a582 \
  'Official text published in the Bundesblatt: Swiss URG Art. 5(1)' \
  'A draft act: its enactment and entry into force were not found on 2026-10-04.'
pinned_corpus ch-fedlex-epr "Swiss EPR legislation (Fedlex)" \
  "the Swiss electronic patient record legislation" \
  "The Federal Act on the Electronic Patient Record (EPDG, SR 816.1), its
ordinance (EPDV, SR 816.11), the FDHA ordinance (EPDV-EDI, SR 816.111) with
Annexes 2 and 8 and Supplements 1, 2.1 and 2.3 to Annex 5, and the draft
Federal Act on the electronic health dossier (EGDG) as the Bundesblatt
publishes it, each as Fedlex serves it. These are official texts without
copyright protection under Art. 5 of the Swiss Urheberrechtsgesetz (URG,
SR 231.1): \"Durch das Urheberrecht nicht geschützt sind: a. Gesetze,
Verordnungen, völkerrechtliche Verträge und andere amtliche Erlasse; [...]
c. Entscheidungen, Protokolle und Berichte von Behörden und öffentlichen
Verwaltungen\"
(<https://fedlex.data.admin.ch/filestore/fedlex.data.admin.ch/eli/cc/1993/1798_1798_1798/20250701/de/html/fedlex-data-admin-ch-eli-cc-1993-1798_1798_1798-20250701-de-html-7.html>)."

pin commit ch.fhir.ig.ch-epr-fhir-5.0.0.tgz "5.0.0 (2025-12-18)" \
  https://packages.fhir.org/ch.fhir.ig.ch-epr-fhir/5.0.0 \
  05290196573f674e4fcc1cf5a7f21360c5ade2ec93e60abf4d8a4d4f31b63385 \
  'CC0-1.0: the license of the package manifest' \
  'The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
pinned_corpus ch-epr-fhir "Swiss CH EPR FHIR package" \
  "the CH EPR FHIR implementation guide" \
  "The package \`ch.fhir.ig.ch-epr-fhir\` 5.0.0 with CH:PIXm, CH:PDQm,
CH:mCSD, CH:IUA, CH:PPQm and CH:ATC, normative through EPDV-EDI Annex 5 from
1 June 2026, committed under CC0-1.0."

pin cache Central-Services_20260601_PROD.zip "20260601 PROD" \
  'https://www.e-health-suisse.ch/payload/api/documents/file/Central-Services_20260601_PROD.zip?prefix=documents' \
  1abcf40206643d0e7dfd85c758ca20c55b4f621c50d551891106ca89f43e10b7 \
  'No licence statement: neither the archive (an interface PDF, two attribute spreadsheets, WSDL.zip and LDIF.zip) nor its download names one'
pinned_corpus ch-ehs-central-services "Swiss EPR central services interface pack" \
  "the EPR central services interface documentation" \
  "The interface pack of the EPR central services (the CPI and HPD
attributes, the WSDL and the LDIF), as eHealth Suisse publishes it. It states
no licence, so it stays in the cache and this directory holds the provenance
alone."

say "done"
