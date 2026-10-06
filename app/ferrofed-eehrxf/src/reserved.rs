// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The namespace the gateway reserves for its own stored queries.
//!
//! ITS-REST names a stored query `[{namespace}::]{query-name}`, the
//! namespace "in a form of a reverse domain name, which allows for separation
//! of use of stored queries by teams, companies, etc." (ITS-REST Query API,
//! "Qualified query name"). The gateway's own queries sit under
//! [`NAMESPACE`], the reverse of the product's domain, `ferrofed.eu`,
//! followed by the component. They are read-only and held at one immutable
//! [`VERSION`] (§12.7, N44): a `PUT` into the namespace is refused, and a
//! store that holds a definition there is refused when it is read, so a
//! deployment can never shadow one. No specification governs the
//! reservation: our own design.

use ferrofed_registry::definition::{QueryName, QueryVersion};
use jiff::Timestamp;

/// The reserved namespace, a reverse domain name under `ferrofed.eu`.
pub const NAMESPACE: &str = "eu.ferrofed.eehrxf";

/// The one version every reserved definition is held at.
pub const VERSION: QueryVersion = QueryVersion::new(1, 0, 0);

/// The ITS-REST `saved` instant of every reserved definition: the day its
/// version was fixed, 2026-10-06T00:00:00Z, so the answer does not change
/// from one start to the next.
pub const SAVED: Timestamp = Timestamp::constant(1_791_244_800, 0);

/// Whether `name` sits in the reserved namespace or one nested under it,
/// ASCII case ignored, so no spelling of it can be stored beside the
/// gateway's own.
#[must_use]
pub fn reserves(name: &QueryName) -> bool {
    name.namespace().is_some_and(|namespace| {
        namespace
            .to_ascii_lowercase()
            .strip_prefix(NAMESPACE)
            .is_some_and(|tail| tail.is_empty() || tail.starts_with('.'))
    })
}

#[cfg(test)]
mod tests {
    use ferrofed_registry::definition::QueryName;

    use super::reserves;

    fn name(text: &str) -> QueryName {
        QueryName::new(text).unwrap()
    }

    #[test]
    fn the_namespace_and_every_namespace_under_it_are_reserved_in_any_case() {
        for reserved in [
            "eu.ferrofed.eehrxf::patient-summary-problems",
            "eu.ferrofed.eehrxf::anything",
            "EU.FerroFED.EEHRXF::patient-summary-problems",
            "eu.ferrofed.eehrxf.v2::anything",
        ] {
            assert!(reserves(&name(reserved)), "{reserved}");
        }
    }

    #[test]
    fn a_neighbouring_or_absent_namespace_is_not_reserved() {
        for free in [
            "patient-summary-problems",
            "eu.ferrofed::patient-summary-problems",
            "eu.ferrofed.eehrxfx::patient-summary-problems",
            "org.example::eu.ferrofed.eehrxf",
        ] {
            assert!(!reserves(&name(free)), "{free}");
        }
    }
}
