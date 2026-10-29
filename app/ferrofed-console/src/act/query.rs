// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A query run through the gateway as the operator.
//!
//! The form the query console posts is read and checked on the server,
//! sent as the gateway's own ITS-REST query, and its federated answer read
//! for the page (§9, §11.1, §11.4, N37).
//!
//! The form posts to a server function, so nothing the operator entered, a
//! patient named in a parameter among it, reaches a URL, the history or a
//! log (N33); the answer is drawn into the page and kept nowhere. A press
//! of a parameter row's Remove or of Add posts the form too, and the page
//! answers with the rows changed and no query run (FerroUI F7).
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]

use std::collections::BTreeSet;
use std::fmt;

#[cfg(not(target_arch = "wasm32"))]
use std::collections::BTreeMap;

use ferrohealth_ui::i18n::Locale;
use leptos::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use openehr_federation::headers::{
    COMPLETENESS, COMPLETENESS_PARTIAL, DEDUP, ENDPOINT, ORGANISATION,
};
use openehr_federation::options::OptionsRoot;
use openehr_federation::outcome::ErrorDetail;
#[cfg(not(target_arch = "wasm32"))]
use openehr_its::rest::generated::query::QueryParameters;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
use crate::act::field;
use crate::act::fields::Fields;
#[cfg(not(target_arch = "wasm32"))]
use crate::gateway::{FederatedAnswer, QueryCall, QueryTarget};
use crate::locale::{Msg, tr_in};
use crate::read::{Fault, Refusal, Stop};

/// One parameter row of the form: its key on the page, its name and its
/// value.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterRow {
    /// The row's key, which names its fields.
    pub key: String,
    /// The parameter's name, with or without its `$`.
    pub name: String,
    /// The value as the operator entered it.
    pub value: String,
}

/// The query console's form as the browser posts it.
///
/// Its `Debug` output names which fields are filled, never what they hold,
/// because the AQL text and the parameter values can name a patient (N33).
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryForm {
    /// `aql` or `stored`.
    pub kind: String,
    /// The AQL text.
    pub aql: String,
    /// The stored query's qualified name.
    pub name: String,
    /// The stored query's version, when one is named.
    pub version: String,
    /// The parameters, one row each.
    pub parameters: Vec<ParameterRow>,
    /// The first row to answer.
    pub offset: String,
    /// How many rows to answer.
    pub fetch: String,
    /// Not empty when the operator asks for a partial answer.
    pub partial: String,
    /// The dedup mode.
    pub dedup: String,
    /// The endpoints to ask, each by id.
    pub endpoints: Vec<String>,
    /// The organisations to ask, each by id.
    pub organisations: Vec<String>,
}

impl fmt::Debug for ParameterRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParameterRow")
            .field("key", &self.key)
            .field("value", &!self.value.trim().is_empty())
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for QueryForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = |text: &str| !text.trim().is_empty();
        f.debug_struct("QueryForm")
            .field("kind", &self.kind)
            .field("aql", &filled(&self.aql))
            .field("name", &filled(&self.name))
            .field("parameters", &self.parameters.len())
            .finish_non_exhaustive()
    }
}

/// The prefix of a parameter row's name field, before its key.
pub const PARAMETER_NAME: &str = "parameter-name-";

/// The prefix of a parameter row's value field, before its key.
pub const PARAMETER_VALUE: &str = "parameter-value-";

/// The field Add submits.
pub const ADD: &str = "add-parameter";

/// The field a row's Remove submits, its value the row's key.
pub const REMOVE: &str = "remove-parameter";

impl QueryForm {
    /// The form the posted `fields` hold.
    #[must_use]
    pub fn of(fields: &[(String, String)]) -> Self {
        let text = |name: &str| {
            fields
                .iter()
                .rev()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.replace("\r\n", "\n"))
                .unwrap_or_default()
        };
        let mut parameters: Vec<ParameterRow> = Vec::new();
        for (key, value) in fields {
            if let Some(row) = key.strip_prefix(PARAMETER_NAME) {
                parameters.push(ParameterRow {
                    key: row.to_owned(),
                    name: value.clone(),
                    value: text(&format!("{PARAMETER_VALUE}{row}")),
                });
            }
        }
        let listed = |name: &str| -> Vec<String> {
            fields
                .iter()
                .filter(|(key, _)| key == name)
                .map(|(_, value)| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .collect()
        };
        Self {
            kind: text("kind"),
            aql: text("aql"),
            name: text("name"),
            version: text("version"),
            parameters,
            offset: text("offset"),
            fetch: text("fetch"),
            partial: text("partial"),
            dedup: text("dedup"),
            endpoints: listed("endpoint"),
            organisations: listed("organisation"),
        }
    }
}

