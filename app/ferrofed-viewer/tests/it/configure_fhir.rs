// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A console member's FHIR endpoints (`hl7-fhir-rest`) on the configuration
//! page against a stub admin listener: the edit form shows each with every
//! key the registry document gives it, and a save adds, edits or removes
//! one as the form shows them, merged onto the held endpoint by its
//! `endpoint_id`; a FHIR endpoint that does not read asks no replica, and
//! every value the gateway answers is escaped.

use std::error::Error;

use ferrofed_registry::console::{ConsoleEndpoint, ConsoleMember};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use ferrofed_testkit::mock::Server;
use http::StatusCode;

use crate::configure::{answers, bodies, changes, configured, form, signed_in_console};
use crate::support::{get_as, send};

/// The running revision 1, and revision 2 saved.
const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":1,"previous":null,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":1,"in_step":true,"refused":null}"#;

/// Revision 2 as a save answers it.
const SAVED: &str = r#"{"id":2,"base":1,"origin":"operator","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:00:00Z","reason":"FHIR","toml":"x = 1"}"#;

/// The route a save of console member C is put to.
const SAVE: (&str, &str) = ("PUT", "/admin/config/members/node-c");

/// Console member C with one Query API endpoint and the FHIR endpoints
/// `fhir`, as the resources answer them.
fn resources(fhir: &str) -> String {
    format!(
        r#"{{"running":1,"head":2,"stored_query_registry":"absent","stored_queries":[],"members":[
          {{"node":"node-c","organisation":"org-c","system_id":"cdr-c.example.org",
           "endpoints":["node-c-pub"],"owner":"console","state":"pending",
           "member":{{"organisation":{{"id":"org-c"}},"node":{{"id":"node-c","system_id":"cdr-c.example.org"}},
             "endpoints":[
               {{"id":"node-c-pub","url":"https://cdr-c.example.org/openehr","connection_type":"openehr-rest-query"}}
               {fhir}]}}}}]}}"#
    )
}

/// The held FHIR endpoint of console member C.
const HELD_FHIR: &str = r#",{"id":"node-c-fhir","url":"https://cdr-c.example.org/fhir","connection_type":"hl7-fhir-rest",
    "status":"suspended","categories":["Patient-Summaries"],"profiles":["http://example.org/ps"],
    "ehr_id_system":"urn:oid:2.999.1"}"#;

/// An admin listener whose console member C holds the FHIR endpoints
/// `fhir`, and which takes a save of it.
async fn listener(fhir: &str) -> Server {
    let listener = Server::start().await;
    answers(&listener, ("GET", "/admin/config"), 200, STANDING).await;
    answers(
        &listener,
        ("GET", "/admin/config/resources"),
        200,
        &resources(fhir),
    )
    .await;
    answers(
        &listener,
        ("GET", "/admin/config/revisions"),
        200,
        r#"{"items":[],"offset":0,"total":0}"#,
    )
    .await;
    answers(
        &listener,
        ("GET", "/admin/config/revisions/1/diff/2"),
        200,
        r#"{"from":1,"to":2,"changes":[]}"#,
    )
    .await;
    answers(&listener, SAVE, 201, SAVED).await;
    listener
}

/// The confirmed save of console member C with the FHIR endpoints `fhir`.
fn save_c(fhir: &str) -> String {
    form(&[
        ("action", "save-member"),
        ("base", "2"),
        ("organisation", "org-c"),
        ("node", "node-c"),
        ("system_id", "cdr-c.example.org"),
        ("endpoints", "node-c-pub https://cdr-c.example.org/openehr"),
        ("fhir_endpoints", fhir),
        ("reason", "FHIR"),
        ("confirm", "yes"),
    ])
}

/// The body of a member's save, as the gateway reads it.
#[derive(serde::Deserialize)]
struct SaveBody {
    member: ConsoleMember,
}

