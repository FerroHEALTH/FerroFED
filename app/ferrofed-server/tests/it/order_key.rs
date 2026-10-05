// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A client `ORDER BY` key AQL defines no order for, through the façade
//! (§11.6.1, AQL master03-syntax §ORDER BY). With a `LIMIT` it is refused
//! `400` before any node is asked, since a node's first rows on such a key
//! need not hold the federated first rows. With no `LIMIT` every row reaches
//! the Tier, which orders them all, so the answer does not depend on the
//! order a node returned them in.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_rm::v1_2::data_types::text::dv_text::DvText;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

use crate::facade::{body, gateway, post, received, registry, schema};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// One synthetic `COMPOSITION` a node holds: its uid and the `value` of its
/// `DV_TEXT` name.
type Named = (&'static str, &'static str);

/// A node answering `SELECT c/uid/value, c/name` with each name as a whole
/// `DV_TEXT`, in an order of its own that turns on every call: AQL defines no
/// order on a `DV_TEXT` (AQL master03-syntax §ORDER BY).
struct TextNode {
    held: Vec<Named>,
    calls: AtomicUsize,
}

impl Respond for TextNode {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let mut held = self.held.clone();
        if !held.is_empty() {
            let turn = call % held.len();
            held.rotate_left(turn);
        }
        let rows: Vec<String> = held
            .iter()
            .map(|(uid, name)| format!(r#"["{uid}",{{"_type":"DV_TEXT","value":"{name}"}}]"#))
            .collect();
        let answer = format!(r#"{{"q":"node","rows":[{}]}}"#, rows.join(","));
        ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json")
    }
}

async fn text_node(held: &[Named]) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(TextNode {
            held: held.to_vec(),
            calls: AtomicUsize::new(0),
        })
        .mount(&server)
        .await;
    server
}

const NAMES_A: [Named; 2] = [("a-uid-1", "beta"), ("a-uid-2", "Alpha")];
const NAMES_B: [Named; 2] = [("b-uid-1", "alpha"), ("b-uid-2", "Beta")];

/// The answer's rows, each a uid and a `DV_TEXT` name.
#[derive(Debug, Deserialize)]
struct Answer {
    rows: Vec<(String, DvText)>,
}

/// The value of a row's name, which each node sent as a plain `DV_TEXT`.
fn text_value(uid: String, name: DvText) -> Result<(String, String), String> {
    match name {
        DvText::DvText(text) => Ok((uid, text.value)),
        DvText::DvCodedText(coded) => Err(format!(
            "the name of {uid} came back a DV_CODED_TEXT: {}",
            coded.value
        )),
    }
}

// conformance: CP-32
#[tokio::test]
async fn a_limit_on_a_dv_text_key_is_refused_400_and_asks_no_node() -> TestResult {
    let a = text_node(&NAMES_A).await;
    let b = text_node(&NAMES_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let aql = "SELECT c/uid/value, c/name FROM EHR e CONTAINS COMPOSITION c \
               ORDER BY c/name LIMIT 2";

    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "incomparable-order-key",
        error_body(&text)?.code,
        "AQL §ORDER BY defines no order for a DV_TEXT, so a node's first two rows need not \
         hold the federated first two (§11.6.1)"
    );
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "a refused query asks no node"
        );
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn top_on_a_dv_text_key_is_refused_400_and_asks_no_node() -> TestResult {
    let a = text_node(&NAMES_A).await;
    let b = text_node(&NAMES_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let aql = "SELECT TOP 2 c/uid/value FROM EHR e CONTAINS COMPOSITION c ORDER BY c/name DESC";

    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "incomparable-order-key",
        error_body(&text)?.code,
        "AQL §TOP: TOP n is read as LIMIT n, which each node would be sent"
    );
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "a refused query asks no node"
        );
    }
    Ok(())
}

// conformance: CP-32
#[tokio::test]
async fn with_no_limit_a_dv_text_key_is_answered_in_one_order_on_every_repeat() -> TestResult {
    let a = text_node(&NAMES_A).await;
    let b = text_node(&NAMES_B).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &registry(&a.uri(), &b.uri(), ""), "", "")?;
    let aql = "SELECT c/uid/value, c/name FROM EHR e CONTAINS COMPOSITION c ORDER BY c/name";

    let mut answers = Vec::new();
    for _ in 0..2 {
        let (status, text) = call(app.clone(), post(body(aql)?)?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        schema::validate(&text)?;
        let answer: Answer = serde_json::from_str(&text)?;
        answers.push(
            answer
                .rows
                .into_iter()
                .map(|(uid, name)| text_value(uid, name))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let row = |uid: &str, name: &str| (uid.to_owned(), name.to_owned());
    assert_eq!(
        Some(&vec![
            row("a-uid-2", "Alpha"),
            row("b-uid-2", "Beta"),
            row("b-uid-1", "alpha"),
            row("a-uid-1", "beta"),
        ]),
        answers.first(),
        "every row, in the Tier's own order on the canonical JSON of the name"
    );
    assert_eq!(
        answers.first(),
        answers.get(1),
        "§11.6.1: repeating a query returns rows in the same order, though each node \
         returned its rows in another order"
    );
    for server in [&a, &b] {
        for sent in received(server).await? {
            assert!(
                sent.contains("ORDER BY c/name, c/uid/value ASC") && !sent.contains("LIMIT"),
                "the key is sent as written, with the uid tie-break and no LIMIT: {sent}"
            );
        }
    }
    Ok(())
}
