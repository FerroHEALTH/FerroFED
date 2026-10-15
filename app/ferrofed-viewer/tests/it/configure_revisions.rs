// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A revision downloaded as TOML and TOML imported as a dry-run revision,
//! against a stub admin listener: each revision of the history offers its
//! download, which the console's server reads from the first replica with
//! the operator's own token and answers as an attachment no cache keeps, a
//! refusal shown with its status and code; an import sends the TOML to the
//! replica alone, shows what it changes escaped, and a plain form post or an
//! empty TOML asks no replica.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::session::COOKIE;
use http::{Request, StatusCode};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::configure::{answers, changes, configured, form, signed_in_console};
use crate::support::{OPERATOR_TOKEN, console, get_as, send};

/// Revision 2 runs, applied by the file.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":2,"previous":1,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":2,"in_step":true,"refused":null}"#;

/// No member, stored query or section.
const RESOURCES: &str = r#"{"running":2,"head":2,"members":[],"stored_query_registry":"absent",
    "stored_queries":[]}"#;

/// The history: revision 2 imported, revision 1 by the file.
const HISTORY: &str = r#"{"items":[
    {"id":2,"base":1,"origin":"import","restores":null,
     "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
     "at":"2026-10-15T09:00:00Z","reason":"from git","toml":"x = 1"},
    {"id":1,"base":null,"origin":"file","restores":null,"author":{"kind":"file"},
     "at":"2026-10-15T08:00:00Z","reason":"the configuration file was loaded","toml":""}],
    "offset":0,"total":2}"#;

/// Revision 2 as the export writes it.
const EXPORTED: &str = "# FerroFED configuration revision 2\n\n[federation]\nbest_effort = false\n";

/// Revision 3, the import, with markup in what it changes.
const IMPORTED: &str = r#"{"revision":{"id":3,"base":2,"origin":"import","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"from git","toml":"x = 1"},
    "diff":{"from":2,"to":3,"changes":[
      {"key":"federation.<i>id</i>","change":"changed","from":"\"a\"","to":"\"<script>x</script>\" onerror=\"1"}]}}"#;

/// The TOML the operator imports.
const TOML: &str = "[federation]\nbest_effort = true\n";

/// An admin listener with revision 2 running.
async fn listener() -> Server {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 200, STANDING).await;
    answers(
        &listener,
        ("GET", "/admin/config/resources"),
        200,
        RESOURCES,
    )
    .await;
    answers(&listener, ("GET", "/admin/config/revisions"), 200, HISTORY).await;
    listener
}

