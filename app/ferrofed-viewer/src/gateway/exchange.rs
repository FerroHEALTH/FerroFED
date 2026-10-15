// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A configuration revision exported as TOML, and TOML imported as a
//! dry-run revision, on a replica's admin listener.
//!
//! Both go to the admin listener at the URL the console is configured with,
//! through the same `openehr-its` client as every other gateway call, with
//! the operator's own access token. The export is relayed as the bytes the
//! gateway sent; the import is never retried. No specification governs the
//! admin listener: the gateway's own design.

use ferrofed_registry::revision::{Imported, RevisionId};
use http::{Method, StatusCode};
use openehr_its::rest::client::Request;

use crate::config::settings::ReplicaSettings;
use crate::gateway::config::Change;
use crate::gateway::{AccessToken, Gateway, GatewayError};

/// The media type of the TOML an import sends.
pub const TOML: &str = "application/toml";

impl Gateway {
    /// Reads revision `id` as TOML,
    /// `GET {admin}/admin/config/revisions/{id}/export` on `replica`.
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for any
    /// answer but `200`, with its status and body, and
    /// [`GatewayError::Call`] when the call failed.
    pub async fn export_revision(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        id: RevisionId,
    ) -> Result<Vec<u8>, GatewayError> {
        let path = format!("/admin/config/revisions/{id}/export");
        let client = self.client(replica.url.clone(), token)?;
        let answer = client
            .execute(Request::new(Method::GET, path))
            .await
            .map_err(|source| GatewayError::Call { source })?;
        if answer.status() != StatusCode::OK {
            return Err(GatewayError::Status {
                status: answer.status(),
                body: answer.error_body(),
            });
        }
        Ok(answer.into_body())
    }

    /// Saves `toml` as a dry-run revision on the change's base,
    /// `POST {admin}/admin/config/import` on `replica`; it is not applied.
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for any
    /// answer but `201`, with its status and body, and
    /// [`GatewayError::Call`] when the call failed or the body is not an
    /// import's.
    pub async fn import_config(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        change: Change<'_>,
        toml: &str,
    ) -> Result<Imported, GatewayError> {
        let mut request = Request::new(Method::POST, String::from("/admin/config/import"));
        if let Some(base) = change.base {
            request.query("base", base);
        }
        request.query("reason", change.reason);
        request
            .text_body(toml, TOML)
            .map_err(|source| GatewayError::Call { source })?;
        let client = self.client(replica.url.clone(), token)?;
        let answer = client
            .execute(request)
            .await
            .map_err(|source| GatewayError::Call { source })?;
        if answer.status() != StatusCode::CREATED {
            return Err(GatewayError::Status {
                status: answer.status(),
                body: answer.error_body(),
            });
        }
        answer
            .json()
            .map_err(|source| GatewayError::Call { source })
    }
}
