<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Scoring a deployment

`ferrofed conformance run` drives the Connectathon test tracks of the
Federation Tier with AQL
([§16.3](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/testing.html))
against your own deployment: its registry, its nodes and its
cross-reference. It plays an unmodified openEHR client over ITS-REST and
writes the same per-track and per-point report the FerroFED test harness
writes from its own run
([§16.4](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/testing.html)).
Use it to score an installation before a Connectathon, after an upgrade, or
when you admit a member.

The run drives tracks 1 to 7, 9 and 11. Track 8 is provisional and deferred.
Track 10 is judged on what reached each node, which only node-side wire
capture shows, so the run reports it `not-run`.

## Before you run it

The run needs four things beside the configuration file.

- **A synthetic patient your cross-reference knows.** Choose an identifier in
  the example arc `urn:oid:2.999` that ITU-T X.660 reserves for examples, such
  as namespace `urn:oid:2.999.1.1` and value `ffd-test-0038`, and register it
  in your cross-reference at the members you want scored, each with an
  `ehr_id` no node holds yet. In a development profile that is a
  `[[dev.crossref]]` row per member; with a PIX Manager it is a `Patient` your
  identity source feeds. The run refuses a namespace outside the arc, and a
  value other than ASCII letters, digits, `.`, `_` and `-`.
- **A caller token.** A file holding a bearer access token your gateway's
  `[auth]` accepts, with the scopes to query, read and write EHRs and
  compositions, and to store and run stored queries. The run sends it on
  every request but the one that checks an unauthenticated caller is
  refused.
- **The seed files.** The reference implementation's synthetic demo data:
  the `International Patient Summary` operational template and the two
  compositions `composition-12345-hospital.json` and
  `composition-12345-clinic.json`, whose only party is `PARTY_SELF`. Every
  release attaches them as `ferrofed-conformance-seed-data.json`, one file
  that also carries their Apache-2.0 licence and notice, with its SHA-256 in
  `ferrofed-conformance-seed-data.json.sha256sum`:

  ```sh
  base=https://github.com/FerroHEALTH/FerroFED/releases/latest/download
  for f in ferrofed-conformance-seed-data.json ferrofed-conformance-seed-data.json.sha256sum; do
    curl -LO "$base/$f"
  done
  sha256sum -c ferrofed-conformance-seed-data.json.sha256sum
  ```

  `…/releases/download/vX.Y.Z/` downloads the file of one version. Pass the
  file to `--seed-data`. In a checkout, you can pass the directory FerroFED
  vendors them in, `docs/specs/federation-ref/docker/demo-data/`, instead.
  Either way the run checks each file's SHA-256 and refuses any other
  content.
- **At least two members holding the patient** for the scenarios that compare
  answers across nodes. With one, those scenarios are `not-run` with the
  reason.

## Running it

```text
ferrofed conformance run --config /etc/ferrofed/ferrofed.toml \
  --allow-writes \
  --patient-namespace urn:oid:2.999.1.1 --patient-value ffd-test-0038 \
  --token-file /run/secrets/conformance-token \
  --seed-data ./ferrofed-conformance-seed-data.json \
  --out ./conformance-report
```

Without `--gateway`, the run starts the gateway in-process from the
configuration, as `serve` builds it, on a loopback port of its own, and
stops it at the end. The report is then of exactly the configuration you
gave, and nothing else need be running. With `--gateway <url>`, the run
drives the gateway already serving at that base URL, and reads the
configuration for the registry, the node clients it seeds through and the
cross-reference. Give it the same configuration that gateway serves.

### What it sends in the clear

The run carries your caller token and synthetic patient data, so it holds
every connection it makes to a stricter rule than the gateway's own:

- the `--gateway` URL and the URL of every member endpoint in the registry
  are `https`, or plain `http` to a loopback host (`127.0.0.1`, `::1`,
  `localhost`) under the development profile alone;
- plain `http` to any other host is refused under every profile, and any
  `http` outside the development profile, before the run sends anything;
- the token endpoints and the other credential sites of the configuration
  are held to the rule `serve` applies
  ([Configuration](configuration.md)).

A refusal exits `78` and names the URL's key, never the URL or a value. A
loopback `http` connection the development profile admits is printed as a
warning and listed under *Unencrypted connections* in `report.md`. The
in-process gateway is the run's own listener on `127.0.0.1`, inside the
same process.

The run follows no redirect, to the gateway or to a node: a `3xx` is the
answer it reads, so the token never travels to an origin you did not give
it. The token is read from `--token-file` into a secret type, and no report
file, log line or error message carries it. Each node receives the onward
credentials the configuration names for it, never your token.

`--node-profile` also checks every active member against the node
obligations of
[§16.2](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/testing.html),
through the member's node client with its onward credentials, as the
gateway reaches it, and records the findings as the node profile:

