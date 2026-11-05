// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The limit of the emergency accesses a national contact point relays for
//! one health professional, counted in FerroFED's own store on a real
//! PostgreSQL 18 serving TLS (eHDSI Section II Security Services 10.0.0
//! §2.5; `NCPeH` Architecture Specifications 10.0.0 §5.1.9).
//!
//! With no `[emergency]` declared, five emergency accesses of one
//! professional pass and the sixth is denied `emergency-limit-reached`
//! before any member is asked, its record carrying the emergency mark and
//! the anomaly warning, while another professional is counted apart; on the
//! FHIR face the denial names `ERROR_SEC_GENERIC` (eHDSI XCA Profile 8.0.1
//! §2.2.3). A tighter threshold denies earlier, the window slides, and the
//! count holds across two replicas over one database, under a race, and
//! across a restart of both. With the database stopped an emergency access
//! is refused `emergency-uncounted` and no member is asked.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the records and the outcome are read as JSON values"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::containers::database::PostgresTls;
use ferrofed_testkit::containers::{self, database};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::TestResult;
use super::store::store_toml;
use crate::access::node_with_rows;
use crate::auth::{bearing, minted, sent};
use crate::contact_point::emergency::{
    ANOMALY, asked, contact_point_router, emergency_settings, marked, records, relayed_for,
};
use crate::facade::received;
use crate::support::error_body;

/// The professional most tests count.
const HP: &str = "Qz7-hp-81";

/// Another professional, counted apart.
const OTHER_HP: &str = "Qz7-hp-82";

/// One replica: its router, its state and the directory its files live in.
struct Replica {
    app: Router,
    _state: Arc<AppState>,
    _dir: tempfile::TempDir,
}

/// The two members every replica asks and the repository every replica
/// records to.
struct Members {
    a: Server,
    b: Server,
    repository: FeedRepository,
}

impl Members {
    /// Two members answering no rows, and a repository.
    async fn start() -> Self {
        Self {
            a: node_with_rows(&[]).await,
            b: node_with_rows(&[]).await,
            repository: FeedRepository::start().await,
        }
    }

    /// A replica over these members and the store database `name` on
    /// `server`, with the `[emergency]` keys `limit`.
    fn replica(
        &self,
        server: &PostgresTls,
        name: &str,
        limit: &str,
    ) -> Result<Replica, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let tables = format!(
            "{}\n[emergency]\n{limit}",
            store_toml(server, name, dir.path())?
        );
        let settings = emergency_settings(
            dir.path(),
            (&self.a.uri(), &self.b.uri()),
            &self.repository,
            &tables,
        )?;
        let state = Arc::new(tokio::task::block_in_place(|| AppState::build(&settings))?);
        Ok(Replica {
            app: contact_point_router(Arc::clone(&state))?,
            _state: state,
            _dir: dir,
        })
    }

    /// How many patient queries node A was asked.
    async fn asked_at_a(&self) -> Result<usize, Box<dyn Error>> {
        Ok(received(&self.a).await?.len())
    }
}

/// Sends one emergency access of `hp_identifier` to `replica`.
async fn emergency(
    replica: &Replica,
    hp_identifier: &str,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    asked(&replica.app, &relayed_for(hp_identifier, "EMERGENCY")).await
}

/// Asserts that `answer` denies an emergency access past the limit.
fn assert_denied((status, text): &(StatusCode, String)) -> TestResult {
    assert_eq!(StatusCode::FORBIDDEN, *status, "{text}");
    let body = error_body(text)?;
    assert_eq!("emergency-limit-reached", body.code, "{text}");
    assert!(
        text.contains("too many attempts for an emergency access"),
        "§2.5: the reason notes too many attempts: {text}"
    );
    Ok(())
}

/// The records among `records` whose outcome description names the
/// anomaly warning.
fn warned(records: &[Value]) -> Vec<&Value> {
    records
        .iter()
        .filter(|record| {
            record["outcomeDesc"]
                .as_str()
                .is_some_and(|description| description.contains(ANOMALY))
        })
        .collect()
}

