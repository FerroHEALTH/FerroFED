<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Withholding consent exclusions

A member the consent pre-filter excludes ([Consent](identity.md#consent)) is
reported `consent-denied` by default, as the specification requires (N27a).
`consent-denied` tells the requesting clinician, member by member, that the
patient restricted access there. Regulation (EU) 2025/327 (the European
Health Data Space) Art 8 forbids that: "The fact that a natural person has
restricted access ... shall not be visible to healthcare providers." A
deployment in the EU sets:

```toml
[federation.consent]
disclose = false    # the default is true: the specification's consent-denied (N27a)
```

With `disclose = false`, a member the pre-filter excludes is still never sent
a request, and the gateway asks its own cross-reference about it with the
other candidates, so its record matches theirs. The node learns nothing of
that lookup. Then:

- **A federated query** reports it as a member the cross-reference does not
  know the patient at: `not-resolved`, with the same `error` text that member
  carries in this deployment. The gateway resolves the excluded member with
  the others, so when the cross-reference cannot answer, it carries the same
  failure as every other member. `not-resolved` clears
  `meta.federation.complete` and fails nothing, as `consent-denied` does
  (§11.1, §11.3), so the answer never presents itself as whole while a
  member that may hold data was not asked (N16, N37). `not-localized` would
  leave `complete` true, which N37 requires of a member that was never in
  scope, so the gateway does not use it.
- **A read of an EHR by subject** that only an excluded member could serve
  answers `404 subject-unavailable`, the same answer as for a subject no
  member knows: in this deployment that code replaces `no-destination` for
  a subject with no EHR the gateway may reach. A `404` covers both, since
  HTTP defines it for a resource the server did not find or "is not willing
  to disclose" (RFC 9110 §15.5.5). The gateway never answers
  `consent-denied` there.
- **Nothing a client sees names consent:** no status, `error`, header or
  error body. `OPTIONS {base}/` declares the choice as
  `federation.consent.disclose: false`, so a client knows a `not-resolved`
  member may be one the patient restricted.
- **The operator still sees every exclusion:**
  `ferrofed_consent_prefilter_requests_total{outcome="denied"}` counts each
  call that excluded a member, and an outage of the consent service stays in
  `meta.federation.consent.error` and on `GET {base}/health/dependencies`,
  since it says nothing about a patient.

A node's own consent refusal, a `403` whose ITS-REST `Error` carries a code
the registry lists in `consent_refusal_codes`
([Consent](identity.md#consent)), is withheld the same way on every path:

- **A federated query** reports the node `not-resolved`, with the same
  `error` text as a member that does not know the patient and no
  `latency_ms`, so its record is identical to that member's. N40 asks for the
  latency of every node the gateway sent a request to; a record carrying one
  would show the refusal, so this deployment leaves it out, a conflict with
  the specification that is recorded on #212.
- **A read of an EHR by subject** whose holder refuses answers
  `404 subject-unavailable` and names no acting endpoint, as for a subject no
  member knows. The two can still differ in how long they take: a refusal
  costs one request to the holder, and a subject no member knows costs none.
- **A routed request under `{base}/v1/ehr/`, the creation of an EHR and a
  DEMOGRAPHIC request** that the node refuses answers
  `404 subject-unavailable`, still naming the endpoint the request was routed
  to. So that this answer never stands for a refusal alone, a node's own `404`
  on those paths gets the same gateway answer in this deployment, instead of
  passing through as §11.2 has it, and the node's body is never passed on.
  Both take one request to the node. The definition area holds no patient's
  data, so its answers pass through unchanged.
- **An ask-all probe** reads the refusing member as one that does not hold the
  `ehr_id`; when no member answers with the EHR, the answer is
  `404 subject-unavailable`, which in this deployment also replaces
  `no-destination` for an `ehr_id` no member holds. Every member is asked
  either way.

Every `subject-unavailable` answer carries the one message "the requested
resource is not available to this request (§11.2)".
- **The operator** still counts the refusal in
  `ferrofed_node_requests_total{outcome="consent-denied"}`, and a refusal on a
  routed path is logged with the endpoint and the request id.

Record the choice with your
[§13.4 deployment decisions](deployment-decisions.md).
