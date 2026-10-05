<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Consent

A node checks consent itself, and a deployment may add a pre-filter that
drops members before dispatch. This page covers both, and the Dutch
pre-filter, Mitz.

Each node checks consent before it releases data, whatever the gateway did
first (N26, N27). The gateway never decides on release itself, and a node it
dispatches to is not thereby cleared: a localization or consent service that
named the node only means nothing upstream ruled it out (§14.3).

**A node's own refusal.** ITS-REST defines no consent signal, so the gateway
does not infer one from a status code. A node's answer is reported
`consent-denied` only when it is a `403` whose ITS-REST `Error` body carries a
`code` that the registry lists for that endpoint in `consent_refusal_codes`
([The registry](registry.md#the-registry-document)). Every other refusal is
`node-error`. The list is empty by default, so until you name the codes a
node uses, its consent refusal fails the query `424` as any node error does.
This key is FerroFED's own design, because no specification defines the
signal. A refusing node contributes no rows, never fails the query in either
completeness mode, clears `meta.federation.complete`, and its record carries
the `latency_ms` of the request it refused (§11.3, N40).

**The optional Step-1 pre-filter.** A deployment with a consent service may
drop members before dispatch (N27a). The pre-filter runs after localization
and before resolution. Each member it denies is reported `consent-denied` with
no `latency_ms`, is never resolved and never sent a request, and any `ehr_id`
the client session cached for it is dropped. A member it does not deny is
asked, and its node decides. When the consent service cannot answer, Step 1
carries no consent signal, which is the state of a deployment with no consent
service at all, so every candidate is asked and each node checks consent
itself (§13.2.1, N27a). This `pass-to-node` policy is FerroFED's own design.
The outage is never silent: the answer to a query carries it as
`meta.federation.consent.error`, mirroring `localization.error` of §14.1
([The client contract](../integrate/client-contract.md)), the pre-filter's
state on `GET {base}/health/dependencies` turns `down` or `failing`
([Health probes](health.md)), and each call is counted in
`ferrofed_consent_prefilter_requests_total` ([Metrics](metrics.md)). A call
the pre-filter could not put to its service at all, for the patient's
namespace or for missing caller claims, is counted as `not-asked` with that
reason and leaves the health state as it was, since the service saw
nothing. The client's answer is the one a pre-filter that found nothing
gives. The pre-filter applies to every patient route: a federated query and the read
of an EHR by subject
([Follow-ups](../integrate/follow-ups.md#reading-an-ehr-by-subject)).
`OPTIONS {base}/` declares a configured pre-filter under `federation.consent`,
with its mode, that policy and `disclose`; a deployment with no pre-filter
declares nothing there. A deployment under Regulation (EU) 2025/327 Art 8
sets `[federation.consent] disclose = false`, so an answer never shows an
exclusion ([Withholding consent exclusions](consent-exclusions.md)).

For development, rows under `[[dev.consent_denied]]` beside the
cross-reference are a static pre-filter, accepted only under
`profile = "development"` and declared as `development-static`:

```toml
[[dev.consent_denied]]
namespace = "urn:oid:2.999.1.1"
value = "ffd-test-0001"
member = "node-b"        # this patient's consent denies asking node-b
```

In the Netherlands the pre-filter is Mitz, below. Set `[[dev.consent_denied]]`
rows or `[nl_gf.mitz]`, never both: both refuse the configuration.

## Dutch consent: `[nl_gf.mitz]`

Mitz keeps the consent Dutch patients record and answers one closed
question about it, the *gesloten autorisatievraag* (Annex B §B.6): may this
data holder make this patient's data of these categories available to this
data user, for this purpose? The gateway asks it once per data holder among
the candidates, after localization and before resolution. A member whose
holder Mitz denies for every category asked is `consent-denied` and never
asked. Every other member is asked, and its node checks consent itself:
Mitz is a filter in front of the gate, not the gate (§14.3, N27).

The wire is the VZVZ *Implementatiehandleiding Open en gesloten
autorisatievraag* 3.8.2: a SOAP 1.2 request carrying one XACML 3.0
`XACMLAuthzDecisionQuery`, over mutual TLS, with an `X-Request-Id` on every
request. VZVZ states no licence for the document, so the repository pins it
by sha256 and does not ship it; `scripts/vendor/mitz.sh` fetches it for
reading.

```toml
profile = "production"

[nl_gf.mitz]
url = "https://mitz.example.org/geslotenautorisatievraag"
client_identity_file = "/run/secrets/mitz-client.pem"   # mutual TLS
trust_roots_file = "/etc/ferrofed/mitz-roots.pem"       # optional
credentials = { bearer_token_file = "/run/secrets/mitz-token" }  # optional
namespaces = ["urn:oid:2.999.1"]  # client namespaces that stand for the BSN
purpose = "TREAT"                 # TREAT or COC
data_categories = ["GGC002"]      # the Mitz data categories asked about
timeout_ms = 1000                 # one round of questions, within the query's budget

[nl_gf.mitz.holders]              # each member's care provider
"node-a" = { type = "V6" }        # the URA from [nl_gf.nvi.custodians] or the directory
"node-b" = { type = "V6", ura = "ura-test-0002" }

[[auth.issuer]]                   # the issuer of your callers' tokens
issuer = "https://issuer.example.org"
jwks_uri = "https://issuer.example.org/jwks"

[auth.issuer.requester]           # the claims of its tokens that name the requester
professional = "uzi_number"       # the professional's UZI number
role = "uzi_role"                 # the professional's UZI role code
organisation = "ura"              # the organisation's URA
organisation_type = "organisation_type"
```

`timeout_ms` bounds one round of questions, and it is a part of
`federation.overall_timeout_ms`: with `pdqm.timeout_ms` and the localizer's
budget it must end before the overall budget, or the configuration is
refused, naming the keys (§11.5). `OPTIONS {base}/` declares it as
`timeout.consent_ms`.

What a deployment must provide:

- **The BSN.** Mitz is asked by BSN. A client names the patient in a BSN
  system (`http://fhir.nl/fhir/NamingSystem/bsn`, or the BSN's OID as
  `urn:oid:2.16.840.1.113883.2.4.6.3` or dotted) or in one `namespaces`
  lists. A patient named by the pseudonymised BSN, as the NVI requires,
  cannot be asked about: the pre-filter then carries no consent signal and
  every candidate is asked, and the call is counted as `not-asked` with the
  reason `namespace` ([Metrics](metrics.md)). The pseudonym's system is never accepted in
  `namespaces`. The BSN reaches Mitz and nothing else: never a node, a log
  line or an error.
- **A holder per member.** Every registry member needs a `type`, and one
  URA from its `ura`, `[nl_gf.nvi.custodians]` or a directory that
  publishes URAs; where more than one gives it they must agree. A member
  with no holder, a holder with no URA, or a holder naming no member
  refuses the configuration.
- **The requester, in the caller's token.** The question names the
  professional who asks, by UZI number and role, and their organisation, by
  URA and type. That is always the verified caller: Mitz records the
  professional and decides on their role, so the gateway never asks for
  anyone else. Map, per trusted issuer, the four token claims that carry
  them under `[auth.issuer.requester]`
  ([Client authentication](authentication.md#configuration)); no
  specification the gateway binds names these claims, so each is configured
  and none has a default. A caller whose token does not carry all four is
  not asked about: Mitz is not called, no member is filtered, and each node
  checks consent itself (N27). The call is counted as `not-asked` with the
  reason `caller-claims`. A token that carries them in a form the question
  does not take, such as a UZI number that is not alphanumeric, is treated
  the same way, with the reason `caller-claims-invalid`, and so is a patient
  value the question does not take as a BSN, with `patient-value`. Neither
  is reported as a Mitz outage, since Mitz is never called, and no claim
  value appears in a label, a log line or an error.
- **TLS.** The `url` must be `https` outside `profile = "development"`.
  `credentials` takes a bearer token or basic credentials, never an OAuth
  2.0 grant. Whether a gateway may ask Mitz at all is a matter of admission
  to the Mitz afsprakenstelsel.

Mitz answers `Permit` or `Deny` per category. `Indeterminate`, a fault, a
status other than `200`, silence past `timeout_ms` and an answer that does
not hold to the question are no decision: the members of that holder are
asked, and the failure is carried in `meta.federation.consent.error`. When
Mitz denies one holder and fails for another, the denied members are
`consent-denied`, the others are asked, and the failure is still carried.
`OPTIONS {base}/` declares the pre-filter as `"nl-gf-mitz"`.