/// §2.5: "No more than five emergency access attempts per HP per 24-hour
/// period": the sixth is denied with no member asked, recorded with the
/// emergency mark and the anomaly warning; another professional is counted
/// apart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn five_emergency_accesses_pass_and_the_sixth_is_denied_and_recorded() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_default", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, "emergency_default", "")?;
    for attempt in 1..=5 {
        let (status, text) = emergency(&replica, HP).await?;
        assert_eq!(StatusCode::OK, status, "attempt {attempt}: {text}");
    }
    assert_eq!(5, members.asked_at_a().await?);
    assert_denied(&emergency(&replica, HP).await?)?;
    assert_eq!(
        5,
        members.asked_at_a().await?,
        "the sixth reached no member"
    );
    let (status, text) = emergency(&replica, OTHER_HP).await?;
    assert_eq!(StatusCode::OK, status, "another professional: {text}");

    let records = records(&members.repository, 7).await?;
    assert_eq!(7, records.len(), "one record per attempt");
    assert!(
        records.iter().all(marked),
        "every attempt is an emergency access"
    );
    let [denied] = warned(&records)
        .try_into()
        .map_err(|_unexpected| "one warned record")?;
    assert_ne!(Some("0"), denied["outcome"].as_str(), "{denied}");
    assert!(
        denied["outcomeDesc"]
            .as_str()
            .is_some_and(|description| description.contains("§2.5")),
        "{denied}"
    );

    let url = server.plain_url("emergency_default");
    let held = column(&url, "SELECT professional FROM ferrofed.emergency_access").await?;
    assert_eq!(2, held.len(), "one row per professional");
    assert!(
        held.iter()
            .all(|key| key.len() == 64 && !key.contains(HP) && !key.contains(OTHER_HP)),
        "the table names no professional: {held:?}"
    );
    Ok(())
}

/// §2.5 lets a country tighten the threshold, which then denies earlier.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tighter_threshold_denies_earlier() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_tight", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, "emergency_tight", "threshold = 2\n")?;
    for _ in 0..2 {
        let (status, text) = emergency(&replica, HP).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    assert_denied(&emergency(&replica, HP).await?)?;
    assert_eq!(2, members.asked_at_a().await?);
    Ok(())
}

/// The window slides: once the access granted in it leaves, the professional
/// is granted again, and a refused attempt is not counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_window_slides_and_a_professional_is_granted_again_once_it_passes() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_window", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, "emergency_window", "threshold = 1\nwindow_s = 2\n")?;
    let (status, text) = emergency(&replica, HP).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_denied(&emergency(&replica, HP).await?)?;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_denied(&emergency(&replica, HP).await?)?;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    let (status, text) = emergency(&replica, HP).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the first access left the window, and no refusal was counted: {text}"
    );
    assert_eq!(2, members.asked_at_a().await?);
    Ok(())
}

/// The count is one across two replicas over one database and survives a
/// restart of both.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_count_holds_across_two_replicas_and_a_restart_of_both() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_replicas", &[]).await?;
    let members = Members::start().await;
    let limit = "threshold = 3\n";
    let first = members.replica(&server, "emergency_replicas", limit)?;
    let second = members.replica(&server, "emergency_replicas", limit)?;
    for replica in [&first, &second, &first] {
        let (status, text) = emergency(replica, HP).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    assert_denied(&emergency(&second, HP).await?)?;
    assert_denied(&emergency(&first, HP).await?)?;
    drop((first, second));

    let first = members.replica(&server, "emergency_replicas", limit)?;
    let second = members.replica(&server, "emergency_replicas", limit)?;
    assert_denied(&emergency(&first, HP).await?)?;
    assert_denied(&emergency(&second, HP).await?)?;
    assert_eq!(
        3,
        members.asked_at_a().await?,
        "three accesses reached a member"
    );
    let granted = column(
        &server.plain_url("emergency_replicas"),
        "SELECT cardinality(granted)::text FROM ferrofed.emergency_access",
    )
    .await?;
    assert_eq!(vec![String::from("3")], granted);
    Ok(())
}

/// Two replicas granting at once never pass the threshold together: of ten
/// attempts sent at once, five to each, exactly the threshold is granted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_replicas_racing_grant_exactly_the_threshold() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_race", &[]).await?;
    let members = Members::start().await;
    let first = members.replica(&server, "emergency_race", "")?;
    let second = members.replica(&server, "emergency_race", "")?;
    let mut attempts = tokio::task::JoinSet::new();
    for index in 0..10 {
        let app = if index % 2 == 0 {
            first.app.clone()
        } else {
            second.app.clone()
        };
        let request = bearing(
            crate::auth::query()?,
            &minted(&relayed_for(HP, "EMERGENCY"))?,
        )?;
        attempts.spawn(async move {
            sent(&app, request)
                .await
                .map(|(status, _, _)| status)
                .map_err(|error| error.to_string())
        });
    }
    let mut statuses = Vec::new();
    while let Some(status) = attempts.join_next().await {
        statuses.push(status??);
    }
    let granted = statuses
        .iter()
        .filter(|status| **status == StatusCode::OK)
        .count();
    let denied = statuses
        .iter()
        .filter(|status| **status == StatusCode::FORBIDDEN)
        .count();
    assert_eq!((5, 5), (granted, denied), "{statuses:?}");
    Ok(())
}

