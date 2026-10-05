---
paths: ["**/*.rs", "**/tests/**", "**/fixtures/**", "docs/architecture.md"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Identifier hygiene (§5.4, N33)

The gateway exists so that a node is located by its own `ehr_id` and nothing
else. The patient identifier a client supplies is consumed by resolution at
the gateway and never travels further (§5.4.1, N33). This is the one property
of FerroFED that a merely plausible implementation gets wrong, which is why
the specification makes it an adversarial test track (§16, track 10) and why
it is a hard rule here with no case-by-case exceptions.

## What the rule covers

Everything the gateway itself composes for a node: the dispatched AQL, the
query parameters it binds, the request path, the query string and the
headers (§5.4.1, N33). FerroFED adds its own surfaces to that list, because a
value that reaches them is on a wire or in a store the node operator, a log
pipeline or an attacker can read:

- the echoed or logged query text, including a `400` diagnostic that quotes
  the refused predicate;
- `tracing` spans and events, metrics labels, and error `Display` and `Debug`
  output;
- anything the gateway persists (the resolution bindings, the `ehr_id`
  index, an integrity incident, a stored-query definition).

The gateway logs that it stripped or refused a value, as a security-relevant
event, and never the value itself (§5.4.3).

## The carriers

A patient identifier can ride in any of these, and each is in scope (§5.4.2):

| Carrier | Handling |
|---|---|
| `EHR_STATUS.subject.external_ref.id.value`, with its `namespace` | Resolution input. Consumed and replaced by `e/ehr_id/value = '<node ehr_id>'`; the `namespace` predicate is consumed with it (golden cases 01, 15). |
| An `ENTRY`-level `subject` (`PARTY_IDENTIFIED` / `DV_IDENTIFIER`: `id`, `issuer`, `type`, `assigner`) | Resolution input on equal terms; both carriers are mandatory, never a deployment choice (§5.4.3, N33, CP-2). The `issuer` / `type` supply the namespace and are consumed with the value (golden cases 13, 14). |
| `PARTY_IDENTIFIED.identifiers` on `COMPOSITION.composer`, `EVENT_CONTEXT.health_care_facility`, `PARTICIPATION.performer`, `ATTESTATION.committer` | Never resolution input. A clinician or facility identifier there is ordinary query material and passes (golden case 08). The patient's own identifier on one of these paths is smuggling and is rejected (golden case 16). |
| `PARTY_RELATED.identifiers` | A third party's identifier. Not a patient identifier, so §5.4.1 alone does not require rejecting it; the privacy question is recorded, not decided silently (§5.4.3). |
| `SELECT` and `ORDER BY` paths, and query parameter bindings | An identifier in a projection or a binding is still on the wire (§5.4.2). A projected subject column is re-injected at the gateway after the merge, never asked of the node (golden case 03). |
| A comment, or any other text the parser drops | Never forwarded. The node AQL is printed from the rewritten AST with `openehr-query`'s `printer::to_aql`, so text that is not in the AST cannot reach a node (golden case 12). |
| The request path, the query string and the headers | The node is addressed by `ehr_id` only; no header or query parameter is copied from the client request without a rule naming it. |

## Strip or reject, nothing in between

- A value the gateway resolved on is consumed and stripped exactly as the
  `external_ref` predicate is (§7.1, §5.4.3).
- A patient-identifying value the gateway did not resolve on, a second value
  for the same subject, or a subject predicate that cannot be consumed exactly
  (one under `OR`, a `LIKE` pattern) is a `400` before any dispatch, never a
  partial rewrite (§5.4.1, golden cases 07, 09, 10, 16, 17).
- The test is the value, not the path. A gateway that refuses every predicate
  over a `PARTY_IDENTIFIED` path is over-strict and fails CP-2; one that passes
  the resolved value on any path fails CP-26 (§5.4.3).
- The rewrite is an AST transformation over `openehr-query`, never a string
  splice or a regular expression over AQL (`spec-adherence.md`).

## Write payloads pass through unchanged

A `COMPOSITION` (or any other body) a client commits is archetyped clinical
content. A `DV_IDENTIFIER` inside it is modelled data, and the gateway has no
right to alter it: the body reaches the node byte-identical, which also keeps
its `ETag` and uid semantics (§5.4 scope note, N22, track 10's converse
check). This rule governs what the gateway composes, never what the client
sends.

## Tests

- **One negative test per carrier** in the table above, asserting on what
  reaches the node: the captured request at a mock node (`wiremock`) in unit
  and integration tests, and node-side wire capture in the end-to-end suite,
  never the gateway's own logs (§16, track 10). Zero occurrences of the value,
  or a `400`, is a pass.
- **The converse test:** a committed body carrying a `DV_IDENTIFIER` arrives
  byte-identical.
- **The positive test for both carriers:** the same patient query through
  `external_ref` and through an `ENTRY`-level `subject` returns the same rows
  (CP-2), and a clinician predicate on `composer` is dispatched (CP-2).
- **The log and telemetry test:** a stripped or refused value appears in no
  captured log line, span field, metric label or error message.
- Each test is tagged with the CP it scores (CP-2, CP-26) and N33
  (`testing.md`). The vendored golden cases under
  `docs/specs/federation-ref/src/test/resources/aql-golden/` run as a corpus,
  adjudicated against the specification, not against the reference
  implementation (`spec-adherence.md`).

## Fixtures

Fixture identifiers are synthetic and visibly so (`'12345'`, a namespace
under an example OID). No BSN, NHS number, or any other real national
identifier, and no number that passes a national check-digit algorithm, is
ever committed (`vendored-inputs.md`). A value that looks real is replaced
with one that does not before review.
