// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's admin actions on one gateway replica's admin listener:
//! the clear of a held `ehr_id` and the stored-query drift repair.
//!
//! Each is a `POST` to the replica's admin listener at the URL the console
//! is configured with, sent through the same `openehr-its` client and
//! transport as every other gateway call, with the operator's own access
//! token. The gateway admits it only for a token carrying its issuer's
//! operator scope and records the operator who ran it. A `POST` is never
//! retried, so an action runs at most once per call. No specification
//! governs the admin listener: the gateway's own design.

use ferrofed_registry::operator::ClearedEhrId;
use http::{Method, StatusCode};
use openehr_federation::headers::ENDPOINT;
use openehr_federation::meta::FederationMeta;
use openehr_its::rest::client::{ClientError, ErrorBody, Request, path_segment};
use openehr_its::rest::generated::definition::StoredQuery;

use crate::config::settings::ReplicaSettings;
use crate::gateway::{AccessToken, Gateway, GatewayError};

/// The stored-query version the drift repair sends again, and the members
/// it goes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redistribution {
    /// The qualified name of the stored query.
    pub name: String,
    /// Its `major.minor.patch` version.
    pub version: String,
    /// The members to send it to, as the `openEHR-federation-endpoint`
    /// header names them: `*` for every active member, or a comma-separated
    /// list of `endpoint_id`s (§8.4).
    pub endpoints: String,
}

/// What a replica answered the drift repair: the status, the version it
/// sent, what the registry did, and each member's outcome (§9.5, §12.7).
#[derive(Debug, Clone)]
pub struct Distribution {
    /// The status the replica answered with: `200` when every member took
    /// the version, `207` when some did, and `424` or `504` when none did.
    pub status: StatusCode,
    /// The version sent, as the registry holds it.
    pub definition: StoredQuery,
    /// What the registry did with the version, `held` for a repair.
    pub registry: Option<String>,
    /// Each member's outcome.
    pub federation: FederationMeta,
}

/// The body of a distribution's answer: the registry's `StoredQuery` and
/// its `meta`.
#[derive(serde::Deserialize)]
struct Distributed {
    #[serde(flatten)]
    definition: StoredQuery,
    meta: DistributedMeta,
}

/// The `meta` of a distribution's answer.
#[derive(serde::Deserialize)]
struct DistributedMeta {
    federation: FederationMeta,
    registry: Option<String>,
}

impl Gateway {
    /// Clears `ehr_id` from `replica`'s `ehr_id` index and resolution
    /// bindings, `POST {admin}/admin/ehr-ids/{ehr_id}/clear`, as the
    /// operator.
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for any
    /// answer but `200`, with its status and body, and
    /// [`GatewayError::Call`] when the call failed or the body is not a
    /// clear's.
    pub async fn clear_ehr_id(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        ehr_id: &str,
    ) -> Result<ClearedEhrId, GatewayError> {
        let path = format!("/admin/ehr-ids/{}/clear", path_segment(&ehr_id));
        let client = self.client(replica.url.clone(), token)?;
        let answer = client
            .execute(Request::new(Method::POST, path))
            .await
            .map_err(|source| GatewayError::Call { source })?;
        if answer.status() != StatusCode::OK {
            return Err(GatewayError::Status {
                status: answer.status(),
                body: answer.error_body(),
            });
        }
        answer
            .json()
            .map_err(|source| GatewayError::Call { source })
    }

    /// Sends the stored-query version `sent` names again to the members it
    /// names, `POST {admin}/admin/stored-queries/{name}/{version}/distribute`
    /// on `replica`, as the operator (§12.7 stored-query-drift).
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for a
    /// refusal, an answer that carries no per-member outcome, with its
    /// status and body, [`GatewayError::Report`] for a `200` whose body is
    /// no distribution, and [`GatewayError::Call`] when the call failed or
    /// the member list cannot be sent as a header.
    pub async fn repair_drift(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        sent: &Redistribution,
    ) -> Result<Distribution, GatewayError> {
        let path = format!(
            "/admin/stored-queries/{}/{}/distribute",
            path_segment(&sent.name),
            path_segment(&sent.version)
        );
        let mut request = Request::new(Method::POST, path);
        request
            .header(ENDPOINT, &sent.endpoints)
            .map_err(|source| GatewayError::Call { source })?;
        let client = self.client(replica.url.clone(), token)?;
        let (status, body) = match client.execute(request).await {
            Ok(answer) => (answer.status(), answer.error_body()),
            // NOTE: §11.4: a distribution no member took in time is a 504 that still
            // carries each member's outcome, which the client classes as a failure.
            Err(ClientError::ServiceFailure { status, body, .. })
                if status == StatusCode::GATEWAY_TIMEOUT =>
            {
                (status, body)
            }
            Err(source) => return Err(GatewayError::Call { source }),
        };
        distribution(status, body)
    }
}

/// The distribution a replica answered with `status` and `body`, or the
/// refusal it is.
fn distribution(status: StatusCode, body: ErrorBody) -> Result<Distribution, GatewayError> {
    let reports = [
        StatusCode::OK,
        StatusCode::MULTI_STATUS,
        StatusCode::FAILED_DEPENDENCY,
        StatusCode::GATEWAY_TIMEOUT,
    ];
    if !reports.contains(&status) {
        return Err(GatewayError::Status { status, body });
    }
    // NOTE: §11.4: a 424 or 504 whose body is no distribution is the gateway's
    // error document, legitimately not a per-member report, so it is a refusal.
    match serde_json::from_slice::<Distributed>(body.raw()) {
        Ok(read) => Ok(Distribution {
            status,
            definition: read.definition,
            registry: read.meta.registry,
            federation: read.meta.federation,
        }),
        Err(_not_a_distribution) if status != StatusCode::OK => {
            Err(GatewayError::Status { status, body })
        }
        Err(source) => Err(GatewayError::Report { status, source }),
    }
}
