#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/eu.sh
#
# Vendors the European Union sources of the #488 country research and the
# EHDS readiness work, one corpus each: the European Health Data Space
# Regulation, its adopted implementing acts, the exchange format
# Recommendation, the eHealth Network guidelines and the Commission
# Decision on the reuse of Commission documents (docs/specs/eu-ehds/),
# and the MyHealth@EU NCPeH API implementation guide with the OpenNCP
# reference source (docs/specs/ehdsi/).
#
# Each artefact is pinned by URL and sha256 (scripts/vendor/lib/pinned.sh),
# and each corpus row of docs/VERSIONS.md carries its pin-set digest.
# OpenNCP is evidence only and is fetched into the git-ignored
# .vendor-cache/; no line of its code enters this repository. The eHDSI
# wiki sits behind EU Login and is recorded for manual retrieval.
#
# Usage:
#   scripts/vendor/eu.sh
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

ec_notice='A Commission document, reusable under Commission Decision 2011/833/EU (dec-eu-2011-833-en.xhtml, here) and CC BY 4.0 under the European Commission legal notice (https://commission.europa.eu/legal-notice_en): "Unless otherwise indicated (e.g. in individual copyright notices), content owned by the EU on this website is licensed under the Creative Commons Attribution 4.0 International (CC BY 4.0) licence". The PDF states no licence of its own'
pin commit reg-eu-2025-327-en.xhtml "OJ L, 2025/327, 5.3.2025 (CELEX 32025R0327), English" \
  https://publications.europa.eu/resource/cellar/531b8c37-f962-11ef-b7db-01aa75ed71a1.0006.03/DOC_1 \
  bf331ac48264118fb0461f793123a2726da88195a11d4bccb2075fd6326909da \
  'An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents' \
  'Pinned at the Publications Office (Cellar) manifestation, the XHTML EUR-Lex serves at https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=OJ:L_202500327. The EUR-Lex copy adds a monitoring script tag with per-request ids, so its sha256 differs on every fetch.'
eu_act='An official EU legal act published in the Official Journal; reuse under Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents'
pin commit impl-reg-eu-2026-2083-en.xhtml "OJ L, 2026/2083, 21.9.2026 (CELEX 32026R2083), English" \
  https://publications.europa.eu/resource/cellar/5a79d49d-b554-11f1-81de-01aa75ed71a1.0006.03/DOC_1 \
  51340e65db3e85bc892d723a630663172ce89001f376dea0b870d74ab2231c58 "$eu_act" \
  'Commission Implementing Regulation (EU) 2026/2083 of 18 September 2026 on MyHealth@EU, adopted under Article 23(4) and (8) of Regulation (EU) 2025/327; applies from 26 March 2027 (its Article 19). Pinned at the Cellar manifestation the CELEX resource resolves to.'
pin commit impl-reg-eu-2026-2099-en.xhtml "OJ L, 2026/2099, 22.9.2026 (CELEX 32026R2099), English" \
  https://publications.europa.eu/resource/cellar/dff777d4-b61d-11f1-81de-01aa75ed71a1.0006.03/DOC_1 \
  6389d047750ed8fca879d53ebeda22af444d1f1619ed3bf6f13560097aa887d0 "$eu_act" \
  'Commission Implementing Regulation (EU) 2026/2099 of 21 September 2026 on the cross-border identification and authentication mechanism, adopted under Article 16(2) of Regulation (EU) 2025/327; applies from 26 March 2027, its Article 3(3) and Article 5(2) from 26 March 2029 (its Article 9). Pinned at the Cellar manifestation the CELEX resource resolves to.'
pin commit rec-eu-2019-243-en.xhtml "OJ L 39, 11.2.2019, p. 18 (CELEX 32019H0243), English" \
  https://publications.europa.eu/resource/cellar/cf529e8a-2dcb-11e9-8d04-01aa75ed71a1.0006.03/DOC_1 \
  d6c8d817271b376e836744d81c2ddfe5d40d8a9f5f0b24eb100c1bc507183480 "$eu_act" \
  'Commission Recommendation (EU) 2019/243 of 6 February 2019 on a European Electronic Health Record exchange format, which recital 26 of Regulation (EU) 2025/327 names as the foundation of the exchange format. Pinned at the Cellar manifestation the CELEX resource resolves to.'
pin commit ehn-guidelines-patientsummary.pdf "Release 3.4, November 2024" \
  'https://health.ec.europa.eu/document/download/e020f311-c35b-45ae-ba3d-03212b57fa65_en?filename=ehn_guidelines_patientsummary_en.pdf' \
  9daaab30ef8e8cb5f8ab2be1480d80869b17702d118ab3d13268324184267f5d "$ec_notice"
pin commit ehn-guidelines-eprescription.pdf "Release 3.1, November 2024" \
  'https://health.ec.europa.eu/document/download/b744f30b-a05e-4b9c-9630-ad96ebd0b2f0_en?filename=ehn_guidelines_eprescriptions_en.pdf' \
  e0dcb7671e3f5a92707e3bc9f67c38304addb20952da60fea0733c35ae80bbca "$ec_notice" \
  'The eHealth Network guideline on ePrescription and eDispensation, Release 3.1, adopted in Budapest in November 2024, as its title page states.'
