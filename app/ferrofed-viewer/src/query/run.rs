// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server functions of the query console: what the gateway offers a
//! query, and the query itself.
//!
//! Each is a public HTTP endpoint of the console, so each checks the
//! operator's session itself before it asks the gateway anything
//! (`server/25_server_functions`, the Leptos book). The query reaches the
//! console in a `POST` body and the gateway in a `POST` body, and nothing it
//! carries is logged: a refusal is logged by status and code alone (N33).

use leptos::prelude::*;

use crate::query::model::{QueryForm, QueryOptionsView, RenderedAnswer};
use crate::views::model::ViewError;

/// Loads what the gateway's self-description offers a query, `OPTIONS
/// {base}/` (§7a.2).
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// gateway's refusal or failure as its [`ViewError`].
#[server(endpoint = "query-options")]
pub async fn query_options() -> Result<QueryOptionsView, ViewError> {
    let (state, token) = crate::views::load::server::signed_in()?;
    let description = state
        .gateway()
        .self_description(&token)
        .await
        .map_err(|error| crate::views::load::server::refused(&error))?;
    Ok(server::options(&description))
}

/// Runs the query `form` describes through the gateway as the operator.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session,
/// [`ViewError::Invalid`] for a form that cannot be sent, and the gateway's
/// refusal or failure as its [`ViewError`]. A failing all-or-nothing answer
/// that carries the diagnostic envelope is an answer, not an error (§11.4).
#[server(endpoint = "query")]
pub async fn run_query(
    /// The query console's form.
    form: QueryForm,
) -> Result<RenderedAnswer, ViewError> {
    let (state, token) = crate::views::load::server::signed_in()?;
    let call = server::call(&form)?;
    let answer = state
        .gateway()
        .query(&token, &call)
        .await
        .map_err(|error| crate::views::load::server::refused(&error))?;
    let answer = server::answer(&answer);
    Ok(RenderedAnswer {
        status: answer.status,
        complete: answer.complete,
        html: crate::query::answer::html(&answer),
    })
}

