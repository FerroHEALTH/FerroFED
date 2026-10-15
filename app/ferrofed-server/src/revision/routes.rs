// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration revisions on the admin listener, served only under
//! `config.ui = "managed"`.
//!
//! Every route is admitted by client authentication for the issuer's
//! operator scope, as every other admin action is
//! ([`crate::admin::operator_only`], [`crate::admin::operator_read`]); a
//! write records the [`Operator`] in the revision or the mark it makes.
//!
//! | Route | Answer |
//! |---|---|
//! | `GET` [`CONFIG`] | the running revision's mark, the newest revision, and the revision this replica runs |
//! | `GET` [`REVISIONS`] | one page of the history, newest first |
//! | `POST` [`REVISIONS`] | `201` with the revision saved |
//! | `GET` [`REVISION`] | one revision |
//! | `GET` [`DIFF`] | the keys that differ from one revision to another |
//! | `POST` [`APPLY`] | the mark once the revision is applied |
//! | `POST` [`ROLLBACK`] | the revision a rollback saved, and its mark |
//! | `GET` [`RESOURCES`] | every member and stored query, with its owner ([`console`]) |
//! | `PUT` [`MEMBER`] | `201` with the revision saved with the console member in place |
//! | `POST` [`MEMBER_REMOVE`] | `201` with the revision saved without the console member |
//! | `PUT` [`STORED_QUERY`] | `201` with the stored-query version stored (§12.7, N44) |
//!
//! A refusal is an error body: `400` (`body-invalid`) for a body that does
//! not read, `404` (`revision-unknown`), `409` (`revision-stale`) when
//! another save, apply or rollback came first, `409`
//! (`rollback-unavailable`), `409` (`resource-file-owned`) for a change to a
//! resource the file provides, `404` (`resource-unknown`), `422`
//! (`configuration-invalid`) with the errors `config check` reports, and
//! `500` (`internal`) when the store fails. No specification governs this:
//! our own design.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use ferrofed_registry::operator::{MAX_PAGE, Page, PageRequest};
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use crate::admin::Operator;
use crate::error::{self, Code};
use crate::reload::Reloader;
use crate::request_id;
use crate::revision::service::{APPLIED, ROLLED_BACK, RevisionError, Revisions, SAVED};
use crate::revision::tree::diff;
use ferrofed_registry::revision::{Applied, Author, Diff, Revision, RevisionId, Standing};

/// The running revision's mark and the newest revision.
pub const CONFIG: &str = "/admin/config";

/// The history, and a save.
pub const REVISIONS: &str = "/admin/config/revisions";

/// One revision.
pub const REVISION: &str = "/admin/config/revisions/{id}";

/// The keys that differ from revision `id` to revision `other`.
pub const DIFF: &str = "/admin/config/revisions/{id}/diff/{other}";

/// The apply of one revision.
pub const APPLY: &str = "/admin/config/revisions/{id}/apply";

/// The rollback of the running revision.
pub const ROLLBACK: &str = "/admin/config/rollback";

/// Every member and stored query, with its owner.
pub const RESOURCES: &str = "/admin/config/resources";

/// The save of one member the console owns, by its `node_id`.
pub const MEMBER: &str = "/admin/config/members/{node_id}";

/// The removal of one member the console owns, by its `node_id`.
pub const MEMBER_REMOVE: &str = "/admin/config/members/{node_id}/remove";

/// The store of one stored-query version.
pub const STORED_QUERY: &str = "/admin/config/stored-queries/{qualified_query_name}/{version}";

pub mod console;

/// The revisions and the reload path they are applied through.
#[derive(Debug)]
pub struct Managed {
    revisions: Arc<Revisions>,
    reloader: Arc<Reloader>,
}

impl Managed {
    /// The revisions `revisions`, applied through `reloader`.
    #[must_use]
    pub fn new(revisions: Arc<Revisions>, reloader: Arc<Reloader>) -> Self {
        Self {
            revisions,
            reloader,
        }
    }
}

/// Returns the read routes over `managed`, for an operator read gate.
pub fn reads(managed: Arc<Managed>) -> Router {
    Router::new()
        .route(CONFIG, get(config))
        .route(REVISIONS, get(history))
        .route(REVISION, get(revision))
        .route(DIFF, get(difference))
        .route(RESOURCES, get(console::resources))
        .with_state(managed)
}

