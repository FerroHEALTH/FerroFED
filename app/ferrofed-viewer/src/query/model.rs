// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the query console posts, and what it renders of the gateway's answer.
//!
//! The form crosses to the server in a `POST` body alone, and its `Debug`
//! output names which fields are filled, never what they hold, because the
//! AQL text and the parameter values can name a patient (N33). The answer
//! holds what the gateway answered the operator, read out of the federated
//! `RESULT_SET` and its `meta.federation` on the server; every integer is of
//! a fixed size, because the browser half is 32-bit.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The query console's form, as the browser posts it.
///
/// Every field is the text the operator entered, parsed on the server; a
/// checkbox left clear and a field the browser does not send are empty.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueryForm {
    /// `aql` for an AQL query, `stored` for a stored query by name.
    pub kind: String,
    /// The AQL text.
    pub aql: String,
    /// The stored query's qualified name.
    pub name: String,
    /// The stored query's version, when the operator names one.
    pub version: String,
    /// The query parameters, one `name=value` per line.
    pub parameters: String,
    /// The ITS-REST `offset`.
    pub offset: String,
    /// The ITS-REST `fetch`.
    pub fetch: String,
    /// Not empty when the operator opts into best-effort completion (§11.4).
    pub partial: String,
    /// The dedup mode to ask for, empty for the gateway's default (§10).
    pub dedup: String,
    /// The endpoints to target, comma-separated (§8.4).
    pub endpoints: String,
    /// The organisation to target (§8.4).
    pub organisation: String,
}

impl fmt::Debug for QueryForm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled = |text: &str| !text.trim().is_empty();
        let kind = match self.kind.as_str() {
            "aql" => "aql",
            "stored" => "stored",
            _ => "other",
        };
        f.debug_struct("QueryForm")
            .field("kind", &kind)
            .field("aql", &filled(&self.aql))
            .field("name", &filled(&self.name))
            .field("parameters", &filled(&self.parameters))
            .field("partial", &filled(&self.partial))
            .finish_non_exhaustive()
    }
}

/// What the gateway's self-description offers a query: the choices the form
/// shows, as `OPTIONS {base}/` declares them (§7a.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryOptionsView {
    /// The dedup modes the gateway offers (§10, N15).
    pub dedup_modes: Vec<String>,
    /// Whether the gateway offers best-effort completion (§11.4, N37).
    pub best_effort: bool,
    /// Every member endpoint, by `endpoint_id`, for targeting (§8.4).
    pub endpoints: Vec<String>,
    /// Every managing organisation, once each, for targeting (§8.4).
    pub organisations: Vec<String>,
}

/// A federated answer as the server rendered it for the page: the status
/// and completeness the answer carries, and its HTML.
///
/// Its `Debug` output names the status, the completeness and the size of the
/// HTML alone: the HTML holds the rows, which can name a patient (N33).
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedAnswer {
    /// The status the gateway answered with.
    pub status: u16,
    /// `meta.federation.complete` (§11.4, N37).
    pub complete: bool,
    /// The answer rendered as HTML: its status and completeness in words,
    /// every endpoint, and the rows.
    pub html: String,
}

/// One column of the answer, as `columns[]` names it (§9.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnLine {
    /// The column's name.
    pub name: String,
    /// The column's path, when the gateway gave one.
    pub path: Option<String>,
}

/// One endpoint's record in `meta.federation.endpoints[]` (§9.5, §11.1).
///
/// Its `Debug` output leaves out the error text a node wrote, which is the
/// node's and can quote what it was asked.
#[derive(Clone, PartialEq, Eq)]
pub struct EndpointLine {
    /// The `endpoint_id`.
    pub id: String,
    /// The §11.1 status, as the wire spells it.
    pub status: String,
    /// The latency the gateway measured, in milliseconds, when it dispatched.
    pub latency_ms: Option<u64>,
    /// How many rows the endpoint contributed, when the gateway says.
    pub row_count: Option<u64>,
    /// The managing organisation.
    pub organisation: Option<String>,
    /// The node-reported or gateway-reported error.
    pub error: Option<String>,
}

/// What the gateway answered a query: its status, whether the answer is
/// complete, every endpoint's record, and the rows.
///
/// It is rendered on the server and never crosses to the browser as data.
/// Its `Debug` output names the status, the completeness and the counts
/// alone: a row can carry a patient identifier the query selected (N33).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct QueryAnswer {
    /// The status the gateway answered with: `200`, or the `504` or `424` of
    /// an all-or-nothing query a node in scope did not answer (§11.4).
    pub status: u16,
    /// Whether that status is `200 OK`, the answer of a query that ran.
    pub succeeded: bool,
    /// `meta.federation.complete`: whether every node in scope answered
    /// (§11.4, N37).
    pub complete: bool,
    /// Every endpoint the gateway reports, in its order.
    pub endpoints: Vec<EndpointLine>,
    /// The dedup mode the gateway applied, when it says (§10.2).
    pub dedup: Option<String>,
    /// How many rows the dedup suppressed, when it says.
    pub suppressed_rows: Option<u64>,
    /// The columns, in row order.
    pub columns: Vec<ColumnLine>,
    /// The rows, each cell as text: a string as itself, any other value as
    /// its JSON.
    pub rows: Vec<Vec<String>>,
}

impl fmt::Debug for RenderedAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenderedAnswer")
            .field("status", &self.status)
            .field("complete", &self.complete)
            .field("html_bytes", &self.html.len())
            .finish()
    }
}

impl fmt::Debug for EndpointLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EndpointLine")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("latency_ms", &self.latency_ms)
            .field("row_count", &self.row_count)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for QueryAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryAnswer")
            .field("status", &self.status)
            .field("complete", &self.complete)
            .field("endpoints", &self.endpoints.len())
            .field("columns", &self.columns.len())
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{EndpointLine, QueryAnswer, QueryForm, RenderedAnswer};

    const PATIENT: &str = "synthetic-patient-48151623";

    #[test]
    fn no_debug_output_prints_a_row_an_error_or_what_was_entered() {
        let endpoint = EndpointLine {
            id: String::from("node_1"),
            status: String::from("node-error"),
            latency_ms: Some(1),
            row_count: None,
            organisation: None,
            error: Some(format!("no EHR for {PATIENT}")),
        };
        let answer = QueryAnswer {
            status: 200,
            succeeded: true,
            complete: true,
            endpoints: vec![endpoint.clone()],
            rows: vec![vec![PATIENT.to_owned()]],
            ..QueryAnswer::default()
        };
        let rendered = RenderedAnswer {
            status: 200,
            complete: true,
            html: format!("<td>{PATIENT}</td>"),
        };
        let form = QueryForm {
            kind: PATIENT.to_owned(),
            aql: PATIENT.to_owned(),
            parameters: format!("patient={PATIENT}"),
            ..QueryForm::default()
        };
        let shown = format!("{endpoint:?} {answer:?} {rendered:?} {form:?}");
        assert!(!shown.contains(PATIENT), "{shown}");
        assert!(shown.contains(r#"kind: "other""#), "{shown}");
        assert!(shown.contains("rows: 1"), "{shown}");
    }
}
