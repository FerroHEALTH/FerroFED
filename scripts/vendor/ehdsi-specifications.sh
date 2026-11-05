#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/ehdsi-specifications.sh
#
# Vendors the eHDSI interoperability specifications the owner retrieved from
# the eHDSI wiki (docs/specs/ehdsi-specifications/): the Interoperability
# Specifications, the transport, security and audit profiles under them, and
# the NCPeH architecture, components and OpenNCP implementation documents.
# The wiki sits behind EU Login, so this script downloads nothing. The owner
# places the files in a local directory, and the script finds each one by
# the sha256 it pins (scripts/vendor/lib/pinned.sh, mode `supplied`) and
# commits it verbatim under a normalised name.
#
# By the owner's decision of 2026-11-05 a document is committed when its
# cover states distribution level PU and that reuse is authorised provided
# the source is acknowledged. The MyHealth@EU Requirements Catalogue carries
# no such statement: it is pinned `withheld`, its sha256 is recorded, and no
# copy enters the repository or the cache (scripts/checks/myhealth-eu.sh
# refuses one).
#
# Usage:
#   scripts/vendor/ehdsi-specifications.sh DIR   commit the files DIR holds
#   scripts/vendor/ehdsi-specifications.sh --check
#                                                check the committed tree
#
# Requires: shasum, awk.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

case "${1:-}" in
  --check) export PINNED_CHECK=1 ;;
  '')
    if [ "${PINNED_DIGESTS_ONLY:-}" != "1" ]; then
      echo "usage: $0 DIR | --check" >&2
      exit 2
    fi
    ;;
  *)
    [ -d "$1" ] || {
      echo "$0: '$1' is not a directory" >&2
      exit 2
    }
    PINNED_SUPPLIED="$(cd "$1" && pwd)"
    export PINNED_SUPPLIED
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

wiki='https://webgate.ec.europa.eu/fpfis/wikis/display/EHDSI/2.+eHDSI+INTEROPERABILITY+SPECIFICATIONS%2C+Requirements+and+Frameworks'
reuse='"Reuse is authorised, provided the source is acknowledged." and "Distribution level*: PU", where "PU = Public" (the cover and control page, read 2026-11-05)'
docx_title='The docx core properties carry a stale title (dc:title "D3.7.2 Section I - Security Policies_v0.8"), so the sha256 identifies the file, not the metadata.'
pdf_title='The PDF document information carries a stale title ("D3.7.2 Section I - Security Policies_v0.8"), so the sha256 identifies the file, not the metadata.'
pdf_none='The PDF document information carries no title; the sha256 identifies the file.'

appendix_6_4='Its appendix 6.4, MyHealth@EU Error Codes, is legacy: its codes "MUST be supported by MyHealth@EU for backward compatibility. New implementations of MyHealth@EU NCP-A MUST NOT issue these errors." The current list it points to, the wiki page Exception Handling in MyHealth@EU (https://webgate.ec.europa.eu/fpfis/wikis/x/HIfdPw), is section 1.25 of the Requirements Catalogue 10.0.0 pinned withheld below (#1624).'

# ack SOURCE_LINE: the acknowledgement the cover names, quoted from its
# source line.
ack() { printf 'Acknowledgement: "Source: %s", the source line of the cover.' "$1"; }

pin supplied ehdsi-interoperability-specifications-8.0.1.docx \
  'eHDSI Interoperability Specifications, document version 8.0.1, 24/02/2025, status Wave 8 Operation Ready' \
  "$wiki" e4b79689176b8547ae0987f065ee0e1dce7734cb111b95c6fda75f33fd6230c1 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2025') The owner supplied eHDSI_Interoperability_Specifications_v8.0.1.docx. $docx_title"
