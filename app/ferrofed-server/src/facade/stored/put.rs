// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Storing a definition: `PUT {base}/v1/definition/query/{name}/{version}`
//! at the version named, and `PUT {base}/v1/definition/query/{name}` at the
//! version the registry assigns, each a version of its own that never
//! replaces a held one (§12.7, N44; ITS-REST Definition API).

use std::sync::Arc;
use std::time::Instant;

use axum::response::{IntoResponse, Response};
use ferrofed_eehrxf::reserved;
use ferrofed_engine::declared::query;
use ferrofed_registry::definition::store::{Assignment, Definitions, Insertion, StoreError};
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};
use http::{HeaderMap, StatusCode, header};
use jiff::Timestamp;
use openehr_its::rest::generated::definition::{
    DefinitionQueryStoreYamlParams, DefinitionQueryVersionStoreYamlParams,
};
use openehr_its::rest::routes::RouteMatch;

use crate::config::public_url::PublicUrl;
use crate::error::Code;
use crate::facade::request;
use crate::facade::route::Arrived;
use crate::facade::security;
use crate::facade::stored::distribution::{self, Registry};
use crate::facade::stored::location::{self, Answered};
use crate::facade::stored::{AQL, QUERY_STRING_INVALID, Refused, VERSION_PARAM, name, segment};
use crate::federation::Federation;
use crate::stored::{self, Inadmissible};
/// Checks the query string of a `PUT`: only the declared `query_type`, and
/// only AQL (ITS-REST Definition API).
///
/// Parameter names are held to those the operation declares, by position,
/// and the query string is decoded by the operation's generated parameters
/// (RFC 3986 §2.1, so a `+` is a literal plus).
fn query_string(matched: &RouteMatch, arrived: &Arrived<'_>) -> Result<(), Refused> {
    let query = arrived.uri.query();
    if let Some(query) = query {
        query::every_declared(matched, query).map_err(|unlisted| {
            security::query_parameter_refused(unlisted.position, &arrived.outbound.to_string());
            Refused::with(Code::QueryParameterRefused, &unlisted)
        })?;
    }
    let headers = &HeaderMap::new();
    let query_type = if matched.path_param(VERSION_PARAM).is_some() {
        DefinitionQueryVersionStoreYamlParams::from_request(matched, query, headers)
            .map(|params| params.query_type)
    } else {
        DefinitionQueryStoreYamlParams::from_request(matched, query, headers)
            .map(|params| params.query_type)
    }
    .map_err(|_named| Refused::with(Code::BodyInvalid, &QUERY_STRING_INVALID))?;
    match query_type {
        Some(language) if !language.eq_ignore_ascii_case(AQL) => {
            Err(Refused::fixed(Code::QueryTypeUnsupported))
        }
        Some(_) | None => Ok(()),
    }
}

/// `PUT {base}/v1/definition/query/{name}/{version}`: stores the definition
/// unless its name and version are held (§12.7, N44). `PUT
/// {base}/v1/definition/query/{name}` stores it at the version the registry
/// assigns, `1.0.0` for a name it holds no version of, else the next patch
/// of the newest it holds, and never replaces a held version (§12.7
/// stored-query-versioning; ITS-REST Definition API, `definition_query_store`).
///
/// The body is the AQL text (ITS-REST `text/plain`), and a `Content-Type`
/// naming another media type is a `415` before anything is read or stored.
/// A name in the namespace the gateway reserves for its own read-only
/// queries is a `409` (`stored-query-reserved`) at any version, as a held
/// version is (ITS-REST `409_StoredQuery_version`; RFC 9110 §15.5.10).
/// It is analysed as a
/// façade query, with every `$parameter` standing in for a value an
/// invocation binds, and refused with a `400` when the rewrite would refuse
/// it whatever is bound, or when it names its patient by a literal, before
/// any version is assigned. The text
/// held is the canonical print of the parse. A stored definition answers
/// `200` with `Location` naming its URL ([`location::location`]; ITS-REST
/// `200_StoredQuery_stored`).
///
/// A request naming members for distribution is refused before anything is
/// stored when its targeting cannot be answered, or when the definition
/// carries a `FROM ENDPOINT` or `ORGANISATION` directive (§12.7
/// fanout-endpoint-targeted-refused); otherwise the definition is stored and
/// then distributed at the version stored ([`distribution::distribute`]).
pub(super) async fn store(
    federation: &Federation,
    (definitions, public): (&Arc<Definitions>, Option<&PublicUrl>),
    matched: &RouteMatch,
    arrived: &Arrived<'_>,
) -> Result<Response, Refused> {
    let started = Instant::now();
    let logged = arrived.outbound.to_string();
    let ids = (arrived.request_id, logged.as_str());
    if let Some(response) =
        request::unsupported_media(matched, (arrived.headers, &arrived.body), ids)
    {
        return Ok(response);
    }
    let name = name(matched)?;
    if reserved::reserves(&name) {
        return Err(Refused::fixed(Code::StoredQueryReserved));
    }
    let version = match matched.path_param(VERSION_PARAM) {
        Some(_) => Some(
            segment(matched, VERSION_PARAM)
                .unwrap_or_default()
                .parse::<QueryVersion>()
                .map_err(|refused| Refused::with(Code::QueryVersionInvalid, &refused))?,
        ),
        None => None,
    };
    query_string(matched, arrived)?;
    let distributed = distribution::requested(federation, arrived.headers)?;
    let text =
        std::str::from_utf8(&arrived.body).map_err(|_text| Refused::fixed(Code::BodyInvalid))?;
    let admitted = stored::admit(text, federation.context()).map_err(|refused| match refused {
        Inadmissible::Refused(refusal) => {
            security::refused(&refusal, &logged);
            Refused::with(Code::Refused((&refusal).into()), &refusal)
        }
        Inadmissible::SubjectLiteral { at } => {
            security::definition_refused(at.as_ref(), &logged);
            Refused::fixed(Code::SubjectLiteral)
        }
    })?;
    if distributed.is_some() {
        distribution::distributable(admitted.aql(), &logged)?;
    }
    let stored = kept(definitions, (name, version), admitted.aql(), &logged).await?;
    let answered = if version.is_some() {
        Answered::Versioned
    } else {
        Answered::Unversioned
    };
    let location = location::location(&stored, answered, public)?;
    let mut response = match distributed {
        Some(selected) => {
            let sent = (Registry::Stored, &selected);
            let sent_as = (arrived.outbound, &arrived.conveyance, started);
            distribution::distribute(federation, &stored, sent, sent_as).await?
        }
        None => StatusCode::OK.into_response(),
    };
    if let Some(location) = location {
        response.headers_mut().insert(header::LOCATION, location);
    }
    Ok(response)
}

