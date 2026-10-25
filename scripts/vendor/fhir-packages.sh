#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/fhir-packages.sh
#
# Fetches the FHIR packages the dependency closure of the HL7 Europe Patient
# Summary needs beside the packages scripts/vendor/eehrxf.sh commits: the
# FHIR R4 core package, HL7 Terminology, the HL7 extension packages, the
# HL7 Europe Base and Core version the Patient Summary names, the R5 to R4
# cross-version extensions, the International Patient Access and SMART App
# Launch, with International Patient Summary 1.1.0 and the DICOM code
# systems, which the MyHealth@EU value sets name. FerroBRIDGE's profile
# validator refuses a package set that lacks a package a manifest names,
# so a received document is validated against the whole closure (#808). Each package is the FHIR package registry's
# tarball at the exact version a dependency names, pinned by URL and sha256
# (scripts/vendor/lib/pinned.sh). They come to 58 MB, so each is kept in
# the git-ignored cache and never committed; PROVENANCE.md under
# docs/specs/fhir-packages/ records them, and the testkit
# (ferrofed_testkit::fhir_packages) fetches a missing one from the same
# pin. A second corpus, docs/specs/fhir-packages-myhealth-eu/, pins the
# packages the MyHealth@EU guides and the HL7 Europe re-pins name beside
# that closure (#1306). scripts/checks/pin-freshness.sh lists this script as unwatched: each
# pin is the version a vendored manifest names, so a newer release never
# applies.
#
# Usage:
#   scripts/vendor/fhir-packages.sh
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

registry=https://packages.fhir.org
cc0='CC0-1.0: the license of the package manifest'
cached='The registry tarball as served, kept in the cache for its size.'
read_by=$'#808 (a received document validated in full by FerroBRIDGE\x27s\n  profile validator), the testkit and the receive tests'

pin cache hl7.fhir.r4.core-4.0.1.tgz "4.0.1" "$registry/hl7.fhir.r4.core/4.0.1" \
  ebd7731df7d36b5b7d39d5fb6c9d77b44bb7fe5742f1a2e87f164738c3289d44 "$cc0" \
  "$cached Every package of the closure depends on it."
pin cache hl7.fhir.eu.base-2.0.0.tgz "2.0.0 (FHIR 4.0.1)" "$registry/hl7.fhir.eu.base/2.0.0" \
  b2858677ae11245bbc768ced2e205c5354ebf71ae8d7a7a0f2915c3eb6124999 "$cc0" \
  "$cached The version hl7.fhir.eu.eps 1.0.0-ballot and every MyHealth@EU guide of scripts/vendor/myhealth-eu.sh depend on; scripts/vendor/eehrxf.sh commits 2.0.1."
pin cache hl7.fhir.uv.extensions.r4-5.2.0.tgz "5.2.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.extensions.r4/5.2.0" \
  b406e75575f05676559d0759770c5939d023ee72fb2ef38e0b3259328487720a "$cc0" "$cached"
pin cache hl7.fhir.uv.extensions.r4-5.3.0.tgz "5.3.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.extensions.r4/5.3.0" \
  dfbc3ac95df91ed845cc6b60920d2875f679361fa0e16f227cee93c4a9ab2104 "$cc0" "$cached"
pin cache hl7.fhir.uv.xver-r5.r4-0.1.0.tgz "0.1.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.xver-r5.r4/0.1.0" \
  7ee6f04d78ced803dd567559a0d178bafbd2d3b71db61bbbf6b15c796a1a664d "$cc0" "$cached"
pin cache hl7.terminology.r4-6.2.0.tgz "6.2.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/6.2.0" \
  79404c9cc95491fc0155627cd039c401a6eb4748175328131e91b709a41300e2 "$cc0" "$cached"
pin cache hl7.terminology.r4-6.3.0.tgz "6.3.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/6.3.0" \
  5e080e5343406f964022b086da713df374e410245c518166c5b1b93a800cdd71 "$cc0" "$cached"
pin cache hl7.terminology.r4-6.5.0.tgz "6.5.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/6.5.0" \
  a28b638483a11df696ed92198276236d759e41c7a3a8960c9e7e7d0a1185bd06 "$cc0" "$cached"
pin cache hl7.terminology.r4-7.1.0.tgz "7.1.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/7.1.0" \
  1cb0cd5601972925fcd04f2c175d9cc63a9ecd9346a91a6e735f1a865dc5fba1 "$cc0" "$cached"
pin cache hl7.terminology.r4-7.3.0.tgz "7.3.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/7.3.0" \
  1a1ef2aa22ecc820341267f2bdba3b2d1f4adafb9bdddfc9e51c611cd64f3b54 "$cc0" "$cached"