pin supplied ehdsi-audit-trail-profile-10.1.0.docx \
  'Audit Trail Profile, document version 10.1.0, 05/10/2026, status Wave 10 Operation Ready' \
  "$wiki" 8cbc4edbc39071d7fdad09e1d913a83f1585e3a41009ece3b63942b2fba146e3 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_Audit_Trail_Profile_v10.1.0.docx. $docx_title"
pin supplied ehdsi-saml-profile-10.0.0.pdf \
  'SAML Profile, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" edd17e79b49ec56cd3799a58fd392ef51a09aa53c4bc726a05b4252dd7c2501d "$reuse" \
  "$(ack 'DG SANTE, MyHealth@EU, 2026') The owner supplied eHDSI_SAML_Profile_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-section-i-security-policies-10.0.0.pdf \
  'Section I – Security Policies, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 010bed197540bdcd92574a0df4824d21d7ca615fa5a905dea469a7cc70c89167 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_Section_I-Security_Policies_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-section-ii-security-services-10.0.0.pdf \
  'Section II – Security Services, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 0faf5f8895b0b500ea4e3f46e5de21a3c048e4c38939989e73483ab763c7c06d "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_Section_II-Security_Services_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-security-services-specification-10.0.0.pdf \
  'Security Services Specification, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 7d02a876e6ea395316555f9bb176484df5f2723d77748f7c480f33ff1307c416 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_Security_Services_Specification_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-xca-profile-8.0.1.pdf \
  'XCA Profile, document version 8.0.1, 24/02/2025, status Wave 8 Operation Ready' \
  "$wiki" f76eebf01e10d719d015dd4bba943d2eb3af915f8402795479c8a8ed8d21c79e "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2025') The owner supplied eHDSI_XCA_Profile_v8.0.1.pdf. $pdf_none"
pin supplied ehdsi-xcpd-profile-10.0.0.pdf \
  'XCPD Profile, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" b16df79f6812c8501ba54226ebc705644ccc3e23eed4402b1fc5a0863630ea29 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_XCPD_Profile_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-xdr-profile-8.0.0.pdf \
  'XDR Profile, document version 8.0.0, 31/05/2024, status Wave 8 Operation Ready' \
  "$wiki" 1b50478a1185e601d8048f72b2735f8b3d1e5cb5ebacae7b728db21ac9485018 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2024') The owner supplied eHDSI_XDR_Profile_v8.0.0.pdf. $pdf_none"
pin supplied ehdsi-messaging-profile-10.0.0.pdf \
  'eHDSI Messaging Profile, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 4b9aa0c6dd383417eb4c90944bb04b895f6b4b4095b46393957ab9f6e34c7976 "$reuse" \
  "$(ack 'DG SANTE, MyHealth@EU, 2026') The owner supplied eHDSI_Messaging_Profile_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-x509-certificates-profile-7.0.0.pdf \
  'X.509 Certificates Profile (document name X.509 Certificate Profile), document version 7.0.0, 19/07/2023, status Wave 7 Operation Ready' \
  "$wiki" c17ac1cc60ca686a805306856259f92d53a23e15652041474cf2008d20d40548 "$reuse" \
  "$(ack 'DG SANTE, MyHealth@EU, 2023') The owner supplied eHDSI_X509_Certificates_Profile_v7.0.0.pdf. $pdf_none"
pin supplied ehdsi-cryptographic-algorithms-9.0.0.pdf \
  'Cryptographic Algorithms, document version 9.0.0, 04/06/2025, status Wave 9 Operation Ready' \
  "$wiki" 2b2cf3612649f69b2b1e3b68ef0b4924c8d75ce00b02808a4a19fb28f942f952 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2025') The owner supplied eHDSI_Cryptographic_Algorithms_v9.0.0.pdf. $pdf_none"
