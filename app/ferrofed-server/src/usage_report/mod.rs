// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What FerroFED puts into the installation report of
//! `ferrohealth-usage-report`.
//!
//! The crate carries `FerroPULSE`'s report envelope v2, the reporter with
//! its one `start` report at each `serve` start and one `daily` report a
//! day, the instance id under the state directory, and the window. FerroFED
//! adds its `status` ([`status::Status`]: the profile, the member count as a
//! range, the configured identity and consent services, and whether the
//! stored-query registry and the FHIR face are offered), its request groups
//! by matched route template, and the node request counts
//! ([`group`]). A report carries no patient data, no AQL text, no request
//! path, no node URL or name, and no identifier other than the instance id.
//! It is on by default; `[usage_report] enabled = false` stops it, a CI
//! environment stops it, and so does the lack of a writable state
//! directory, which one boot line says. No specification governs any of
//! this: our own design.

pub mod group;
pub mod status;

use std::sync::Arc;
use std::time::Instant;

use ferrohealth_usage_report::envelope::{LicenceGrant, Product, report_git_sha};
use ferrohealth_usage_report::reporter::{Identity, Notice, ProductSection};
use ferrohealth_usage_report::section::{UsageReportSection, ci_of};
use tokio::task::JoinHandle;

use crate::config::settings::Settings;
use crate::state::AppState;
use status::{Facts, Status};

/// What the boot line says of the report.
pub const NOTICE: Notice = Notice {
    contents: "instance id, version, licence type, deployment, uptime, host size, member count \
               as a range, the configured identity services, coarse request latency per route \
               group and node request counts",
    switch: "FERROFED__USAGE_REPORT__ENABLED=false",
    state_dir_variable: "FERROFED__USAGE_REPORT__STATE_DIR",
    url: "https://ferrofed.eu/docs/operate/usage-report.html",
};

/// The facts about this build a report states.
///
/// The gateway installs no commercial licence, so it runs under what the
/// Business Source License grants everyone, the non-commercial grant.
#[must_use]
pub fn identity() -> Identity {
    let build = crate::report::provenance::Build::current();
    Identity {
        version: build.version.to_owned(),
        git_sha: report_git_sha(build.commit.unwrap_or_default()),
        licence: LicenceGrant::NonCommercial,
    }
}

/// The gateway as the reporter asks it for its `status`: the facts of the
/// configuration it started with, and the members of the registry it
/// federates now.
#[derive(Debug, Clone)]
pub struct Fed {
    facts: Facts,
    state: Arc<AppState>,
}

impl Fed {
    /// The gateway serving `state`, started with the configuration whose
    /// facts are `facts`.
    #[must_use]
    pub const fn new(facts: Facts, state: Arc<AppState>) -> Self {
        Self { facts, state }
    }
}

impl ProductSection for Fed {
    type Status = Status;

    const PRODUCT: Product = Product::Fed;

    fn status(&self) -> impl Future<Output = Status> + Send {
        let members = self
            .state
            .federation()
            .map_or(0, |federation| federation.snapshot().nodes().count());
        std::future::ready(self.facts.status(members))
    }
}

/// What `serve` hands the report: the section and the facts of the
/// configuration the gateway started with.
#[derive(Debug, Clone)]
pub struct Usage {
    section: UsageReportSection,
    facts: Facts,
}

impl Usage {
    /// Returns what `settings` hand the report.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        Self {
            section: settings.usage_report.clone(),
            facts: Facts::of(settings),
        }
    }

    /// Starts the report of a gateway that serves `state` since `started`,
    /// reading the CI variables from the process environment.
    ///
    /// The report starts once the gateway serves and never holds boot or
    /// readiness: the crate logs the boot line and returns `None` when the
    /// report is off.
    pub async fn start(self, state: Arc<AppState>, started: Instant) -> Option<JoinHandle<()>> {
        let ci = ci_of(|name| std::env::var(name).ok());
        let window = Arc::clone(state.metrics().usage());
        ferrohealth_usage_report::reporter::start(
            &self.section,
            &NOTICE,
            identity(),
            Arc::new(Fed::new(self.facts, state)),
            window,
            started,
            ci,
        )
        .await
    }
}
