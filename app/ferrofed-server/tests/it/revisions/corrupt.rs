// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stored revision whose configuration does not read is refused as a
//! damaged store, and the log names where the fault is and what kind it
//! is, never a line of the configuration. No specification governs this:
//! our own design.

use crate::support::console::subscriber;
use ferrofed_registry::revision::{Author, Origin};
use ferrofed_server::revision::{Draft, Saved};
use ferrohealth_telemetry::section::Rendering;
use http::StatusCode;
use jiff::Timestamp;

use super::{Gateway, TestResult};
use crate::support::Logs;

/// A value the planted line carries, which no log line may hold.
const PLANTED: &str = "synthetic-planted-5521";

#[tokio::test]
async fn a_corrupt_stored_revision_is_logged_without_its_text() -> TestResult {
    let gateway = Gateway::start("")?;
    let store = gateway.revisions.store();
    let head = store.head()?.ok_or("the file load is a revision")?;
    let draft = Draft {
        origin: Origin::File,
        restores: None,
        author: Author::File,
        at: Timestamp::UNIX_EPOCH,
        reason: String::from("synthetic"),
        toml: format!("[registry]\ndocument = \"https://cdr.example.org/x?key={PLANTED}\n"),
        console: Vec::new(),
    };
    let Saved::Stored(planted) = store.save(Some(head), draft)? else {
        return Err("the planted revision is stored".into());
    };

    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "error", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, body) = gateway
        .get(&format!(
            "/admin/config/revisions/{}/diff/{}",
            head.get(),
            planted.id.get()
        ))
        .await?;
    drop(guard);

    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{body}");
    assert!(!body.contains(PLANTED), "the answer carries the line");
    let logged = logs.text();
    assert!(
        logged.contains("the configuration revision store failed"),
        "the failure is logged"
    );
    assert!(
        logged.contains("registry.document") && logged.contains("line 2"),
        "the log names the key and the line"
    );
    for line in logged.lines() {
        assert!(
            !line.contains(PLANTED),
            "a log line carries the planted value"
        );
        assert!(
            !line.contains("cdr.example.org"),
            "a log line carries the planted line"
        );
    }
    Ok(())
}
