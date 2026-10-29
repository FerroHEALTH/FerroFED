// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's request groups and counters in the window of
//! `ferrohealth-usage-report`.
//!
//! A request is grouped by the route template it matched, never by its path,
//! and a probe is left out. The node requests are counted by their outcome
//! (§11.1). No specification governs the report: our own design.

use ferrohealth_usage_report::envelope::Key;
use ferrohealth_usage_report::window::Window;
use openehr_federation::status::EndpointStatus;

use crate::base_path::BasePath;
use ferrohealth_telemetry::request_log::UNMATCHED;

/// The counter of the requests the gateway sent to a member node.
pub const NODE_REQUESTS: &str = "node_requests";

/// The counter of the node requests that ran past their budget.
pub const NODE_TIMEOUTS: &str = "node_timeouts";

/// The counter of the node requests that failed or found the node offline.
pub const NODE_ERRORS: &str = "node_errors";

/// A request group of the report, by the route template a request matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// The composition routes under an EHR.
    Composition,
    /// The definition routes.
    Definition,
    /// The EHR routes, the compositions excepted.
    Ehr,
    /// The FHIR face.
    Fhir,
    /// The read-only operator surface.
    Operator,
    /// Every other route, and a request no route matched.
    Other,
    /// The federated and the stored queries.
    Query,
}

impl Group {
    /// Returns the group of a request from the route template it matched,
    /// under the deployment's `base` and the FHIR face's `fhir` base, or
    /// `None` for a probe the report does not count.
    ///
    /// The health routes and the JWK Set are probes. A template's parameter
    /// segments are matched by position and never read; [`UNMATCHED`] is
    /// `other`.
    #[must_use]
    pub fn of(base: &BasePath, fhir: Option<&str>, template: &str) -> Option<Self> {
        if template == UNMATCHED {
            return Some(Self::Other);
        }
        if let Some(fhir) = fhir
            && (template == fhir
                || template.starts_with(&format!("{}/", fhir.trim_end_matches('/'))))
        {
            return Some(Self::Fhir);
        }
        let relative = if base.is_root() {
            template
        } else {
            template.strip_prefix(base.as_str()).unwrap_or(template)
        };
        let segments: Vec<&str> = relative.split('/').filter(|s| !s.is_empty()).collect();
        let group = match segments.as_slice() {
            ["health" | ".well-known", ..] => return None,
            ["v1", "query", ..] => Self::Query,
            ["v1", "ehr", _, "composition", ..] => Self::Composition,
            ["v1", "ehr", ..] => Self::Ehr,
            ["v1", "definition", ..] => Self::Definition,
            ["operator", ..] => Self::Operator,
            _ => Self::Other,
        };
        Some(group)
    }

    /// The group's name in the report.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Composition => "composition",
            Self::Definition => "definition",
            Self::Ehr => "ehr",
            Self::Fhir => "fhir",
            Self::Operator => "operator",
            Self::Other => "other",
            Self::Query => "query",
        }
    }
}

/// The window of a gateway under `base` whose FHIR face, when served, sits
/// at `fhir`: each request grouped by [`Group::of`].
#[must_use]
pub fn window(base: BasePath, fhir: Option<String>) -> Window {
    Window::new(move |_method, template| {
        Group::of(&base, fhir.as_deref(), template.unwrap_or(UNMATCHED))
            .and_then(|group| Key::new(group.key()))
    })
}

