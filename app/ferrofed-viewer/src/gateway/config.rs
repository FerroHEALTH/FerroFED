// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's configuration revisions and the resources the console
//! manages, on a replica's admin listener.
//!
//! Each call goes to the admin listener at the URL the console is
//! configured with, through the same `openehr-its` client and transport as
//! every other gateway call, with the operator's own access token. The
//! revisions live in the store the replicas share, so the console reads and
//! changes them through the first replica configured; an applied revision
//! reaches every other replica through the store. A `POST` or `PUT` is never
//! retried. No specification governs the admin listener: the gateway's own
//! design.

use ferrofed_registry::console::{ConsoleMember, Resources};
use ferrofed_registry::operator::{Page, PageRequest};
use http::{Method, StatusCode};
use openehr_its::rest::client::{Request, path_segment};
use openehr_its::rest::generated::definition::StoredQuery;
use serde::{Deserialize, Serialize};

use crate::config::settings::ReplicaSettings;
use crate::gateway::{AccessToken, Gateway, GatewayError};

/// Who made a revision or applied one, as the gateway records it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AuthorLine {
    /// `file`, `operator` or `development-loopback`.
    pub kind: String,
    /// The operator's issuer.
    pub issuer: Option<String>,
    /// The operator's subject at that issuer.
    pub subject: Option<String>,
}

/// The mark of the revision the gateway runs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Mark {
    /// The revision in effect.
    pub revision: u64,
    /// The revision before it, which a rollback restores.
    pub previous: Option<u64>,
    /// `applied`, or `pending-restart`.
    pub outcome: String,
    /// The changed settings that take effect only at a restart.
    pub needs_restart: Vec<String>,
    /// Who applied it.
    pub applied_by: AuthorLine,
    /// When, as RFC 3339 text.
    pub applied_at: String,
}

/// The body of `GET /admin/config`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Standing {
    /// The newest revision.
    pub head: Option<u64>,
    /// The mark of the running revision.
    pub current: Option<Mark>,
}

/// A revision as the gateway answers it, its configuration text left out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RevisionLine {
    /// Its number.
    pub id: u64,
    /// The revision it was saved on top of.
    pub base: Option<u64>,
    /// `file`, `operator` or `rollback`.
    pub origin: String,
    /// The revision whose content a rollback restores.
    pub restores: Option<u64>,
    /// Who made it.
    pub author: AuthorLine,
    /// When, as RFC 3339 text.
    pub at: String,
    /// Why, as its author wrote it.
    pub reason: String,
}

/// One key that differs between two revisions.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChangeLine {
    /// The dotted key.
    pub key: String,
    /// `added`, `removed` or `changed`.
    pub change: String,
    /// The value in the earlier revision.
    pub from: Option<String>,
    /// The value in the later revision.
    pub to: Option<String>,
}

/// The body of a diff.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DiffLine {
    /// The earlier revision.
    pub from: u64,
    /// The later revision.
    pub to: u64,
    /// The keys that differ, in key order.
    pub changes: Vec<ChangeLine>,
}

/// The answer to an apply or a rollback.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppliedLine {
    /// The revision applied.
    pub revision: RevisionLine,
    /// The mark the store now holds.
    pub current: Mark,
}

/// The body of a member's save.
#[derive(Serialize)]
struct MemberBody<'a> {
    base: Option<u64>,
    reason: &'a str,
    member: &'a ConsoleMember,
}

/// The body of a member's removal.
#[derive(Serialize)]
struct RemoveBody<'a> {
    base: Option<u64>,
    reason: &'a str,
}

/// The body of a rollback.
#[derive(Serialize)]
struct RollbackBody<'a> {
    current: u64,
    reason: &'a str,
}

/// The body of a stored-query version's store.
#[derive(Serialize)]
struct QueryBody<'a> {
    aql: &'a str,
}

/// The change a revision is saved on: the revision the operator read, and
/// why.
#[derive(Debug, Clone, Copy)]
pub struct Change<'a> {
    /// The revision the change is based on, the newest the operator read.
    pub base: Option<u64>,
    /// Why the change is made.
    pub reason: &'a str,
}

impl Gateway {
    /// Reads the running revision's mark and the newest revision,
    /// `GET {admin}/admin/config` on `replica`.
    ///
    /// # Errors
    /// Returns [`GatewayError::Status`] or [`GatewayError::Call`] for any
    /// answer but `200`, with its status and body, `404` among them where
    /// the gateway's configuration is not managed, and
    /// [`GatewayError::Call`] when the call failed or the body is not the
    /// one the route answers.
    pub async fn config_standing(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
    ) -> Result<Standing, GatewayError> {
        let request = Request::new(Method::GET, String::from("/admin/config"));
        self.admin(replica, token, request, StatusCode::OK).await
    }

