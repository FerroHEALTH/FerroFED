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
use ferrofed_registry::resource::{Resource, SectionRequest};
use ferrofed_registry::revision::{Applied, Current, Diff, Review, Revision, RevisionId, Standing};
use ferrofed_registry::running::{RunningConfig, SettingsRequest};
use http::{Method, StatusCode};
use openehr_its::rest::client::{Request, path_segment};
use openehr_its::rest::generated::definition::StoredQuery;
use serde::Serialize;

use crate::config::settings::ReplicaSettings;
use crate::gateway::{AccessToken, Gateway, GatewayError};

/// The body of a member's save.
#[derive(Serialize)]
struct MemberBody<'a> {
    base: Option<RevisionId>,
    reason: &'a str,
    member: &'a ConsoleMember,
}

/// The body of a member's removal.
#[derive(Serialize)]
struct RemoveBody<'a> {
    base: Option<RevisionId>,
    reason: &'a str,
}

/// The body of a rollback.
#[derive(Serialize)]
struct RollbackBody<'a> {
    current: RevisionId,
    reason: &'a str,
}

/// The body of a stored-query version's store.
#[derive(Serialize)]
struct QueryBody<'a> {
    aql: &'a str,
}

/// The change a revision is saved on: the revision the operator read, and
/// why.
#[derive(Clone, Copy)]
pub struct Change<'a> {
    /// The revision the change is based on, the newest the operator read.
    pub base: Option<RevisionId>,
    /// Why the change is made.
    pub reason: &'a str,
}

impl std::fmt::Debug for Change<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Change")
            .field("base", &self.base)
            .field("reason", &!self.reason.trim().is_empty())
            .finish()
    }
}

/// The admin route of `resource`.
fn section_path(resource: &Resource) -> String {
    match resource {
        Resource::Credentials(endpoint) => format!(
            "/admin/config/credentials/{}",
            path_segment(&endpoint.as_str())
        ),
        Resource::Binding(section) => format!("/admin/config/bindings/{}", section.as_str()),
        _ => String::from("/admin/config/resources/unknown"),
    }
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
    ) -> Result<Page<Revision>, GatewayError> {
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
        (from, to): (RevisionId, RevisionId),
    ) -> Result<Diff, GatewayError> {
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
    ) -> Result<Revision, GatewayError> {
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
    ) -> Result<Revision, GatewayError> {
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
        id: RevisionId,
    ) -> Result<Applied, GatewayError> {
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
        current: RevisionId,
        reason: &str,
    ) -> Result<Applied, GatewayError> {
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

    /// Saves a revision with `resource`, a credential or a binding section,
    /// as `request` gives it, `PUT {admin}/admin/config/credentials/{id}` or
    /// `PUT {admin}/admin/config/bindings/{section}` on `replica`; it is not
    /// applied. A secret value in `request` travels in this body alone.
    ///
    /// # Errors
    /// As [`Gateway::save_member`].
    pub async fn save_section(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        resource: &Resource,
        request: &SectionRequest,
    ) -> Result<Revision, GatewayError> {
        let mut call = Request::new(Method::PUT, section_path(resource));
        call.json_body(request, None)
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, call, StatusCode::CREATED).await
    }

    /// Reads the running configuration, `GET {admin}/admin/config/running`
    /// on `replica`: each deployment-wide setting with its value, source
    /// and default, its secrets by reference alone.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn config_running(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
    ) -> Result<RunningConfig, GatewayError> {
        let request = Request::new(Method::GET, String::from("/admin/config/running"));
        self.admin(replica, token, request, StatusCode::OK).await
    }

    /// Saves a revision with the settings of `request` changed,
    /// `PUT {admin}/admin/config/settings` on `replica`; it is not applied.
    ///
    /// # Errors
    /// As [`Gateway::save_member`].
    pub async fn save_settings(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        request: &SettingsRequest,
    ) -> Result<Revision, GatewayError> {
        let mut call = Request::new(Method::PUT, String::from("/admin/config/settings"));
        call.json_body(request, None)
            .map_err(|source| GatewayError::Call { source })?;
        self.admin(replica, token, call, StatusCode::CREATED).await
    }

    /// Saves a revision without `resource`, `POST …/remove` of its route on
    /// `replica`; it is not applied.
    ///
    /// # Errors
    /// As [`Gateway::save_member`].
    pub async fn remove_section(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        change: Change<'_>,
        resource: &Resource,
    ) -> Result<Revision, GatewayError> {
        let path = format!("{}/remove", section_path(resource));
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

    /// Confirms the running revision `id`, which awaits its confirmation,
    /// `POST {admin}/admin/config/revisions/{id}/confirm` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn confirm_revision(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        id: RevisionId,
    ) -> Result<Current, GatewayError> {
        let path = format!("/admin/config/revisions/{id}/confirm");
        self.admin(
            replica,
            token,
            Request::new(Method::POST, path),
            StatusCode::OK,
        )
        .await
    }

    /// Approves revision `id` as an operator other than its author,
    /// `POST {admin}/admin/config/revisions/{id}/approve` on `replica`.
    ///
    /// # Errors
    /// As [`Gateway::config_standing`].
    pub async fn approve_revision(
        &self,
        replica: &ReplicaSettings,
        token: &AccessToken,
        id: RevisionId,
    ) -> Result<Review, GatewayError> {
        let path = format!("/admin/config/revisions/{id}/approve");
        self.admin(
            replica,
            token,
            Request::new(Method::POST, path),
            StatusCode::OK,
        )
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
