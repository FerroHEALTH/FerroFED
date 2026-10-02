<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-registry

The federation registry: members, endpoints and system ids from a reviewed document, the learned maps, integrity incidents and the stored-query definition store.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

It loads the federation's membership from a reviewed TOML bootstrap document
(organisations, nodes with their openEHR `system_id`, endpoints with their base
URL, connection type and one managing organisation) into an immutable
`RegistrySnapshot`, refusing any document with a dangling reference, a
duplicate id or `system_id`, or an endpoint without exactly one managing
organisation. Another reader can build the same `Document` from another form
(`ferrofed-identity` reads FHIR `Organization` and `Endpoint` resources into
it), and `RegistrySnapshot::from_document` holds it to the same rules.
`node_id`, `endpoint_id` and `system_id` are three types with no conversion
between them.

The snapshot is also the follow-up routing table: every `creating_system_id`
to the node that answers for it, from the members' own `system_id`s and the
document's `[[creating_system]]` mappings. A `LearnedMap` adds the ids learned
from answers, never overriding the document, and a conflicting sighting
raises an integrity incident and routes nothing. The design is recorded in
the repository's architecture document.

An application crate under `app/`: FerroFED's own glue, never published.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
