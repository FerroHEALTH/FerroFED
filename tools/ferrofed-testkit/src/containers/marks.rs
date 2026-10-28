// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroEHR nodes whose ADMIN API is on, so the harness can arrange a
//! restriction of processing at a node (GDPR Art. 18) and show that the
//! federated query honours it (§13.2, N27; CP-19).
//!
//! The restriction is FerroEHR's own extension, `POST {base}/admin/restriction`
//! behind `[admin] enabled = true`: no openEHR specification defines a consent
//! or restriction resource, so the harness sets the mark the way the node's
//! operator would, outside ITS-REST. A restricted object leaves every AQL
//! result of the node, and a point read of it answers `403`. The harness
//! nodes run with authentication and the role gate off, so the ADMIN API
//! admits the harness as it admits every caller.

use std::sync::Arc;

use http::StatusCode;
use testcontainers::ImageExt;
use uuid::Uuid;

use super::database::database_server;
use super::images::FERROEHR;
use super::retry;
use super::{
    DatabaseServer, FERROEHR_PRODUCT, HarnessError, NODE_A_DATABASE, NODE_A_SYSTEM_ID,
    NODE_B_DATABASE, NODE_B_SYSTEM_ID, Node, ProxiedNode, TwoNodes, ferroehr_request, ready,
};

/// The GDPR Art. 18 ground the harness restricts with: the accuracy of the
/// data is contested (Art. 18(1)(a)), in FerroEHR's ground vocabulary.
const GROUND: &str = "gdpr-18-1-a";

/// Starts node A and node B on one database server, each with its ADMIN API
/// on, and puts a proxy in front of each.
///
/// # Errors
///
/// Returns the first [`HarnessError`] the database server or either node
/// reports.
pub async fn two_nodes() -> Result<TwoNodes, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[NODE_B_DATABASE]).await?);
    let (a, b) = tokio::try_join!(
        node(&database, NODE_A_DATABASE, NODE_A_SYSTEM_ID),
        node(&database, NODE_B_DATABASE, NODE_B_SYSTEM_ID)
    )?;
    Ok(TwoNodes {
        a: ProxiedNode::new(a).await?,
        b: ProxiedNode::new(b).await?,
    })
}

/// Starts one FerroEHR node as `system_id` with its ADMIN API on, on a
/// database server of its own.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ferroehr(system_id: &'static str) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    node(&database, NODE_A_DATABASE, system_id).await
}

/// Starts FerroEHR as `system_id` on the database `name` of `database`, its
/// ADMIN API on and its access controls off.
async fn node(
    database: &Arc<DatabaseServer>,
    name: &str,
    system_id: &'static str,
) -> Result<Node, HarnessError> {
    let server = retry::start(|| {
        ferroehr_request(database, name, system_id)
            .with_env_var("FERROEHR__AUTH__ENABLED", "false")
            .with_env_var("FERROEHR__AUTHZ__RBAC__ENABLED", "false")
            .with_env_var("FERROEHR__ADMIN__ENABLED", "true")
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROEHR.repository,
        source,
    })?;
    ready(FERROEHR_PRODUCT, system_id, server, Arc::clone(database)).await
}

/// Why a restriction could not be set at a node.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MarkError {
    /// The request reached no answer.
    #[error("the restriction request to the node reached no answer")]
    Unanswered(#[source] reqwest::Error),
    /// The node answered with another status than `204`.
    #[error("the node answered the restriction with {0}, where it records one with 204")]
    Refused(StatusCode),
}

/// Restricts processing of the versioned object `vo_id` of the EHR `ehr_id`
/// at the node of `api_root` (its ITS-REST API root), through the node's
/// `POST {api_root}/v1/admin/restriction`.
///
/// # Errors
///
/// Returns [`MarkError`] when the request reaches no answer or the node does
/// not record the restriction.
pub async fn restrict(api_root: &str, ehr_id: Uuid, vo_id: Uuid) -> Result<(), MarkError> {
    let answer = reqwest::Client::new()
        .post(format!(
            "{}/v1/admin/restriction",
            api_root.trim_end_matches('/')
        ))
        .json(&serde_json::json!({
            "ehr_id": ehr_id.to_string(),
            "vo_id": vo_id.to_string(),
            "ground": GROUND,
        }))
        .send()
        .await
        .map_err(MarkError::Unanswered)?;
    match answer.status() {
        StatusCode::NO_CONTENT => Ok(()),
        refused => Err(MarkError::Refused(refused)),
    }
}
