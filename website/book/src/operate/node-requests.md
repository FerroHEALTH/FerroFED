<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What the gateway sends a node

Each request the gateway sends a node carries an id the gateway mints, and
headers the gateway composes or forwards by what the ITS-REST operation
declares. This page says where each comes from.

## Request ids

The gateway mints its own id, a fresh version 4 UUID, for every request. That
id is the `X-Request-Id` of every request the gateway sends to a node for it,
the same id on every node of one fan-out. The client's `x-request-id` never
reaches a node: it is free text, and the gateway cannot tell whether it names
a patient (§5.4.1, N33). When the client sends no id, the response carries the
gateway's id, so the client, the log and every node name the same request.
The outbound gate searches every other part of a node request for the
identifiers resolution consumed, and skips the minted id: it holds no client
input, and a short hexadecimal identifier can occur inside a random UUID.

Every other header the gateway sends to a node for a federated query is fixed
by the gateway: `Accept` and `Content-Type` (`application/json`),
`Authorization` (the endpoint's configured onward credential, when it has
one), `openEHR-federation-client` (the verified caller, signed for that node
with the `[signing]` key;
[What a node is told about the caller](caller-token.md)),
and the `Host`, `Content-Length` and `Accept-Encoding` the HTTP client
writes. None is copied from the client request. `Host` is the authority of
the endpoint URL in your registry, never a value from a request, so the
outbound gate does not search it, or the URL's host and port, for a
withheld identifier (§5.4.1, N33). It searches the path, the query string
and the other headers, except the minted id above, and every caller claim
of `openEHR-federation-client`.

A request routed to one node (`{base}/v1/ehr/{ehr_id}` and below) carries the
same `Authorization`, `openEHR-federation-client`, `X-Request-Id`, `Host`,
`Content-Length` and `Accept-Encoding`, and each request header the matched
ITS-REST operation declares, of `Accept`, `Content-Type`, `If-Match`, `Prefer`,
`openehr-version`, `openehr-audit-details`, `openehr-template-id`,
`openehr-item-tag` and `openehr-version-item-tag`, only those that operation
lists. The list comes from the `openehr-its` parameter table, never from the
gateway's own copy. `Accept`, `Content-Type` and `Prefer` are composed by the
gateway as values the operation lists; every other declared header is the
client's value byte for byte (see [Declared values](#declared-values)). Every
other client header is stripped, the client's `Authorization` and
`x-request-id` and the federation's own headers included, and the outbound
gate reads every forwarded value.

## Declared values

A routed request resolves no patient, so the outbound gate has no identifier
to compare a forwarded value against. The gateway works from what the
operation declares instead, before anything is sent.

It composes three headers itself, so a node receives a value the operation
lists, in the operation's own spelling, and never the client's text:

- `Accept` is read as a list of media ranges with weights (RFC 9110
  §12.5.1). Each listed media type takes the weight of the most specific
  range that covers it, and the node receives the heaviest one, the first
  listed on a tie. `*/*`, or no `Accept` at all, sends the first media type
  the operation lists; ITS-REST declares no default media type, so the first
  listed is the gateway's own choice, `application/json` in every EHR
  operation. A range with a parameter other than `q` or `charset=utf-8`
  covers nothing. An `Accept` that admits no listed type is a `406`
  (`media-type-not-acceptable`), as a node would answer it.
- `Content-Type` is read as a media type (RFC 9110 §8.3). When its type and
  subtype are a listed value, the node receives that value. A
  `charset=utf-8` is accepted and dropped, since the listed value carries no
  parameter and JSON is UTF-8 (RFC 8259 §8.1). Any other parameter, or a
  type that is not listed, is a `415` (`media-type-unsupported`). The listed
  values are the media types the operation's request body is declared in
  (`openehr-its`'s `request_media`), which are the values of its
  `Content-Type` parameter where it declares one; the versioned stored-query
  `PUT` declares no parameter and takes `text/plain`. A body never travels
  without a `Content-Type`: when the client sends none, the node receives
  the first media type the operation lists. ITS-REST makes `Content-Type`
  optional and declares no default, and RFC 9110 §8.3 names none, so the
  first listed is the gateway's own choice, as for `Accept`. Wherever an
  ITS-REST operation lists several, `application/json`, the canonical JSON,
  comes first.
- `Prefer` is read as a list of preferences (RFC 7240 §2). The node receives
  only the preferences the operation lists, in their listed spelling: a
  preference name is compared without regard to case, its value exactly, and
  only its first instance counts. Any other preference is dropped and never
  refused, as RFC 7240 allows a server to ignore it.

It holds every other value to what the operation declares, and a value that
does not match is a `400` (`parameter-value-invalid`) with nothing sent:

- a path identifier is the openEHR identifier class the `openehr-its` table
  states for it: a `version_uid`, and the `uid_based_id` of a delete, an
  `OBJECT_VERSION_ID`; any other text `uid_based_id` an `OBJECT_VERSION_ID`
  or a `HIER_OBJECT_ID`; and a path parameter the table states as a UUID,
  such as `versioned_object_uid` or the `uid_based_id` of a composition
  update, a UUID in its canonical hyphenated form, each parsed by
  `openehr-base` (a composition delete whose path names no
  `OBJECT_VERSION_ID` is `preceding-version-invalid` instead, since the path
  names the version it amends). The path then travels as
  the client sent it, since an openEHR uid is never rewritten (N22). The
  `ehr_id` is parsed by the routing itself;
- a date-time, such as `version_at_time`, is an extended ISO 8601 date-time
  in the openEHR BASE sense, with an offset only when needed (ITS-REST
  Overview, "Datetime format"), read by the `openehr-base` parser;
- an enumerated query value, such as `detail_level`, is exactly one of the
  values the operation lists;
- a UUID, an integer, a number and a boolean are each parsed as one.

The refusal names the header, or the path or query parameter by its position
and declared name, and never the value (§5.4.3). A value of such a kind
carries only what the kind admits: a four-digit year is a valid partial
date-time, and a `HIER_OBJECT_ID` admits a bare number as a one-arc ISO OID.

ITS-REST states the identifier class of a path parameter only in its
description; the `openehr-its` table carries it, and the gateway reads it
from there, never from a copy of its own.

The parameter table states no kind for the rest, so the gateway cannot
classify them and forwards them as the client sent them. In the EHR area they
are the headers `If-Match`, `openehr-audit-details`, `openehr-item-tag`,
`openehr-template-id`, `openehr-version` and `openehr-version-item-tag`, the
path parameter `key` of an item tag, and the query parameters `path`,
`tag_key`, `tag_value` and `tag_target_path`. The creation of an EHR adds
none beyond `openehr-version` and `openehr-audit-details`. The definition
area has the path parameters `qualified_query_name`, `template_id` and
`version`, and the query parameters `concept`, `query_type`, `template_id`
and `version`. The DEMOGRAPHIC area, where it is routed, has the same ones as
the EHR area except `path`.
N33 forbids an identifier in the parts of a request the gateway composes
(§5.4.1). Whether a client value the gateway forwards unchanged is one of
those parts is a question the specification leaves open, recorded on
[#212](https://github.com/FerroHEALTH/FerroFED/issues/212).