| Check | Point | What the run does |
|---|---|---|
| invocable on `ehr_id` alone | CP-27 | reads the patient's EHR it seeded at the member, its `EHR_STATUS` and its compositions by the `ehr_id` alone |
| subject never required | CP-27 | creates an EHR with no `EHR_STATUS` and reads and queries it by its `ehr_id` |
| node errors passed through | CP-18 | asks for an EHR the node does not hold and sends an unparsable query, and reads the status the node answers |
| access decided at the node | CP-18 | records `not-observable`: the refusal under test is the node's own policy, which a run cannot arrange |
| consent before release | CP-19 | records `not-observable`: ITS-REST defines no consent resource, so a run cannot arrange a consent refusal |
| the identifier-integrity conditions of §12b.2 | CP-33a, and CP-27 for the `ehr_id` exchange | runs the admission check ([Admitting a node](admission.md)) |

A status ITS-REST does not document for an operation is recorded as the
node's answer, and a member the checks cannot reach is recorded
`not-observable` with the cause, never a pass.

The command exits `0` when no scenario failed, `1` when one did or the run
could not reach its report, `2` when a safety rule refused it, and `78` for a
configuration that does not load or has no registry, or a connection that
would carry the token or the synthetic data in the clear.

## What it writes

**The run writes to your nodes, and removes nothing it writes.** It refuses
to start without `--allow-writes`. Against a configuration whose `profile` is
not `development`, it also needs
`--i-understand-this-writes-synthetic-data-to-the-nodes`, because such nodes
may hold real records.

Through each node's own ITS-REST API, with the endpoint's onward credentials
and the gateway's outbound gate, it writes:

| What | Where |
|---|---|
| the patient's EHR, `PUT {base}/v1/ehr/{ehr_id}` with an `EHR_STATUS` naming the synthetic patient | every member the cross-reference names |
| the vendored template, unless the node holds it | every member holding the patient |
| one vendored composition in the patient's EHR | every member holding the patient |
| three EHRs with no subject, for the routing scenarios that need an `ehr_id` the gateway has not seen, one holding the vendored composition | the first two members holding the patient |

Through the gateway, the scenarios write:

| What | Where |
|---|---|
| an EHR with no subject, created at the member the request names | a member holding the patient |
| the vendored composition with the synthetic patient's identifier added to its composer as a `DV_IDENTIFIER` | the patient's EHR at the first member |
| two stored queries, under `org.example.conformance::` with a name unique to the run, the patient bound only as `$patient` | the gateway's stored-query registry, when it declares one |

With `--node-profile`, the node profile also creates one EHR with no subject
on every active member, and the admission check three test EHRs, each for a
synthetic subject of its own.

The run reads no EHR it did not create. When a node already holds an
`ehr_id` the cross-reference names for the patient, the node answers the
create `409`, and the run seeds the patient nowhere and reports the
patient's scenarios `not-run`. Register a fresh patient for every run: the
run never writes into an EHR it did not create, so a second run with the
same patient seeds nothing.

The patient's identifier travels in the queries the run sends the gateway,
in the `EHR_STATUS` body of its own creates and in the composition it
commits, and to your cross-reference. The report never prints it.

## Reading the report

The run writes four files to `--out`.

- `report.tsv` and `report.md` hold one row per track and per conformance
  point of
  [§17](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/conformance.html),
  with the columns `kind`, `id`, `title`, `actor`, `status`, `result`,
  `passed`, `failed`, `not_run`, `issue` and `reason` the harness report
  has. `status` is FerroFED's own: `covered`, `deferred`, `node-profile` or
  `operator`, from the conformance matrix the binary was built with.
- `node-profile.tsv` holds the node profile findings, one row per finding and
  point.
- `written.tsv` lists every write the run made, by endpoint.

A row's `result` reads:

| Result | Meaning |
|---|---|
| `pass` | every scenario that scores the row ran against your deployment and passed |
| `fail` | a scenario that scores it failed; the reason says which check |
| `not-run` | a scenario that scores it did not run, or none does; the reason says why |
| `deferred` | FerroFED defers the point by a recorded decision |
| `node-pass`, `node-fail`, `node-not-observable`, `unchecked` | a Node point, scored against the member and never the gateway ([§16.2](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/testing.html)) |
| `operator-pass`, `operator-fail`, `operator-not-observable`, `not-applicable` | an Operator point, scored against you |

**A `pass` from the run is narrower than a `pass` from the harness.** Many
scenarios have a part a live deployment cannot provide for: a fault injected
at a node (a refusal, an outage, a delay), node-side wire capture (what
reached each node: the `ehr_id` scope, no `subject`, no directive), or a
gateway configured for the one scenario (a consent pre-filter that denies, a
different aggregate declaration). The run reports each such part as its own
scenario, `not-run` with that reason, and every row it scores reads
`not-run` too, however much else passed. The section *Scenarios that did not
pass* in `report.md` lists each one. The harness scores those parts against
its own FerroEHR nodes.

Some scenarios also need something of the deployment: two members holding the
patient, a member where the patient does not resolve, or a stored-query
registry declared in `OPTIONS {base}/`. Without it the scenario is `not-run`,
naming what was missing.

The run reads each answer with the typed result envelope and does not
validate it against the published JSON Schemas; the harness does.
