---
name: openehr-crates-are-the-model
description: "The whole published openehr-* family is the model, nothing redone: openehr-its (rest-server facade traits, rest-client node dispatch, canonical JSON), openehr-query (AQL parse and printer::to_aql), openehr-sdt (SMART on openEHR scopes), openehr-base and openehr-rm (typed ids for routing); a gap is a FerroEHR issue, never a local workaround; owner rulings 2026-10-01, after FerroBRIDGE's of 2026-09-25"
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroBRIDGE on 2026-09-25 the owner: "i see that we are doing some double
work regarding AM or RM or BASE. we have crates for that you know right?" An
audit there found model knowledge restated by hand and one live drift.

On 2026-10-01 the owner ruled the same for FerroFED from the start, in two
messages: the ITS-REST surface and AQL come from the published crates and
are never re-implemented, and then that the whole `openehr-*` family is the
model, nothing redone. FerroEHR publishes the family as one lockstep line at the pin in `docs/VERSIONS.md`; the gaps FerroFED raised, FerroEHR #3505 to #3514, closed in its 0.0.74 release of 2026-10-01:

- `openehr-its`: the client face is the generated axum server traits
  (`rest-server` feature), dispatch to each node is the generated client
  (`rest-client` feature), and wire bodies use its canonical JSON (`json`
  feature).
- `openehr-query`: AQL is parsed into its model and re-emitted with
  `printer::to_aql` for the subject-to-`ehr_id` rewrite and the identifier
  hygiene gate (§5.4, N33).
- `openehr-sdt`: the SMART on openEHR scope grammar (`smart_scopes`) for the
  §13 authentication and authorization handoff; the simplified formats and
  their validation if FerroFED ever needs them. It is not a dependency until
  client authentication (#80) first uses it.
- `openehr-base` and `openehr-rm`: the typed identifiers (`HIER_OBJECT_ID`,
  `OBJECT_VERSION_ID`, the `ehr_id`, `system_id` and `creating_system_id`
  forms) behind §12 and §12a follow-up routing, and every RM fact
  (`EHR_STATUS.subject`, `PARTY_IDENTIFIED`, `DV_IDENTIFIER`).

**Why:** the crates are generated from the BMM, the OpenAPI documents and the
published specifications; a local copy drifts silently, and for a gateway a
drifted copy means a rewritten query that no longer says what the client
asked, or an identifier that leaks past the hygiene gate.

**How to apply:** before a module touches openEHR data, a REST route, or an
AQL string, grep the crates for the item first. A gap (a missing public
reader, a printer that does not round-trip a construct, a route the server
traits lack) is filed as an issue in FerroEHR's tracker with its labels
([[sibling-projects]]) and recorded on the FerroFED issue that depends on it.
It is never worked around here: no local AQL parser, no hand-written route
table, no copied DTO. If the gateway cannot proceed without it, the FerroFED
issue is blocked by the FerroEHR one (`scripts/gh/rel.sh`).