/// Fail closed: with the database stopped an emergency access is refused
/// `emergency-uncounted` and no member is asked, while a request for
/// treatment, which is never counted, is served.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_store_down_an_emergency_access_is_refused_and_no_member_is_asked() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("emergency_down", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, "emergency_down", "")?;
    let (status, text) = emergency(&replica, HP).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    server.container().stop().await?;
    let (status, text) = emergency(&replica, HP).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!("emergency-uncounted", error_body(&text)?.code, "{text}");
    assert_eq!(
        1,
        members.asked_at_a().await?,
        "the refused access reached no member"
    );
    let (status, text) = asked(&replica.app, &relayed_for(HP, "TREATMENT")).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

/// The denial on the FHIR face, which the IHE binding's build serves.
#[cfg(feature = "binding-ihe")]
mod fhir_face {
    use std::error::Error;
    use std::path::Path;
    use std::sync::Arc;

    use axum::Router;
    use ferrofed_server::state::AppState;
    use ferrofed_testkit::atna_feed::FeedRepository;
    use ferrofed_testkit::containers::database::PostgresTls;
    use ferrofed_testkit::containers::{self, database};
    use ferrofed_testkit::mock::Server;
    use ferrofed_testkit::pdqm_stub;
    use http::StatusCode;
    use serde_json::Value;

    use super::super::TestResult;
    use super::super::store::store_toml;
    use super::HP;
    use crate::auth::{bearing, minted, sent};
    use crate::contact_point::emergency::{contact_point_router, emergency_purpose, relayed_for};
    use crate::facade::{EHR_A, EHR_B, received};
    use crate::feed_audit::audit_tables;
    use crate::fhir::{UID_A, UID_B, node_holding, summary, supplier, text_registered};

    /// On the FHIR face the denial is `403 forbidden` naming the refusal and
    /// `ERROR_SEC_GENERIC` (eHDSI XCA Profile 8.0.1 §2.2.3), with no member
    /// asked.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_fhir_face_denies_naming_error_sec_generic() -> TestResult {
        if !containers::e2e_enabled() {
            return Ok(());
        }
        let server = database::postgres_tls("emergency_fhir", &[]).await?;
        let (a, b, pdq) = (
            node_holding(UID_A).await,
            node_holding(UID_B).await,
            supplier().await,
        );
        let repository = FeedRepository::start().await;
        let dir = tempfile::tempdir()?;
        let app = fhir_gateway(dir.path(), (&a, &b, &pdq), &repository, &server)?;
        let token = minted(&relayed_for(HP, "EMERGENCY"))?;
        let (status, _, text) = sent(&app, bearing(summary("")?, &token)?).await?;
        assert_eq!(
            StatusCode::OK,
            status,
            "the first access is granted: {text}"
        );
        let asked = received(&a).await?.len();

        let (status, _, text) = sent(&app, bearing(summary("")?, &token)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
        let outcome: Value = serde_json::from_str(&text)?;
        assert_eq!(
            Some("forbidden"),
            outcome.pointer("/issue/0/code").and_then(Value::as_str)
        );
        let codes: Vec<&str> = outcome
            .pointer("/issue/0/details/coding")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|coding| coding.get("code").and_then(Value::as_str))
            .collect();
        assert_eq!(
            vec!["emergency-limit-reached", "ERROR_SEC_GENERIC"],
            codes,
            "{text}"
        );
        assert_eq!(asked, received(&a).await?.len(), "no member was asked");
        Ok(())
    }

    /// The FHIR face over `a` and `b`, the summary header asked of `pdq`,
    /// recording to `repository`, its emergency accesses counted in the store
    /// database `emergency_fhir` on `server` at a threshold of one, its test
    /// issuer declared a national contact point.
    fn fhir_gateway(
        dir: &Path,
        (a, b, pdq): (&Server, &Server, &Server),
        repository: &FeedRepository,
        server: &PostgresTls,
    ) -> Result<Router, Box<dyn Error>> {
        let tables = format!(
            "{}{}{}\n[emergency]\nthreshold = 1\n",
            audit_tables(repository, ""),
            emergency_purpose(),
            store_toml(server, "emergency_fhir", dir)?
        );
        let text = text_registered(
            dir,
            (&a.uri(), &b.uri(), &pdqm_stub::base_url(pdq)),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
            "",
            ("", &tables),
        )?;
        let settings = ferrofed_server::config::Config::from_sources(
            Some(&crate::support::signed(&text)),
            &std::collections::BTreeMap::new(),
        )?
        .resolve()?;
        let state = Arc::new(tokio::task::block_in_place(|| AppState::build(&settings))?);
        contact_point_router(state)
    }
}

/// The first column of every row `query` answers at `url`, as text.
async fn column(url: &str, query: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls).await?;
    let driven = tokio::spawn(connection);
    let rows = client.query(query, &[]).await?;
    drop(client);
    driven.await??;
    Ok(rows.iter().map(|row| row.get::<usize, String>(0)).collect())
}