pin supplied ehdsi-service-location-and-capability-lookup-profile-10.0.0.pdf \
  'Service Location and Capability Lookup Profile, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 0b238368a81c5710a2578e3b4154aa34fec32ecd035b051af72d617b5ae5221a "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_Service_Location_and_Capability_Lookup_Profile_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-ncpeh-architecture-specifications-10.0.0.pdf \
  'NCPeH Architecture Specifications (document name NCPeH Architecture Specification), document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 9b53ee18c1c89be3fa97e4735da42c78455d95da7c3bd4eaeee56c0a3f578627 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_NCPeH_Architecture_Specifications_v10.0.0.pdf. $pdf_title"
pin supplied ehdsi-ncpeh-components-specifications-10.0.0.pdf \
  'NCPeH Components Specifications, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 5da44873fefdf3f9bb2a855d3d6c053990f8730a25a0b5ffe1ccfe75e0e73ba2 "$reuse" \
  "$(ack 'DG SANTE, CEF MyHealth@EU, 2026') The owner supplied eHDSI_NCPeH_Components_Specifications_v10.0.0.pdf. $pdf_title $appendix_6_4"
pin supplied ehdsi-openncp-implementation-10.0.0.pdf \
  'OpenNCP Implementation, document version 10.0.0, 01/06/2026, status Wave 10 Operation Ready' \
  "$wiki" 1dcb77cfa1c7eac20bd922db66fc9a8a20c1f1d7c5d86ac415134aa30b546e04 "$reuse" \
  "$(ack 'DG SANTE, CEF eHealth DSI, 2026') The owner supplied eHDSI_OpenNCP_Implementation_v10.0.0.pdf. $pdf_title"
pin withheld myhealth-eu-requirements-catalogue-10.0.0.pdf \
  'MyHealth@EU Requirements Catalogue, version 10.0.0 OR, target release W10-Operation Ready, document status DONE, status date 06 Mar 2026' \
  'https://webgate.ec.europa.eu/fpfis/wikis/display/EHDSI/1.+MyHealth@EU+Requirements+Catalogue' \
  4743869bbd57564a81b567c86619523f73f2ff0fabfec9a12f78127fd128408a \
  'None stated: the export carries no reuse statement, no distribution level and no licence on its cover or anywhere in its text (read in full 2026-11-05)' \
  'The owner supplied eHDSI Requirements Catalogue v10.0.0 OR.pdf, a 438-page export of the wiki page (10005748 bytes), read under an EU Login reading grant. Xt-EHR D6.1 section 5 says the catalogue is available only with authorisation, requested from DG SANTE, so it stays privileged-access and is not committed; the MyHealth@EU self-assessment cites it by version, section and requirement id only.'
pinned_corpus ehdsi-specifications "eHDSI interoperability specifications" \
  "the eHDSI interoperability specifications supplied by the owner" \
  "The eHDSI Interoperability Specifications 8.0.1 and the twelve profiles
under them that the transport, security and audit work reads (SAML, Audit
Trail, the two Sections and the Security Services Specification, XCA, XCPD,
XDR, Messaging, X.509 Certificates, Cryptographic Algorithms and Service
Location and Capability Lookup), with the NCPeH Architecture
Specifications, the NCPeH Components Specifications and the OpenNCP
Implementation 10.0.0, retrieved by the owner from the eHDSI wiki, which
sits behind EU Login. Each cover states distribution level PU (Public)
and \"Reuse is authorised, provided the source is acknowledged.\", read on
2026-11-05, so by the owner's decision of that day each is committed
verbatim, as the owner supplied it, with the source line of its cover as
the acknowledgement. DG SANTE, the Directorate-General for Health and
Food Safety of the European Commission, publishes them for MyHealth@EU.

The MyHealth@EU Requirements Catalogue 10.0.0 was read and states no reuse
or distribution terms. It is recorded here by version, date and sha256, and
no copy of it enters the repository." none \
  $'#1621 (the eHDSI interoperability specifications), read beside the\n  MyHealth@EU NCPeH API of docs/specs/ehdsi/ by the cross-border\n  transport and audit decisions'

say "done"
