<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Conformance statement

FerroFED claims the Federation-Gateway profile of the Federation Tier with
AQL specification, at the version and commit the claim table names. This page
is that claim. It names the specification text the claim is made against,
the node products the tests ran against, every conformance point of
[§17](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/conformance.html)
and every test track of
[§16.3](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/testing.html#_16_3_test_tracks)
with how each is scored, and every deferral with its reason.

The part between the statement markers is rendered by
`scripts/conformance/matrix.sh --statement-write-into` when the site is
built, from the conformance matrix, `docs/VERSIONS.md`, the vendored specification and the
`// conformance:` markers on the tests. The docs build and the
conformance-matrix guard both fail when it disagrees with them, so the
statement cannot claim more than the matrix holds. The
[conformance matrix](conformance.md) shows the same points by requirement,
and the [obligations checklist](obligations.md) covers every normative
statement of the text.

## What the claim covers

- **Scored by a test** means at least one test carries the point's marker
  and CI runs it. The [conformance tests](conformance-tests.md) page lists
  each test by point and track. A test in an `e2e` module runs in the CI e2e
  job, with `FERROFED_E2E=1`, against the node products below.
- **The Federation-Gateway profile only.** FerroFED holds no clinical data and
  is not a CDR, so it makes no Federation-Node claim. A point whose actor is
  Node or Operator is scored against that actor, and a gateway is not marked
  down for it
  ([§17](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/conformance.html),
  [§16.2](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/testing.html#_16_2_two_conformance_profiles)).
  FerroFED assists those actors with the node profile checks and the
  admission check, and the last section of the generated part records what
  each found.
- **CP-20 and CP-39 are tested without a marker.** A marker on a Node or
  Operator point belongs to a harness check under `tools/`, which writes the
  finding the report reads. The gateway's part of each is tested in its own
  crate: the registry load refuses an endpoint with a FHIR connection type
  (CP-20,
  `app/ferrofed-registry/tests/it/refusal.rs::a_fhir_rest_connection_type_is_refused`),
  and the book answers each §13.4 deployment decision in a section of its
  own (CP-39, `app/ferrofed-server/tests/it/deployment_decisions.rs`).
- **A deferral is the owner's decision**, recorded on the issue its row
  names, with the reason the specification gives for it.

<!-- statement:begin -->
<!-- Rendered by scripts/conformance/matrix.sh --statement-write-into when the site is assembled. Do not edit between the statement markers. -->

The statement is rendered from the conformance matrix and the test markers when the site is built. Run `scripts/site/assemble.sh` to read it locally.

<!-- statement:end -->

## Not observable

Some obligations cannot be observed from outside the node that holds them.

- **CP-18 and CP-19 in a deployment run.** `ferrofed conformance run
  --node-profile` records the CP-18 access check and the CP-19 consent check
  as `not-observable`. The refusal under test is the node's own access policy
  or consent decision, which an operator cannot arrange through ITS-REST, and
  ITS-REST defines no consent resource
  ([Scoring a deployment](../operate/conformance-run.md)).
- **CP-18 in the harness.** The harness starts FerroEHR and EHRbase with an
  access policy that withholds one EHR from one principal, and its checks
  read the node's answer on the read and on both `ehr_id`-scoped query forms.
- **CP-19 in the harness.** Neither product records a consent decision, so
  the harness arranges no consent refusal and the point reads
  `node-not-observable` for both. The gateway's own part of consent is CP-36
  and track 7, both scored by tests
  ([N27](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/requirements.html#n27),
  [N27a](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/requirements.html#n27a)).
- **Track 10 in a deployment run.** The run reports it `not-run`, because only
  node-side wire capture shows what reached a node
  ([§16.3](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/testing.html#_16_3_test_tracks)).
  The harness scores it with a capturing proxy in front of each node.

## Where the specification disagrees with itself

[CP-14](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/conformance.html#cp-14)
scores
[N22](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/requirements.html#n22):
a follow-up read of a version is routed on its `creating_system_id` first.
The EHR-scoped rule of
[§12a.1](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/identifiers.html#route-ehr)
routes every request under `{base}/v1/ehr/{ehr_id}/…` on the `ehr_id`
binding, "not on `creating_system_id`", and
[N41](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/requirements.html#n41)
orders the steps for every path `ehr_id`
([§12.5.1](https://syntaric.github.io/openehr-federation-spec/federation-aql/1.0/follow-up-routing.html#ehr-id-routing)).
Every ITS-REST version read sits under a path `ehr_id`, so the two rules
disagree for every such read. FerroFED routes by N41, scored under CP-13 and
CP-33, and defers CP-14 until the text settles it. The other half of N22,
that a uid is never rewritten, is held by tests the obligations checklist
names.

Every defect, contradiction or silence found in a specification while
scoring is reported on the standing upstream-reports issue,
[#212](https://github.com/FerroHEALTH/FerroFED/issues/212). That includes the
findings of the differential run
([#94](https://github.com/FerroHEALTH/FerroFED/issues/94)), which sent the
same Connectathon requests to FerroFED and to the reference implementation
over the same FerroEHR nodes and judged each difference against the
specification text.

## Rerunning the suite

The record, offline: the matrix against the vendored specification, every
marker, and the generated pages.

```sh
bash scripts/checks/conformance-matrix.sh
```

The tests, and then the per-track and per-point report of a run over the
whole workspace with the e2e harness, which needs Docker. The report lands in
`target/conformance/report.md`.

```sh
cargo nextest run --workspace --locked --all-features
scripts/conformance/report.sh --run
```

To score your own deployment, run `ferrofed conformance run`
([Scoring a deployment](../operate/conformance-run.md)).

After a change to a status or a pin, render the matrix page again. The
statement and the tests page are rendered when the site is built, so a new
marker needs nothing; `scripts/site/assemble.sh` shows them locally:

```sh
scripts/conformance/matrix.sh --render-write
```

## When the specification is re-pinned

The pinned text is the 1.0.0 release, re-pinned from the 0.9.0 release
candidate ([#17](https://github.com/FerroHEALTH/FerroFED/issues/17)); the
release moved no point, requirement or track. When a later release is
pinned, `scripts/conformance/matrix.sh --derive` refreshes the derived
columns. A point new to the text arrives `planned`, and the guard refuses it
until the re-pin scores it with a test or records the owner's deferral. This
statement then names the new version and commit. CP-14 and the other choices
the reports on #212 bear on are revisited against the 1.0.0 text
([#354](https://github.com/FerroHEALTH/FerroFED/issues/354)).
