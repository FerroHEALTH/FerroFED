// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every page against the stub gateway: each answers whole in the HTML the
//! server sends, needs a signed-in session, and carries the browser
//! security headers.

use std::error::Error;

use http::StatusCode;

use crate::stub::{gateway, signed_in_console};
use crate::support::{get, get_as, header, send};

/// Every page path the console serves a signed-in operator, with words
/// each must show.
const PAGES: [(&str, &str); 18] = [
    ("/", "Nodes that need attention"),
    ("/topology", "The federation as a map"),
    ("/organisations", "Organisations"),
    ("/organisations/nodes", "All nodes"),
    ("/services", "Identity cross-reference"),
    ("/services/resolver", "Resolves a patient"),
    ("/integrity", "Two nodes claim one EHR id"),
    ("/integrity?show=resolved", "Resolved"),
    ("/query", "Run"),
    ("/query/stored", "Stored query"),
    ("/stored-queries", "org.example::compositions"),
    ("/stored-queries/org.example::compositions", "SELECT"),
    ("/self-description", "Federation"),
    ("/self-description/limits", "Per node"),
    ("/configuration", "Members"),
    ("/configuration/history", "History"),
    ("/preferences", "Appearance"),
    ("/about", "Manufacturer"),
];

#[tokio::test]
async fn every_page_answers_whole_with_its_words_and_the_security_headers()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let dump = std::env::var("FERROFED_CONSOLE_DUMP").ok();
    for (path, words) in PAGES {
        let (response, body) = send(&service, get_as(path, &session)?).await?;
        if let Some(dir) = &dump {
            let name = path.trim_matches('/').replace(['/', '?', '=', ':'], "_");
            std::fs::write(
                format!(
                    "{dir}/{}.html",
                    if name.is_empty() { "dashboard" } else { &name }
                ),
                &body,
            )?;
        }
        assert_eq!(StatusCode::OK, response.status(), "{path}: {body}");
        assert!(body.contains(words), "{path} shows {words}: {body}");
        assert!(body.contains("</html>"), "{path} is whole");
        assert_eq!("no-store", header(&response, "cache-control"), "{path}");
        crate::support::strict(header(&response, "content-security-policy"), &body)
            .map_err(|refused| format!("{path}: {refused}"))?;
        assert!(
            !body.contains(crate::support::OPERATOR_TOKEN),
            "{path} never shows the token"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_page_with_no_session_sends_the_visitor_to_sign_in() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for (path, _) in PAGES
        .into_iter()
        .filter(|(path, _)| !matches!(*path, "/" | "/about"))
    {
        let (response, _body) = send(&service, get(path)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{path}");
    }
    let (response, body) = send(&service, get("/")?).await?;
    assert_eq!(
        StatusCode::SERVICE_UNAVAILABLE,
        response.status(),
        "no provider is configured"
    );
    assert!(
        body.contains("<title>Sign in · FerroFED Console</title>"),
        "{body}"
    );
    Ok(())
}

// A query posts to its own page, which answers with every endpoint's part
// and the rows, and neither the AQL nor a parameter reaches the log.
#[tokio::test]
async fn a_query_answers_on_its_own_page_and_logs_nothing_it_carried() -> Result<(), Box<dyn Error>>
{
    let gateway = gateway().await?;
    let answer = crate::stub::result_set_example()?;
    crate::stub::answers(&gateway, ("POST", "/v1/query/aql"), 200, &answer).await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let captured = crate::support::Captured::default();
    let guard = captured.logging();
    let (response, body) = send(
        &service,
        crate::stub::post_as(
            "/query",
            &session,
            &[
                ("kind", "aql"),
                ("aql", "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = $ehr"),
                ("parameter-name-1", "ehr"),
                ("parameter-value-1", "synthetic-parameter-value"),
            ],
        )?,
    )
    .await?;
    drop(guard);
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert!(
        body.contains("8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1"),
        "{body}"
    );
    assert!(body.contains("node_3"), "{body}");
    let logged = captured.text();
    assert!(!logged.contains("synthetic-parameter-value"), "{logged}");
    assert!(!logged.contains("COMPOSITION"), "{logged}");
    Ok(())
}

// Add and Remove of a parameter row post the form and change the rows, and
// no query runs.
#[tokio::test]
async fn a_parameter_row_is_added_and_removed_by_a_plain_post() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (_response, body) = send(
        &service,
        crate::stub::post_as(
            "/query",
            &session,
            &[
                ("kind", "aql"),
                ("parameter-name-1", "a"),
                ("parameter-value-1", "1"),
                ("add-parameter", "1"),
            ],
        )?,
    )
    .await?;
    assert!(body.contains("parameter-name-1"), "{body}");
    assert!(body.contains("parameter-name-new-1"), "{body}");
    assert!(
        body.contains("Run a query to see its answer here."),
        "{body}"
    );
    let (_response, body) = send(
        &service,
        crate::stub::post_as(
            "/query",
            &session,
            &[
                ("kind", "aql"),
                ("parameter-name-1", "a"),
                ("parameter-value-1", "1"),
                ("remove-parameter", "1"),
            ],
        )?,
    )
    .await?;
    assert!(!body.contains(r#"id="parameter-name-1""#), "{body}");
    Ok(())
}

// A page a post answers carries the strict policy and loads FerroUI's one
// loader module from the console's own origin, as every page does.
#[tokio::test]
async fn a_page_a_post_answers_carries_the_strict_policy() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let (response, body) = send(
        &service,
        crate::stub::post_as(
            "/query",
            &session,
            &[("kind", "aql"), ("add-parameter", "1")],
        )?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    crate::support::strict(header(&response, "content-security-policy"), &body)?;
    Ok(())
}

#[tokio::test]
async fn the_node_and_endpoint_lists_show_the_median_and_the_95th_percentile()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let options = crate::stub::options_example()?.replace(
        "\"latency_ms_p50\": 120",
        "\"latency_ms_p50\": 120, \"latency_ms_p95\": 480",
    );
    assert!(
        options.contains("480"),
        "the §7a.2 example names a p50 of 120"
    );
    wiremock::Mock::given(wiremock::matchers::method("OPTIONS"))
        .and(wiremock::matchers::path("/"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_raw(options.into_bytes(), "application/json"),
        )
        .with_priority(1)
        .mount(&gateway)
        .await;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for path in ["/organisations/nodes", "/self-description/endpoints"] {
        let (response, body) = send(&service, get_as(path, &session)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{path}: {body}");
        for words in [">p50<", ">p95<", "120 ms", "480 ms", "last 100 requests"] {
            assert!(body.contains(words), "{path} shows {words}: {body}");
        }
        assert!(
            body.contains("Not measured yet"),
            "{path}: an endpoint with no p95 shows a dash"
        );
    }
    Ok(())
}
