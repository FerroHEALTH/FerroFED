---
name: spec-lookup
description: Look up the authoritative Federation Tier with AQL, openEHR ITS-REST, AQL, RM, IHE, or Dutch Generic Functions requirement for any spec-facing behaviour, in the correct oracle order, cited by section, N-number and CP-number. Use before implementing or reviewing gateway behaviour, or to settle a "what does the spec say" question.
allowed-tools: Read, Grep, Glob, WebFetch
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Spec lookup

Answer a "what does the specification say" question about the federation
model, the openEHR wire, or a binding, and cite the source (document plus
section, and for the federation specification the N and CP numbers). Never
resolve a spec-facing question from memory or from the reference
implementation's behaviour alone (`.claude/rules/spec-adherence.md`).

## Route by surface, then cite

| The question is about | The authority |
|---|---|
| the façade, the rewrite, the endpoint directive, the result set, de-duplication, partial results, follow-up routing, the identifiers, membership, the security handoff, `OPTIONS` | the Federation Tier with AQL specification, §1 to §18, and its two JSON schemas |
| which requirement a behaviour answers, or how it is scored | §6 (`requirements.adoc`, the N-numbers) and §17 (`conformance.adoc`, the CP-numbers), with §16 for the test tracks |
| a request or response on either face, its status codes, headers, or error bodies | openEHR ITS-REST 1.1.0 |
| what a query means, before or after the rewrite | openEHR AQL |
| an identifier, a version id, `EHR_STATUS.subject`, `system_id` | the openEHR RM and BASE |
| identity resolution, localization, addressing in the proposed binding | Annex A and the IHE profile it names (PIXm, PDQm, XCPD, PMIR, mCSD) |
| the same roles in the Dutch binding | Annex B and the Netherlands Generic Functions IG |

Within a surface, the machine-readable artifact (a JSON schema, an OpenAPI
document) outranks a remembered summary, and the normative prose gives it
meaning. Read both where both exist. In the federation schemas, `$defs/itsRest`
restates ITS-REST; a federation rule lives outside it.

The reference implementation (`openehr-federation-ref`) and any reference node
(FerroEHR or another CDR) are behavioural evidence for spec-silent edge cases
only. Cite them explicitly as such and record the decision; never treat one as
authority.

## Where to look

- **Vendored specifications, first:** `docs/specs/`, one directory per corpus,
  each with a `PROVENANCE.md` naming the pin (`.claude/rules/vendored-inputs.md`).
  Grep there first, because it is the exact pinned text this project
  implements:
  - `docs/specs/federation-spec/modules/ROOT/pages/*.adoc`: the specification,
    one page per section; `requirements.adoc` carries the `[[nN]]` anchors and
    `conformance.adoc` the `[[cp-N]]` anchors, so
    `grep -n '\[\[n27\]\]'` or `grep -n 'cp-14'` lands on the text.
  - `docs/specs/federation-spec/modules/ROOT/attachments/*.schema.json`: the
    two schemas.
  - `docs/specs/federation-spec/tools/traceability.tsv`: the requirement to
    conformance-point to track map.
  - `docs/specs/federation-ref/`: the reference implementation and its AQL
    golden cases, evidence only.
  - `docs/specs/its-rest/`: the ITS-REST OpenAPI documents.
- **Published sources (fetch to confirm, or for what is not vendored):**
  - Federation Tier with AQL:
    <https://syntaric.github.io/openehr-federation-spec/>
  - openEHR ITS-REST:
    <https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>
  - openEHR AQL:
    <https://specifications.openehr.org/releases/QUERY/latest/AQL.html>
  - openEHR RM: <https://specifications.openehr.org/releases/RM/latest/>
  - IHE ITI technical framework and the PIXm, PDQm, PMIR and mCSD IGs:
    <https://profiles.ihe.net/ITI/>
  - Netherlands Generic Functions IG:
    <https://build.fhir.org/ig/nuts-foundation/nl-generic-functions-ig/>
  - openEHR specifications index:
    <https://specifications.openehr.org/releases>

## How to answer

State the requirement, quote the decisive sentence, and cite the document plus
section (for the federation specification: `§11.4`, `N39`, `CP-22`; and the
schema path where the machine-readable form carries it). If the sources are
silent, say so explicitly and name the behaviour you would match; flag it as a
spec-silent decision to record on the tracker, never as a spec fact. If the
answer depends on the pin (the specification is a release candidate until its
1.0), say which pin it was read at.
