<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Your first federated query

**Audience:** integrators who write a client of a FerroFED federation. In
about fifteen minutes you start the quickstart, read the gateway's
self-description, query a patient across four member CDRs, and read the
answers a client has to handle: a patient some members do not hold, a query
directed at one member, and a refusal.

You need Docker Compose 2.23.1 or newer, `curl`, `jq` and `openssl`. You
need no openEHR CDR of your own: the quickstart runs four FerroEHR nodes and
the FerroPIX patient index beside the gateway, with synthetic patients
already in them.

A test in CI sends every request on this page, in order, to a gateway over
four member nodes with the quickstart's registry and patients, and checks
each status and each value the page says the answer holds
(`app/ferrofed-server/tests/it/book_tutorial.rs`).

<!-- toc -->

## 1. Start the quickstart

Download the latest release's quickstart into an empty directory, write the
gateway's signing key beside it, and start it:

```sh
curl -fsSLO https://github.com/FerroHEALTH/FerroFED/releases/latest/download/compose.yaml
mkdir -p secrets
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-384 -out secrets/signing-key.pem
chmod 644 secrets/signing-key.pem
docker compose up --wait
```

`--wait` returns once the gateway answers its readiness probe. The gateway
listens on `http://127.0.0.1:8080`, the base URL of every request below.
[The quickstart](../operate/container.md#the-quickstart) lists what each
service is.

## 2. Get an access token

Every request carries an access token. The quickstart's development issuer
signs one for whoever asks:

```sh
TOKEN="$(curl -s http://127.0.0.1:8190/quickstart/token -d grant_type=client_credentials \
  -d client_id=quickstart-client -d client_secret=any | jq -r .access_token)"
```

A production issuer checks who asks, and the gateway checks the token's
issuer, audience, scope and purpose of use
([Client authentication](../operate/authentication.md)). Send each request
below with `Authorization: Bearer $TOKEN`.

## 3. Read the self-description

`OPTIONS /` says what the gateway does and which members stand behind it
(Federation Tier §7a.2, N30):

<!-- book-request: options expect=200 holds=/federation/id=quickstart holds=/endpoints/0/id=node-a-query holds=/endpoints/3/id=node-d-query -->
```http
OPTIONS /
Authorization: Bearer $TOKEN
```

The answer names the federation, `quickstart`, and its four endpoints,
`node-a-query` to `node-d-query`, each with its node's `system_id`. Your
client can read the timeouts, the completeness policy and the paging limits
here before it sends a query
([Self-description](../integrate/client-contract.md#self-description)).

## 4. Query a patient every member holds

Send AQL as you would to one CDR. The patient `ffd-test-0001` has an EHR at
all four nodes:

<!-- book-request: everyone expect=200 holds=/meta/federation/complete=true rows=4 status=node-a-query:active status=node-d-query:active -->
```http
POST /v1/query/aql
Authorization: Bearer $TOKEN
Content-Type: application/json

{"q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
```

The gateway resolves the patient to the four `ehr_id`s at FerroPIX, sends
each node an ordinary AQL query scoped to its own `ehr_id`, and merges the
answers into one `RESULT_SET`: four rows, one composition from each node.
No node receives the patient identifier (§5.4.1, N33).

`meta.federation` is what a federated answer adds. `complete` is `true`,
and `endpoints` reports each member `active` with its `row_count`
([What a client gets back](../integrate/client-contract.md#what-a-client-gets-back)).

## 5. Query a patient some members do not hold

`ffd-test-0002` has an EHR at node B and node D alone:

<!-- book-request: some expect=200 holds=/meta/federation/complete=false rows=2 status=node-a-query:not-resolved status=node-b-query:active status=node-c-query:not-resolved status=node-d-query:active -->
```http
POST /v1/query/aql
Authorization: Bearer $TOKEN
Content-Type: application/json

{"q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0002' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
```

The answer is `200` with two rows. The gateway asks neither node A nor node
C, and reports both `not-resolved`; `complete` is `false`. A `not-resolved`
member is an answer and never fails the query (§11.3, N6, N37), so your
client reads `complete` and the statuses before it treats the rows as the
whole record.

A patient no member holds is a `200` with no rows:

<!-- book-request: nobody expect=200 rows=0 status=node-a-query:not-resolved status=node-d-query:not-resolved -->
```http
POST /v1/query/aql
Authorization: Bearer $TOKEN
Content-Type: application/json

{"q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0004' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
```

## 6. Direct a query at one member

Name a member in the query with the endpoint directive, and select its
attributes beside the data (§8, §9.4):

<!-- book-request: directed expect=200 rows=1 holds=/rows/0/0=node-c-query holds=/rows/0/1=node-c.quickstart.local status=node-a-query:excluded status=node-c-query:active -->
```http
POST /v1/query/aql
Authorization: Bearer $TOKEN
Content-Type: application/json

{"q": "SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition FROM ENDPOINT p [\"node-c-query\"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
```

Only node C is asked. The row carries the endpoint's id and `system_id`,
which the gateway fills in from its registry; no node is asked for them.
The other three members are `excluded`, and `complete` stays `true`,
because an excluded member was never in scope. The header
`openEHR-federation-endpoint` selects the same way without changing the
query ([Pinning a query to named systems](../integrate/client-contract.md#pinning-a-query-to-named-systems)).

## 7. Handle a refusal

A directive that names an endpoint the registry does not hold is refused
before any node is asked:

<!-- book-request: unknown-endpoint expect=400 holds=/code=endpoint-unknown -->
```http
POST /v1/query/aql
Authorization: Bearer $TOKEN
Content-Type: application/json

{"q": "SELECT c/uid/value FROM ENDPOINT p [\"node-x-query\"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = 'ffd-test-0001' AND e/ehr_status/subject/external_ref/namespace = 'urn:oid:2.999.1.1'"}
```

Every refusal is an `application/problem+json` body whose `code` your
client can branch on, and it never quotes your query
([Errors and status codes](../integrate/errors.md)).

## What next

- [Follow-ups](../integrate/follow-ups.md): read and write the composition a
  row names, routed to the member that owns it.
- [Stored queries](../integrate/stored-queries.md): run a query by name.
- [The patient summary over FHIR](../integrate/patient-summary.md): the same
  members, read in the European exchange format.
- [How FerroFED federates](../explanation/how-ferrofed-federates.md): the
  model behind each answer.

Stop the quickstart with `docker compose down`, and add `-v` to remove its
volumes.
