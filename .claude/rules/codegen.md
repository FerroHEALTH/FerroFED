<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Code generation: none of our own, and the rules if that changes

**FerroFED generates nothing of its own** (owner decision 2026-10-01,
`docs/architecture.md` §10, decision A33). The federation specification
publishes two machine-readable contracts, `federated-result-set.schema.json`
(the §9 result envelope, an ITS-REST `RESULT_SET` with a constrained, inlined
subset of the ITS-REST definitions) and `options-root.schema.json` (the §7a.2
`OPTIONS {base}/` body, entirely federation-defined), vendored under
`docs/specs/federation-spec/modules/ROOT/attachments/`. Their Rust types are
hand-written in `openehr-federation`: every federation object is
`additionalProperties: true` and four rules are `if`/`then` conditionals,
which the available generator drops. Open objects keep unknown members in a
flattened map, the conditionals are invariants of construction, and three test
layers hold the types to the schemas (validation, drift, semantics). A
machine-readable input FerroFED needs that no family crate generates is a
request to that crate first; only a corpus too large to model by hand would
justify a generator here, and the rules below then apply.

The openEHR side is not generated here, and never re-implemented (owner
ruling 2026-10-01). It is generated upstream and consumed from crates.io:
`openehr-its` with `rest-server` (the generated axum server traits the façade
implements) and `rest-client` (the generated client the dispatcher calls),
`openehr-query` (the AQL parser and `printer::to_aql`, the rewrite's input and
output), and `openehr-rm` and `openehr-base` (the model). The "fix the
emitter, never the consumer" rule below applies across the repository
boundary: a wrong or missing shape in one of those crates is fixed by their
generator, in FerroEHR, through an issue on its tracker, never by a local
type, a fork, or a hand-written adapter (`spec-adherence.md` §The openEHR
surface comes from the published crates).

## Why a generated layer is the expected shape

The Ferro family projects generate their specification model rather than
hand-writing it, for the same reason: when a specification publishes its model
in machine-readable form, a hand-transcribed copy drifts from its source with
no way to detect it. The federation specification states the same discipline
for its own schemas: a schema that lags the prose makes a stale contract look
enforced. Whatever the research decides, the two schemas are exercised: every
envelope and every `OPTIONS` body the gateway emits is validated against them
in a test, and the specification's own `[source,json]` examples are the first
fixtures.

Keep the schema's split visible in the code. `$defs/itsRest` is a restatement
of ITS-REST; a federation constraint is never added inside it, and a type
derived from it is the ITS-REST shape, not a federation one.

## The rules, once anything is generated

- **Never hand-edit a `// @generated` file.** Every generated file starts with
  a `// @generated … DO NOT EDIT` banner. To change output, edit the emitter or
  its override map, then regenerate. A doc defect, a wrong field type, or a
  missing variant in a generated file is a generator fix plus regeneration,
  never an edit of the output.
- **Fix the emitter, never the consumer.** When engine code hits a shape in a
  generated crate that is wrong or insufficient versus the vendored input (a
  missing field, a type too narrow, a per-version difference absent), the fix
  is an emitter or override change plus regeneration. A shadow type, a
  duplicate model, an adapter layer, a placeholder value, or a "temporary"
  local representation in the consumer is forbidden: it silently forks the
  model and defeats the whole design. If the emitter fix is large, register a
  tracker issue; the workaround is still forbidden. On discovering an existing
  workaround, register its removal.
- **The emission scope is a DECLARED root-set closure, emitted COMPLETE.** A
  gateway does not need every type a specification defines, so the
  generator declares its root set and emits the COMPLETE transitive closure of
  everything those roots reference, at its source-mirrored location. Within
  that closure, completeness is absolute: never narrow a schema merge, prune a
  referenced type, or suppress a "missing" generated file to quiet a diff or
  dodge a build error. That is hiding code that should exist. Widening or
  narrowing the root SET is a deliberate, recorded decision, never an ad-hoc
  per-file omission.
- **Per-version correctness is by construction.** Where a specification has
  versions, the generator emits per version from the machine-readable input, so
  a version difference is a generated difference rather than a hand-written
  conditional that can drift. If two versions genuinely coincide the emitter
  may share, and the decision is the emitter's, driven by its inputs.
- **The output is byte-deterministic.** The emitter iterates ordered structures
  (`BTreeMap`, sorted vectors), so regeneration with unchanged inputs produces
  an identical tree. That is what makes a drift check meaningful
  (`reliability.md` §Determinism).
- **A drift check regenerates in CI and fails on any diff**, so the generated
  tree is always in sync with the vendored inputs and the current emitter. The
  check lands in the same change as the generator, never later.
- **A generated file is marked as generated to git too**: the
  `.gitattributes` entry (`linguist-generated`) collapses it in diffs and keeps
  it out of language statistics.
- **The inputs are vendored verbatim with provenance** and are never hand
  edited (`vendored-inputs.md`).

## What is never generated

The gateway engine: the AQL rewrite and its identifier-hygiene pass, the
identity-resolution pipeline and its bindings, the dispatcher, the merge with
its `DISTINCT`, `ORDER BY`, `LIMIT` and aggregate rules, the partial-result
policy, follow-up routing, the node registry, the stored-query registry, and
the server surface. That is hand-written idiomatic Rust of our own design
(`rust-style.md`), consuming the generated or published types directly, and it
is the product.
