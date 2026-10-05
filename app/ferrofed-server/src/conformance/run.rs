// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One conformance run, from the configuration to the written report.
//!
//! Without a gateway URL the run starts the gateway in-process from the
//! configuration, as `serve` builds it, on a loopback port of its own, and
//! drives it over HTTP like any client; the report is then of exactly the
//! configuration given, and nothing else need be running. With one, it
//! drives the gateway already serving there and reads the configuration for
//! the registry, the node clients it seeds through and the cross-reference.
//! Either way the scenarios reach the gateway by its base URL alone (§16.3
//! track 9, N28). No specification governs the run: our own design.

use std::path::PathBuf;
use std::sync::Arc;

use secrecy::SecretString;
use tokio::net::TcpListener;
use url::Url;
use uuid::Uuid;

use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, Encryption, ProtectedSite};
use crate::conformance::catalogue::CATALOGUE;
use crate::conformance::client::{GatewayError, HttpGateway};
use crate::conformance::execute::{Context, execute};
use crate::conformance::fixture::{SeedData, SeedDataError, SyntheticPatient};
use crate::conformance::node_profile::{Profile, members};
use crate::conformance::report::Report;
use crate::conformance::seed::{self, SeedError};
use crate::federation::Federation;
use crate::federation::error::FederationError;
use crate::state::{self, AppState, StateError};

/// What a run is asked to do, past the safety refusals.
#[derive(Debug)]
pub struct RunOptions {
    /// The gateway already serving, or `None` to start one in-process.
    pub gateway: Option<Url>,
    /// The caller's bearer token the gateway admits.
    pub token: SecretString,
    /// The synthetic patient the run seeds and queries.
    pub patient: SyntheticPatient,
    /// The directory of the vendored synthetic content the run writes.
    pub seed_data: PathBuf,
    /// The directory the report is written to.
    pub out: PathBuf,
    /// Whether to run the Federation-Node profile checks and the admission
    /// check against every active member and record their findings as the
    /// node profile.
    pub node_profile: bool,
}

/// What a run produced.
#[derive(Debug)]
pub struct Summary {
    /// The base URL the gateway was reached at.
    pub base: Url,
    /// The report.
    pub report: Report,
    /// The files written.
    pub paths: Vec<PathBuf>,
    /// The sites the run sent a credential or synthetic data to unencrypted,
    /// which only the development profile allows.
    pub cleartext: Vec<ProtectedSite>,
}