/// Returns the write routes over `managed`, for an operator write gate.
pub fn writes(managed: Arc<Managed>) -> Router {
    Router::new()
        .route(REVISIONS, post(save))
        .route(APPLY, post(apply))
        .route(ROLLBACK, post(rollback))
        .route(MEMBER, put(console::save_member))
        .route(MEMBER_REMOVE, post(console::remove_member))
        .route(STORED_QUERY, put(console::store_query))
        .with_state(managed)
}

/// The body of a save.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    /// The revision the save is based on, which must be the newest; `null`
    /// for the first.
    base: Option<RevisionId>,
    /// Why the revision is made.
    reason: String,
    /// The configuration less its bootstrap core, as TOML text.
    toml: String,
}

/// The members of a save's body.
const SAVE_SHAPE: &str = r#"{"base": <revision or null>, "reason": "<text>", "toml": "<text>"}"#;

/// The members of a rollback's body.
const ROLLBACK_SHAPE: &str = r#"{"current": <revision>, "reason": "<text>"}"#;

/// The body of a rollback.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RollbackRequest {
    /// The revision the caller read as running.
    current: RevisionId,
    /// Why the rollback is made.
    reason: String,
}
/// Runs `work` on a thread that may block, and answers what it returns or
/// the failure that stopped it.
async fn blocking<T, F>(headers: &HeaderMap, work: F) -> Response
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, RevisionError> + Send + 'static,
    Result<T, RevisionError>: Answer,
{
    answered(headers, work).await
}

/// Runs `work` on a thread that may block, and answers the outcome it
/// returns or the failure that stopped it.
async fn answered<R, F>(headers: &HeaderMap, work: F) -> Response
where
    R: Answer + Send + 'static,
    F: FnOnce() -> R + Send + 'static,
{
    let request_id = request_id::of(headers).unwrap_or_default().to_owned();
    match tokio::task::spawn_blocking(work).await {
        Ok(outcome) => outcome.answer(&request_id),
        Err(failure) => {
            tracing::error!(
                error = %failure,
                "a configuration revision request did not complete"
            );
            error::fixed(Code::Internal, &request_id)
        }
    }
}

/// How an outcome is answered.
trait Answer {
    /// The answer to this outcome, under `request_id`.
    fn answer(self, request_id: &str) -> Response;
}

impl<T: Serialize> Answer for Result<T, RevisionError> {
    fn answer(self, request_id: &str) -> Response {
        match self {
            Ok(body) => Json(body).into_response(),
            Err(refusal) => refused(&refusal, request_id),
        }
    }
}

/// The answer to `refusal`.
fn refused(refusal: &RevisionError, request_id: &str) -> Response {
    let code = match refusal {
        RevisionError::Store(_) | RevisionError::Exhausted => {
            tracing::error!(
                error = crate::chain(refusal),
                "the configuration revision store failed"
            );
            return error::fixed(Code::Internal, request_id);
        }
        RevisionError::Unknown(_) | RevisionError::NotANumber => Code::RevisionUnknown,
        RevisionError::Refused(_) | RevisionError::Reload(_) => Code::ConfigurationInvalid,
        RevisionError::Stale(_) | RevisionError::NoCurrent => Code::RevisionStale,
        RevisionError::NoPrevious => Code::RollbackUnavailable,
    };
    error::response(code, crate::chain(refusal), request_id)
}

/// `GET` [`CONFIG`].
async fn config(State(managed): State<Arc<Managed>>, headers: HeaderMap) -> Response {
    let revisions = Arc::clone(&managed.revisions);
    blocking(&headers, move || {
        let store = revisions.store();
        let current = store.current()?;
        let replica = revisions.replica();
        Ok(Standing {
            ui: crate::config::revisions::Ui::Managed.as_str().to_owned(),
            head: store.head()?,
            in_step: replica.running == current.as_ref().map(|mark| mark.revision),
            running: replica.running,
            refused: replica.refused,
            current,
        })
    })
    .await
}

/// `GET` [`REVISIONS`]: one page of the history, newest first.
async fn history(
    State(managed): State<Arc<Managed>>,
    Query(page): Query<PageRequest>,
    headers: HeaderMap,
) -> Response {
    if !page.is_valid() {
        return error::response(
            Code::ParameterInvalid,
            format!("limit must be from 1 to {MAX_PAGE}"),
            request_id::of(&headers).unwrap_or_default(),
        );
    }
    let revisions = Arc::clone(&managed.revisions);
    blocking(&headers, move || {
        let mut history = revisions.store().history()?;
        history.reverse();
        Ok(Page::of(history, page))
    })
    .await
}

