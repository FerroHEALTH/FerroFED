#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/sogis-acm.sh
#
# Pins the SOG-IS Crypto Evaluation Scheme Agreed Cryptographic Mechanisms
# (ACM) 1.3, the document whose "Legacy valid beyond 2025" level eHDSI
# Cryptographic Algorithms 9.0.0 §2 and the NCPeH Components Specifications
# 10.0.0 §5.1 set as the floor of every MyHealth@EU key, hash and cipher
# suite (#1624, #1620).
#
# Cryptographic Algorithms 9.0.0 §5.1 names its reference "Version 1.4.
# January 2020" beside a URL that ends in `-1.3.pdf`. No 1.4 exists: the
# SOG-IS supporting documents page lists 1.0, 1.1, 1.2 (January 2020) and
# 1.3 (February 2023), and the document's own change history reads "Update
# with SOGIS specifications v1.3", so the pin is 1.3, the file that URL
# serves.
#
# Redistribution: neither the document nor the SOG-IS page states a
# copyright, licence, terms of use or reuse permission, so the file is
# pinned `withheld` (scripts/vendor/lib/pinned.sh): its URL and sha256 are
# recorded, nothing is copied into the repository or the cache, and the
# script writes only docs/specs/sogis-acm/PROVENANCE.md.
#
# Usage:
#   scripts/vendor/sogis-acm.sh           write docs/specs/sogis-acm/PROVENANCE.md
#   scripts/vendor/sogis-acm.sh --check   check the committed tree
#
# Requires: shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

case "${1:-}" in
  --check) export PINNED_CHECK=1 ;;
  '') ;;
  *)
    echo "usage: $0 [--check]" >&2
    exit 2
    ;;
esac
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"
# shellcheck source=scripts/vendor/lib/pinned.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/pinned.sh"

corpus_require shasum awk
pinned_begin

pin withheld SOGIS-Agreed-Cryptographic-Mechanisms-1.3.pdf \
  'SOG-IS Crypto Evaluation Scheme, Agreed Cryptographic Mechanisms, Version 1.3, February 2023 (SOG-IS Crypto Working Group)' \
  'https://www.sogis.eu/documents/cc/crypto/SOGIS-Agreed-Cryptographic-Mechanisms-1.3.pdf' \
  5560ab5c86de3e62d63304c433691f5ad01116b88aceaea52a8e4b8887e0afb3 \
  'None stated: the document carries no copyright notice, licence or terms of use, and the SOG-IS supporting documents page (https://www.sogis.eu/uk/supporting_doc_en.html, read 2026-11-05) states no copyright, licence, terms of use or reuse permission' \
  'Fetched from the URL on 2026-11-05 (382787 bytes, created 2023-02-16 by pdfTeX) and read for #1624. eHDSI Cryptographic Algorithms 9.0.0 §5.1 names this URL under the label "Version 1.4. January 2020", an editorial error: no 1.4 exists. The floor eHDSI reads from it is the "Legacy valid beyond 2025" level: RSA moduli of at least 3000 bits (§4.1 and Note 27), SHA-256 and above, PSS and PKCS#1 v1.5 signatures (§5.2), and no TLS 1.2 CBC suite.'
pinned_corpus sogis-acm "SOG-IS Agreed Cryptographic Mechanisms" \
  "the SOG-IS Agreed Cryptographic Mechanisms" \
  "The SOG-IS Crypto Evaluation Scheme Agreed Cryptographic Mechanisms 1.3
(February 2023), published by the SOG-IS Crypto Working Group at
<https://www.sogis.eu/uk/supporting_doc_en.html>. eHDSI Cryptographic
Algorithms 9.0.0 §2 and the NCPeH Components Specifications 10.0.0 §5.1
(both in docs/specs/ehdsi-specifications/) set its \"Legacy valid beyond
2025\" level as the floor of the MyHealth@EU keys, hashes and cipher
suites.

Neither the document nor the SOG-IS page states a copyright, licence,
terms of use or reuse permission, so the document is recorded here by
URL, version and sha256, and no copy of it enters the repository or the
cache. Its content is cited by section only." none \
  $'#1624 (the SOG-IS key sizes behind the eHDSI cryptographic floor), read\n  by the cross-border assertion checks of #1620'

say "done"
