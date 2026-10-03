<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Admitting a node

A federation admits a node before the node serves any request, and it does
not admit a node that fails its admission conditions
([§12b.1](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/membership.html),
[N42a](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/requirements.html#n42a)).
The conditions cover at least the identifier-integrity rules of §12b.2.
Admission is your act as the federation operator. FerroFED does not decide
it, and the specification scores it against you, not against the gateway
([CP-33a](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/conformance.html#cp-33a)).

The specification asks you to verify the conditions by test and not by
attestation alone. FerroFED helps in two ways:

- the registry refuses, at load, a second member with a `system_id` an
  existing member already has, compared without regard to ASCII case;
- `ferrofed admission check` exercises one configured member against each
  condition a test can reach and writes a report you keep as evidence.

## Running the check

Add the candidate node to the registry document first, with its node, its
`system_id` and its endpoint. Keep the endpoint at `status = "suspended"`
while you check it: the gateway sends a suspended endpoint no query and
routes no request to it, and the check still reaches it.

```text
ferrofed admission check --endpoint node-c-pub --config /etc/ferrofed/ferrofed.toml
ferrofed admission check --endpoint node-c-pub --count 5
```

`--endpoint` names the endpoint id from the registry. `--count` is the number
of test EHRs the check creates, from 2 to 50, and 3 when you leave it out.
The check uses the same configuration as `serve`: the endpoint's onward
credentials, the per-node timeout, and the configured cross-reference.

**The check writes to the node.** It creates the test EHRs through
`POST {base}/v1/ehr`, and the node keeps them. Each EHR's `EHR_STATUS` names
a synthetic subject: the namespace `urn:oid:2.999.1.0`, inside the example
arc ITU-T X.660 reserves, and a value `ffd-admission-` followed by 32 random
hexadecimal digits, minted fresh for each run. No real identifier is used.
The report states in its first lines that it created test EHRs. Remove them
under the node's own procedure if your governance requires it.

The subjects travel only in the `EHR_STATUS` body and to your
cross-reference. Every request passes the same outbound gate as a query, with
the run's subjects withheld from the path, the query string and the headers
([§5.4.1](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/identity-resolution.html#identifier-hygiene),
[N33](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/requirements.html#n33)).
The report prints the endpoint id, the node id, `system_id`s and the
`ehr_id`s the node created. It never prints a subject or a node's error body,
which could echo one.

Each condition gets one verdict:

| Verdict | Meaning |
|---|---|
| `pass` | the evidence shows the node meets the condition |
| `fail` | the evidence shows it does not, or the check could not reach the node |
| `cannot-check` | the check cannot decide the condition; the report says why |

The command exits `0` when no condition failed, `1` when one did, `2` for an
endpoint the registry does not hold, and `78` for a configuration that does
not load or has no registry document. A node the check cannot reach fails
every condition it exercises, with the cause: a check that reached nothing
passes nothing.

## The conditions

### `ehr_id` generation

**The rule:** `ehr_id`s are version-4 UUIDs, or come from another scheme with
equivalent collision resistance and no coordination requirement. A node that
issues sequential, short or deployment-local `ehr_id`s is not admitted
without remediation.

**What the check does:** it reads the `ehr_id` of each test EHR from the
node's `ETag` (or else its `Location`), reads the EHR back through
`GET {base}/v1/ehr/{ehr_id}`, and compares.

- A version-4 UUID in its hyphenated form passes.
- A UUID of another version is `cannot-check`. §12b.2 accepts another scheme
  only on equivalent collision resistance, and that judgement is yours.
- A value that is not a UUID fails, and so does an `ehr_id` the node issued
  for two subjects, or an EHR the node reads back under another `ehr_id`.

**What it proves:** the `ehr_id`s the node issues now, for new EHRs created
through ITS-REST, have the right form and are distinct. It does not prove the
generator's randomness: a few samples cannot.

### No reuse

**The rule:** a node never re-issues or reuses an `ehr_id`, including across
restores, migrations and test-data resets.

**What the check does:** it reports `cannot-check`. Reuse happens across a
restore, a migration or a reset, and a check run before or after one sees no
trace of the `ehr_id`s the node issued on the other side of it. The `ehr_id`s
of one run are compared with each other under `ehr_id` generation. Your
evidence is the node's documented `ehr_id` generation and its restore and
reset procedures.

### No adoption of foreign `ehr_id`s

**The rule:** a node that ingests EHRs from elsewhere issues a fresh local
`ehr_id`, or is registered as a holder of the origin node's `ehr_id` space.
It never silently adopts a foreign `ehr_id` as its own.

**What the check does:** it reports `cannot-check`. Adoption happens on an
import, and the check performs none. Whether an import issues a fresh
`ehr_id` is the node's import procedure. A registration as holder of another
node's `ehr_id` space is your registry decision.

### `system_id` uniqueness

**The rule:** a node's openEHR `system_id` is unique across the federation
and is not shared with another node. A shared one makes `creating_system_id`
routing ambiguous
([§12.2](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/follow-up-routing.html)),
so a follow-up write could reach the wrong CDR.

**What the check does:** the registry already holds every member's
`system_id` unique, because its load refuses a duplicate. The check then
reads the `system_id` the node itself reports in each EHR it created, the
`EHR.system_id` of the RM. It passes when that is the `system_id` the
registry records for the node. It fails when the node reports another
member's `system_id`, or one the registry does not record for it.

**What it proves:** the registry's uniqueness covers the `system_id` the node
really writes into its data. It does not prove that no system outside the
federation uses the same `system_id`.

### `ehr_id` exchange

**The rule:** the node's environment can resolve a patient identifier to the
node's local `ehr_id`
([§5.5](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/identity-resolution.html#node-obligation)).
Who provides it, the node, its organisation's MPI or a regional service, does
not matter.

**What the check does:** for each test EHR it asks the configured
cross-reference (`[dev]` or `[pixm]`) which `ehr_id` the synthetic subject has
at the node.

- It passes when the answer is the `ehr_id` the node created.
- It fails when the answer is another `ehr_id`, when no cross-reference is
  configured, or when the node created no EHR.
- It is `cannot-check` when the cross-reference does not know the subject, or
  does not answer for it.

The gateway writes to no cross-reference. The `[dev]` table is static
configuration, and a PIX Manager learns patients from its own identity
sources. The round trip therefore passes only where the node's environment
registers a new EHR's subject with the cross-reference by itself. Where it
does not, which is common, the report says `cannot-check`, and you verify the
exchange with a test patient your environment registers.

## What the check cannot prove

Two conditions are outside the reach of any test run from outside the node:
reuse across restores, migrations and resets, and adoption of foreign
`ehr_id`s on import. Both depend on what the node does at a moment the check
does not see. The report marks both `cannot-check` on every run. For those,
the evidence is the node operator's documented procedures, and the decision
is yours.

The gateway also keeps a backstop at request time. An `ehr_id` that two
members claim raises an integrity incident and is never served
([§12.5.2](https://syntaric.github.io/openehr-federation-spec/federation-aql/0.9/follow-up-routing.html#ehr-id-collision)).
That alarm surfaces a collision admission missed. It does not replace
admission.
