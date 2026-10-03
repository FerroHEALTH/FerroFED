<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Templates, definitions and demographics

A request under the definition area or the DEMOGRAPHIC area goes to the one
node you name. The one exception is a template upload, which a deployment may
offer to fan out to several members.

## Templates and definitions

A template lives at the node it was uploaded to, and a `COMPOSITION` built on
it validates only there. So every request under `{base}/v1/definition/` (an
ADL 1.4 or ADL 2 template upload, the template list, one template, its
example composition, and stored-query management where the gateway holds no
registry) goes to the one endpoint you name in `openEHR-federation-endpoint`
or `openEHR-federation-organisation` (§7a.1, §12.6, N43). The gateway never
picks a node for you and never probes for one:

- Without a header the request is a `400` (`target-required`).
- Headers that select several endpoints are a `400` (`endpoint-several`), and
  `*` is no endpoint the registry knows, so it is a `400`
  (`endpoint-unknown`) like any other unknown id. A template upload is the
  one exception, and only where the deployment offers
  [fan-out template upload](#fan-out-template-upload).
- The body reaches that node byte for byte, and only the headers and query
  parameters the ITS-REST operation declares travel with it (§5.4.1, N33).
- The answer is that node's answer, its status, body, `Location` and `ETag`
  as the node sent them, with `openEHR-federation-endpoint` and
  `openEHR-federation-system-id` naming who acted (§7a.3, N31). A template
  list is one node's list: the gateway never combines two nodes' templates
  into one catalogue (§12.6).
- A node's own error comes back as the node sent it. A `404` for a template
  the node does not hold, or a `400` for a template it rejects, is never
  masked or rewritten (§12.6, §11.2).

Without the registry, both stored-query `PUT`s are routed, the versioned
`PUT {base}/v1/definition/query/{name}/{version}` included, and `OPTIONS`
lists `PUT` on both paths. The query text reaches the node byte for byte,
always as `text/plain`, the media type ITS-REST declares for it: send that
`Content-Type`, or none and the gateway sets it.

A malformed value of a header or query parameter the operation declares is
refused before the target is read: a `400` (`parameter-value-invalid`), or
the `406` or `415` a node would answer an `Accept` or a `Content-Type` it
cannot serve, never `target-required`.

Where the gateway offers the stored-query registry, the registry answers every
request under `{base}/v1/definition/query/` itself, with or without a header,
and templates still go to the one node you name ([stored
queries](stored-queries.md), §7a.2).

### Fan-out template upload

A deployment may offer one template upload applied to several members, so
that a later `COMPOSITION` commit validates wherever it lands (§12.6, N43).
It is off by default, and `OPTIONS {base}/` declares it as
`definition.fan_out_template_upload` (§7a.2). Where it is offered:

- Only an ADL 1.4 or ADL 2 template upload
  (`POST {base}/v1/definition/template/adl1.4` or `.../adl2`) fans out, and
  only when you ask for it: `openEHR-federation-endpoint: *` names every
  active member, and a header that selects several endpoints names those.
  An upload that names one endpoint goes to that node alone and comes back
  as that node answered; an upload that names none is still a `400`
  (`target-required`). Every other definition request routes to one node,
  and `*` stays `endpoint-unknown` there. A list naming a suspended
  endpoint, or `*` with no active member, is a `404` (`no-destination`) and
  nothing is sent; `*` leaves a suspended member out and reports it
  `excluded`.
- Each member is sent the upload on its own, the body byte for byte, with
  only the headers the operation declares. A member that accepts keeps the
  template whatever the others answer: nothing is rolled back.
- The answer is a JSON body holding `meta.federation`, in the shape of a
  federated result set's (§9.5): `complete`, the `timeout` in force, and one
  `endpoints[]` entry per registry member. A member that accepted is
  `active`, one that failed is `node-error` with the node's HTTP status and
  an excerpt of its message in `error` (see
  [A node's error in `endpoints[]`](client-contract.md#a-nodes-error-in-endpoints)), or
  `time-out` or `offline`, and one you did not name is `excluded`. No other
  part of a node's body, and no `Location`, is copied into it.
- The status is `200` when every member you named accepted, and `207` with
  `complete: false` when some accepted and others failed: a partial success
  is never reported as success (§12.6). When none accepted, the status is
  `504` if a member timed out or could not be reached, and `424` otherwise
  (§11.2).
- `openEHR-federation-endpoint` and `openEHR-federation-system-id` list the
  members that accepted, comma-separated in registry order (§7a.3, N31).

```http
POST {base}/v1/definition/template/adl1.4
openEHR-federation-endpoint: *
Content-Type: application/xml

<template xmlns="http://schemas.openehr.org/v1">…</template>
```

## Demographics

The federation keeps demographics outside the CDRs: the gateway resolves the
patient through its identity binding, and never federates the openEHR
DEMOGRAPHIC API (§5.1, §7a.1, N32). Read `its_rest.demographic` in
`OPTIONS {base}/` to see which of two behaviours a gateway runs:

- `unsupported: 501`, the default. Every request under
  `{base}/v1/demographic/` answers `501` (`not-implemented`), and no node is
  asked.
- `routed-single-node`, naming one endpoint. The deployment keeps its
  demographics at that member, and you name that endpoint in
  `openEHR-federation-endpoint` on every DEMOGRAPHIC request, as you name the
  node of a definition request: the gateway never picks the node for you
  (§7a.1, §12.4, §12.6, N23). The request then goes there alone: the body
  byte for byte, only the headers and query parameters the operation
  declares, and the node's answer as it sent it, with
  `openEHR-federation-endpoint` and `openEHR-federation-system-id` naming who
  acted (§7a.3, N31). Without the header the request is a `400`
  (`target-required`) and no node is asked; a header naming another endpoint
  is a `400` (`targeting-conflict`) naming both, and
  several endpoints, `*` or an unknown id are refused as on any routed
  request (`endpoint-several`, `endpoint-unknown`). Nothing is fanned out or
  merged.