/// The server half of the query console: the form read into a gateway call,
/// and the gateway's answer read into what the console renders.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use std::collections::{BTreeMap, BTreeSet};

    use http::{HeaderValue, StatusCode};
    use openehr_federation::headers::{
        COMPLETENESS, COMPLETENESS_PARTIAL, DEDUP, ENDPOINT, ORGANISATION,
    };
    use openehr_federation::options::OptionsRoot;
    use openehr_federation::outcome::ErrorDetail;
    use openehr_its::rest::generated::query::QueryParameters;

    use crate::gateway::{FederatedAnswer, QueryCall, QueryTarget};
    use crate::query::model::{ColumnLine, EndpointLine, QueryAnswer, QueryForm, QueryOptionsView};
    use crate::views::model::ViewError;

    /// The choices `description` offers a query.
    #[must_use]
    pub fn options(description: &OptionsRoot) -> QueryOptionsView {
        let gateway = &description.federation;
        let organisations: BTreeSet<&str> = description
            .endpoints
            .iter()
            .map(|endpoint| endpoint.organisation.as_str())
            .collect();
        QueryOptionsView {
            dedup_modes: gateway.dedup.modes.as_slice().to_vec(),
            best_effort: gateway.completeness.best_effort(),
            endpoints: description
                .endpoints
                .iter()
                .map(|endpoint| endpoint.id.to_string())
                .collect(),
            organisations: organisations.into_iter().map(str::to_owned).collect(),
        }
    }

    /// A refusal of the form, naming the field and never its value.
    fn invalid(reason: impl Into<String>) -> ViewError {
        ViewError::Invalid {
            reason: reason.into(),
        }
    }

    /// The text of `field`, trimmed, or `None` when it is empty.
    fn given(field: &str) -> Option<&str> {
        let text = field.trim();
        (!text.is_empty()).then_some(text)
    }

    /// The gateway call `form` describes.
    ///
    /// # Errors
    /// Returns [`ViewError::Invalid`] for a form that names no query, a
    /// parameter that is not `name=value` or is given twice, an `offset` or
    /// `fetch` that is not a whole number from 0, or a header value that is
    /// not legal on the wire. No message quotes what the operator entered.
    pub fn call(form: &QueryForm) -> Result<QueryCall, ViewError> {
        let target = match form.kind.as_str() {
            "aql" => QueryTarget::Aql(
                given(&form.aql)
                    .ok_or_else(|| invalid("Write the AQL query to run."))?
                    .to_owned(),
            ),
            "stored" => QueryTarget::Stored {
                name: given(&form.name)
                    .ok_or_else(|| invalid("Name the stored query to run."))?
                    .to_owned(),
                version: given(&form.version).map(str::to_owned),
            },
            _ => return Err(invalid("Choose an AQL query or a stored query.")),
        };
        let mut headers = Vec::new();
        let endpoints: Vec<&str> = form
            .endpoints
            .split(',')
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .collect();
        if !endpoints.is_empty() {
            headers.push((ENDPOINT, header_value("endpoints", &endpoints.join(", "))?));
        }
        if let Some(organisation) = given(&form.organisation) {
            headers.push((ORGANISATION, header_value("organisation", organisation)?));
        }
        if let Some(dedup) = given(&form.dedup) {
            headers.push((DEDUP, header_value("dedup mode", dedup)?));
        }
        if given(&form.partial).is_some() {
            headers.push((COMPLETENESS, COMPLETENESS_PARTIAL.to_owned()));
        }
        Ok(QueryCall {
            target,
            offset: whole("offset", &form.offset)?,
            fetch: whole("fetch", &form.fetch)?,
            parameters: parameters(&form.parameters)?,
            headers,
        })
    }

    /// `value` as the value of the header the form calls `field`.
    fn header_value(field: &str, value: &str) -> Result<String, ViewError> {
        HeaderValue::from_str(value)
            .map(|_legal| value.to_owned())
            .map_err(|_illegal| invalid(format!("The {field} cannot be sent as a header.")))
    }

    /// The whole number `text` holds, or `None` when it is empty.
    fn whole(field: &str, text: &str) -> Result<Option<i64>, ViewError> {
        let Some(text) = given(text) else {
            return Ok(None);
        };
        match text.parse::<i64>() {
            Ok(number) if number >= 0 => Ok(Some(number)),
            _ => Err(invalid(format!("The {field} is a whole number from 0."))),
        }
    }

    /// The query parameters `text` holds, one `name=value` per line, each
    /// value sent as a string.
    fn parameters(text: &str) -> Result<QueryParameters, ViewError> {
        let mut parameters = BTreeMap::new();
        for (index, line) in text.lines().enumerate() {
            let number = index.saturating_add(1);
            if line.trim().is_empty() {
                continue;
            }
            let Some((name, value)) = line.split_once('=') else {
                return Err(invalid(format!(
                    "Parameter line {number} is not name=value."
                )));
            };
            let name = name.trim();
            let name = name.strip_prefix('$').unwrap_or(name);
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(invalid(format!(
                    "Parameter line {number} does not start with a parameter name."
                )));
            }
            let value = serde_json::to_value(value.trim()).map_err(|_unencodable| {
                invalid(format!("Parameter line {number} cannot be sent."))
            })?;
            if parameters.insert(name.to_owned(), value).is_some() {
                return Err(invalid(format!(
                    "Parameter line {number} names a parameter given before."
                )));
            }
        }
        Ok(parameters)
    }

    /// What the console renders of `answer`.
    #[must_use]
    pub fn answer(answer: &FederatedAnswer) -> QueryAnswer {
        let federation = &answer.federation;
        let endpoints = federation
            .endpoints()
            .iter()
            .map(|endpoint| EndpointLine {
                id: endpoint.id().as_str().to_owned(),
                status: endpoint.status().as_str().to_owned(),
                latency_ms: endpoint.outcome().latency_ms(),
                row_count: endpoint.row_count(),
                organisation: endpoint.organisation().map(str::to_owned),
                error: endpoint.outcome().error().map(error_text),
            })
            .collect();
        let columns = answer
            .result_set
            .columns
            .iter()
            .flatten()
            .map(|column| ColumnLine {
                name: column.name.clone(),
                path: column.path.clone(),
            })
            .collect();
        let rows = answer
            .result_set
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        cell.as_str()
                            .map_or_else(|| cell.to_string(), str::to_owned)
                    })
                    .collect()
            })
            .collect();
        QueryAnswer {
            status: answer.status.as_u16(),
            succeeded: answer.status == StatusCode::OK,
            complete: federation.complete(),
            endpoints,
            dedup: federation.dedup().and_then(|dedup| dedup.mode.clone()),
            suppressed_rows: federation.dedup().and_then(|dedup| dedup.suppressed_rows),
            columns,
            rows,
        }
    }

    /// The text of an endpoint's error: a message as itself, an object as
    /// its JSON.
    fn error_text(error: &ErrorDetail) -> String {
        match error {
            ErrorDetail::Text(message) => message.clone(),
            ErrorDetail::Object(_) => serde_json::to_string(error).unwrap_or_else(|_unprintable| {
                String::from("an error object the console cannot print")
            }),
        }
    }
}
