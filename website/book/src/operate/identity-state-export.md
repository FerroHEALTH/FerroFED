<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Exporting the identity state

When you replace FerroFED with another product, the routing state the
gateway learned while it served goes with you. Regulation (EU) 2025/327
Annex II 2.6 says the harmonised components of an EHR system "shall not
include features that prohibit, restrict or place an undue burden on
authorised exporting of personal electronic health data for the reasons of
replacing the EHR system by another product". The access records are
exported from the Audit Record Repository ([The audit trail](audit.md)).
This page covers the rest: the state the gateway learned and holds in
memory.

## What is exported and what is not

The export holds the `creating_system_id` mappings the gateway learned from
answers. A member's CDR can hold versions another system created, and the
gateway learns which member to send a follow-up read of such a version to
(§12.2, N21). The mapping is lost when the process stops, and a replacement
that has it routes those follow-ups from its first request.

The export leaves out:

| State | Why it is left out |
|---|---|
| The resolution bindings of each caller | They are scoped to one client session and expire after their time to live. A replacement asks the identifier cross-reference (the PIX Manager or the NVI) itself, which is the source of truth. A binding is pseudonymous personal data, and the export carries none. |
| The `ehr_id` index | It holds one entry per patient's EHR and is a hint: the next read that names no node asks every member and finds the owner again (§12.5.1). |
| The membership | The [registry document](registry.md) is the record, and you already hold it. |
| The stored queries | Their backend holds them, and `GET {base}/operator/stored-queries` reads them ([Client authentication](authentication.md#the-operator-surface)). |

The export therefore carries `system_id`s, `endpoint_id`s and `node_id`s
only, and no value a patient is known by: no patient identifier, no
`ehr_id`, no demographic (§5.4.1, N33).

## Running the export

Send the request to the [admin listener](metrics.md#who-the-admin-listener-serves)
with an access token that carries `auth.product_scope`:

```http
POST /admin/identity-state/export
Authorization: Bearer <an access token with the operator scope>
```

| Answer | When |
|---|---|
| `200` | The export, as JSON, with `Cache-Control: no-store`. |
| `400` (`body-invalid`) | The request carries a body. The export takes none. |
| `401`, `403` | The token is missing, does not verify, or lacks the operator scope. |
| `501` | The gateway runs without a registry document, so it learned nothing. |

The gateway records the export like every admin action: under
`ferrofed::security`, once when it begins and once when it ends, with your
issuer and subject, the route and the status ([Metrics](metrics.md#who-the-admin-listener-serves)). It
also logs how many mappings the export held, never a value.

Each replica learns its own mappings and shares them with no other replica
([Running several replicas](deployment-shape.md#running-several-replicas)). Export every replica,
one request to the admin listener of each, and keep every file. The files
of two replicas can disagree, because each saw different answers. Each
file names its replica in `replica`, so give each replica a name you can
tell apart (`[server] replica`, or the pod name Kubernetes gives it).

## The format

```json
{
  "format_version": 1,
  "federation_id": "example-federation",
  "exported_at": "2026-10-15T09:30:00Z",
  "replica": "ferrofed-7d9c4b5f6-x2k8q",
  "learned_creating_systems": [
    {
      "creating_system_id": "legacy-x.example.org",
      "status": "learned",
      "node": "node-a",
      "endpoint": "node-a-pub"
    },
    {
      "creating_system_id": "legacy-y.example.org",
      "status": "withdrawn",
      "node": null,
      "endpoint": null
    }
  ],
  "not_exported": [
    "resolution-bindings",
    "ehr-id-index",
    "development-cross-reference",
    "membership",
    "stored-queries"
  ]
}
```

| Field | Meaning |
|---|---|
| `format_version` | The version of this format, `1`. A change that removes a field or changes what a field means raises it. A reader refuses a version it does not know. |
| `federation_id` | The `[federation] id` the gateway declares in `OPTIONS {base}/` (§7a.2, N30). |
| `exported_at` | When the process took the export, as an RFC 3339 timestamp in UTC. |
| `replica` | The name of the process whose state the export holds: `[server] replica`, or the host name the process runs with, which Kubernetes sets to the pod's name, or `unnamed` without either ([Configuration](configuration.md)). Each file names the replica it came from. |
| `learned_creating_systems` | Every mapping the process learned, ordered by `creating_system_id`. It is empty when the process learned none. |
| `learned_creating_systems[].creating_system_id` | The `creating_system_id` of the versions, as their version uid spells it. |
| `learned_creating_systems[].status` | `learned`: the process saw versions of this `creating_system_id` at one member only and routes follow-up reads there. `withdrawn`: an integrity incident withdrew the mapping, because the versions were seen at two members or the registry document names another member, and the process routes nothing by it ([Integrity incidents](registry.md#integrity-incidents)). |
| `learned_creating_systems[].node` | The `node_id` of the member, from the registry document the process runs on. `null` for a withdrawn mapping. |
| `learned_creating_systems[].endpoint` | The `endpoint_id` the versions were seen at. `null` for a withdrawn mapping. |
| `not_exported` | The state the process holds that the export leaves out, as the table above lists it, so a reader of the file alone knows it is not there. |

The `node_id` and `endpoint_id` values are those of your registry
document, so keep the document of the same date with the export. A
member's own `system_id` and the document's `[[creating_system]]` mappings
are routes too; they are in the document, and the export does not repeat
them.

A mapping learned from answers shows where versions are held, never which
CDR controls them: the gateway routes reads by it and never a write
(§10.3, §12a.1, N23). A replacement that imports the file should read it
the same way.