    /// Reads every member and stored query with its owner,
    /// `GET {admin}/admin/config/resources` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn config_resources(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
    ) -> Result<Resources, GatewayError> {
        let request = Request::new(Method::GET, String::from("/admin/config/resources"));
        self.admin(replica, token, request, StatusCode::OK).await
    }

    /// Reads one page of the revision history, newest first,
    /// `GET {admin}/admin/config/revisions` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn config_history(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        page: PageRequest,
    ) -> Result<Page<RevisionLine>, GatewayError> {
        let mut request = Request::new(Method::GET, String::from("/admin/config/revisions"));
        request.query("offset", page.offset);
        request.query("limit", page.limit);
        self.admin(replica, token, request, StatusCode::OK).await
    }

    /// Reads the keys that differ from revision `from` to revision `to`,
    /// `GET {admin}/admin/config/revisions/{from}/diff/{to}` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn config_diff(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        (from, to): (u64, u64),
    ) -> Result<DiffLine, GatewayError> {
        let path = format!("/admin/config/revisions/{from}/diff/{to}");
        self.admin(
            replica,
            token,
            Request::new(Method::GET, path),
            StatusCode::OK,
        )
        .await
    }

    /// Saves a revision with `member` in place of the console's member of
    /// its `node_id`, `PUT {admin}/admin/config/members/{node_id}` on
    /// `replica`; it is not applied.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`], for any answer but `201`.
    pub async fn save_member(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        change: Change<'_>,
        member: &ConsoleMember,
    ) -> Result<RevisionLine, GatewayError> {
        let path = format!(
            "/admin/config/members/{}",
            path_segment(&member.node.id.as_str())
        );
        let mut request = Request::new(Method::PUT, path);
        request
            .json_body(
                &MemberBody {
                    base: change.base,
                    reason: change.reason,
                    member,
                },
                None,
            )
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, request, StatusCode::CREATED)
            .await
    }

    /// Saves a revision without the console's member `node`,
    /// `POST {admin}/admin/config/members/{node}/remove` on `replica`; it is
    /// not applied.
    ///
    /// # Errors
    /// As [`Gateway::save_member`].
    pub async fn remove_member(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        change: Change<'_>,
        node: &str,
    ) -> Result<RevisionLine, GatewayError> {
        let path = format!("/admin/config/members/{}/remove", path_segment(&node));
        let mut request = Request::new(Method::POST, path);
        request
            .json_body(
                &RemoveBody {
                    base: change.base,
                    reason: change.reason,
                },
                None,
            )
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, request, StatusCode::CREATED)
            .await
    }

    /// Applies revision `id` through the reload path,
    /// `POST {admin}/admin/config/revisions/{id}/apply` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn apply_revision(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        id: u64,
    ) -> Result<AppliedLine, GatewayError> {
        let path = format!("/admin/config/revisions/{id}/apply");
        self.admin(
            replica,
            token,
            Request::new(Method::POST, path),
            StatusCode::OK,
        )
        .await
    }

    /// Rolls the running revision `current` back to the one before it,
    /// `POST {admin}/admin/config/rollback` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn rollback(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        current: u64,
        reason: &str,
    ) -> Result<AppliedLine, GatewayError> {
        let mut request = Request::new(Method::POST, String::from("/admin/config/rollback"));
        request
            .json_body(&RollbackBody { current, reason }, None)
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, request, StatusCode::OK).await
    }

    /// Stores `aql` as version `version` of the stored query `name` (§12.7,
    /// N44), `PUT {admin}/admin/config/stored-queries/{name}/{version}` on
    /// `replica`.
    ///
    /// # Errors
    /// As [`Gateway::save_member`].
    pub async fn store_query(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        (name, version): (&str, &str),
        aql: &str,
    ) -> Result<StoredQuery, GatewayError> {
        let path = format!(
            "/admin/config/stored-queries/{}/{}",
            path_segment(&name),
            path_segment(&version)
        );
        let mut request = Request::new(Method::PUT, path);
        request
            .json_body(&QueryBody { aql }, None)
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, request, StatusCode::CREATED)
            .await
    }

    /// Sends `request` to `replica`'s admin listener as the operator and
    /// decodes an answer of `expected`.
    async fn admin<T: serde::de::DeserializeOwned>(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        request: Request,
        expected: StatusCode,
    ) -> Result<T, GatewayError> {
        let client = self.client(replica.url.clone(), token)?;
        let answer = client
            .execute(request)
            .await
            .map_err(|source| GatewayError::Call { source })?;
        if answer.status() != expected {
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
