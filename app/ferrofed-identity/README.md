<!-- SPDX-FileCopyrightText: Vernum Projecten B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferrofed-identity

The identity roles of the Federation Tier as traits (resolver, localizer, directory, consent pre-filter, onward authentication) and the patient reference carrier, with no FHIR and no transport.

Part of [FerroFED](https://ferrofed.eu), a pure-Rust openEHR federation
gateway: a transparent ITS-REST intermediary that resolves the patient outside
the query, sends standard AQL to each node scoped to its own EHR id, and
merges what comes back with each node's provenance.

It holds `PatientRef`, the patient identifier as the gateway carries it
(redacted in every rendering and never serialized), the `Resolver` seam that
turns a patient into each member's local `ehr_id`, and the static development
cross-reference, a testing device accepted only under the development profile.
The remaining seams and the resolution step land with
[FerroFED issue #43](https://github.com/rubentalstra/FerroFED/issues/43); the
design is recorded in the repository's architecture document. The adapters
that plug the published `ihe-iti` and `nl-generic-functions` clients into these
seams live here too.

An application crate under `app/`: FerroFED's own glue, never published.

## Licence

Business Source License 1.1 (`LICENSE`): free for every non-production use and
for non-commercial production use; a commercial licence for other production
use; Apache License 2.0 four years after each version.