/// What a post of the query console asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Asking {
    /// Run the query.
    Run,
    /// Add a parameter row.
    Add,
    /// Remove the parameter row of this key.
    Remove(String),
}

/// One endpoint's part of a federated answer (§9.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointLine {
    /// The `endpoint_id`.
    pub id: String,
    /// Its status (§11.1).
    pub status: openehr_federation::status::EndpointStatus,
    /// How long it took, in milliseconds.
    pub latency_ms: Option<u64>,
    /// How many rows it gave.
    pub row_count: Option<u64>,
    /// Why it failed, as its error says.
    pub error: Option<String>,
}

/// A federated answer as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryAnswer {
    /// The status the gateway answered with.
    pub status: u16,
    /// Whether every endpoint asked answered (§11.4).
    pub complete: bool,
    /// Each endpoint's part.
    pub endpoints: Vec<EndpointLine>,
    /// The columns, each its name.
    pub columns: Vec<String>,
    /// The rows, each cell as text.
    pub rows: Vec<Vec<String>>,
    /// The whole result set as indented JSON.
    pub raw: String,
    /// How long the query took at the console, in milliseconds.
    pub elapsed_ms: Option<u64>,
}

/// What a run gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ran {
    /// The gateway answered.
    Answered(QueryAnswer),
    /// The gateway refused, or the form cannot be sent.
    Refused(Refusal),
    /// The console could not reach the gateway or read its answer.
    Fault(Fault),
}

impl From<Stop> for Ran {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Refused(refusal) => Self::Refused(refusal),
            Stop::Fault(fault) => Self::Fault(fault),
        }
    }
}

/// The query console's post and what it gave, which the page draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryPost {
    /// The form as posted, its rows changed by Add or Remove.
    pub form: QueryForm,
    /// What the run gave, when the post ran the query.
    pub ran: Option<Ran>,
}

/// Runs the query the console's form `fields` hold through the gateway as
/// the operator of the request, or changes its parameter rows for an Add
/// or a Remove and runs nothing.
///
/// # Errors
/// Returns [`crate::read::Fault::Unavailable`] when the console's server
/// does not answer; a form that cannot be sent and the gateway's refusal
/// are answers.
#[server(endpoint = "query")]
pub async fn run_query(
    /// The query console's form, every field as posted.
    #[server(flatten)]
    fields: Fields,
) -> Result<QueryPost, Stop> {
    Ok(posted(fields.pairs()).await)
}

/// The query console's post of `fields` and what it gave.
#[cfg(not(target_arch = "wasm32"))]
async fn posted(fields: &[(String, String)]) -> QueryPost {
    let mut form = QueryForm::of(fields);
    let asking = if field(fields, ADD).is_some() {
        Asking::Add
    } else if let Some(key) = field(fields, REMOVE) {
        Asking::Remove(key.to_owned())
    } else {
        Asking::Run
    };
    let ran = match asking {
        Asking::Add => {
            let next = crate::pages::query::fresh_key(&form.parameters);
            form.parameters.push(ParameterRow {
                key: next,
                ..ParameterRow::default()
            });
            None
        }
        Asking::Remove(key) => {
            form.parameters.retain(|row| row.key != key);
            None
        }
        Asking::Run => Some(run(&form).await),
    };
    QueryPost { form, ran }
}

/// Runs `form` through the gateway as the operator of the request.
#[cfg(not(target_arch = "wasm32"))]
async fn run(form: &QueryForm) -> Ran {
    let Ok((state, token)) = crate::read::signed_in() else {
        return Ran::Refused(Refusal::NotAuthenticated { code: None });
    };
    let locale = ferrohealth_ui::i18n::locale();
    let call = match call(&locale, form) {
        Ok(call) => call,
        Err(refused) => return Ran::Refused(refused),
    };
    let started = std::time::Instant::now();
    match state.gateway().query(&token, &call).await {
        Ok(federated) => {
            let mut read = answer(&locale, &federated);
            read.elapsed_ms = u64::try_from(started.elapsed().as_millis()).ok();
            Ran::Answered(read)
        }
        Err(error) => match crate::read::stopped(&error, crate::read::Asked::Query) {
            Stop::Refused(refused) => Ran::Refused(refused),
            Stop::Fault(fault) => Ran::Fault(fault),
        },
    }
}

