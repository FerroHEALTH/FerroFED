// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings tabs of the configuration page against a stub admin
//! listener: each setting drawn from the running configuration with its
//! value, its source, its key and its Next start mark, a secret by its file
//! alone, an Edit whose plain form post saves a revision with only the
//! settings the operator changed, and the tabs drawn read-only when the
//! gateway's configuration revisions are off.

use std::error::Error;

use ferrofed_testkit::mock::Server;
use http::StatusCode;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::configure::{SAVED, answers, form, listener, posted, signed_in_console};
use crate::support::{get_as, send};

/// The running configuration revision 1 sets: a per-node timeout the next
/// start changes, a node selection, the store's connection by its file and
/// a category map.
const RUNNING: &str = r#"{"ui":"managed","revision":1,"settings":[
    {"key":"federation.per_node_timeout_ms","section":"federation","value":5000,"source":"revision",
     "default":10000,"restart":true,"next":5500,"editable":true},
    {"key":"federation.node_selection","section":"federation","value":"localized","source":"revision",
     "restart":true,"editable":true},
    {"key":"federation.best_effort","section":"federation","value":true,"source":"default",
     "default":true,"restart":true,"editable":true},
    {"key":"federation.consent.disclose","section":"consent","value":false,"source":"environment",
     "default":true,"restart":false,"editable":true},
    {"key":"stored_queries.url","section":"stored-queries","source":"revision","restart":true,
     "secret":{"set":true,"reference":"/run/secrets/synthetic-stored-url"},"editable":false},
    {"key":"access_log.templates","section":"access-log","value":{"t.v1":{"categories":["lab"]},
     "t.v2":{"categories":["ips"]}},"source":"revision","default":{},"restart":false,"editable":false},
    {"key":"fhir.base","section":"fhir","value":"/fhir","source":"revision","restart":true,"editable":true}
]}"#;

