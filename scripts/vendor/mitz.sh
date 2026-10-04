#!/usr/bin/env bash
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
# scripts/vendor/mitz.sh
#
# Pins the Mitz documents that define the closed authorization question
# ("gesloten autorisatievraag"), the consent question Annex B §B.6 of the
# Federation Tier specification names, and fetches them into the git-ignored
# docs/specs/mitz/cache/ for local reading:
#
# - the Implementatiehandleiding Open en gesloten autorisatievraag, the wire
#   of the question (SOAP 1.2 with an XACML 3.0 decision query);
# - the PvE AMC Aansluiting Mitz-connector, the attributes a question carries
#   and how a connector handles the answer and its errors;
# - the Implementatiehandleiding Berichtauthenticatie, the optional message
#   token a connector may opt into when it is admitted.
#
# The Mitz feature of crates/nl-generic-functions (#475) is built over them.
#
# VZVZ publishes the documents as attachments of one page of the Mitz
# afsprakenstelsel on its Confluence site, readable without an account, and
# nowhere else: no repository, no package and no WSDL or schema. The
# "Mitz closed authorization question" row of docs/VERSIONS.md pins the page,
# the attachment version and the sha256 of each document; the script fails
# when a download does not match its pin.
#
# Redistribution: none of the documents states a licence or terms of use, and
# the PvE is marked "Classificatie: Vertrouwelijk", so no byte of them is
# committed. The script writes only docs/specs/mitz/PROVENANCE.md into the
# tree; the cache is git-ignored.
#
# Usage:
#   scripts/vendor/mitz.sh
#
# Requires: curl, shasum.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

# shellcheck source=scripts/vendor/lib/corpus.sh
# shellcheck disable=SC1091 # shellcheck is not run with -x; the library is checked on its own
. "$root/scripts/vendor/lib/corpus.sh"

corpus_require curl shasum

dest="docs/specs/mitz"
cache="$dest/cache"
site="https://vzvz.atlassian.net/wiki"

pin="$(corpus_pin_cell "Mitz closed authorization question")"
page="$(corpus_pin_field page "$pin")"
version="$(corpus_pin_field version "$pin")"
version="${version%,}"
[[ "$page" =~ ^[0-9]+$ ]] || die "the pin names no Confluence page id"
[[ "$version" =~ ^[0-9]+$ ]] || die "the pin names no attachment version"

documents=()
while IFS= read -r pair; do
  documents+=("$pair")
done < <(grep -oE '[A-Za-z0-9_.-]+\.pdf sha256 [0-9a-f]{64}' <<< "$pin")
[ "${#documents[@]}" -gt 0 ] || die "the pin names no document"

mkdir -p "$cache"
rows=""
for pair in "${documents[@]}"; do
  name="${pair%% *}"
  want="${pair##* }"
  url="$site/download/attachments/$page/$name?version=$version&api=v2"
  corpus_download "$url" "$cache/$name.part" || die "download of $name failed"
  got="$(corpus_sha256 "$cache/$name.part")"
  if [ "$got" != "$want" ]; then
    rm -f "$cache/$name.part"
    die "$name at attachment version $version has sha256 $got, the pin records $want"
  fi
  mv "$cache/$name.part" "$cache/$name"
  size="$(wc -c < "$cache/$name" | tr -d '[:space:]')"
  say "$name matches its pin ($size bytes)"
  rows="$rows
| \`$name\` | $size | \`$want\` |"
done

fetched="$(corpus_fetched)"

cat > "$dest/PROVENANCE.md" << PROV
<!-- This file describes third-party material that is pinned and not
     redistributed; the documents it names keep their publisher's terms. -->

# Provenance: the Mitz closed authorization question

Pinned by \`scripts/vendor/mitz.sh\`, which fetches the documents into the
git-ignored \`$cache/\` for local reading. No document is committed: change
the pin in docs/VERSIONS.md and re-run the script.

- Publisher: VZVZ Servicecentrum, the Mitz afsprakenstelsel
- Source: the attachments of the Confluence page "Bijlage
  Architectuurdocumenten", page id \`$page\` in space \`MA11\`,
  <$site/spaces/MA11/pages/$page/Bijlage+Architectuurdocumenten>, readable
  without an account
- Pin: attachment version \`$version\` of each document below, and its sha256
- Download URL: \`$site/download/attachments/$page/<document>?version=$version&api=v2\`,
  which redirects to the Atlassian media store
- Fetched: $fetched
- Upstream licence: none stated. The documents carry no licence, copyright
  notice or terms of use, and the page states none; the PvE AMC is marked
  "Classificatie: Vertrouwelijk". Redistribution is therefore not permitted,
  and the repository ships none of the bytes.
- Read by: #475 (the closed authorization question of
  \`crates/nl-generic-functions\` feature \`mitz\`, its harness service in
  \`tools/ferrofed-testkit\`, and the Mitz consent pre-filter of
  \`app/ferrofed-identity\`)

| Document | Bytes | sha256 |
|---|---|---|$rows

## What each document defines

- \`VZVZ_Mitz_Implementatiehandleiding_OpenGesloten_v3.8.2.pdf\` (version
  3.8.2, 27 May 2024, Definitief): §2.3 and §3.2 the closed authorization
  question, a SOAP 1.2 web service under ITI TF-2x Appendix V carrying one
  XACML 3.0 \`XACMLAuthzDecisionQuery\` (or the XACML 2.0 PIP variant), with
  its attributes (§3.2.4.2), request examples (§3.2.4.4, §3.2.4.5), expected
  actions (§3.2.4.6), response attributes and examples (§3.2.5); §3.3 the
  transport (mutual TLS under IHE ATNA, IHE CT); §4 the OIDs; §5 the
  professional identifier; §6 the HTTP headers.
- \`VZVZ_Mitz_PvE_AMC_Aansluiting_Mitz-connector_v3.8.1.ad1.pdf\` (version
  3.8.1.ad1, 2 June 2023): AUS-TR-e0040 and AUS-TR-e0050, the attributes of a
  question and how Mitz answers it; AUS-TR-e0900, error handling;
  AUS-TR-e0250, the optional tokens.
- \`VZVZ_Mitz_Implementatiehandleiding_Berichtauthenticatie_v3.8.1.ad1.pdf\`
  (version 3.8.1.ad1): the message token a connector receives on every
  interface when it opts in at admission (AUS-TR-e0250).

## Examined and not taken

- The FHIR package \`vzvz.fhir.mitz\` 3.8.0 on packages.fhir.org (tarball
  sha256 \`f36b10fb7fbf4aa5f7ae76ddc3a99b9cfcdb0b9beaec1aba0344ac739832f7fe\`):
  the Consent, Subscription and notification profiles of the subscribe,
  notify and migrate interfaces, and their code systems. It defines no
  closed authorization question; its \`package.json\` states no licence and
  its resources name VZVZ as copyright holder.
- GitHub: the \`VZVZ\` organisation publishes no Mitz repository.
- The other attachments of the page (the PvE AUS, AXI and ZNP, the
  subscription and ZNP guides, the generic token guide, the glossary and the
  release notes) define no part of the closed authorization question.
PROV

say "done"
