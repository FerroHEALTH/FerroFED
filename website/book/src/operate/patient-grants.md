<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Patient grants

A patient-facing app holds a token whose `patient/` scopes are confined to
one patient, named by the token's `ehrId` claim (SMART on openEHR master04
§Capabilities, master07 §Context Selection). That `ehrId` is an `ehr_id` at
the platform that issued the token, and at no other member. You opt in per
issuer by naming that platform's member and the identifier system under
which your cross-reference service knows its `ehr_id`s:

```toml
[[auth.issuer]]
issuer = "https://patient-portal.example.org"
kind = "patients"
audience = ["https://gateway.example.org/fed"]

[[auth.attributes]]
issuer = "https://patient-portal.example.org"
mapping = "iua"
authorization_details_purposes = true

[[trust.issuer]]
issuer = "https://patient-portal.example.org"

[trust.issuer.patient]
endpoint = "node-a-pub"
ehr_id_system = "urn:oid:2.999.9.1"
```

The issuer is a `patients` issuer: `ferrohealth-smart` admits its token only
when the token carries `patient/` scopes and no `user/` or `system/` scope,
so a professional of the same platform signs in through an issuer of its
own ([Who the caller is](authentication.md#who-the-caller-is)). Its
`[[auth.attributes]]` table reads the purpose of use each token declares,
the patient's and a representative's alike; it names no field but
`purpose_of_use`, since a `patients` token states no professional. Without
the table, every caller of the issuer is refused
`purpose-of-use-required`.

`config check` and the start refuse a binding whose endpoint the registry
lacks, a binding without a registry, and a binding without a cross-reference
service (`[pixm]`), each naming its key. Without the section, a
`patient/` grant of that issuer admits nothing, as above.

With the opt-in, a `patient/` scope of that issuer counts on an EHR's data
alone: the `composition-` family, and an `aql-` search. When only
`patient/` scopes cover an operation, the gateway confines the request to
the token's patient:

1. It reads the `ehrId` claim from the access token, or from the
   introspection answer. A token without one, or with one that is no
   `HIER_OBJECT_ID`, is `403` (`patient-context-missing`).
2. It resolves the pair (`ehr_id_system`, `ehrId`) through the
   cross-reference service at every member (§5.2), giving the patient's own
   `{node, ehr_id}` pairs. The bound member keeps the token's `ehrId`. A
   cross-reference that cannot answer, or that places the patient under
   another `ehr_id` at the bound member, is `424`
   (`patient-context-unavailable`).
3. It admits the request only when every `{node, ehr_id}` pair it would
   send to is one of those. Everything else is `403`
   (`patient-confinement`) with nothing sent: a query for another patient,
   a query that names no patient, an `ehr_id` that is not the patient's at
   the member it would go to, a read by subject of another patient, the
   creation of an EHR and a definition request. An `ehr_id` the patient's
   own pairs do not place is never looked up in the gateway's index or
   probed for at the members, and a refused read by subject leaves no
   resolution binding for the caller and no entry in the index.
4. A query must read the patient's EHR alone: every class in its `FROM`
   is contained, conjunctively, under the one `EHR` it is scoped to
   (`FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o`). A class
   beside that `EHR` under `AND` or `OR`, a second `EHR`, or an `EHR`
   under `NOT CONTAINS` would let a node answer other patients' rows, so
   such a query is `403` (`patient-confinement`) before anything is looked
   up (§7.1).
5. A query or a read by subject that names a patient is first resolved
   at the bound member alone. When the patient there is not the token's
   own `ehrId`, the request is `403` (`patient-confinement`) before any
   localizer (XCPD or the NVI), consent pre-filter or other member is
   asked about it, so no ITI-55 exchange or audit event is made for that
   patient. A patient summary on the FHIR face is held the same way
   before the PDQm Supplier is asked for its header, so no ITI-78
   exchange is made for another patient either.

An electronic health data access service (Regulation (EU) 2025/327 Art 3(1)
and Art 4) reads a person's own patient summary with such a token: the
scope `patient/aql-eu.ferrofed.eehrxf::*.s` covers the summary's section
queries and nothing else, and the token names the person as its `sub`.
The summary comes from every member the cross-reference places the person
at, and the access record names the person as the caller and as the data
subject ([The patient summary over FHIR](../integrate/patient-summary.md#authentication-and-the-access-log)).

A person inserts information in their own EHR (Art 5) with such a token
too. A document the FHIR face receives under a token whose `sub` names the
person, with a `patient/` scope that creates compositions
(`patient/composition-*.c`), is written into the person's own EHR at the
receiving member, every entry naming the record's subject as its provider,
and its access record says the person inserted it ([Inserted by the
person](../evaluate/receiving-documents.md#inserted-by-the-person)).

A proxy or a legal representative (Art 4(2)) who inserts for the person is
told apart only when the token says so. RFC 8693 §4.1 names any delegate
in the `act` claim, a client service as well as a person, so set
`act_represents_patient = true` on the `[[auth.issuer]]` of `kind =
"patients"` only for an issuer whose patient tokens name the
representative there; on any other issuer, a patient token that carries
`act` is refused as an invalid token. Every entry of
a document such a token inserts then names the representative, a
`PARTY_IDENTIFIED` holding the `act` `sub`, and its access record says a
representative inserted it ([Inserted by the
person](../evaluate/receiving-documents.md#inserted-by-the-person)). An
issuer that declares nothing cannot say a representative acts, so what its
tokens insert is marked as the person's own.

Over ITS-REST the gateway routes a person's write as it routes any
caller's. Under a patient grant whose scopes cover a composition update or
delete, it sends the request to the member that owns the composition,
confined to the person's own EHR as above, with the body unchanged (§5.4).
It does not mark such a write as the person's, and it cannot tell a
person's entry from a professional's: only the member, which records the
committer of each version and is told in the caller token that a person
acts (`acting` is `person`), can stop a person changing an entry a health
professional wrote, as Art 5 asks.

A token whose resource scopes are all `patient/` never reaches the
DEMOGRAPHIC API, whether or not its issuer is bound and even when its client
is listed in `demographic_clients`: a patient scope reaches its patient's
own EHR alone (master08 §Resource Scopes). The gate refuses it `403`
(`patient-confinement`) before anything is resolved.

The gateway never compares the bare `ehrId` with an `ehr_id` at another
member, because one `ehr_id` can name another patient's EHR there (§12.5.2).
Each node is told the patient's own `ehr_id` at that node in the `ehrId`
claim of the caller's token, with only the `patient/` scopes that cover the
operation in `scope`, so the node can enforce the grant as well (N26;
[below](authentication.md#what-a-node-is-told-about-the-caller)). The outbound gate holds
that claim to the `ehr_id` the request to that node is composed for: the
one its query is scoped to, or the one its path names. A request whose
token would tell the node another `ehr_id`, or that names none, is never
sent (§5.4.1, N33). No specification defines a patient grant across nodes,
so this opt-in is FerroFED's own design.

The residual risk is the cross-reference service. The confinement is only
as correct as its link between the `ehrId` at the bound member and the
patient's `ehr_id` at every other member: a wrong link admits the wrong
EHR at that member. Bind an issuer only to the member whose platform issued
its tokens, and only when your cross-reference service holds that member's
`ehr_id`s as identifiers under `ehr_id_system`. Record the choice in your
[§13.4 decisions](deployment-decisions.md#5-what-the-technique-does-not-cover).
