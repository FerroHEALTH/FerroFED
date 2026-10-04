#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fr.sh
#
# Vendors the French sources of the #488 country research into
# docs/specs/fr-ans/: the ANS FHIR packages (FR Core, the Annuaire Santé,
# the transport security of Pro Santé Connectée, PDSm), the Annuaire Santé
# API documentation and the PDSm for DMP source, with one page of the
# transport security guide. The INSi and DMP pages answer automated clients
# with a bot challenge and are recorded for manual retrieval.
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and the corpus row of docs/VERSIONS.md carries the pin-set digest.
#
# Usage:
#   scripts/vendor/fr.sh
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

cc0='CC0-1.0: the license of the package manifest'
whole='The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
mit='MIT: the repository licence inside the archive, "MIT License, Copyright (c) 2022 Github de l'"'"'Agence du Numérique en Santé (ANS)"'
archive='A GitHub archive fetched by commit, kept whole so its sha256 stays the pin; GitHub does not promise byte-stable archives, so a re-generated archive fails the pin loudly.'

pin commit hl7.fhir.fr.core-2.2.0.tgz "2.2.0" \
  https://packages.simplifier.net/hl7.fhir.fr.core/2.2.0 \
  085e61bf579829a6d5520c6f388d763d27ba400f9f6ca548bc88da5545ba13f9 "$cc0" "$whole"
pin commit ans.fhir.fr.annuaire-1.1.0.tgz "1.1.0" \
  https://packages.simplifier.net/ans.fhir.fr.annuaire/1.1.0 \
  e6b09d7bc012ae5cfc9a60719ed4fc1eb86d68f04618e529937ba759c69f67d0 "$cc0" "$whole"
pin commit annuaire-sante-fhir-documentation-cdb0e03.tar.gz "commit cdb0e03ca57811d849254c369147bcf6ee426af8 (2026-09-24)" \
  https://github.com/ansforge/annuaire-sante-fhir-documentation/archive/cdb0e03ca57811d849254c369147bcf6ee426af8.tar.gz \
  4976fb2b103df926aa80abbf8aa9f89ecdbe5230399ec101820eeec47bb6b606 "$mit (LICENSE.md)" "$archive"
pin commit ans.fr.securisation-transport-1.2.0.tgz "1.2.0 (2023-12-05)" \
  https://packages.simplifier.net/ans.fr.securisation-transport/1.2.0 \
  f938422c3a304a03ac1a4263c58f1b30a4ce296ddcdf555cc5880070e8708b3a "$cc0" \
  'The package carries the ImplementationGuide resource and no narrative; the narrative is in the rendered guide.'
pin commit securisation-transport-api_prosanteconnectee_web.html "1.2.0" \
  https://interop.esante.gouv.fr/ig/securisation-transport/api_prosanteconnectee_web.html \
  3e66d869026d94562a3391386a9874251489334f0dde0d5832a31ffc53ba9759 \
  'CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0' \
  'Rendered HTML page whose bytes may change on a re-render; re-fetched on 2026-10-04 with the same sha256.'
pin commit ans.fhir.fr.pdsm-3.1.1.tgz "3.1.1" \
  https://packages.simplifier.net/ans.fhir.fr.pdsm/3.1.1 \
  33943c9842d3d7869a72028aa2356e7b3d0def52d03982110f4a1ddfdaefb416 "$cc0" "$whole"
pin commit IG-FHIR-PDSM4DMP-EEDS-00d99b5.tar.gz "0.1.0 ci-build, commit 00d99b591ae6b1f2d040a7a215ee253481d9ee18" \
  https://github.com/ansforge/IG-FHIR-PDSM4DMP-EEDS/archive/00d99b591ae6b1f2d040a7a215ee253481d9ee18.tar.gz \
  8de3c4a82be0bb0bef2fb256ebd2e4eb3fbedc0f1237cb140cdbd15023b37c1e "$mit (LICENSE)" "$archive"
pin manual referentiel-ins "not retrieved" \
  https://esante.gouv.fr/produits-et-services/referentiel-ins \
  - 'Unknown: the page could not be read' \
  'The INSi teleservice, the Référentiel INS and the DMP DSFT: esante.gouv.fr and industriels.esante.gouv.fr answer automated clients with an Incapsula challenge.'
pinned_corpus fr-ans "French ANS publications" \
  "the Agence du Numérique en Santé FHIR guides and sources" \
  "FR Core 2.2.0 (the INS-NIR and INS-NIA identifier systems), the Annuaire
Santé guide 1.1.0 and its API documentation, the Pro Santé Connectée transport
security guide 1.2.0 (RFC 8693 token exchange over RFC 8705 mutual TLS) with
its web-flow page, PDSm 3.1.1 and the draft PDSm for DMP in EEDS source. The
packages and the guide page are CC0-1.0 and the two repositories MIT, all
committed whole. The INSi and DMP pages sit behind a bot challenge and are
recorded for manual retrieval."

say "done"
