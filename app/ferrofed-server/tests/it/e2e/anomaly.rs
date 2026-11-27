// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The anomaly warnings of the requests national contact points relay,
//! counted in FerroFED's own store on a real PostgreSQL 19 serving TLS
//! (`NCPeH` Architecture Specifications 10.0.0 §5.1.9).
//!
//! Past each rule's threshold a request is served and its record carries
//! the rule's warning: by the patient of the treatment relationship
//! confirmation, by the point of care, and across all requests; the table
//! names no patient and no point of care. With no access log the requests
//! are still counted, the count is one across two replicas, and a window
//! that ended opens a new one.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the records are read as JSON values"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use ferrofed_server::config::audit::AuditSettings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::containers::database::PostgresTls;
use ferrofed_testkit::containers::{self, database};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::TestResult;
use super::emergency::column;
use super::store::store_toml;
use crate::access::node_with_rows;
use crate::contact_point::emergency::{
    asked, contact_point_router, emergency_settings, records, relayed_for,
};
use crate::facade::{NAMESPACE, PATIENT};

/// The warning of too many requests for one patient.
const FOR_PATIENT: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS_FOR_UNIQUE_PATIENT";

/// The warning of too many requests from one point of care.
const FOR_POINT_OF_CARE: &str =
    "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS_FOR_UNIQUE_POINT_OF_CARE";

/// The warning of too many requests in all.
const IN_ALL: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS";

/// The point of care the test contact point's assertion names.
const POINT_OF_CARE: &str = "Qz9-ncp-point-of-care";

/// The rules of most tests: a third request for the patient, a fourth from
/// the point of care and a fifth in all are each a breach.
const RULES: &str = "\n[anomaly.patient]\nthreshold = 2\nwindow = '1h'\n\n[anomaly.point_of_care]\nthreshold = 3\nwindow = '1h'\n\n[anomaly.requests]\nthreshold = 4\nwindow = '1h'\n";

/// One replica: its router, and what it must keep alive.
struct Replica {
    app: Router,
    _state: Arc<AppState>,
    _dir: tempfile::TempDir,
}

/// Two members answering no rows and the repository the replicas record to.
struct Members {
    a: Server,
    b: Server,
    repository: FeedRepository,
}

impl Members {
    /// Two members and a repository.
    async fn start() -> Self {
        Self {
            a: node_with_rows(&[]).await,
            b: node_with_rows(&[]).await,
            repository: FeedRepository::start().await,
        }
    }

    /// A replica over the store database `name` on `server` with the
    /// `[anomaly]` tables `rules`, keeping an access log when `audited`.
    fn replica(
        &self,
        server: &PostgresTls,
        (name, rules): (&str, &str),
        audited: bool,
    ) -> Result<Replica, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let tables = format!("{}{rules}", store_toml(server, name, dir.path())?);
        let mut settings = emergency_settings(
            dir.path(),
            (&self.a.uri(), &self.b.uri()),
            &self.repository,
            &tables,
        )?;
        if !audited {
            settings.audit = AuditSettings::default();
        }
        let state = Arc::new(tokio::task::block_in_place(|| AppState::build(&settings))?);
        Ok(Replica {
            app: contact_point_router(Arc::clone(&state)),
            _state: state,
            _dir: dir,
        })
    }
}

/// Sends one request for treatment, as the test contact point relays it,
/// to `replica`, and asserts it is served.
async fn treated(replica: &Replica) -> TestResult {
    let (status, text) = asked(&replica.app, &relayed_for("Qz7-hp-91", "TREATMENT")).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§5.1.9: traffic is not blocked: {text}"
    );
    Ok(())
}

/// How many of `records` carry `warning` in their outcome description.
fn warned(records: &[Value], warning: &str) -> usize {
    records
        .iter()
        .filter(|record| {
            record["outcomeDesc"]
                .as_str()
                .is_some_and(|description| description.split("; ").any(|part| part == warning))
        })
        .count()
}

/// The requests the store counted for `rule` at `url`.
async fn counted(url: &str, rule: &str) -> Result<Vec<String>, Box<dyn Error>> {
    column(
        url,
        &format!(
            "SELECT requests::text FROM ferrofed.contact_point_requests WHERE rule = '{rule}'"
        ),
    )
    .await
}

/// §5.1.9: each rule raises its warning on every request past its
/// threshold in the window, and the request is served.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_rule_warns_past_its_threshold_and_the_request_is_served() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("anomaly_rules", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, ("anomaly_rules", RULES), true)?;
    for _ in 0..5 {
        treated(&replica).await?;
    }
    let records = records(&members.repository, 5).await?;
    assert_eq!(5, records.len(), "one record per request");
    assert_eq!(3, warned(&records, FOR_PATIENT), "the third to the fifth");
    assert_eq!(
        2,
        warned(&records, FOR_POINT_OF_CARE),
        "the fourth and the fifth"
    );
    assert_eq!(1, warned(&records, IN_ALL), "the fifth");

    let url = server.plain_url("anomaly_rules");
    let subjects = column(&url, "SELECT subject FROM ferrofed.contact_point_requests").await?;
    assert_eq!(3, subjects.len(), "one row per rule");
    assert!(
        subjects.iter().all(|subject| subject.len() == 64
            && !subject.contains(PATIENT)
            && !subject.contains(NAMESPACE)
            && !subject.contains(POINT_OF_CARE)),
        "the table names no patient and no point of care: {subjects:?}"
    );
    Ok(())
}

/// The requests are counted whether or not an access log records them.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_no_access_log_the_requests_are_still_counted() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("anomaly_unaudited", &[]).await?;
    let members = Members::start().await;
    let replica = members.replica(&server, ("anomaly_unaudited", RULES), false)?;
    for _ in 0..3 {
        treated(&replica).await?;
    }
    let url = server.plain_url("anomaly_unaudited");
    assert_eq!(vec![String::from("3")], counted(&url, "patient").await?);
    assert_eq!(vec![String::from("3")], counted(&url, "requests").await?);
    Ok(())
}

/// The count is one across two replicas over one database, and a window
/// that ended opens a new one at the next request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_count_is_one_across_replicas_and_a_new_window_starts_over() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("anomaly_replicas", &[]).await?;
    let members = Members::start().await;
    let rules = "\n[anomaly.requests]\nthreshold = 2\nwindow = '2s'\n";
    let first = members.replica(&server, ("anomaly_replicas", rules), true)?;
    let second = members.replica(&server, ("anomaly_replicas", rules), true)?;
    for replica in [&first, &second, &first] {
        treated(replica).await?;
    }
    let url = server.plain_url("anomaly_replicas");
    assert_eq!(vec![String::from("3")], counted(&url, "requests").await?);
    tokio::time::sleep(Duration::from_millis(2200)).await;
    treated(&second).await?;
    assert_eq!(
        vec![String::from("1")],
        counted(&url, "requests").await?,
        "the window ended, so the request opened a new one"
    );
    let records = records(&members.repository, 4).await?;
    assert_eq!(1, warned(&records, IN_ALL), "the third request alone");
    Ok(())
}
