---
paths: ["**/*.rs", "**/tests/**"]
---

<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Testing discipline

Test discipline is non-negotiable (a standing hard rule; see `CLAUDE.md`). It
applies to every crate, generated and hand-written alike.

## The hard rule

- **Never** silently weaken, skip, or delete an existing test to make a build
  pass.
- **Never** edit a test to route around a runtime bug it exposes. If a test
  fails and the fix is unclear, leave it failing and record a
  `// TODO(#NNNN):` naming its issue; do not touch the test to make it green.
- Conformance tests assert the **federation specification, openEHR ITS-REST,
  AQL and the RM, and the selected binding**: cite the clause a test encodes
  (the § and N, and the CP it scores), and never adjust an
  expectation to match an implementation bug. A fixture defect is ADJUDICATED
  with a first-hand spec citation (an expected-rejection entry in the owning
  test), never routed around by editing the case.

## Tooling

- **Runner:** `cargo-nextest` (`cargo nextest run --workspace`), not
  `cargo test`.
- **Snapshots:** `insta` pins wire output against golden vectors, the key tool
  for a gateway whose product is a rewritten query and a merged answer: the
  AQL each node receives for a client query, and the federated result set the
  client receives back. Redact volatile fields (latency, request ids,
  timestamps) before snapshotting.
  Review intentional changes with `cargo insta review`; never accept a
  snapshot change you have not read.
- **Properties:** `proptest` for the merge invariants (`ORDER BY` plus `LIMIT`
  over N nodes equals the same over the union, de-duplication is idempotent,
  no rewrite lets the resolution identifier through) and for the AQL rewrite
  round trip.
- **HTTP mocking:** `wiremock` for every upstream. A gateway talks to many
  nodes and to an identity, localization and addressing service, so the unit
  and integration layers stub them, including slow, failing and timing-out
  nodes for the partial-result rules, and only the end-to-end layer runs real
  servers.
- **Benches:** `criterion` or `divan`, kept separate from correctness tests.

## Oracles

The acceptance instrument is not yet chosen; the research program decides it,
and this section records what the oracles are regardless of the harness that
runs them.

- **The federation specification** is the authority for the gateway's
  behaviour, and its consolidated conformance points (§17) and test approach
  (§16) are the shape of the conformance suite: one test per CP the gateway is
  the actor for, named by its CP number. The two published JSON schemas
  validate every result envelope and every `OPTIONS` body the gateway emits;
  the specification's own `[source,json]` examples are fixtures.
- **openEHR ITS-REST** is the authority for both faces: what the client may
  send and receive, and every call the gateway makes into a node, including
  the status codes and headers it must handle.
- **AQL** is the authority for what a query means before and after the
  rewrite.
- **The IHE profiles** (or Annex B, for the Dutch binding) are the authority
  for the identity, localization and addressing exchanges, stubbed at their
  wire.
- **The reference implementation's golden cases** (`docs/specs/federation-ref/`,
  the AQL rewrite cases) are evidence, run as a corpus. A disagreement with
  one is adjudicated against the specification text, never settled by the
  case.
- **FerroEHR is a reference node** for end-to-end runs, never the oracle:
  where a node and the specification disagree, the specification wins and the
  divergence is recorded.
- Prefer a specification-published example over a hand-written fixture. A test
  that encodes a spec rule cites the section it asserts
  (`spec-adherence.md`).
- **No patient data, ever.** Fixtures are synthetic content invented for the
  test. Never paste a real clinical document, a real patient identifier, or an
  extract from a production system into the repository.

## Fuzz findings

The `fuzz/` crate feeds arbitrary bytes to the parsers a caller reaches first
(`docs/ci-cd.md` §The fuzz lane): the AQL rewrite, the ITS-REST query body,
and the federated `RESULT_SET` and `OPTIONS` bodies. A finding is a panic, an
abort or a hang in library code: that is a violation of `reliability.md` (no
panicking path on caller-controlled input) and becomes a `bug` issue with the
reproducing input committed as a regression seed under `fuzz/seeds/<target>/`
and a unit test asserting the typed `Err`. A parser returning `Err` on garbage
is the correct answer and is never a finding; never "fix" one by making the
parser accept more.

The `aql_rewrite` target asserts a property as well as the absence of a
panic: a node query of an accepted query never carries the patient identifier
(`identifier-hygiene.md`, §5.4.1, N33), in a literal or rebuilt by a string
function. A violation is the most serious finding the lane can report: fix it
before anything else, and add the input to the rewrite's negative corpus.

