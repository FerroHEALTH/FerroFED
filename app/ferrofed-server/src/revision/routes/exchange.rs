// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A revision exported as TOML, and TOML imported as a dry-run revision, on
//! the admin listener, so a configuration moves between the console and
//! version control.
//!
//! The export is the revision's TOML text under a comment header naming the
//! revision, its author and when it was saved. A revision holds every
//! secret as its `_file` reference alone, a secret the console wrote to
//! `config.secrets_dir` among them, so the export holds no secret value; it
//! holds the console's members and sections beside the file's, and never the
//! bootstrap core, which no revision holds.
//!
//! The import saves the TOML it is sent as a new revision on the base it
//! names, checked as every save is, and answers it with the keys it changes
//! against the running revision; it applies nothing, and the operator applies
//! it as any revision, with its approval and its confirmation. A change to a
//! member, a credential or a binding section the file provides is refused
//! `409` (`resource-file-owned`), as the console's own saves are. No
//! specification governs this: our own design.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Extension, Path, Query, State};
use axum::response::{IntoResponse, Response};
use ferrofed_registry::revision::{Author, Diff, Imported, Origin, Revision, RevisionId};
use http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::Deserialize;

use super::console::{ConsoleError, created, guarded};
use super::{Managed, answered, held, number, record, refused};
use crate::admin::Operator;
use crate::error::{self, Code};
use crate::request_id;
use crate::revision::service::{RevisionError, held_table};
use crate::revision::tree::{CORE, diff};

/// The export of one revision as TOML.
pub const EXPORT: &str = "/admin/config/revisions/{id}/export";

/// The import of TOML as a dry-run revision.
pub const IMPORT: &str = "/admin/config/import";

/// The largest TOML body an import takes, in bytes.
pub const IMPORT_LIMIT: usize = 1_048_576;

/// The media type of TOML an import takes beside `text/plain`.
pub const TOML: &str = "application/toml";

/// The `event` of the record of an imported revision.
pub const IMPORTED: &str = "config-revision-imported";

/// The query of an import.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ImportQuery {
    /// The revision the import is based on, which must be the newest.
    base: Option<String>,
    /// Why the revision is made.
    reason: String,
}

/// The parameters an import takes.
const IMPORT_SHAPE: &str = "an import takes the query ?base=<revision>&reason=<text> and a TOML body of at most 1048576 bytes";

/// Who `author` is, as the export's header writes it; every text is quoted
/// with its control characters escaped, so it stays on its comment line.
fn author_said(author: &Author) -> String {
    match author {
        Author::File => String::from("the configuration file"),
        Author::Operator { issuer, subject } => {
            format!("operator {subject:?} of issuer {issuer:?}")
        }
        Author::DevelopmentLoopback => {
            String::from("a loopback peer under the development profile")
        }
        Author::Gateway => String::from("the gateway, rolling back an unconfirmed revision"),
        _ => String::from("an author this gateway does not name"),
    }
}

/// Where a revision came from, as the export's header writes it.
fn origin_said(revision: &Revision) -> String {
    match (revision.origin, revision.restores) {
        (_, Some(restored)) => format!("a rollback to the content of revision {restored}"),
        (Origin::File, None) => String::from("the configuration file"),
        (Origin::Import, None) => String::from("an import"),
        _ => String::from("an operator's save"),
    }
}

/// `revision` as TOML: a comment header naming it, then its configuration.
#[must_use]
pub fn exported(revision: &Revision) -> String {
    let core = CORE.join(", ");
    format!(
        "# FerroFED configuration revision {id}, saved at {at}\n\
         # by {author}, from {origin}.\n\
         # It is the configuration less its bootstrap core ({core}), which the\n\
         # configuration file alone sets: no revision holds it, so it is not exported.\n\
         # Every secret is its `_file` reference, never its value.\n\
         # Import it with POST /admin/config/import to record it as a dry-run revision.\n\n\
         {toml}",
        id = revision.id,
        at = revision.at,
        author = author_said(&revision.author),
        origin = origin_said(revision),
        toml = revision.toml,
    )
}

