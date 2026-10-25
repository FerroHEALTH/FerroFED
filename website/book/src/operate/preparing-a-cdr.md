<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Preparing each CDR

Each member CDR meets the conditions below before
[A production deployment](production.md) admits it, as step 1 of that guide
asks.

Agree each of these with the member's operator before you admit the
member. The first five are the conditions of membership (§12b.1, §12b.2,
N42a); the rest are what the gateway needs to reach the member.

- [ ] **A unique `system_id`.** The CDR stamps one openEHR `system_id` into
  every EHR and version it creates, and no other member uses it (§12b.2).
  Find it by asking the CDR itself, at the member, as the member's
  operator:

  ```sql
  SELECT DISTINCT e/system_id/value FROM EHR e
  ```

  Expect one value. If the CDR reports several, ask its operator which one
  it stamps into new EHRs, and map any other system whose versions it holds
  with a `[[creating_system]]` entry ([The registry](registry.md#the-registry-document)).
- [ ] **`ehr_id`s are version-4 UUIDs**, or come from a scheme with the same
  collision resistance (§12b.2).
- [ ] **No `ehr_id` is reused**, across restores, migrations and test-data
  resets (§12b.2). The evidence is the member's documented procedures.
- [ ] **No foreign `ehr_id` is adopted.** An imported EHR gets a fresh
  `ehr_id` (§12b.2).
- [ ] **Each `ehr_id` reaches the PIX Manager** in the member's `ehr_id`
  domain, with the patient identifier your clients use, from the moment the
  EHR is created, and the EHRs that exist today in a bulk load (§5.5;
  [step 6](production.md#6-identity-resolution)).
- [ ] **ITS-REST is reachable from the gateway** at one base URL per
  endpoint, over `https` whenever the gateway sends it a credential
  ([What must travel encrypted](configuration.md#what-must-travel-encrypted)).
- [ ] **The member accepts the gateway's credential**: a user and a
  password, or an OAuth 2.0 client registered under the gateway's
  `client_id` for the client credentials grant
  ([step 5](production.md#5-onward-credentials)).
- [ ] **The member makes its own access decision and enforces consent**
  (§13.2, N26, N27). If it marks a consent refusal with an ITS-REST `Error`
  code, write the code down for the registry's `consent_refusal_codes`
  ([Consent](consent.md)).
- [ ] **The member verifies who asks**, recommended: it reads the
  `openEHR-federation-client` token the gateway signs, with the gateway's
  JWK Set, and records the caller in its own audit trail
  ([Verifying it at the node](caller-token.md#verifying-it-at-the-node)).
