// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[usage_report]` section, as written and resolved: the switch, the
//! collector, the state directory and the deployment kind, plus the CI
//! detection read from the process environment.
//!
//! ```toml
//! [usage_report]
//! enabled = true
//! state_dir = "/var/lib/ferrofed/usage-report"
//! deployment = "compose"
//! ```
//!
//! No specification governs this: our own design.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::error::Error;

/// The collector every report goes to unless the deployment names another
/// (`FerroPULSE` `docs/specs/ferropulse/docs/report-v2.md` §Transport).
pub const ENDPOINT: &str = "https://report.ferropulse.eu/v1/report";

/// The process environment variables that mark a CI runner, read in this
/// order.
pub const CI_VARIABLES: [&str; 2] = ["GITHUB_ACTIONS", "CI"];

/// The key of the section.
pub const KEY: &str = "usage_report";

/// How the gateway was deployed, as the report states it.
///
/// The deployment artifact sets it: the compose file says `compose`. It is
/// never detected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Deployment {
    /// A Helm chart.
    Helm,
    /// A Docker Compose file.
    Compose,
    /// The plain binary.
    Binary,
    /// Not stated.
    #[default]
    Unknown,
}

/// `[usage_report]` as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageReport {
    /// Whether the report is sent; on by default.
    pub enabled: bool,
    /// The collector the reports are posted to.
    pub endpoint: String,
    /// The writable directory the instance id lives in; without one the
    /// report stays off.
    pub state_dir: Option<PathBuf>,
    /// How the gateway was deployed.
    pub deployment: Deployment,
}

impl Default for UsageReport {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: String::from(ENDPOINT),
            state_dir: None,
            deployment: Deployment::Unknown,
        }
    }
}

/// The usage report configuration of one process, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageReportSettings {
    /// Whether the deployment left the report on.
    pub enabled: bool,
    /// The collector endpoint: `https`, or `http` to a loopback host, and no
    /// credentials.
    pub endpoint: url::Url,
    /// The writable directory the instance id lives in; `None` keeps the
    /// report off.
    pub state_dir: Option<PathBuf>,
    /// How the gateway was deployed.
    pub deployment: Deployment,
}

/// Resolves the `[usage_report]` section.
///
/// # Errors
/// Returns [`Error::Url`] for an endpoint that does not parse and
/// [`Error::ReportEndpoint`] for one that carries credentials or is plain
/// `http` to a host other than a loopback one.
pub fn resolve(section: &UsageReport) -> Result<UsageReportSettings, Error> {
    let key = || format!("{KEY}.endpoint");
    let endpoint =
        url::Url::parse(&section.endpoint).map_err(|source| Error::Url { key: key(), source })?;
    if !admitted(&endpoint) {
        return Err(Error::ReportEndpoint { key: key() });
    }
    Ok(UsageReportSettings {
        enabled: section.enabled,
        endpoint,
        state_dir: section.state_dir.clone(),
        deployment: section.deployment,
    })
}

/// Whether a report may be posted to `url`: `https`, or plain `http` to a
/// loopback host, and no credentials in the authority.
///
/// `FerroPULSE` `docs/specs/ferropulse/docs/report-v2.md` §Transport has a client use https except
/// to a loopback host.
fn admitted(url: &url::Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.scheme() {
        "https" => url.host().is_some(),
        "http" => match url.host() {
            Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        },
        _ => false,
    }
}

/// Returns the first CI variable `lookup` answers `true` or `1` for, in any
/// case.
///
/// A CI runner sets one of them for every step it runs, so a gateway it
/// starts would otherwise count as an installation.
#[must_use]
pub fn ci_of(lookup: impl Fn(&str) -> Option<String>) -> Option<&'static str> {
    CI_VARIABLES.into_iter().find(|name| {
        lookup(name).is_some_and(|value| {
            let value = value.trim();
            value.eq_ignore_ascii_case("true") || value == "1"
        })
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{admitted, ci_of};

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn ci_is_detected_from_either_variable_and_true_or_one_only() {
        assert_eq!(ci_of(lookup(&[])), None);
        assert_eq!(
            ci_of(lookup(&[("GITHUB_ACTIONS", "TRUE")])),
            Some("GITHUB_ACTIONS")
        );
        assert_eq!(ci_of(lookup(&[("CI", "1")])), Some("CI"));
        assert_eq!(
            ci_of(lookup(&[("CI", "true"), ("GITHUB_ACTIONS", "true")])),
            Some("GITHUB_ACTIONS"),
            "GITHUB_ACTIONS is read first"
        );
        for no in ["false", "0", "", "yes"] {
            assert_eq!(ci_of(lookup(&[("CI", no), ("GITHUB_ACTIONS", no)])), None);
        }
    }

    #[test]
    fn only_https_or_loopback_http_without_credentials_is_admitted() {
        for (url, expected) in [
            ("https://report.ferropulse.eu/v1/report", true),
            ("http://127.0.0.1:9/v1/report", true),
            ("http://[::1]:9/v1/report", true),
            ("http://localhost:9/v1/report", true),
            ("http://report.example/v1/report", false),
            ("https://user:pw@report.example/v1/report", false),
            ("ftp://report.example/", false),
        ] {
            let parsed = url::Url::parse(url).expect("a URL");
            assert_eq!(admitted(&parsed), expected, "{url}");
        }
    }
}
