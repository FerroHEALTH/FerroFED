<!-- This file describes third-party material that is pinned and not
     redistributed; the documents it names keep their publisher's terms. -->

# Provenance: the Mitz closed authorization question

Pinned by `scripts/vendor/mitz.sh`, which fetches the documents into the
git-ignored `docs/specs/mitz/cache/` for local reading. No document is committed: change
the pin in docs/VERSIONS.md and re-run the script.

- Publisher: VZVZ Servicecentrum, the Mitz afsprakenstelsel
- Source: the attachments of the Confluence page "Bijlage
  Architectuurdocumenten", page id `828314367` in space `MA11`,
  <https://vzvz.atlassian.net/wiki/spaces/MA11/pages/828314367/Bijlage+Architectuurdocumenten>, readable
  without an account
- Pin: attachment version `1` of each document below, and its sha256
- Download URL: `https://vzvz.atlassian.net/wiki/download/attachments/828314367/<document>?version=1&api=v2`,
  which redirects to the Atlassian media store
- Fetched: 2026-10-04
- Upstream licence: none stated. The documents carry no licence, copyright
  notice or terms of use, and the page states none; the PvE AMC is marked
  "Classificatie: Vertrouwelijk". Redistribution is therefore not permitted,
  and the repository ships none of the bytes.
- Read by: #475 (the closed authorization question of
  `crates/nl-generic-functions` feature `mitz`, its harness service in
  `tools/ferrofed-testkit`, and the Mitz consent pre-filter of
  `app/ferrofed-identity`)

| Document | Bytes | sha256 |
|---|---|---|
| `VZVZ_Mitz_Implementatiehandleiding_OpenGesloten_v3.8.2.pdf` | 465817 | `a5ce8f0d7eba8969a395a8560cf69f9e359f9da4c76145adc0748b9907ac0cf3` |
| `VZVZ_Mitz_PvE_AMC_Aansluiting_Mitz-connector_v3.8.1.ad1.pdf` | 816925 | `b1f18b48969475ce969472299179b37663cdcf67b067e9275532e0fa35bb59cb` |
| `VZVZ_Mitz_Implementatiehandleiding_Berichtauthenticatie_v3.8.1.ad1.pdf` | 431202 | `9659bdcd20a4deebc699357a455aaca7b3643c78a53b078329edcb899ef02f01` |

## What each document defines

- `VZVZ_Mitz_Implementatiehandleiding_OpenGesloten_v3.8.2.pdf` (version
  3.8.2, 27 May 2024, Definitief): §2.3 and §3.2 the closed authorization
  question, a SOAP 1.2 web service under ITI TF-2x Appendix V carrying one
  XACML 3.0 `XACMLAuthzDecisionQuery` (or the XACML 2.0 PIP variant), with
  its attributes (§3.2.4.2), request examples (§3.2.4.4, §3.2.4.5), expected
  actions (§3.2.4.6), response attributes and examples (§3.2.5); §3.3 the
  transport (mutual TLS under IHE ATNA, IHE CT); §4 the OIDs; §5 the
  professional identifier; §6 the HTTP headers.
- `VZVZ_Mitz_PvE_AMC_Aansluiting_Mitz-connector_v3.8.1.ad1.pdf` (version
  3.8.1.ad1, 2 June 2023): AUS-TR-e0040 and AUS-TR-e0050, the attributes of a
  question and how Mitz answers it; AUS-TR-e0900, error handling;
  AUS-TR-e0250, the optional tokens.
- `VZVZ_Mitz_Implementatiehandleiding_Berichtauthenticatie_v3.8.1.ad1.pdf`
  (version 3.8.1.ad1): the message token a connector receives on every
  interface when it opts in at admission (AUS-TR-e0250).

## Examined and not taken

- The FHIR package `vzvz.fhir.mitz` 3.8.0 on packages.fhir.org (tarball
  sha256 `f36b10fb7fbf4aa5f7ae76ddc3a99b9cfcdb0b9beaec1aba0344ac739832f7fe`):
  the Consent, Subscription and notification profiles of the subscribe,
  notify and migrate interfaces, and their code systems. It defines no
  closed authorization question; its `package.json` states no licence and
  its resources name VZVZ as copyright holder.
- GitHub: the `VZVZ` organisation publishes no Mitz repository.
- The other attachments of the page (the PvE AUS, AXI and ZNP, the
  subscription and ZNP guides, the generic token guide, the glossary and the
  release notes) define no part of the closed authorization question.