/// Mounts `running` on `listener` as its running configuration, ahead of
/// the one the shared stub answers.
async fn running(listener: &Server, running: &str) {
    Mock::given(method("GET"))
        .and(path("/admin/config/running"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(running),
        )
        .with_priority(1)
        .mount(listener)
        .await;
}

/// The page at `path`, as the signed-in operator reads it.
async fn page(
    service: &axum::Router,
    session: &ferrofed_console::session::SessionId,
    path: &str,
) -> Result<String, Box<dyn Error>> {
    let (response, body) = send(service, get_as(path, session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{body}");
    Ok(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn each_tab_draws_its_settings_with_their_values_sources_keys_and_next_start()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    running(&listener, RUNNING).await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration").await?;
    assert!(body.contains("Federation behaviour"), "{body}");
    assert!(body.contains("Per-node timeout"), "{body}");
    assert!(body.contains("5,000 ms"), "{body}");
    assert!(body.contains("5,500 ms at the next start"), "{body}");
    assert_eq!(
        1,
        body.matches(r#"class="fui-setting-source""#).count(),
        "a value the running revision sets is the page's own source, unmarked; the one \
         nothing sets says Default: {body}"
    );
    assert!(body.contains("Default"), "{body}");
    assert!(body.contains("federation.per_node_timeout_ms"), "{body}");
    assert!(body.contains("Next start"), "{body}");
    assert!(body.contains("localized</code>"), "{body}");
    assert!(
        body.contains(r#"href="/configuration/federation/edit""#),
        "{body}"
    );
    for tab in [
        "/configuration/fan-out",
        "/configuration/consent",
        "/configuration/access-log",
        "/configuration/fhir",
        "/configuration/members",
    ] {
        assert!(body.contains(&format!(r#"href="{tab}""#)), "{tab}: {body}");
    }

    let consent = page(&service, &session, "/configuration/consent").await?;
    assert!(consent.contains("Disclose exclusions"), "{consent}");
    assert!(consent.contains("From the environment"), "{consent}");

    let stored = page(&service, &session, "/configuration/stored-queries").await?;
    assert!(
        stored.contains("/run/secrets/synthetic-stored-url"),
        "{stored}"
    );
    assert!(stored.contains("Store a new version"), "{stored}");

    let access = page(&service, &session, "/configuration/access-log").await?;
    assert!(access.contains("2 ids mapped"), "{access}");
    assert!(
        !access.contains(r#"href="/configuration/access-log/edit""#),
        "a tab with no editable setting offers no Edit: {access}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edit_is_a_plain_form_with_a_control_for_each_editable_setting()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    running(&listener, RUNNING).await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration/federation/edit").await?;
    assert!(body.contains(r#"method="post""#), "{body}");
    assert!(
        body.contains(r#"name="form[action]" value="save-settings""#),
        "{body}"
    );
    assert!(body.contains(r#"name="form[base]" value="2""#), "{body}");
    assert!(body.contains(r#"value="5000""#), "{body}");
    assert!(
        body.contains(r#"<option value="localized" selected"#),
        "{body}"
    );
    assert!(body.contains(r#"name="form[reason]""#), "{body}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_sends_only_the_settings_the_operator_changed() -> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    running(&listener, RUNNING).await;
    Mock::given(method("PUT"))
        .and(path("/admin/config/settings"))
        .and(body_json(serde_json::json!({
            "base": 2,
            "reason": "synthetic longer budget",
            "settings": {"federation.per_node_timeout_ms": "6000"},
        })))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("content-type", "application/json")
                .set_body_string(SAVED),
        )
        .expect(1)
        .mount(&listener)
        .await;
    let (service, session) = signed_in_console(&listener)?;
    let body = form(&[
        ("action", "save-settings"),
        ("base", "2"),
        ("reason", "synthetic longer budget"),
        ("setting_rows", "0"),
        ("setting][0][key", "federation.per_node_timeout_ms"),
        ("setting][0][was", "5000"),
        ("setting][0][value", "6000"),
        ("setting_rows", "1"),
        ("setting][1][key", "federation.node_selection"),
        ("setting][1][was", "localized"),
        ("setting][1][value", "localized"),
    ]);
    let html = posted(&service, &session, "/configuration/federation/edit", body).await?;
    assert!(
        html.contains("Replica gateway-0 saved revision 2."),
        "{html}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_save_that_changes_nothing_is_refused_before_any_replica_is_asked()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    running(&listener, RUNNING).await;
    Mock::given(method("PUT"))
        .and(path("/admin/config/settings"))
        .respond_with(ResponseTemplate::new(201))
        .expect(0)
        .mount(&listener)
        .await;
    let (service, session) = signed_in_console(&listener)?;
    let body = form(&[
        ("action", "save-settings"),
        ("base", "2"),
        ("reason", "synthetic nothing"),
        ("setting_rows", "0"),
        ("setting][0][key", "federation.per_node_timeout_ms"),
        ("setting][0][was", "5000"),
        ("setting][0][value", "5000"),
    ]);
    let html = posted(&service, &session, "/configuration/federation/edit", body).await?;
    assert!(html.contains("Change a setting before you save."), "{html}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn with_revisions_off_the_settings_tabs_show_alone_and_read_only()
-> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(
        &listener,
        ("GET", "/admin/config"),
        404,
        r#"{"type":"https://ferrofed.eu/docs/integrate/errors.html#configuration-unmanaged","detail":"not managed","code":"configuration-unmanaged"}"#,
    )
    .await;
    let off = RUNNING
        .replace(r#""ui":"managed","revision":1,"#, r#""ui":"off","#)
        .replace(r#""source":"revision""#, r#""source":"file""#)
        .replace(r#""editable":true"#, r#""editable":false"#);
    answers(&listener, ("GET", "/admin/config/running"), 200, &off).await;
    let (service, session) = signed_in_console(&listener)?;
    let body = page(&service, &session, "/configuration").await?;
    assert!(body.contains("Revisions are off on this gateway"), "{body}");
    assert!(body.contains("Per-node timeout"), "{body}");
    assert_eq!(
        1,
        body.matches(r#"class="fui-setting-source""#).count(),
        "with revisions off the file is the page's own source, unmarked; the one \
         nothing sets says Default: {body}"
    );
    assert!(!body.contains("From the file"), "{body}");
    assert!(!body.contains(r#"href="/configuration/members""#), "{body}");
    assert!(!body.contains("/edit\""), "{body}");
    assert!(!body.contains("Save as"), "{body}");
    let fhir = page(&service, &session, "/configuration/fhir/edit").await?;
    assert!(!fhir.contains(r#"value="save-settings""#), "{fhir}");
    Ok(())
}

/// The authentication settings of a gateway that trusts a national contact
/// point: the file sets them, the introspection secret by its file alone.
const AUTHENTICATION: &str = r#"{"ui":"managed","revision":1,"settings":[
    {"key":"auth.audience","section":"authentication","value":["urn:example:gw"],"source":"file",
     "restart":true,"editable":false},
    {"key":"auth.product_scope","section":"authentication","value":"__ferrofed.operator",
     "source":"file","restart":false,"editable":false},
    {"key":"trust.issuer[0].issuer","section":"authentication","value":"https://ncp.example.org",
     "source":"file","restart":false,"editable":false},
    {"key":"trust.issuer[0].client_secret","section":"authentication","source":"file","restart":false,
     "secret":{"set":true,"reference":"/run/secrets/synthetic-introspection"},"editable":false},
    {"key":"trust.issuer[0].national_contact_point.family_name","section":"authentication",
     "value":"ncp_family_name","source":"file","restart":false,"editable":false},
    {"key":"trust.issuer[0].national_contact_point.correlation_header","section":"authentication",
     "value":"ncp-correlation-id","source":"file","restart":false,"editable":false},
    {"key":"trust.issuer[0].national_contact_point.additional_countries","section":"authentication",
     "value":["XA"],"source":"file","restart":false,"editable":false}
]}"#;

/// Regulation (EU) 2025/327 Art 23: an operator sees the national contact
/// point the file declares, every claim name with its key, the correlation
/// header and the added countries, on a read-only tab, and its secret by
/// the file it is read from alone.
#[tokio::test(flavor = "multi_thread")]
async fn the_authentication_tab_shows_the_national_contact_point_read_only()
-> Result<(), Box<dyn Error>> {
    let listener = listener().await;
    running(&listener, AUTHENTICATION).await;
    let (service, session) = signed_in_console(&listener)?;
    let overview = page(&service, &session, "/configuration").await?;
    assert!(
        overview.contains(r#"href="/configuration/authentication""#),
        "{overview}"
    );
    let body = page(&service, &session, "/configuration/authentication").await?;
    for shown in [
        "Authentication",
        "Trusted issuer",
        "https://ncp.example.org",
        "Family name claim",
        "ncp_family_name",
        "trust.issuer[0].national_contact_point.family_name",
        "Correlation header",
        "ncp-correlation-id",
        "Added countries",
        "XA",
        "Introspection secret",
        "/run/secrets/synthetic-introspection",
        "From the file",
    ] {
        assert!(body.contains(shown), "{shown}: {body}");
    }
    assert!(
        !body.contains(r#"href="/configuration/authentication/edit""#),
        "the file alone sets the tab, so it offers no Edit: {body}"
    );
    Ok(())
}
