// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ENDPOINT attribute set of §9.3 (N12, CP-37).
//!
//! A query selects these through the variable of its `FROM ENDPOINT`
//! directive, and the Tier adds them to every row from the registry entry of
//! the endpoint the row came from. The rewrite of the `aql` feature names the attribute each such column
//! selects, and the gateway fills it in; the merge of the `merge` feature
//! carries the values of each endpoint beside its rows, so a `DISTINCT`
//! answer keeps rows of two endpoints apart when their attributes differ
//! (N13). It is plain data, so neither feature depends on the other.
//!
//! # Examples
//!
//! ```
//! use openehr_federation::attribute::EndpointAttribute;
//!
//! assert_eq!(Some(EndpointAttribute::EndpointId), EndpointAttribute::from_name("id"));
//! assert_eq!(Some(EndpointAttribute::SystemId), EndpointAttribute::from_name("system_id"));
//! assert_eq!(None, EndpointAttribute::from_name("name"));
//! assert_eq!("endpoint_id", EndpointAttribute::EndpointId.name());
//! ```

/// One selectable ENDPOINT attribute (§9.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EndpointAttribute {
    /// `endpoint_id`: the stable registry identifier of the endpoint (N19),
    /// the value of `meta.federation.endpoints[].id` (§9.5).
    EndpointId,
    /// `organisation` / `organization_id`: the endpoint's managing
    /// organisation (N20), the value of `meta.federation.endpoints[].organisation`.
    Organisation,
    /// `system_id`: the openEHR logical EHR-system id of the endpoint's node,
    /// the follow-up routing key (§12, N21).
    SystemId,
    /// `url`: the CDR base URL the registry holds for the endpoint.
    Url,
}

impl EndpointAttribute {
    /// Every attribute, in the order §9.3 lists them.
    pub const ALL: [Self; 4] = [
        Self::EndpointId,
        Self::Organisation,
        Self::SystemId,
        Self::Url,
    ];

    /// The attribute a path `<variable>/<name>` through the directive's
    /// variable selects, or `None` for a name §9.3 does not list.
    ///
    /// The §9.3 table names `endpoint_id`, `organisation` / `organization_id`,
    /// `system_id` and `url`, and §8.3 and §9.4 select the endpoint id as
    /// `p/id`, the name §9.5 gives the same value. Names compare as written,
    /// as every attribute name of an openEHR path does.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "id" | "endpoint_id" => Some(Self::EndpointId),
            "organisation" | "organization_id" => Some(Self::Organisation),
            "system_id" => Some(Self::SystemId),
            "url" => Some(Self::Url),
            _ => None,
        }
    }

    /// The attribute's name in the §9.3 table: `endpoint_id`,
    /// `organisation`, `system_id` or `url`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::EndpointId => "endpoint_id",
            Self::Organisation => "organisation",
            Self::SystemId => "system_id",
            Self::Url => "url",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EndpointAttribute;

    #[test]
    fn every_listed_name_selects_its_attribute_and_nothing_else_does() {
        for (name, attribute) in [
            ("id", EndpointAttribute::EndpointId),
            ("endpoint_id", EndpointAttribute::EndpointId),
            ("organisation", EndpointAttribute::Organisation),
            ("organization_id", EndpointAttribute::Organisation),
            ("system_id", EndpointAttribute::SystemId),
            ("url", EndpointAttribute::Url),
        ] {
            assert_eq!(
                Some(attribute),
                EndpointAttribute::from_name(name),
                "{name}"
            );
        }
        for name in ["", "ID", "Url", "organization", "name", "node_id", "uid"] {
            assert_eq!(None, EndpointAttribute::from_name(name), "{name}");
        }
    }

    #[test]
    fn each_attribute_is_named_by_its_table_name() {
        for attribute in EndpointAttribute::ALL {
            assert_eq!(
                Some(attribute),
                EndpointAttribute::from_name(attribute.name()),
                "{attribute:?}"
            );
        }
    }
}