/// The revision numbered `text`, refused as unknown when `text` names none.
fn number(text: &str) -> Result<RevisionId, RevisionError> {
    text.parse().map_err(|_unparsed| RevisionError::NotANumber)
}

/// The revision `id` the store holds.
fn held(revisions: &Revisions, id: RevisionId) -> Result<Revision, RevisionError> {
    revisions
        .store()
        .revision(id)?
        .ok_or(RevisionError::Unknown(id))
}

/// `GET` [`REVISION`].
async fn revision(
    State(managed): State<Arc<Managed>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let revisions = Arc::clone(&managed.revisions);
    blocking(&headers, move || held(&revisions, number(&id)?)).await
}

/// `GET` [`DIFF`].
async fn difference(
    State(managed): State<Arc<Managed>>,
    Path((id, other)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let revisions = Arc::clone(&managed.revisions);
    blocking(&headers, move || {
        let from = held(&revisions, number(&id)?)?;
        let to = held(&revisions, number(&other)?)?;
        Ok(Diff {
            from: from.id,
            to: to.id,
            changes: diff(&table(&from)?, &table(&to)?),
        })
    })
    .await
}

/// The configuration `revision` holds, as a table.
fn table(revision: &Revision) -> Result<toml::Table, RevisionError> {
    toml::from_str(&revision.toml).map_err(|error| {
        RevisionError::Store(crate::revision::StoreError::Corrupt(Box::new(error)))
    })
}

/// The `400` of a body that does not read as `shape`, the JSON object the
/// action takes; the parser's message is not kept, since it may quote the
/// body.
fn body_refused(headers: &HeaderMap, shape: &str) -> Response {
    error::response(
        Code::BodyInvalid,
        format!("the body is not the JSON object this action takes: {shape}"),
        request_id::of(headers).unwrap_or_default(),
    )
}

/// Records `event` for `revision` by `operator`, never its configuration or
/// its reason.
fn record(event: &'static str, revision: RevisionId, operator: &Operator) {
    tracing::info!(
        target: crate::facade::security::TARGET,
        event,
        revision = %revision,
        issuer = operator.issuer(),
        subject = operator.subject(),
        at = %jiff::Timestamp::now(),
        "an operator changed the configuration revisions"
    );
}

/// `POST` [`REVISIONS`]: saves the body's configuration as a revision,
/// `201` with it.
async fn save(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(request) = serde_json::from_slice::<SaveRequest>(&body) else {
        return body_refused(&headers, SAVE_SHAPE);
    };
    let revisions = Arc::clone(&managed.revisions);
    let author = Author::from(&operator);
    let answer = blocking(&headers, move || {
        let saved = revisions.save(request.base, (author, request.reason), &request.toml)?;
        record(SAVED, saved.id, &operator);
        Ok(saved)
    })
    .await;
    if answer.status() == StatusCode::OK {
        (StatusCode::CREATED, answer).into_response()
    } else {
        answer
    }
}

/// The message of an apply that carries a body.
const APPLY_TAKES_NO_BODY: &str =
    "the apply of a revision takes no body: it names the revision in the path alone";

/// `POST` [`APPLY`]: applies the revision through the reload path.
async fn apply(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !body.is_empty() {
        return error::response(
            Code::BodyInvalid,
            APPLY_TAKES_NO_BODY,
            request_id::of(&headers).unwrap_or_default(),
        );
    }
    let managed = Arc::clone(&managed);
    blocking(&headers, move || {
        let id = number(&id)?;
        let revision = held(&managed.revisions, id)?;
        let current = managed
            .revisions
            .apply(&managed.reloader, id, Author::from(&operator))?;
        record(APPLIED, id, &operator);
        Ok(Applied { revision, current })
    })
    .await
}

/// `POST` [`ROLLBACK`]: saves the content of the revision before the
/// running one as a new revision, and applies it.
async fn rollback(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(request) = serde_json::from_slice::<RollbackRequest>(&body) else {
        return body_refused(&headers, ROLLBACK_SHAPE);
    };
    let managed = Arc::clone(&managed);
    blocking(&headers, move || {
        let (revision, current) = managed.revisions.rollback(
            &managed.reloader,
            request.current,
            (Author::from(&operator), request.reason),
        )?;
        record(ROLLED_BACK, revision.id, &operator);
        Ok(Applied { revision, current })
    })
    .await
}