/// `GET` [`EXPORT`]: the revision as TOML, an attachment never cached.
pub(super) async fn export(
    State(managed): State<Arc<Managed>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    let revisions = Arc::clone(&managed.revisions);
    match tokio::task::spawn_blocking(move || held(&revisions, number(&id)?)).await {
        Ok(Ok(revision)) => toml_answer(&revision),
        Ok(Err(refusal)) => refused(&refusal, &request_id),
        Err(failure) => {
            tracing::error!(error = %failure, "a configuration export did not complete");
            error::fixed(Code::Internal, &request_id)
        }
    }
}

/// The `200` of `revision` exported.
fn toml_answer(revision: &Revision) -> Response {
    let disposition = format!(
        "attachment; filename=\"ferrofed-revision-{}.toml\"",
        revision.id
    );
    let mut response = (StatusCode::OK, exported(revision)).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/toml; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    response
}

/// Whether `headers` name TOML or plain text as the body's media type.
fn is_toml(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|media| {
            media.eq_ignore_ascii_case(TOML) || media.eq_ignore_ascii_case("text/plain")
        })
}

/// `POST` [`IMPORT`]: saves the TOML body as a dry-run revision on the
/// query's base, `201` with it and its changes against the running revision.
pub(super) async fn import(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    query: Result<Query<ImportQuery>, QueryRejection>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    let Ok(Query(query)) = query else {
        return error::response(Code::BodyInvalid, IMPORT_SHAPE, &request_id);
    };
    if !is_toml(&headers) {
        return error::response(
            Code::MediaTypeUnsupported,
            "an import takes a body of Content-Type application/toml or text/plain",
            &request_id,
        );
    }
    let Ok(bytes) = axum::body::to_bytes(body, IMPORT_LIMIT).await else {
        return error::response(Code::BodyInvalid, IMPORT_SHAPE, &request_id);
    };
    let Ok(text) = String::from_utf8(bytes.to_vec()) else {
        return error::response(
            Code::BodyInvalid,
            "an import's TOML body is UTF-8 text",
            &request_id,
        );
    };
    created(
        answered(&headers, move || {
            let base = query.base.as_deref().map(number).transpose()?;
            let imported = imported(&managed, (base, query.reason), &operator, &text)?;
            record(IMPORTED, imported.revision.id, &operator);
            Ok::<_, ConsoleError>(imported)
        })
        .await,
    )
}

/// Saves `text` as a dry-run revision on `base` for `reason` by `operator`
/// through [`guarded`], and returns it with its changes against the running
/// revision.
fn imported(
    managed: &Managed,
    (base, reason): (Option<RevisionId>, String),
    operator: &Operator,
    text: &str,
) -> Result<Imported, ConsoleError> {
    let running = managed
        .revisions
        .store()
        .current()
        .map_err(RevisionError::from)?
        .ok_or(RevisionError::NoCurrent)?;
    let saved = guarded(
        managed,
        (Origin::Import, base),
        (Author::from(operator), reason),
        text,
    )?;
    let from = held(&managed.revisions, running.revision)?;
    Ok(Imported {
        diff: Diff {
            from: from.id,
            to: saved.id,
            changes: diff(&held_table(&from)?, &held_table(&saved)?),
        },
        revision: saved,
    })
}

#[cfg(test)]
mod tests {
    use ferrofed_registry::revision::{Author, Origin, Revision, RevisionId};

    use super::exported;

    #[test]
    fn the_header_keeps_an_author_on_its_comment_line() {
        let revision = Revision {
            id: RevisionId::FIRST,
            base: None,
            origin: Origin::Operator,
            restores: None,
            author: Author::Operator {
                issuer: String::from("https://idp.example.org"),
                subject: String::from("op\n[auth]\nissuer = \"x\""),
            },
            at: jiff::Timestamp::UNIX_EPOCH,
            reason: String::from("r"),
            toml: String::from("[federation]\nbest_effort = false\n"),
            console: Vec::new(),
        };
        let text = exported(&revision);
        let table: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(vec!["federation"], table.keys().collect::<Vec<_>>());
        assert!(text.starts_with("# FerroFED configuration revision 1, saved at"));
        assert!(text.contains("bootstrap core (profile, server, metrics, auth, trust, config)"));
    }
}