pin commit dec-eu-2011-833-en.xhtml "OJ L 330, 14.12.2011, p. 39 (CELEX 32011D0833), English" \
  https://publications.europa.eu/resource/cellar/cb76d4a0-c886-40bd-99d7-8db018a723d0.0010.03/DOC_1 \
  2d5bc877b9a5aad948af21c680aca1d3409f41df5dd0dcc57ef9b1b225f60982 "$eu_act" \
  'Commission Decision 2011/833/EU of 12 December 2011 on the reuse of Commission documents, the reuse terms of the acts and the guidelines here (its Articles 3 and 6), kept as their licence evidence. It replaces the Commission legal notice page, whose bytes changed with every render. Pinned at the Cellar manifestation the CELEX resource resolves to.'
pin manual ehdsi-interoperability-specifications "not retrieved" \
  'https://webgate.ec.europa.eu/fpfis/wikis/display/EHDSI/2.+eHDSI+INTEROPERABILITY+SPECIFICATIONS%2C+Requirements+and+Frameworks' \
  - 'Unknown: the page could not be read' \
  'Behind EU Login; the normative eHDSI XCPD, XCA and XUA profiles live here.'
pinned_corpus eu-ehds "EU EHDS Regulation and eHealth Network guidelines" \
  "the European Health Data Space Regulation, its adopted implementing acts and eHealth Network guidelines" \
  "Regulation (EU) 2025/327 on the European Health Data Space (the research
cites Articles 8 to 12, 14 to 16, 23 and 105), the two implementing acts
adopted under it that reach the cross-border exchange (Commission
Implementing Regulation (EU) 2026/2083 on MyHealth@EU and (EU) 2026/2099 on
cross-border identification and authentication), Commission Recommendation
(EU) 2019/243 on a European Electronic Health Record exchange format, the
eHealth Network guidelines on the Patient Summary and on ePrescription and
eDispensation, and Commission Decision 2011/833/EU of 12 December 2011 on
the reuse of Commission documents, which governs the reuse of all of them.
The Regulation, the implementing acts, the Recommendation and the Decision
are official EU legal acts published in the Official Journal. The
Commission legal notice (https://commission.europa.eu/legal-notice_en)
licenses the guidelines under CC BY 4.0; it is cited and not pinned, because
the page's bytes change with every render. The eHDSI
interoperability specifications are on a wiki behind EU Login and are
recorded for manual retrieval." "" \
  $'#488 (the country research into identity resolution,\n  localization, consent, addressing and authentication to nodes) and\n  #519 (EHDS readiness)'

pin commit myhealth.eu.fhir.ncp-api-9.1.0.tgz "9.1.0 (ci-build, draft, 2026-05-05)" \
  https://fhir.ehdsi.eu/ncp-api/package.tgz \
  a9284da494a0402758d912791cac8aa17270bfb27983e9e21e1be23b061817ad \
  'CC0-1.0: the license of the package manifest and of the ImplementationGuide resource' \
  'Not on packages.fhir.org; the URL serves the current build (package.json says notForPublication: true), so a new build fails the pin loudly. Source: https://code.europa.eu/ehdsi/ehdsi-fhir-ig (ncp-api/).'
pin cache fhir-ehdsi-index.html "as served on 2026-10-04" \
  https://fhir.ehdsi.eu/ \
  bc56387f605073238333db88a455d6db028d6bc36cba2625e83e1ae001cb2f0d \
  'No licence statement on the page' \
  'The index of the MyHealth@EU FHIR implementation guides; a live page.'
pin commit ehdsi-ncp-api-sequence-pat.html "9.1.0 build" \
  https://fhir.ehdsi.eu/ncp-api/sequence-pat.html \
  f7102d0bdaadf3a1b0e7dc98ace7879775e91f9c3282bbc9ccb014dac65cce00 \
  'CC0-1.0: a page of the implementation guide whose ImplementationGuide resource declares license CC0-1.0' \
  'The rendered Patient Identification page of the current build; a new build fails the pin loudly.'
pin cache openncp-v10.1.0.tar.gz "tag v10.1.0, commit bac7cc3ef88fe5dee01287c2dfc01fc00d5c945f (2026-09-29)" \
  https://code.europa.eu/ehdsi/ehealth/-/archive/v10.1.0/ehealth-v10.1.0.tar.gz \
  945dcfc7da3f9aaaa4a0aa57be7ee919be0c3f33a1796fffcffcd24f04fae499 \
  'Apache-2.0: the LICENSE and NOTICE files in the archive' \
  'Evidence of the MyHealth@EU transport stack (XCPD, XCA, XDR, SAML assertions, SMP discovery), never an oracle and never a source of code, so it is kept out of the repository by decision.'
pinned_corpus ehdsi "MyHealth@EU NCPeH API and OpenNCP" \
  "the MyHealth@EU NCPeH API implementation guide and OpenNCP" \
  "The FHIR package of the NCPeH API implementation guide and its Patient
Identification page, both CC0-1.0, and the OpenNCP v10.1.0 source, the
reference NCPeH, pinned by tag and commit and kept in the cache as
evidence only."

say "done"