/// Counts in `window` one request the gateway sent to a member node, which
/// ended as `status` (§11.1).
pub fn node(window: &Window, status: EndpointStatus) {
    let outcome = match status {
        EndpointStatus::TimeOut => Some(NODE_TIMEOUTS),
        EndpointStatus::NodeError | EndpointStatus::Offline => Some(NODE_ERRORS),
        _ => None,
    };
    for counter in std::iter::once(NODE_REQUESTS).chain(outcome) {
        if let Some(key) = Key::new(counter) {
            window.count(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use ferrohealth_usage_report::envelope::Key;
    use http::{Method, StatusCode};
    use openehr_federation::status::EndpointStatus;

    use super::{Group, node, window};
    use crate::base_path::BasePath;
    use ferrohealth_telemetry::request_log::UNMATCHED;

    fn group(base: &str, template: &str) -> Option<Group> {
        let base: BasePath = base.parse().expect("a base path");
        Group::of(&base, Some("/fhir"), template)
    }

    #[test]
    fn every_route_template_has_its_group() {
        for base in ["/", "/fed"] {
            let prefix = if base == "/" { "" } else { base };
            let cases = [
                ("/v1/query/aql", Group::Query),
                ("/v1/query/{qualified_query_name}", Group::Query),
                ("/v1/query/{qualified_query_name}/{version}", Group::Query),
                ("/v1/ehr", Group::Ehr),
                ("/v1/ehr/{ehr_id}", Group::Ehr),
                ("/v1/ehr/{ehr_id}/ehr_status", Group::Ehr),
                ("/v1/ehr/{ehr_id}/composition", Group::Composition),
                (
                    "/v1/ehr/{ehr_id}/composition/{uid_based_id}",
                    Group::Composition,
                ),
                ("/v1/definition/template/adl1.4", Group::Definition),
                ("/operator/incidents", Group::Operator),
                ("/", Group::Other),
            ];
            for (template, expected) in cases {
                let template = format!("{prefix}{template}");
                assert_eq!(group(base, &template), Some(expected), "{template}");
            }
        }
        assert_eq!(group("/", "/fhir/Patient/$summary"), Some(Group::Fhir));
        assert_eq!(group("/", "/fhir"), Some(Group::Fhir));
        assert_eq!(group("/", "/fhirx"), Some(Group::Other));
        assert_eq!(group("/", UNMATCHED), Some(Group::Other));
    }

    #[test]
    fn the_probes_are_not_counted() {
        for template in [
            "/health",
            "/health/liveness",
            "/health/readiness",
            "/.well-known/jwks.json",
        ] {
            assert_eq!(group("/", template), None, "{template}");
        }
        assert_eq!(group("/fed", "/fed/health/readiness"), None);
    }

    #[test]
    fn every_group_key_is_one_the_envelope_admits() {
        for group in [
            Group::Composition,
            Group::Definition,
            Group::Ehr,
            Group::Fhir,
            Group::Operator,
            Group::Other,
            Group::Query,
        ] {
            assert!(Key::new(group.key()).is_some(), "{group:?}");
        }
    }

    #[test]
    fn the_window_groups_by_template_and_counts_node_outcomes() {
        let window = window("/fed".parse().expect("a base path"), None);
        let elapsed = Duration::from_millis(3);
        window.record(
            &Method::POST,
            Some("/fed/v1/query/aql"),
            StatusCode::OK,
            elapsed,
        );
        window.record(
            &Method::GET,
            Some("/fed/health/readiness"),
            StatusCode::OK,
            elapsed,
        );
        window.record(&Method::GET, None, StatusCode::NOT_FOUND, elapsed);
        node(&window, EndpointStatus::Active);
        node(&window, EndpointStatus::TimeOut);
        node(&window, EndpointStatus::NodeError);
        let counts = window.take();
        let groups: Vec<&str> = counts.groups.keys().map(Key::as_str).collect();
        assert_eq!(vec!["other", "query"], groups, "the probe is left out");
        let counter = |name: &str| {
            counts
                .counters
                .iter()
                .find(|(key, _)| key.as_str() == name)
                .map_or(0, |(_, count)| *count)
        };
        assert_eq!(
            (3, 1, 1, 1),
            (
                counter("node_requests"),
                counter("node_timeouts"),
                counter("node_errors"),
                counter("responses_4xx"),
            )
        );
    }
}
