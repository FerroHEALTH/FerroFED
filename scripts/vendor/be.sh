#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/be.sh
#
# Vendors the Belgian sources of the #488 country research, one corpus each:
# the eHealth platform cookbooks and Swagger documents with its re-use
# conditions (docs/specs/be-ehealth/) and the HL7 Belgium core profiles
# (docs/specs/be-fhir/).
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest. The
# eHealth platform material is fetched into the git-ignored .vendor-cache/
# and never committed: each cookbook allows circulation, while the
# platform's re-use conditions put the content of downloadable documents
# under prior approval, and that conflict is the owner's to settle.
#
# Usage:
#   scripts/vendor/be.sh
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

file='https://www.ehealth.fgov.be/ehealthplatform/file'
reuse='the platform'"'"'s re-use conditions (be-ehealth-reuse-conditions.html) add: "Het hergebruik van multimediamateriaal (foto'"'"' s, afbeeldingen, geluid, video'"'"' s enz.), met inbegrip van de inhoud die in downloadbare documenten staat (brochures, enz.), is altijd onderworpen aan voorafgaande goedkeuring."'
circulate="Cover page: \"All are free to circulate this document with reference to the URL source.\"; $reuse"
distribute="Cover page: \"Anyone is free to distribute this document, referring to the URL source.\"; $reuse"
swagger="No licence statement in the file; the re-use conditions say \"Tenzij anders vermeld, is de informatie op deze website vrij van rechten.\", and $reuse"

pin cache be-metahub-ws-v2-cookbook-3.1.pdf "v3.1 (30/04/2024); the cover reads Version 2.5" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/5a1b31bb3e64290d11cb5fcf308860a90874b07a/metahub-ws-v2-cookbook-3-1-dd-30042024.pdf" \
  ddea37e8dd8252f7b9e08565c798eb2cfd31653e340485a37455b2d69fbcfa91 "$circulate"
pin cache be-consent-ws-rest-cookbook-1.2.pdf "v1.2 (24/03/26)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/53066ecc61605b00ada5c15151e2796557964bc9/consent-ws-rest-cookbook-v1-2.pdf" \
  e6a6b349c056ff5a319faa6abc873d82e0bda360c514d602574d6c9479c9a1db "$circulate"
pin cache be-consent-rest-optout-1.1.pdf "v1.1 (24/03/26)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/ace71cdffadeba27c0cd2eae72389765e85ca854/ehealth-padac-consent-ws-1-1-rest-opt-out.pdf" \
  ad0fc3de0ad9df861e5f02bd21dd63ba6660b8aa03b12b49663519c394b0e614 "$circulate"
pin cache be-consent-swagger-1.0.json "1.0 (18/02/20), info.version 2.1" \
  "$file/ba2c0291ed5c8a6097af4b3370591e3860bb60d9/5d7e3643b4e50b14186995c98b73920a2cdd462e/consent-swagger.json" \
  3d92515a7c4fa81218a518bd0dba0dd6e2739266db96821f443411ae273f95e1 "$swagger"
pin cache be-therlink-ws-cookbook-2.0.pdf "v2.0 (20/08/25)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/b202405bdc5e258934ca8c2065e32ebe4a4708ee/therapeutic-link-ws-cookbook-2-0.pdf" \
  cf2dfb33aeda3c734aa95cd8c016f4169ce6a1a0ffdd86055df232e9112617ab "$circulate"
pin cache be-therlink-swagger-1.0.json "1.0 (18/02/20), info.version v2.1" \
  "$file/ba2c0291ed5c8a6097af4b3370591e3860bb60d9/efe11715572b2d838b73090f04dca3f69d0c4147/therlink-swagger.json" \
  f13d3adc8f8832cee29f1b242bb26be76b565f1a01d532bbe25e26ef9a87108f "$swagger"
pin cache be-cobrha-consultation-cookbook-1.2.pdf "v1.2 (04/07/18)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/bbce02fa6a2722f70e0ca3e79d96c74f68ca3b51/cobrha-consultation-cookbook-v1-2-dd-04072018.pdf" \
  df67885f7c7e0d5dd1e6d108d2a1491207743ff90b02763dde3796c468cd8c93 "$circulate"