Seeds named `gen-*` are written by `scripts/fuzz/seeds.sh` from the vendored
corpora and are never hand-edited; a regression seed carries any other name.

## Where tests live

Unit tests live beside the code they test (`#[cfg(test)] mod tests` in the same
file), and ONLY there: **dedicated test FILES under `src/` are banned**. A test
that drives the public API belongs in the owning crate's `tests/` directory; a
test of private internals stays a small inline module. If an internals test
grows large, that is a design signal to test through the public seam, not to
split into a src file.

**One integration-test binary per crate**: the `tests/` directory is
`tests/it/main.rs` plus one `mod` per topic file, not one top-level `.rs` per
topic. Cargo compiles and links every top-level `tests/*.rs` as its own crate
("each integration test results in a separate executable binary … this can be
inefficient",
<https://doc.rust-lang.org/cargo/reference/cargo-targets.html>); nextest still
runs each test as its own process, so isolation is unchanged. Shared helpers
live in a plain module under `tests/it/`.

**A binary-only crate is untestable by construction** (Book ch11.3): its
`main.rs` cannot be imported from `tests/`. A server binary therefore keeps a
thin `main.rs` over a testable `lib.rs` run path (Book ch12.3), and its
integration tests import the lib.

## Test shapes (the Book ch11 doctrine)

- **`Result`-returning tests are the preferred shape**: `fn t() -> Result<(),
  E>` with `?` instead of unwrap chains
  (<https://doc.rust-lang.org/book/ch11-01-writing-tests.html>). Plumbing
  failures propagate with `?`, not `.unwrap()`. `clippy::panic_in_result_fn`
  fires on a Result-returning test that also asserts, and clippy offers no
  `allow-…-in-tests` knob for it, so such a test carries the lint in the same
  scoped relaxation its file uses for `panic`/`unwrap`/`expect`
  (`#![allow(…, reason = "test assertions")]` at the test-file root, or a
  `#[expect(…, reason)]` on the single test). Never relaxed at the workspace
  level and never in a non-test module.
- **`#[should_panic]` always carries `expected = "…"`:** bare `should_panic`
  passes when the code panics for the WRONG reason (Book ch11.1).
  `should_panic` is illegal on Result-returning tests; assert `value.is_err()`
  there instead.
- **Assertions**: `assert_eq!` and `assert_ne!` over bare `assert!` for
  comparisons (they print both values); a production-code assert carries a
  message.
- **Doctests are copy-paste templates**: `?` via a hidden `# Ok::<(), E>(())`
  tail or a hidden `fn main`, never `unwrap` (C-QUESTION-MARK; enforced by
  `#![doc(test(attr(deny(warnings))))]` on library roots). `no_run` for an
  example that would open a socket, `text` for non-code, never `ignore`. A
  generated crate keeps `doctest = false` deliberately (generated doc text is
  not a curated example).

## Coverage is a mandate, not just pass rate

A green suite over a thin set of cases proves almost nothing. The bar is
COVERAGE of what the specifications define on the wire:

- Every conformance point the gateway is the actor for (§17), each as its own
  small, ISOLATED case so a failure localizes to one behaviour.
- Every carrier `identifier-hygiene.md` names (§5.4, N33), each with a
  negative case asserting on what reaches the node, plus the converse case
  that a committed body arrives byte-identical.
- Every error family: a query the Tier must reject (an undirected aggregate,
  a cross-node `OFFSET`, a second subject), a resolution that finds nothing,
  a node that refuses, times out or is unreachable under each completeness
  mode (§11), an `ehr_id` collision on a follow-up (§12), and an unreachable
  identity, localization or addressing service.
- **A spec-defined behaviour with no case is a COVERAGE GAP, never an
  acceptable omission.** Close it (a new spec-cited case) or record the honest
  boundary. Silence is not coverage.
- **Coverage only ratchets up.** Cases are added, never removed to go green.
- **One behaviour per case:** many small isolated cases beat one broad case.

## Target

Compiling, clippy-clean, tested increments at all times; a green suite is the
standing bar and every change preserves it. Green comes ONLY from fixing the
defect after spec-adjudicated attribution (`spec-adherence.md`), never from
bending a test or a fixture to match the implementation.