/// Mounts on `listener` the export of revision 2, for the operator's token.
async fn exports(listener: &Server) {
    Mock::given(method("GET"))
        .and(path("/admin/config/revisions/2/export"))
        .and(header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/toml; charset=utf-8")
                .set_body_string(EXPORTED),
        )
        .mount(listener)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn each_revision_offers_its_download_and_the_page_offers_an_import()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, body) = send(&service, get_as("/configuration", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    for id in [1, 2] {
        let link = format!(
            r#"<a href="/configuration/revisions/{id}/export" rel="external">Export revision {id} as TOML</a>"#
        );
        assert!(body.contains(&link), "{body}");
    }
    assert!(body.contains("import, a dry run until applied"), "{body}");
    assert!(body.contains("Import a configuration as TOML"), "{body}");
    assert!(
        body.contains(r#"<textarea id="import-toml" name="form[toml]""#),
        "{body}"
    );
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_is_the_replicas_toml_as_an_attachment_never_cached()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    exports(&listener).await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, body) = send(
        &service,
        get_as("/configuration/revisions/2/export", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    assert_eq!(EXPORTED, body);
    let headers = response.headers();
    assert_eq!(
        Some("attachment; filename=\"ferrofed-revision-2.toml\""),
        headers
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
    );
    assert_eq!(
        Some("no-store"),
        headers.get("cache-control").and_then(|v| v.to_str().ok())
    );
    assert!(
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|media| media.starts_with("application/toml")),
        "{headers:?}"
    );
    assert!(!body.contains(OPERATOR_TOKEN));
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_download_without_a_session_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    exports(&listener).await;
    let (_state, service) = console(&format!(
        "[gateway]\nbase_url = \"http://127.0.0.1:9/\"\n\n[session]\nsecure_cookie = false\n\n\
         [[admin.replicas]]\nname = \"gateway-0\"\nadmin_url = \"{}\"\n",
        listener.uri()
    ))?;
    let request = Request::get("/configuration/revisions/2/export")
        .header("cookie", format!("{COOKIE}=forged-session"))
        .body(Body::empty())?;
    let (response, body) = send(&service, request).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status(), "{body}");
    assert!(!body.contains("best_effort"), "{body}");
    let asked = listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "{} requests reached the replica",
        asked.len()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_download_shows_the_status_and_code() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("GET", "/admin/config/revisions/9/export"),
        404,
        r#"{"message":"no revision 9 is held","code":"revision-unknown"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, body) = send(
        &service,
        get_as("/configuration/revisions/9/export", &session)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, response.status(), "{body}");
    assert!(body.contains("404 (revision-unknown)"), "{body}");
    assert!(response.headers().get("content-disposition").is_none());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_sends_the_toml_to_the_replica_and_shows_its_changes_escaped()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(&listener, ("POST", "/admin/config/import"), 201, IMPORTED).await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "import"),
            ("base", "2"),
            ("toml", TOML),
            ("reason", "from git & co"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 imported revision 3 as a dry run."),
        "{html}"
    );
    assert!(html.contains("against the running revision 2"), "{html}");
    for raw in ["<i>id</i>", "<script>x</script>"] {
        assert!(!html.contains(raw), "{raw} reached the answer raw: {html}");
    }
    assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"), "{html}");
    let received = listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let imports: Vec<_> = received
        .iter()
        .filter(|request| request.url.path() == "/admin/config/import")
        .collect();
    let [import] = imports.as_slice() else {
        return Err(format!("one import, not {}", imports.len()).into());
    };
    assert_eq!("POST", import.method.as_str());
    let pairs: Vec<(String, String)> = import
        .url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        vec![
            (String::from("base"), String::from("2")),
            (String::from("reason"), String::from("from git & co")),
        ],
        pairs
    );
    assert_eq!(
        Some("application/toml"),
        import
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
    );
    assert_eq!(
        Some(format!("Bearer {OPERATOR_TOKEN}").as_str()),
        import
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
    );
    assert_eq!(TOML.as_bytes(), import.body.as_slice());
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_import_shows_the_status_and_code() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    answers(
        &listener,
        ("POST", "/admin/config/import"),
        409,
        r#"{"message":"[credentials.\"node-a-pub\"] is provided by the configuration file","code":"resource-file-owned"}"#,
    )
    .await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "import"),
            ("base", "2"),
            ("toml", TOML),
            ("reason", "from git"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 refused the import: 409 (resource-file-owned)."),
        "{html}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_import_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        form(&[
            ("action", "import"),
            ("base", "2"),
            ("toml", "  \n"),
            ("reason", "from git"),
            ("confirm", "yes"),
        ]),
    )
    .await?;
    assert!(html.contains("Paste the TOML to import."), "{html}");
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// A plain form post, which a page without its bundle sends, asks no replica.
#[tokio::test]
async fn a_plain_import_post_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    let (service, session) = signed_in_console(&listener)?;
    let request = Request::post("/api/configure")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(form(&[
            ("action", "import"),
            ("base", "2"),
            ("toml", TOML),
            ("reason", "from git"),
            ("confirm", "yes"),
        ])))?;
    let (response, body) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status(), "{body}");
    assert!(body.contains("PlainPost"), "{body}");
    let asked = listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "{} requests reached the replica",
        asked.len()
    );
    Ok(())
}
