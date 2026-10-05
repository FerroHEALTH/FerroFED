// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The documents and settings a reload refuses or reports, the running registry kept.

use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use ferrofed_server::reload::{Applied, ReloadError};
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::StatusCode;

use crate::facade::{EHR_A, crossref, node_answering};
use crate::support::Logs;

use super::{Gateway, TOKEN_C, TestResult, ids, member};

#[tokio::test]
async fn an_invalid_document_is_refused_and_the_running_registry_stays() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    let sentinel = "SENTINEL-DOCUMENT-CONTENT-7xq";
    gateway.write_registry(&format!(
        "{}\n[[node]]\nid = \"{sentinel}\"\n",
        member("a", &a.uri())
    ))?;

    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());

    let Err(refused) = reloaded else {
        return Err("a document the boot check refuses is refused".into());
    };
    assert!(
        matches!(refused, ReloadError::Federation { .. }),
        "{refused:?}"
    );
    assert_eq!("registry-invalid", refused.class());
    assert!(
        Arc::ptr_eq(&running, &gateway.federation()?),
        "the running registry stays"
    );
    let text = logs.text();
    assert!(text.contains("registry-invalid"), "{text}");
    assert!(
        text.contains(&gateway.document.display().to_string()),
        "the refusal names the document: {text}"
    );
    assert!(
        !text.contains(sentinel),
        "the refusal never quotes the document: {text}"
    );
    let (status, answer) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_document_that_drops_the_demographic_endpoint_is_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let tables =
        String::from("demographic_endpoint = \"node-b-pub\"\n") + &crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(
        &(member("a", &a.uri()) + &member("b", &b.uri())),
        "",
        &tables,
    )?;
    let running = gateway.federation()?;
    gateway.write_registry(&member("a", &a.uri()))?;

    let refused =
        gateway.reloader.reload().err().ok_or(
            "a demographic endpoint the document no longer declares is refused, as at boot",
        )?;
    assert_eq!("demographic-endpoint", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_changed_demographic_endpoint_takes_a_restart() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let registry = member("a", &a.uri()) + &member("b", &b.uri());
    let rows = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(
        &registry,
        "",
        &(String::from("demographic_endpoint = \"node-a-pub\"\n") + &rows),
    )?;
    gateway.write_config(
        "",
        &(String::from("demographic_endpoint = \"node-b-pub\"\n") + &rows),
    )?;

    let applied = gateway.reloader.reload()?;
    assert_eq!(
        vec!["federation.demographic_endpoint"],
        applied.needs_restart
    );
    Ok(())
}

#[tokio::test]
async fn a_changed_trace_collector_takes_a_restart() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let rows = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(&member("a", &a.uri()), "", &rows)?;
    gateway.write_config(
        "",
        &(rows + "\n[telemetry]\notlp_endpoint = \"http://127.0.0.1:4317\"\n"),
    )?;

    let applied = gateway.reloader.reload()?;
    assert_eq!(vec!["telemetry.otlp_endpoint"], applied.needs_restart);
    Ok(())
}

#[tokio::test]
async fn a_changed_trace_sample_ratio_takes_a_restart() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let rows = crossref(&[("node-a", EHR_A)]);
    let gateway = Gateway::start(&member("a", &a.uri()), "", &rows)?;
    gateway.write_config("", &(rows + "\n[telemetry]\ntrace_sample_ratio = 0.1\n"))?;

    let applied = gateway.reloader.reload()?;
    assert_eq!(vec!["telemetry.trace_sample_ratio"], applied.needs_restart);
    Ok(())
}

#[tokio::test]
async fn an_unreadable_document_is_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    std::fs::remove_file(&gateway.document)?;

    let refused = gateway.reloader.reload().err().ok_or("refused")?;
    assert_eq!("registry-unreadable", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_configuration_that_does_not_load_is_refused() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    let credentials =
        format!("[credentials.\"node-a-pub\"]\nbearer_token = \"{TOKEN_C}\"\nuser = \"both\"\n");

    let logs = Logs::default();
    gateway.write_config("", &(crossref(&[("node-a", EHR_A)]) + "\n" + &credentials))?;
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let reloaded = tracing::subscriber::with_default(capture, || gateway.reloader.reload());

    let refused = reloaded
        .err()
        .ok_or("two schemes are refused, as at boot")?;
    assert_eq!("configuration", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    let text = logs.text();
    assert!(!text.contains(TOKEN_C), "no credential is logged: {text}");
    assert!(
        text.contains(&gateway.config.display().to_string()),
        "the refusal names the configuration file: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn setting_or_unsetting_the_registry_takes_a_restart() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    let running = gateway.federation()?;
    std::fs::write(&gateway.config, "profile = \"development\"\n")?;

    let refused = gateway.reloader.reload().err().ok_or("refused")?;
    assert!(
        matches!(refused, ReloadError::RegistryPresence),
        "{refused:?}"
    );
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_changed_setting_outside_the_registry_is_reported_and_the_rest_applies() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    gateway.write_registry(&(member("a", &a.uri()) + &member("b", &b.uri())))?;
    gateway.write_config(
        "[server]\nlisten = \"127.0.0.1:18080\"",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let applied: Applied = gateway.reloader.reload()?;
    assert_eq!(vec!["server.listen"], applied.needs_restart);
    assert_eq!(ids::<EndpointId>(&["node-b-pub"])?, applied.endpoints_added);
    Ok(())
}