pin cache hl7.fhir.uv.ipa-1.1.0.tgz "1.1.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.ipa/1.1.0" \
  013b9f91683c496fc09eed93a8798b47dcbfb775587890fa3e11253a2409feef "$cc0" "$cached"
pin cache hl7.fhir.uv.smart-app-launch-2.0.0.tgz "2.0.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.smart-app-launch/2.0.0" \
  627e7a64e7abe130201fbae2ee7d2cb85d7d94972458d302c4d06d59e2c7d084 \
  'CC-BY-4.0: the license of the package manifest' "$cached"
pin cache hl7.fhir.uv.ips-1.1.0.tgz "1.1.0 (FHIR 4.0.1)" "$registry/hl7.fhir.uv.ips/1.1.0" \
  403c4141101810e924f2928287985084819d8a5cc3a62e2b3840a557129840ef "$cc0" \
  "$cached The version myhealth.eu.fhir.mvc-package 9.1.0 depends on; scripts/vendor/eehrxf.sh commits 2.0.0."
pin cache hl7.terminology.r4-5.0.0.tgz "5.0.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/5.0.0" \
  c863bef14eddb01d2884d8f14826b9b152f9fcef6b99e107025136af69ce2c18 "$cc0" "$cached"
pin cache fhir.dicom-2022.4.20221006.tgz "2022.4.20221006 (FHIR 4.0.1)" "$registry/fhir.dicom/2022.4.20221006" \
  117a6137c7f08220a92aadaec23f10e603730fc13b583d46011a777a1f23e7ed \
  'free: the license of the package manifest' "$cached The DICOM code systems hl7.fhir.uv.ips 1.1.0 depends on."
pinned_corpus fhir-packages "FHIR package closure of the Patient Summary" \
  "the FHIR packages of the HL7 Europe Patient Summary's dependency closure" \
  "The FHIR packages the manifests of the committed HL7 Europe packages name
and this repository does not commit, each the tarball of
\`packages.fhir.org\` at the exact version a manifest names. Every one
declares CC0-1.0 in its manifest except SMART App Launch, which declares
CC-BY-4.0, and the DICOM code systems, which declare \`free\`; they are kept
in the cache only for their size (58 MB)." "" "$read_by"

# The packages the manifests of the MyHealth@EU guides and of the HL7 Europe
# re-pins name that neither corpus above nor a committed one holds (#1306).
# A corpus of their own, so the Patient Summary's set stays the closure the
# receive tests validate against.
read_by_myhealth=$'#1306 (the dependency closure of the MyHealth@EU guides of\n  #1290), and the testkit\x27s myhealth_eu tests'
pin cache hl7.terminology.r4-7.2.0.tgz "7.2.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/7.2.0" \
  f8c33fd30f8daf78f7496540a841e3fd20585279c0e785fb97394e44d6f24fee "$cc0" \
  "$cached The version the MyHealth@EU Patient Summary, ePrescription and eDispensation and Vaccination guides depend on."
pin cache hl7.terminology.r4-7.4.0.tgz "7.4.0 (FHIR 4.0.1)" "$registry/hl7.terminology.r4/7.4.0" \
  1dc558c204f5ee1f2b96ab9a8cd83d0811475b08a4e06a7c2bcea55d1ab116ed "$cc0" \
  "$cached The version hl7.fhir.eu.laboratory 2.1.0 and hl7.fhir.eu.hdr 1.0.0 depend on."
pin cache hl7.fhir.uv.xver-r5.r4-0.0.1-snapshot-2.tgz "0.0.1-snapshot-2 (FHIR 4.0.1)" \
  "$registry/hl7.fhir.uv.xver-r5.r4/0.0.1-snapshot-2" \
  e28ec496b04c9fa681b4c4971afb4fefc56f4dc45f2680c2935b589656731946 "$cc0" \
  "$cached The version the MyHealth@EU NCPeH API and Medical Imaging Report guides depend on."
pinned_corpus fhir-packages-myhealth-eu "FHIR package closure of the MyHealth@EU guides" \
  "the FHIR packages the MyHealth@EU guides and the HL7 Europe re-pins name beside the Patient Summary's closure" \
  "The FHIR packages the manifests of the MyHealth@EU guides, of HL7 Europe
Laboratory Report 2.1.0 and of Hospital Discharge Report 1.0.0 name that
neither the Patient Summary's closure nor a committed package holds, each
the tarball of \`packages.fhir.org\` at the exact version a manifest names.
Every one declares CC0-1.0 in its manifest; they are kept in the cache only
for their size (19 MB)." "" "$read_by_myhealth"

say "done"