pin cache be-cobrha-xsd-1.8.pdf "v1.8 (04/07/18)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/d296462eba0b6e09c316e58625276d489e1194f9/cobrha-xsd-v1-8-dd-04072018.pdf" \
  555f47b843657b5f08d8c623a598c8aa0a13bc290b4a1b27ef9992aa1100dba3 "$circulate"
pin cache be-addressbook-rest-cookbook-1.3.pdf "v1.3 (24/03/26)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/36a8fbb0eec93cd882ed27ba5dda6743ac68b888/ehealth-addressbook-rest-v1-3-cookbook.pdf" \
  47772795ce32e82e8fdd6df3a9d9ce1b01f8d7e9476de68e7016e65f93915083 "$distribute"
pin cache be-sts-ws-trust-cookbook-1.2.pdf "v1.2" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/5f9e569e0c2436a8ed38f8fc47cd5fba93af5667/sts-ws-trust-cookbook-v1-2.pdf" \
  0a13c6eaab1b1f5782cedbb87e09e6a7ceccec510347ecb30289c066ecc15d14 "$distribute"
pin cache be-iam-mobile-integration-1.13.pdf "v1.13 (10/04/2026)" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/9a7dc4d16e58378f81f999fc3a5333448bcdec8b/iam-mobile-integration-tech-specs-v1-13.pdf" \
  3daa324bf4780167b242b5d7152bc820115cbc3eddd0be02917a3ff570004930 "$circulate"
pin cache be-iam-exchange-tech-specs-1.4.pdf "v1.4" \
  "$file/cc73d96153bbd5448a56f19d925d05b1379c7f21/7b7924a2b772b28caab82b0aec53cd7b1f218f29/iam-exchange-technical-specifications-v1-4.pdf" \
  d710af4b3a14b4ee3582b1b02ca1a97a0f79d0d44a9011de73954002e3910e72 "$circulate"
pin cache be-ehealth-reuse-conditions.html "as served on 2026-10-04" \
  https://www.ehealth.fgov.be/ehealthplatform/nl/voorwaarden-voor-het-hergebruik \
  922c71a01095e462c68e67604cc5f33bc47a816edd9dfd35511f1326e4a95955 \
  'The page states the platform'"'"'s re-use conditions, quoted in the rows above; whether they or the cookbooks'"'"' own clauses govern is unsettled' \
  'A live page; its bytes change with every edit.'
pinned_corpus be-ehealth "Belgian eHealth platform documents" \
  "the Belgian eHealth platform cookbooks" \
  "The cookbooks of the Metahub, consent, the opt-out of referencing, the
therapeutic link, CoBRHA, the AddressBook, the I.AM STS and I.AM Connect and
Exchange, with the consent and therapeutic-link Swagger documents and the
platform's re-use conditions. Every cookbook allows circulation or
distribution with a reference to its URL, while the re-use conditions put the
content of downloadable documents under prior approval. Until the owner
settles which governs, every artefact stays in the cache and this directory
holds the provenance alone."

pin commit hl7.fhir.be.core-2.2.0.tgz "2.2.0" \
  https://packages.simplifier.net/hl7.fhir.be.core/2.2.0 \
  07bb34605fa6f4a85b5bcc3c26d8a478a1646848fca6448a927f5475f6972903 \
  'CC0-1.0: the license of the package manifest' \
  'The registry tarball as served, package manifest included; the archive is committed whole so its sha256 stays the pin.'
pinned_corpus be-fhir "Belgian core profiles (HL7 Belgium)" \
  "the HL7 Belgium core profiles hl7.fhir.be.core" \
  "The package with the \`be-ssin\` naming system
(\`https://www.ehealth.fgov.be/standards/fhir/core/NamingSystem/ssin\`,
\`2.16.840.1.113883.3.6777.5.1\`), committed under CC0-1.0."

say "done"
