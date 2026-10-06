<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-eehrxf

The federation half of the European interoperability software component: the stored queries that select each patient summary section's compositions from the members.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

The component itself, `eehrxf`, holds the dataset, its crosswalk and the
mapping, and links nothing of FerroFED. This crate holds what the gateway
adds to it: one stored query per patient summary section, held read-only
under the reserved namespace `eu.ferrofed.eehrxf` at version `1.0.0`. Each
query selects the whole compositions that contain one of the archetypes the
openEHR International Patient Summary template names for that section, with
their uid and template id, and names the patient through `$patient` and
`$namespace` alone, so the gateway's rewrite scopes it to each member's own
`ehr_id` and no patient identifier reaches a node.

Every query is built as an AQL syntax tree with `openehr-query` and printed
with its printer. A test holds each section's archetypes to the vendored
template and each crosswalk section to a query or a recorded reason.
