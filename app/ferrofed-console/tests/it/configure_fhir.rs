// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A console member's FHIR endpoints (`hl7-fhir-rest`) on the configuration
//! page against a stub admin listener: the edit form shows each as a row
//! with a field for every key the registry document gives it, and a save
//! adds, edits or removes one as the rows give them, merged onto the held
//! endpoint by its `endpoint_id`; a FHIR endpoint that does not read asks
//! no replica and is refused at its field, and every value the gateway
//! answers is escaped.

use std::error::Error;

use ferrofed_registry::console::{ConsoleEndpoint, ConsoleMember};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use ferrofed_testkit::mock::Server;

use crate::configure::{
    answers, bodies, changes, configured, form_with, page, query_row, refusal_of, signed_in_console,
};

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

/// One FHIR endpoint row of a member form.
struct Fhir<'a> {
    /// The `endpoint_id`.
    id: &'a str,
    /// The base URL.
    url: &'a str,
    /// The identifier system of the `ehr_id`.
    system: &'a str,
    /// The categories ticked.
    categories: &'a [&'a str],
    /// The profiles.
    profiles: &'a [&'a str],
    /// Whether it is in service.
    in_service: bool,
}

/// The fields of FHIR endpoint row `row`.
fn fhir_row(row: usize, fhir: &Fhir<'_>) -> Vec<(String, String)> {
    let at = |part: &str| format!("fhir][{row}][{part}");
    let mut fields = vec![
        (String::from("fhir_rows"), row.to_string()),
        (at("id"), fhir.id.to_owned()),
        (at("url"), fhir.url.to_owned()),
        (at("system"), fhir.system.to_owned()),
    ];
    if fhir.in_service {
        fields.push((at("in_service"), String::from("on")));
    }
    for category in fhir.categories {
        fields.push((at("categories"), (*category).to_owned()));
    }
    for profile in fhir.profiles {
        fields.push((at("profiles"), (*profile).to_owned()));
    }
    fields
}

