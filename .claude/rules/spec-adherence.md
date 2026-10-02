---
paths: ["**/*.rs", "scripts/**", "docs/**"]
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Spec adherence (the published specifications are the oracle)

The conformance authority for this project is the published specifications, not
another implementation, not memory, not intuition. FerroFED is a federation
gateway in front of openEHR CDRs, so it answers to these sources, in this
order:

1. **The Federation Tier with AQL specification**
   (<https://syntaric.github.io/openehr-federation-spec/>), the openEHR
   Federation Working Group's proposal: the façade contract, the
   resolve/dispatch/combine model, the endpoint directive, the result set,
   de-duplication, partial results, follow-up routing, the identifiers, node
   membership, the security handoff, and the conformance points. Its two
   published JSON schemas (`federated-result-set.schema.json`,
   `options-root.schema.json`) are the machine-readable half of that
   authority. The normative body is §1 to §18; Annex A (IHE) and Annex B (the
   Dutch Generic Functions) are informative bindings.
2. **openEHR ITS-REST**
   (<https://specifications.openehr.org/releases/ITS-REST/Release-1.1.0/>):
   both faces of the gateway. The client-facing surface the Tier must stay
   transparent over, and every call the gateway makes into a node, with its
   status codes, headers, and error bodies. The federated result envelope is
   an ITS-REST `RESULT_SET`.
3. **openEHR AQL** (the Query component,
   <https://specifications.openehr.org/releases/QUERY/latest/AQL.html>): the
   language the gateway parses, rewrites to an `ehr_id`-scoped query per node,
   and merges across nodes.
4. **The openEHR Reference Model and BASE**
   (<https://specifications.openehr.org/releases/RM/latest/>): `EHR`,
   `EHR_STATUS.subject`, `PARTY_REF`, `OBJECT_VERSION_ID`, `system_id`, and
   the rest of the identifier and versioning model the routing keys stand on.
   The model comes from the published `openehr-*` crates; it is never
   restated here (§The openEHR surface comes from the published crates).
5. **The IHE ITI profiles** the specification names as its proposed binding:
   PIXm (ITI-83, ITI-104), PDQm (ITI-78, ITI-119), XCPD (ITI-55), PMIR
   (ITI-93, ITI-94) and mCSD, each at the version `docs/VERSIONS.md` pins.
6. **The Netherlands Generic Functions IG**
   (<https://build.fhir.org/ig/nuts-foundation/nl-generic-functions-ig/>),
   the regional binding Annex B describes. It governs only a deployment that
   selects the Dutch binding, never the normative body.

The HL7 *Intermediaries White Paper* and the *Hybrid / Intermediary Exchange*
IG are referenced documents of the specification. They explain why a rule is
there; they never override the specification's text.

The precise release of each is pinned in `docs/VERSIONS.md` with its ground in
`docs/architecture.md`, and the machine-readable artefacts are vendored
verbatim with a `PROVENANCE.md` per tree (`vendored-inputs.md`):

- `docs/specs/federation-spec/`: the specification source (the AsciiDoc pages,
  the navigation, the two JSON schemas under `modules/ROOT/attachments/`, the
  diagrams, and the traceability tooling).
- `docs/specs/federation-ref/`: the reference implementation, prior art and a
  test corpus (its AQL golden cases, schema copies and demo data), never an
  oracle.
- `docs/specs/its-rest/`: the ITS-REST 1.1.0 OpenAPI documents.

Read the vendored text for a citation and name the file and section; the
published URLs above are the same content at the pinned version.

## The openEHR surface comes from the published crates

Owner ruling 2026-10-01: FerroFED never re-implements the ITS-REST surface or
AQL. Both faces of the gateway and the query language come from the published
`openehr-*` crates:

- **`openehr-its`, feature `rest-server`**: the client-facing façade. The
  generated axum server traits are what the gateway implements; the routes,
  parameters, headers and bodies are theirs, never a hand-written router over
  the same paths.
- **`openehr-its`, feature `rest-client`**: the node dispatch. Every call into
  a node goes through the generated client.
- **`openehr-query`**: AQL. The gateway parses a client query with it, rewrites
  the syntax tree, and prints the per-node query with `printer::to_aql`; never
  a string splice, a regex over AQL text, or a local grammar.
- **`openehr-rm` and `openehr-base`**: the identifier and versioning model.

**A gap in those crates is a FerroEHR tracker issue, not a local workaround.**
A missing route, a wrong parameter type, an AST node the rewrite needs, a
printer that loses a form: file it on FerroEHR's tracker (labels and milestone
there), link it from the FerroFED issue it blocks with the `blocked-upstream`
label, and wait for the release. A shadow type, a forked copy, a hand-rolled
call beside the generated client, or a post-processing pass over printed AQL
is forbidden. The code change itself is made in FerroEHR's own checkout and
session, never from this repository.

## Citing the federation specification

Cite it the way its own CONTRIBUTING asks, so a citation survives a re-pin:

- **Sections** by § number: `§7.1`, `§11.2`, `§12a`, `§12.7`. Annexes as
  `Annex A §A.1`, `Annex B`.
- **Normative requirements** by N-number: `N7`, `N27a`, `N44`. They live in
  §6 (`requirements.adoc`), one anchor per requirement.
- **Conformance points** by CP-number: `CP-14`. They live in §17
  (`conformance.adoc`), are appended and never renumbered, so a CP number is a
  stable key for a test.

Every conformance test names the CP it scores and the N it covers, in its
name or its doc comment, so `CP-14` can be found from the test and the test
from `CP-14`. A point whose actor is `Node` or `Operator` (§17, §16.2) is not
the gateway's to score; the gateway tests what it can observe of it and says
so.

## Hard rules

- **Before implementing or changing any spec-facing behaviour, read the
  governing section first.** Route by surface:
  - the façade, the rewrite, the result envelope, routing, partial results, or
    an `OPTIONS` body goes to the federation specification, plus its schema;
  - a request or response on the ITS-REST wire, on either face, goes to
    ITS-REST 1.1.0;
  - the grammar or semantics of a query goes to AQL;
  - an identifier, a version id, or `EHR_STATUS` goes to the RM and BASE;
  - identity resolution, localization, or addressing goes to the IHE profile
    the binding selects, or to Annex B for the Dutch binding.
- **NEVER LAX. Strictness is a hard rule.** The gateway accepts EXACTLY what the
  governing specification admits, nothing more and nothing less.
  - Everything a specification REFUSES, we refuse, and every refusal is an
    ASSERTED NEGATIVE TEST pinning its error outcome (an undirected aggregate,
    a cross-node `OFFSET`, a second subject, a directly identifying identifier
    that would survive into a dispatched query), so a silently loosened
    gateway is a failing build rather than quiet drift.
  - A spec-SILENT form is accepted only with a first-hand citation, recorded on
    a tracker issue. Stalled or contradictory upstream material is never
    carried silently.
  - Weakening any existing refusal needs a spec-grounded adjudication recorded
    on an issue, with the flipped test updated to assert the new expected
    outcome. Inventing a prohibition the specification does not contain is the
    same defect class as leniency: strict means exact, in both directions.
- **Identifier hygiene is not negotiable.** The patient identifier used for
  resolution is consumed at the gateway and never reaches a node (§5.4, N33);
  the carriers, the strip-or-reject rule and the tests are in
  `identifier-hygiene.md`.
- **Cite the source.** A conformance-relevant decision names the specification
  and section (and the N or CP it answers) in the commit or PR description. A
  deliberate deviation or gap gets a `// NOTE:` with the reference and the
  reason.
- **Cite ONLY durable references, never an internal markdown file as a design
  authority.** In code, doc comments, and findings, justify behaviour by citing
  one of the sources above or official external documentation (the Rust book
  and reference, a pinned crate's docs.rs page). An internal plan document is
  deleted in the PR that implements it and is never a citable authority; the
  durable record is the closed issues, PR descriptions, `CHANGELOG.md`, git
  history, and the living reference docs. Where the specifications are SILENT
  (the process model, storage mechanics, transport details, infrastructure),
  flag it explicitly: "no specification governs this: our own design".
- **The reference implementation is prior art, never a substitute for the
  specification.** `openehr-federation-ref` validated the 0.9.0 text and its
  golden cases are useful evidence. Read it for how a problem was solved.
  Where it disagrees with the specification text, the specification wins and
  the divergence is worth a note. Never resolve a spec question from its
  observed behaviour alone, and never copy its code.
  - **Its acceptance proves nothing.** That the reference implementation
    accepts a form, merges a result a certain way, or claims a conformance
    point is never evidence that the specification admits it. Where it is
    laxer than the specification (string comparison of ordered values, a dedup
    key coarser than the version identity, a conformance claim with no
    enforcement behind it), FerroFED holds to the specification and to the
    NEVER LAX rule above, and records the divergence. Strictness here is the
    family bar FerroEHR sets, not the bar of any other implementation.
- **FerroEHR and every other CDR are nodes, not oracles.** A response from a
  node is evidence in a comparison, never the reference. A divergence found
  against a node is attributed against ITS-REST before anything is changed,
  and if the defect is the node's it is reported to that project rather than
  worked around here.
- **A defect in a published specification is recorded as an `upstream-report`
  issue** (`issue-workflow.md`) with what the specification says, what this
  implementation does, and the resolution an upstream would need. The issue
  is the record and stays in this tracker; nothing is filed on an external
  tracker, and no issue asks anyone to do so. Do not encode a workaround with
  no record.
- Subagents doing spec-facing work must be handed the relevant sections, N and
  CP numbers or URLs in their prompt, and reviewers verify claims against them.

## The specification is a release candidate

The pinned release is a candidate circulated for comment, and the 1.0 release
follows. A re-pin is a deliberate act, not a refresh: re-run the vendor
script at the new pin, read the specification's own change notes, diff every
N and CP this repository cites, and update the tests, the issues and
`docs/architecture.md` in the same change. A requirement that moved or
disappeared is never left cited at its old number.

## Normative body and regional binding

The normative body is written in abstract roles (localization, addressing,
identifier cross-reference, authentication, authorization, consent) and the
bindings make them concrete. Keep that split in the code: a role is a trait
the core depends on, and the IHE binding and the Dutch binding are
implementations of it. Nothing in the core assumes one region's coupling, such
as localization bundled with a consent check (§2.4, N27, N27a).

## Make no claim beyond the specification

The strongest temptation is to state a technical fact about the federation
model, a binding, or openEHR from memory. Do not. Every claim that appears in
this repository is one the product statement in `CLAUDE.md` already makes, one
`docs/architecture.md` or the research behind it has established with a
citation, or one the code and its tests demonstrate. Anything else is a
question for an issue, not a sentence in a file.