/// The FHIR endpoints of the one save `listener` received.
async fn saved_fhir(listener: &Server) -> Result<Vec<ConsoleEndpoint>, Box<dyn Error>> {
    let sent = bodies(listener, SAVE).await?;
    let [sent] = sent.as_slice() else {
        return Err(format!("one save, not {}", sent.len()).into());
    };
    let body: SaveBody = serde_json::from_str(sent)?;
    Ok(body
        .member
        .endpoints
        .into_iter()
        .filter(|endpoint| endpoint.connection_type == ConnectionType::Hl7FhirRest)
        .collect())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_edit_form_shows_each_fhir_endpoint_with_every_key() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    let (response, page) = send(&service, get_as("/configuration", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{page}");
    assert!(
        page.contains(r#"id="edit-node-c-fhir_endpoints" name="form[fhir_endpoints]" value="node-c-fhir https://cdr-c.example.org/fhir system=urn:oid:2.999.1 categories=Patient-Summaries profiles=http://example.org/ps suspended""#),
        "{page}"
    );
    assert!(
        page.contains(r#"id="add-member-fhir_endpoints" name="form[fhir_endpoints]" value="""#),
        "{page}"
    );
    assert!(
        page.contains(r#"<label for="edit-node-c-fhir_endpoints">FHIR endpoints"#),
        "{page}"
    );
    Ok(())
}

#[tokio::test]
async fn a_save_adds_a_fhir_endpoint_with_every_key() -> Result<(), Box<dyn Error>> {
    let listener = listener("").await;
    let (service, session) = signed_in_console(&listener)?;
    let html = configured(
        &service,
        &session,
        save_c("node-c-fhir https://cdr-c.example.org/fhir system=urn:oid:2.999.1 categories=Patient-Summaries,Laboratory-Results profiles=http://example.org/ps"),
    )
    .await?;
    assert!(
        html.contains("Replica gateway-0 saved revision 2."),
        "{html}"
    );
    let fhir = saved_fhir(&listener).await?;
    let [added] = fhir.as_slice() else {
        return Err(format!("one FHIR endpoint, not {}", fhir.len()).into());
    };
    assert_eq!("node-c-fhir", added.id.as_str());
    assert_eq!("https://cdr-c.example.org/fhir", added.url);
    assert_eq!(EndpointStatus::Active, added.status);
    assert_eq!(
        vec![
            String::from("Patient-Summaries"),
            String::from("Laboratory-Results")
        ],
        added.categories
    );
    assert_eq!(vec![String::from("http://example.org/ps")], added.profiles);
    assert_eq!(Some("urn:oid:2.999.1"), added.ehr_id_system.as_deref());
    assert!(added.consent_refusal_codes.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_save_edits_a_held_fhir_endpoint_by_its_endpoint_id() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    configured(
        &service,
        &session,
        save_c("node-c-fhir https://fhir.example.org/r4 system=urn:oid:2.999.2 categories=Laboratory-Results profiles=http://example.org/lab"),
    )
    .await?;
    let fhir = saved_fhir(&listener).await?;
    let [edited] = fhir.as_slice() else {
        return Err(format!("one FHIR endpoint, not {}", fhir.len()).into());
    };
    assert_eq!("node-c-fhir", edited.id.as_str());
    assert_eq!("https://fhir.example.org/r4", edited.url);
    assert_eq!(
        EndpointStatus::Active,
        edited.status,
        "the form no longer marks it suspended"
    );
    assert_eq!(vec![String::from("Laboratory-Results")], edited.categories);
    assert_eq!(
        vec![String::from("http://example.org/lab")],
        edited.profiles
    );
    assert_eq!(Some("urn:oid:2.999.2"), edited.ehr_id_system.as_deref());
    Ok(())
}

#[tokio::test]
async fn a_save_removes_a_fhir_endpoint_the_form_no_longer_names() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    configured(&service, &session, save_c("")).await?;
    assert!(saved_fhir(&listener).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_fhir_endpoint_that_does_not_read_asks_no_replica() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    for (fhir, said) in [
        (
            "node-c-fhir https://cdr-c.example.org/fhir categories=A profiles=p",
            "Each FHIR endpoint is",
        ),
        (
            "node-c-fhir https://cdr-c.example.org/fhir system=s categories=A, profiles=p",
            "A FHIR endpoint names one or more categories",
        ),
        (
            "node c https://cdr-c.example.org/fhir system=s categories=A profiles=p",
            "Each FHIR endpoint is",
        ),
    ] {
        let html = configured(&service, &session, save_c(fhir)).await?;
        assert!(html.contains(said), "{fhir}: {html}");
    }
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// NOTE: no specification governs this: our own design; a FHIR endpoint's
// values reach the edit form's attribute as text, the `inner_html` obligation.
#[tokio::test(flavor = "multi_thread")]
async fn a_hostile_fhir_endpoint_is_escaped_in_the_edit_form() -> Result<(), Box<dyn Error>> {
    let hostile = r#",{"id":"node-c-fhir","url":"https://x.example.org/\"><script>alert(1)</script>",
        "connection_type":"hl7-fhir-rest","categories":["<b>c</b>"],"profiles":["\" onfocus=\"alert(2)"],
        "ehr_id_system":"<img src=x onerror=alert(3)>"}"#;
    let listener = listener(hostile).await;
    let (service, session) = signed_in_console(&listener)?;
    let (_response, page) = send(&service, get_as("/configuration", &session)?).await?;
    for raw in [
        "\"><script>alert(1)",
        "<b>c</b>",
        "\" onfocus=\"alert(2)",
        "<img src=x",
    ] {
        assert!(!page.contains(raw), "{raw} reached the page raw: {page}");
    }
    assert!(
        page.contains("&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{page}"
    );
    assert!(page.contains("&quot; onfocus=&quot;alert(2)"), "{page}");
    Ok(())
}
