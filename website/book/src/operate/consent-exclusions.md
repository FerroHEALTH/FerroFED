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
a request, and:

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

The setting covers the pre-filter only. A node that refuses on consent
grounds with a code the registry lists in `consent_refusal_codes` is still
reported `consent-denied`, so the setting does not change what a node's
own refusal shows. Record the choice with your
[§13.4 deployment decisions](deployment-decisions.md).
