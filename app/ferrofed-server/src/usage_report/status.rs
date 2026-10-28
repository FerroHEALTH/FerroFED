// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's `status` object of the report envelope v2.
//!
//! Every type here is closed: a member that is not declared cannot be
//! serialized, and a value is a variant name, never a string from a request
//! or the configuration. No specification governs the report: our own
//! design.

use ferrofed_identity::profile::Profile;
use serde::Serialize;

use crate::config::settings::Settings;

/// The member count, as a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MembersBucket {
    /// No member.
    #[serde(rename = "0")]
    None,
    /// One or two.
    #[serde(rename = "1-2")]
    UpTo2,
    /// Three to five.
    #[serde(rename = "3-5")]
    UpTo5,
    /// Six to ten.
    #[serde(rename = "6-10")]
    UpTo10,
    /// Eleven to fifty.
    #[serde(rename = "11-50")]
    UpTo50,
    /// More than fifty.
    #[serde(rename = "51+")]
    Above50,
}

impl MembersBucket {
    /// The range `members` falls in.
    #[must_use]
    pub const fn of(members: usize) -> Self {
        match members {
            0 => Self::None,
            1..=2 => Self::UpTo2,
            3..=5 => Self::UpTo5,
            6..=10 => Self::UpTo10,
            11..=50 => Self::UpTo50,
            _ => Self::Above50,
        }
    }
}

/// An identity, localization or consent service the gateway is configured
/// with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    /// The mCSD care services directory.
    Mcsd,
    /// The Mitz consent pre-filter.
    Mitz,
    /// The NVI localizer.
    Nvi,
    /// The PDQm demographics step.
    Pdqm,
    /// The PIXm resolver.
    Pixm,
    /// The PMIR identity feed.
    Pmir,
    /// The XCPD localizer.
    Xcpd,
}

/// The deployment profile, as the report names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileToken {
    /// `production`.
    Production,
    /// `development`.
    Development,
}

/// The gateway's `status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The deployment profile.
    pub profile: ProfileToken,
    /// The member count of the registry the gateway federates, as a range.
    pub members: MembersBucket,
    /// The configured services, unique and sorted.
    pub services: Vec<Service>,
    /// Whether the stored-query registry is offered.
    pub stored_queries: bool,
    /// Whether the FHIR face is served.
    pub fhir: bool,
}

/// What the configuration the gateway started with says of it, as the
/// report states it: the profile, the configured services, and whether the
/// stored-query registry and the FHIR face are offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The deployment profile.
    pub profile: ProfileToken,
    /// The identity, localization and consent services, unique and sorted.
    pub services: Vec<Service>,
    /// Whether the stored-query registry is offered.
    pub stored_queries: bool,
    /// Whether the FHIR face is served.
    pub fhir: bool,
}

impl Facts {
    /// Returns the facts `settings` state.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        let mut services = Vec::new();
        #[cfg(feature = "binding-ihe")]
        {
            let configured = [
                (settings.registry_directory.is_some(), Service::Mcsd),
                (settings.pdqm.is_some(), Service::Pdqm),
                (settings.pixm.is_some(), Service::Pixm),
                (settings.pmir.is_some(), Service::Pmir),
                (settings.xcpd.is_some(), Service::Xcpd),
            ];
            services.extend(
                configured
                    .into_iter()
                    .filter_map(|(on, service)| on.then_some(service)),
            );
        }
        #[cfg(feature = "binding-nl")]
        if let Some(nl_gf) = &settings.nl_gf {
            if nl_gf.mitz.is_some() {
                services.push(Service::Mitz);
            }
            if nl_gf.nvi.is_some() {
                services.push(Service::Nvi);
            }
        }
        services.sort_unstable();
        services.dedup();
        Self {
            profile: match settings.profile {
                Profile::Production => ProfileToken::Production,
                Profile::Development => ProfileToken::Development,
            },
            services,
            stored_queries: settings.stored_queries.is_some(),
            fhir: settings.fhir.is_some(),
        }
    }

    /// The status of a gateway with these facts that federates `members`
    /// members.
    #[must_use]
    pub fn status(&self, members: usize) -> Status {
        Status {
            profile: self.profile,
            members: MembersBucket::of(members),
            services: self.services.clone(),
            stored_queries: self.stored_queries,
            fhir: self.fhir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MembersBucket;

    #[test]
    fn the_member_ranges_are_closed_at_both_ends() {
        for (members, bucket) in [
            (0, MembersBucket::None),
            (1, MembersBucket::UpTo2),
            (2, MembersBucket::UpTo2),
            (3, MembersBucket::UpTo5),
            (10, MembersBucket::UpTo10),
            (50, MembersBucket::UpTo50),
            (51, MembersBucket::Above50),
        ] {
            assert_eq!(bucket, MembersBucket::of(members), "{members}");
        }
    }
}