/// The message of an unversioned `PUT` whose every assigned version another
/// writer stored first.
const ASSIGNMENT_CONTENDED: &str = "another request stored each version the registry assigned \
     first, and a stored version is immutable: send the PUT again (§12.7, N44)";

/// The message of an unversioned `PUT` whose newest held version has no next
/// patch.
const ASSIGNMENT_EXHAUSTED: &str = "the newest version held has the largest patch a version \
     holds, so the registry assigns none: store at a version (§12.7, N44)";

/// What the store did with a `PUT`'s definition.
enum Kept {
    /// Stored, at the version named or assigned.
    Stored(StoredDefinition),
    /// The version named is held.
    Held,
    /// Another writer stored each version assigned first.
    Contended,
    /// The newest held version has no next patch.
    Exhausted,
}

/// Stores `aql` under `name` at `version`, or with no version at the one the
/// registry assigns, and returns the definition stored.
///
/// A held version, or an assignment another writer won each time or that has
/// no next version, is a `409` (`stored-query-held`), and nothing is stored
/// (§12.7, N44; ITS-REST `409_StoredQuery_version`).
async fn kept(
    definitions: &Arc<Definitions>,
    (name, version): (QueryName, Option<QueryVersion>),
    aql: &str,
    logged: &str,
) -> Result<StoredDefinition, Refused> {
    let (aql, saved, held) = (aql.to_owned(), Timestamp::now(), Arc::clone(definitions));
    // NOTE: no specification governs this: our own design; the insert commits
    // to disk, so it runs where a blocking call may wait.
    let inserted = tokio::task::spawn_blocking(move || match version {
        Some(version) => {
            let definition = StoredDefinition::new(name, version, aql, saved);
            let insertion = held.insert(definition.clone())?;
            Ok::<Kept, StoreError>(match insertion {
                Insertion::Stored => Kept::Stored(definition),
                Insertion::Held => Kept::Held,
            })
        }
        None => Ok(match held.insert_assigned(&name, &aql, saved)? {
            Assignment::Stored(definition) => Kept::Stored(definition),
            Assignment::Contended => Kept::Contended,
            Assignment::Exhausted => Kept::Exhausted,
        }),
    })
    .await;
    match inserted {
        Ok(Ok(Kept::Stored(definition))) => Ok(definition),
        Ok(Ok(Kept::Held)) => Err(Refused::fixed(Code::StoredQueryHeld)),
        Ok(Ok(Kept::Contended)) => Err(Refused::with(Code::StoredQueryHeld, &ASSIGNMENT_CONTENDED)),
        Ok(Ok(Kept::Exhausted)) => Err(Refused::with(Code::StoredQueryHeld, &ASSIGNMENT_EXHAUSTED)),
        Ok(Err(failure)) => {
            tracing::error!(
                error = crate::chain(&failure),
                request_id = logged,
                "a stored query could not be stored"
            );
            Err(Refused::fixed(Code::Internal))
        }
        Err(failure) => {
            tracing::error!(
                error = %failure,
                request_id = logged,
                "the stored-query insert did not complete"
            );
            Err(Refused::fixed(Code::Internal))
        }
    }
}