/// What `description` offers a query: its dedup modes, whether it takes a
/// partial answer, its endpoints and its organisations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryOptions {
    /// The dedup modes the gateway declares.
    pub dedup_modes: Vec<String>,
    /// Whether the gateway offers a best-effort answer.
    pub best_effort: bool,
    /// Every endpoint, by id.
    pub endpoints: Vec<String>,
    /// Every organisation, by id.
    pub organisations: Vec<String>,
}

impl QueryOptions {
    /// The options `description` offers.
    #[must_use]
    pub fn of(description: &OptionsRoot) -> Self {
        let gateway = &description.federation;
        let organisations: BTreeSet<&str> = description
            .endpoints
            .iter()
            .map(|endpoint| endpoint.organisation.as_str())
            .collect();
        Self {
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
}

#[cfg(not(target_arch = "wasm32"))]
/// A refusal of the form, naming the field and never its value.
fn invalid(reason: String) -> Refusal {
    Refusal::Invalid { reason }
}

#[cfg(not(target_arch = "wasm32"))]
/// The text of `field`, trimmed, or `None` when it is empty.
fn given(field: &str) -> Option<&str> {
    let text = field.trim();
    (!text.is_empty()).then_some(text)
}

#[cfg(not(target_arch = "wasm32"))]
/// The gateway call `form` describes, its refusals in `locale`.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for a form that names no query, a parameter
/// with no name or given twice, an `offset` or `fetch` that is not a whole
/// number from 0, or a header value that is not legal on the wire. No
/// message quotes what the operator entered.
pub fn call(locale: &Locale, form: &QueryForm) -> Result<QueryCall, Refusal> {
    let says = |msg: Msg| tr_in(locale, msg, &[]);
    let target = match form.kind.as_str() {
        "stored" => QueryTarget::Stored {
            name: given(&form.name)
                .ok_or_else(|| invalid(says(Msg::QueryNameStored)))?
                .to_owned(),
            version: given(&form.version).map(str::to_owned),
        },
        _ => QueryTarget::Aql(
            given(&form.aql)
                .ok_or_else(|| invalid(says(Msg::QueryWriteAql)))?
                .to_owned(),
        ),
    };
    let mut headers = Vec::new();
    if !form.endpoints.is_empty() {
        headers.push((ENDPOINT, header_value(locale, &form.endpoints.join(", "))?));
    }
    if !form.organisations.is_empty() {
        headers.push((
            ORGANISATION,
            header_value(locale, &form.organisations.join(", "))?,
        ));
    }
    if let Some(dedup) = given(&form.dedup) {
        headers.push((DEDUP, header_value(locale, dedup)?));
    }
    if given(&form.partial).is_some() {
        headers.push((COMPLETENESS, COMPLETENESS_PARTIAL.to_owned()));
    }
    Ok(QueryCall {
        target,
        offset: whole(locale, Msg::QueryOffset, &form.offset)?,
        fetch: whole(locale, Msg::QueryFetch, &form.fetch)?,
        parameters: parameters(locale, &form.parameters)?,
        headers,
    })
}

#[cfg(not(target_arch = "wasm32"))]
/// `value` as a header value, refused when it is not legal on the wire.
fn header_value(locale: &Locale, value: &str) -> Result<String, Refusal> {
    http::HeaderValue::from_str(value)
        .map(|_legal| value.to_owned())
        .map_err(|_illegal| invalid(tr_in(locale, Msg::QueryHeaderIllegal, &[])))
}

#[cfg(not(target_arch = "wasm32"))]
/// The whole number `text` holds, or `None` when it is empty.
fn whole(locale: &Locale, field: Msg, text: &str) -> Result<Option<i64>, Refusal> {
    let Some(text) = given(text) else {
        return Ok(None);
    };
    match text.parse::<i64>() {
        Ok(number) if number >= 0 => Ok(Some(number)),
        _ => Err(invalid(tr_in(
            locale,
            Msg::QueryWholeNumber,
            &[("field", tr_in(locale, field, &[]).into())],
        ))),
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The query parameters `rows` hold, each value sent as a string.
fn parameters(locale: &Locale, rows: &[ParameterRow]) -> Result<QueryParameters, Refusal> {
    let mut parameters = BTreeMap::new();
    for (index, row) in rows.iter().enumerate() {
        let number = u32::try_from(index.saturating_add(1)).unwrap_or(u32::MAX);
        let name = row.name.trim();
        let name = name.strip_prefix('$').unwrap_or(name);
        if name.is_empty() && row.value.trim().is_empty() {
            continue;
        }
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid(tr_in(
                locale,
                Msg::QueryParameterName,
                &[("number", number.into())],
            )));
        }
        let value = serde_json::Value::String(row.value.trim().to_owned());
        if parameters.insert(name.to_owned(), value).is_some() {
            return Err(invalid(tr_in(
                locale,
                Msg::QueryParameterTwice,
                &[("number", number.into())],
            )));
        }
    }
    Ok(parameters)
}

#[cfg(not(target_arch = "wasm32"))]
/// What the page draws of `federated`.
#[must_use]
pub fn answer(locale: &Locale, federated: &FederatedAnswer) -> QueryAnswer {
    let federation = &federated.federation;
    QueryAnswer {
        status: federated.status.as_u16(),
        complete: federation.complete(),
        endpoints: federation
            .endpoints()
            .iter()
            .map(|endpoint| EndpointLine {
                id: endpoint.id().as_str().to_owned(),
                status: endpoint.status(),
                latency_ms: endpoint.outcome().latency_ms(),
                row_count: endpoint.row_count(),
                error: endpoint
                    .outcome()
                    .error()
                    .map(|error| error_text(locale, error)),
            })
            .collect(),
        columns: federated
            .result_set
            .columns
            .iter()
            .flatten()
            .map(|column| column.name.clone())
            .collect(),
        rows: federated
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
            .collect(),
        raw: serde_json::to_string_pretty(&federated.result_set)
            .unwrap_or_else(|_unprintable| tr_in(locale, Msg::QueryRawUnprintable, &[])),
        elapsed_ms: None,
    }
}

/// The text of an endpoint's error: a message as itself, an object as its
/// JSON.
#[must_use]
pub fn error_text(locale: &Locale, error: &ErrorDetail) -> String {
    match error {
        ErrorDetail::Text(message) => message.clone(),
        ErrorDetail::Object(_) => serde_json::to_string(error)
            .unwrap_or_else(|_unprintable| tr_in(locale, Msg::QueryRawUnprintable, &[])),
    }
}

/// Whether `status` is the gateway's whole answer.
#[must_use]
pub const fn succeeded(status: u16) -> bool {
    status == 200
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::i18n::Locale;

    use super::{QueryForm, call};

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn the_form_reads_its_rows_and_its_lists() {
        let form = QueryForm::of(&fields(&[
            ("kind", "aql"),
            ("aql", "SELECT c FROM COMPOSITION c\r\nWHERE c/x = $x"),
            ("parameter-name-a", "$x"),
            ("parameter-value-a", "1"),
            ("endpoint", "node-a-pub"),
            ("endpoint", "node-b-pub"),
        ]));
        assert_eq!("SELECT c FROM COMPOSITION c\nWHERE c/x = $x", form.aql);
        assert_eq!(1, form.parameters.len());
        assert_eq!(vec!["node-a-pub", "node-b-pub"], form.endpoints);
        let call = call(&Locale::english(), &form).expect("a call");
        assert_eq!(1, call.parameters.len());
        assert_eq!(1, call.headers.len());
    }

    #[test]
    fn a_form_that_names_no_query_or_a_bad_number_is_refused_without_quoting_it() {
        let english = Locale::english();
        assert!(call(&english, &QueryForm::of(&fields(&[("kind", "aql")]))).is_err());
        let bad = QueryForm::of(&fields(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("offset", "-3"),
        ]));
        let refused = format!("{:?}", call(&english, &bad).err());
        assert!(!refused.contains("-3"), "{refused}");
        let twice = QueryForm::of(&fields(&[
            ("aql", "SELECT 1"),
            ("parameter-name-a", "x"),
            ("parameter-value-a", "1"),
            ("parameter-name-b", "x"),
            ("parameter-value-b", "2"),
        ]));
        assert!(call(&english, &twice).is_err());
    }
}