/// The save of console member C `action` posts, with the FHIR endpoints
/// `fhir`, and the marker that the form showed them when `shown`.
fn posted(action: &str, fhir: &[Fhir<'_>], shown: bool) -> String {
    let mut rows = query_row(
        "0",
        ("node-c-pub", "https://cdr-c.example.org/openehr", true),
    );
    for (row, endpoint) in fhir.iter().enumerate() {
        rows.extend(fhir_row(row, endpoint));
    }
    let mut fields = vec![
        ("action", action),
        ("base", "2"),
        ("organisation", "org-c"),
        ("node", "node-c"),
        ("system_id", "cdr-c.example.org"),
        ("reason", "FHIR"),
    ];
    if shown {
        fields.push(("fhir_shown", "yes"));
    }
    form_with(&fields, &rows)
}

/// The save of console member C with the FHIR endpoints `fhir`, from the
/// form that shows them.
fn save_c(fhir: &[Fhir<'_>]) -> String {
    posted("save-member", fhir, true)
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
    let page_text = page(&service, &session, "/configuration/members/node-c").await?;
    for shown in [
        r#"name="form[fhir][0][id]" value="node-c-fhir""#,
        r#"name="form[fhir][0][url]" value="https://cdr-c.example.org/fhir""#,
        r#"name="form[fhir][0][system]" value="urn:oid:2.999.1""#,
        r#"name="form[fhir][0][categories]" value="Patient-Summaries" checked"#,
        r#"name="form[fhir][0][profiles]" value="http://example.org/ps""#,
        r#"<label for="cfg-f-0-system" class="fui-label">Identifier system of the ehr_id</label>"#,
        r#"aria-describedby="cfg-f-0-system-help cfg-f-0-system-error""#,
        r#"name="form[fhir_shown]" value="yes""#,
        r#"aria-label="Remove FHIR endpoint node-c-fhir""#,
        r#"name="form[action]" value="save-member""#,
    ] {
        assert!(page_text.contains(shown), "{shown}: {page_text}");
    }
    assert!(
        !page_text.contains(r#"name="form[fhir][0][in_service]" value="on" checked"#),
        "a suspended endpoint is shown out of service: {page_text}"
    );
    let added = page(&service, &session, "/configuration/members/new").await?;
    assert!(
        added.contains(r#"name="form[action]" value="add-member""#),
        "{added}"
    );
    assert!(
        added.contains(r#"name="form[edit]" value="add-fhir""#),
        "Add FHIR endpoint posts the form: {added}"
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
        save_c(&[Fhir {
            id: "node-c-fhir",
            url: "https://cdr-c.example.org/fhir",
            system: "urn:oid:2.999.1",
            categories: &["Patient-Summaries", "Laboratory-Reports"],
            profiles: &["http://example.org/ps"],
            in_service: true,
        }]),
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
            String::from("Laboratory-Reports")
        ],
        added.categories
    );
    assert_eq!(vec![String::from("http://example.org/ps")], added.profiles);
    assert_eq!(Some("urn:oid:2.999.1"), added.ehr_id_system.as_deref());
    assert_eq!(added.consent_refusal_codes, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn a_save_edits_a_held_fhir_endpoint_by_its_endpoint_id() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    configured(
        &service,
        &session,
        save_c(&[Fhir {
            id: "node-c-fhir",
            url: "https://fhir.example.org/r4",
            system: "urn:oid:2.999.2",
            categories: &["Laboratory-Reports"],
            profiles: &["http://example.org/lab"],
            in_service: true,
        }]),
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
        "the form puts it back in service"
    );
    assert_eq!(vec![String::from("Laboratory-Reports")], edited.categories);
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
    configured(&service, &session, save_c(&[])).await?;
    assert_eq!(saved_fhir(&listener).await?, Vec::<ConsoleEndpoint>::new());
    Ok(())
}

#[tokio::test]
async fn a_fhir_endpoint_that_does_not_read_asks_no_replica_and_is_refused_at_its_field()
-> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    let whole = Fhir {
        id: "node-c-fhir",
        url: "https://cdr-c.example.org/fhir",
        system: "urn:oid:2.999.1",
        categories: &["Patient-Summaries"],
        profiles: &["http://example.org/ps"],
        in_service: true,
    };
    for (fhir, said, at) in [
        (
            Fhir {
                system: "",
                ..whole
            },
            "FHIR endpoint 2 names no identifier system",
            "cfg-f-1-system",
        ),
        (
            Fhir {
                categories: &[],
                ..whole
            },
            "FHIR endpoint 2 serves no category",
            "cfg-f-1-categories",
        ),
        (
            Fhir {
                profiles: &[],
                ..whole
            },
            "FHIR endpoint 2 names no profile",
            "cfg-f-1-add-profile",
        ),
        (
            Fhir {
                id: "node c",
                ..whole
            },
            "The endpoint_id of FHIR endpoint 2 is not",
            "cfg-f-1-id",
        ),
        (
            Fhir {
                url: "cdr-c.example.org/fhir",
                ..whole
            },
            "The base URL of FHIR endpoint 2 is not an absolute http or https URL.",
            "cfg-f-1-url",
        ),
    ] {
        let first = Fhir {
            id: "node-c-x",
            ..whole
        };
        let html = crate::configure::posted(
            &service,
            &session,
            "/configuration/members/node-c",
            save_c(&[first, fhir]),
        )
        .await?;
        assert!(html.contains(said), "{said}: {html}");
        let line = refusal_of(&html, at).ok_or_else(|| format!("{said} marks {at}: {html}"))?;
        assert!(line.contains(said), "{line}");
    }
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// NOTE: no specification governs this: our own design; a FHIR endpoint's
// values reach the edit form's attributes as text, the `inner_html` obligation.
#[tokio::test(flavor = "multi_thread")]
async fn a_hostile_fhir_endpoint_is_escaped_in_the_edit_form() -> Result<(), Box<dyn Error>> {
    let hostile = r#",{"id":"node-c-fhir","url":"https://x.example.org/\"><script>alert(1)</script>",
        "connection_type":"hl7-fhir-rest","categories":["<b>c</b>"],"profiles":["\" onfocus=\"alert(2)"],
        "ehr_id_system":"<img src=x onerror=alert(3)>"}"#;
    let listener = listener(hostile).await;
    let (service, session) = signed_in_console(&listener)?;
    let page_text = page(&service, &session, "/configuration/members/node-c").await?;
    for raw in [
        "\"><script>alert(1)",
        "<b>c</b>",
        "\" onfocus=\"alert(2)",
        "<img src=x",
    ] {
        assert!(
            !page_text.contains(raw),
            "{raw} reached the page raw: {page_text}"
        );
    }
    assert!(
        page_text.contains("&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{page_text}"
    );
    assert!(
        page_text.contains("&quot; onfocus=&quot;alert(2)"),
        "{page_text}"
    );
    Ok(())
}

// NOTE: no specification governs this: our own design; a `;` and a `%` are
// legal in a URL (RFC 3986 §3.3), and a field of its own holds either as typed.
#[tokio::test(flavor = "multi_thread")]
async fn a_url_holding_a_semicolon_round_trips_through_the_edit_form() -> Result<(), Box<dyn Error>>
{
    let held = r#",{"id":"node-c-fhir","url":"https://cdr-c.example.org/fhir;v=4","connection_type":"hl7-fhir-rest",
        "categories":["Patient-Summaries"],"profiles":["http://example.org/ps;1","http://example.org/%3Bq"],
        "ehr_id_system":"urn:oid:2.999.1"}"#;
    let listener = listener(held).await;
    let (service, session) = signed_in_console(&listener)?;
    let page_text = page(&service, &session, "/configuration/members/node-c").await?;
    for shown in [
        r#"value="https://cdr-c.example.org/fhir;v=4""#,
        r#"value="http://example.org/ps;1""#,
        r#"value="http://example.org/%3Bq""#,
    ] {
        assert!(page_text.contains(shown), "{shown}: {page_text}");
    }
    configured(
        &service,
        &session,
        save_c(&[Fhir {
            id: "node-c-fhir",
            url: "https://cdr-c.example.org/fhir;v=4",
            system: "urn:oid:2.999.1",
            categories: &["Patient-Summaries"],
            profiles: &["http://example.org/ps;1", "http://example.org/%3Bq"],
            in_service: true,
        }]),
    )
    .await?;
    let fhir = saved_fhir(&listener).await?;
    let [saved] = fhir.as_slice() else {
        return Err(format!("one FHIR endpoint, not {}", fhir.len()).into());
    };
    assert_eq!("https://cdr-c.example.org/fhir;v=4", saved.url);
    assert_eq!(
        vec![
            String::from("http://example.org/ps;1"),
            String::from("http://example.org/%3Bq")
        ],
        saved.profiles
    );
    Ok(())
}

/// Asserts that `listener` received one save that keeps the held FHIR
/// endpoint whole.
async fn kept_whole(listener: &Server) -> Result<(), Box<dyn Error>> {
    let fhir = saved_fhir(listener).await?;
    let [kept] = fhir.as_slice() else {
        return Err(format!("one FHIR endpoint, not {}", fhir.len()).into());
    };
    assert_eq!("https://cdr-c.example.org/fhir", kept.url, "the url");
    assert_eq!(EndpointStatus::Suspended, kept.status, "the status");
    assert_eq!(
        vec![String::from("Patient-Summaries")],
        kept.categories,
        "the categories"
    );
    assert_eq!(
        Some("urn:oid:2.999.1"),
        kept.ehr_id_system.as_deref(),
        "the system"
    );
    Ok(())
}

// The form that adds a member refuses a node_id the gateway holds, so it
// never replaces a member the operator did not open for an edit.
#[tokio::test]
async fn the_add_form_refuses_a_node_id_the_gateway_holds() -> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    let html = crate::configure::posted(
        &service,
        &session,
        "/configuration/members/new",
        posted("add-member", &[], true),
    )
    .await?;
    assert!(html.contains("A member with this node_id exists"), "{html}");
    let line = refusal_of(&html, "cfg-node").ok_or_else(|| format!("cfg-node: {html}"))?;
    assert!(line.contains("A member with this node_id exists"), "{line}");
    assert_eq!(0, changes(&listener).await?);
    Ok(())
}

// A post without the form's marker that it showed the FHIR endpoints, as
// a tab opened before they were shown sends, keeps them whole.
#[tokio::test]
async fn a_form_drawn_before_the_fhir_rows_keeps_the_held_fhir_endpoints()
-> Result<(), Box<dyn Error>> {
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    configured(&service, &session, posted("save-member", &[], false)).await?;
    kept_whole(&listener).await
}

// A post that carries FHIR endpoint rows but not the form's marker that it
// showed them changes no FHIR endpoint: absence is never emptiness.
#[tokio::test]
async fn a_post_without_the_fhir_marker_keeps_the_held_fhir_endpoints() -> Result<(), Box<dyn Error>>
{
    let listener = listener(HELD_FHIR).await;
    let (service, session) = signed_in_console(&listener)?;
    let other = Fhir {
        id: "node-c-other",
        url: "https://other.example.org/fhir",
        system: "urn:oid:2.999.9",
        categories: &["Patient-Summaries"],
        profiles: &["http://example.org/ps"],
        in_service: true,
    };
    configured(&service, &session, posted("save-member", &[other], false)).await?;
    let fhir = saved_fhir(&listener).await?;
    assert!(
        fhir.iter()
            .any(|endpoint| endpoint.id.as_str() == "node-c-fhir"
                && endpoint.status == EndpointStatus::Suspended),
        "the held FHIR endpoint is kept whole"
    );
    Ok(())
}