/// A run that could not reach its report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The seed data is not the vendored synthetic content.
    #[error(transparent)]
    SeedData(#[from] SeedDataError),
    /// The configuration names no registry, so there is nothing to score.
    #[error("set a registry, registry.document or [registry.mcsd], whose members the run scores")]
    NoRegistry,
    /// The federation could not be loaded.
    #[error("the federation could not be loaded")]
    Federation(#[source] FederationError),
    /// The in-process gateway could not be built.
    #[error("the gateway could not be built")]
    State(#[source] StateError),
    /// The in-process gateway could not be bound or served.
    #[error("the in-process gateway could not be served")]
    Serve(#[source] std::io::Error),
    /// The gateway's address is no base URL.
    #[error("the in-process gateway's address is no base URL")]
    Base(#[source] url::ParseError),
    /// A credential or a patient identifier would travel in cleartext outside
    /// the development profile.
    #[error("a credential or a patient identifier would travel in cleartext")]
    Cleartext(#[source] CleartextError),
    /// The gateway client could not be built.
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    /// A write to a node failed.
    #[error("the synthetic fixture could not be seeded")]
    Seed(#[source] SeedError),
    /// The report could not be written.
    #[error("the report could not be written")]
    Write(#[source] std::io::Error),
}

/// Runs the conformance scenarios against the deployment `settings`
/// configure, and writes the report.
///
/// # Errors
///
/// Returns [`RunError`] when the run cannot reach a report: the seed data
/// is not the vendored content, no registry is configured, a connection
/// would carry the token or the synthetic data in the clear
/// ([`transport::loopback_payload`]), the gateway or its client cannot be
/// built, a node refuses a seed write, or the report
/// cannot be written. A scenario that fails is in the report, never an
/// error.
pub async fn run(settings: &Settings, options: RunOptions) -> Result<Summary, RunError> {
    let data = SeedData::read(&options.seed_data)?;
    let mut cleartext = Vec::new();
    if let Some(base) = &options.gateway {
        cleartext.extend(
            transport::loopback_payload(settings.profile, base, gateway_site())
                .map_err(RunError::Cleartext)?,
        );
    }
    let (federation, base, served) = if let Some(base) = &options.gateway {
        let federation = Federation::load(settings)
            .map_err(RunError::Federation)?
            .ok_or(RunError::NoRegistry)?;
        (Arc::new(federation), base.clone(), None)
    } else {
        let (federation, base, served) = in_process(settings).await?;
        (federation, base, Some(served))
    };
    cleartext.extend(members_held(settings, &federation).map_err(RunError::Cleartext)?);
    cleartext.extend(
        transport::check(settings, Some(federation.snapshot())).map_err(RunError::Cleartext)?,
    );
    let gateway = HttpGateway::new(base.clone(), options.token)?;
    let seeded = seed::seed(&federation, &options.patient, &data)
        .await
        .map_err(RunError::Seed)?;
    let mut written = seeded.written;
    let token: String = Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    let context = Context {
        fixture: &seeded.fixture,
        seed: &data,
        run: &token,
    };
    let mut outcomes = Vec::with_capacity(CATALOGUE.len());
    for entry in CATALOGUE {
        let outcome = execute(&gateway, &context, entry, &mut written).await;
        outcomes.push((entry, outcome));
    }
    let findings = if options.node_profile {
        members::every_member(&federation, &seeded.fixture, &mut written)
            .await
            .iter()
            .flat_map(Profile::rows)
            .collect()
    } else {
        Vec::new()
    };
    let report = Report::new(
        &outcomes,
        findings,
        written,
        format!("the gateway at {base}"),
    )
    .with_cleartext(
        cleartext
            .iter()
            .map(|site| format!("{} travels unencrypted to {}", site.payload, site.url_key))
            .collect(),
    );
    let paths = report.write(&options.out).map_err(RunError::Write)?;
    if let Some((stop, serving)) = served {
        drop(stop);
        match serving.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(RunError::Serve(error)),
            Err(joined) => return Err(RunError::Serve(std::io::Error::other(joined))),
        }
    }
    Ok(Summary {
        base,
        report,
        paths,
        cleartext,
    })
}

/// The site of the gateway `--gateway` names: the caller's bearer token and
/// the synthetic patient's identifier travel to it.
fn gateway_site() -> ProtectedSite {
    ProtectedSite {
        url_key: "the --gateway URL".to_owned(),
        payload: "the caller's bearer token and the synthetic patient's identifier".to_owned(),
        requires: Encryption::Https,
    }
}

/// Holds every member endpoint the run may write through to
/// [`transport::loopback_payload`]: the endpoint's onward credentials and
/// the synthetic patient's `EHR_STATUS` travel to it.
///
/// The in-process gateway is the run's own listener on `127.0.0.1`, inside
/// this process, so it is not held here.
///
/// # Errors
///
/// Returns the [`CleartextError`] of the first endpoint, in registry order,
/// the policy refuses.
fn members_held(
    settings: &Settings,
    federation: &Federation,
) -> Result<Vec<ProtectedSite>, CleartextError> {
    let mut cleartext = Vec::new();
    for endpoint in federation.snapshot().endpoints() {
        let site = ProtectedSite {
            url_key: format!("the url of endpoint {} in the registry", endpoint.id()),
            payload: "the onward credentials and the synthetic patient's EHR_STATUS".to_owned(),
            requires: Encryption::Https,
        };
        cleartext.extend(transport::loopback_payload(
            settings.profile,
            endpoint.url(),
            site,
        )?);
    }
    Ok(cleartext)
}

/// The handle that stops the in-process gateway, and its serving task.
type Served = (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<std::io::Result<()>>,
);

/// Starts the gateway `settings` configure on a loopback port of its own,
/// and returns its federation, its base URL and the handle that stops it.
async fn in_process(settings: &Settings) -> Result<(Arc<Federation>, Url, Served), RunError> {
    state::admits_callers(settings, settings.federates()).map_err(RunError::State)?;
    let state = Arc::new(AppState::build(settings).map_err(RunError::State)?);
    let federation = state.federation().ok_or(RunError::NoRegistry)?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(RunError::Serve)?;
    let address = listener.local_addr().map_err(RunError::Serve)?;
    let base_path = if settings.server.base_path.is_root() {
        ""
    } else {
        settings.server.base_path.as_str()
    };
    let base = Url::parse(&format!("http://{address}{base_path}/")).map_err(RunError::Base)?;
    let app = crate::router(state, &settings.server);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(crate::serve_until(
        listener,
        app,
        settings.server.shutdown_timeout,
        async {
            // NOTE: no specification governs this: our own design; a dropped
            // sender ends the wait as a sent signal does.
            let _signalled = stopped.await;
        },
    ));
    Ok((federation, base, (stop, serving)))
}
