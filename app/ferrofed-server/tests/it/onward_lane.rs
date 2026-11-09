// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward lane of a node, `[credentials."<endpoint id>".onward]`,
//! through the real configuration path: a person's token exchanged per
//! request at a `wiremock` token endpoint, the node reached under the issued
//! token alone, a refused exchange or a caller the lane's issuer does not
//! carry failing that node with nothing sent, and the table refused at load
//! when it cannot work (§13.1, N25; RFC 8693 §2.1 and §2.2.1, RFC 8707 §2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::error::credentials::CredentialsFault;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{body_string_contains, header as has_header, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{body, crossref, patient_query, post, registry, settings_with_room};
use crate::support::caller::{ISSUER, bearer_as, issuer};
use crate::support::{call, signed};

type TestResult = Result<(), Box<dyn Error>>;

/// The client the onward issuer registered the gateway as.
const CLIENT_ID: &str = "ferrofed-onward-gateway";

/// The audience, and the resource indicator, of node A's onward tokens.
const NODE: &str = "https://cdr-a.example.org/openehr";

/// The caller the tests send the query as.
const CALLER: &str = "synthetic-clinician-onward";

/// The gateway's own token, which no caller's request may carry.
const OWN_TOKEN: &str = "synthetic-gateway-own-token";

/// A one-row answer from a node.
const ONE_ROW: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-at-a"]]}"##;

/// The token-exchange grant type, form-encoded (RFC 8693 §2.1).
const TOKEN_EXCHANGE: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange";

/// The `oauth` and `onward` tables of node A at the token endpoint `issuer`,
/// whose onward issuer signs tokens of `kind`, with `extra` onward keys.
fn lane(issuer: &str, kind: &str, extra: &str) -> String {
    format!(
        concat!(
            "[credentials.\"node-a-pub\".oauth]\n",
            "token_endpoint = \"{issuer}/token\"\n",
            "client_id = \"{client}\"\n",
            "client_auth = \"private_key_jwt\"\n",
            "private_key_file = {key}\n",
            "scope = \"system/aql-*.s\"\n\n",
            "[credentials.\"node-a-pub\".onward]\n",
            "issuer = \"{onward}\"\n",
            "kind = \"{kind}\"\n",
            "profile = \"rfc9068\"\n",
            "token_endpoint = \"{issuer}/token\"\n",
            "audiences = [\"{node}\"]\n",
            "indicators = [\"{node}\"]\n",
            "{extra}",
            "[credentials.\"node-a-pub\".onward.keys]\n",
            "max_onward_lifetime = 3600\n",
        ),
        issuer = issuer,
        client = CLIENT_ID,
        key = toml::Value::String(crate::support::signing_key_file().to_owned()),
        onward = ISSUER,
        kind = kind,
        node = NODE,
        extra = extra,
    )
}

/// The `oauth` and `onward` tables of node A at the token endpoint `issuer`,
/// whose onward issuer is of kind `federation` and maps the suite's test
/// issuer.
fn federation_lane(issuer: &str) -> String {
    let federation = format!(
        "[credentials.\"node-a-pub\".onward.federation]\ncontact_point = false\nexchanges_from = [\"{ISSUER}\"]\nperson_mapping = \"sub\"\ncountries = [\"NL\"]\nissuing_authorities = [\"synthetic-authority\"]\nprovider_identifier_prefixes = [\"urn:oid:2.999\"]\n"
    );
    lane(issuer, "federation", "").replace(
        "[credentials.\"node-a-pub\".onward.keys]",
        &format!("{federation}\n[credentials.\"node-a-pub\".onward.keys]"),
    )
}

/// The gateway over node A at `a` and an unreachable node B, the patient
/// resolving at node A, with `tables`.
fn gateway(dir: &Path, a: &str, tables: &str) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry(a, ferrofed_testkit::unreachable::BASE, ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout = '2s'\noverall_timeout = '3s'\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n{}\n{tables}",
        crossref(&[("node-a", "2222aaaa-2222-4222-8222-222222222222")])
    );
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let mut server = settings_with_room();
    server.auth = crate::support::caller::auth();
    Ok(ferrofed_server::router(state, &server))
}

/// The token the onward issuer issues for [`CALLER`]: the same person, the
/// node as its audience and the gateway as its client.
fn onward_token() -> Result<String, Box<dyn Error>> {
    let mut claims = Claims::new(ISSUER, NODE);
    CALLER.clone_into(&mut claims.sub);
    CLIENT_ID.clone_into(&mut claims.client_id);
    claims.scope = Some(String::from("user/aql-*.s"));
    Ok(issuer().mint(&claims)?)
}

/// An authorization server issuing the gateway its own token, and, for a
/// token exchange, `exchanged` with `status`.
async fn authorization_server(exchanged: &str, status: u16) -> Server {
    let server = Server::start().await;
    let answer = if status == 200 {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": exchanged,
            "issued_token_type": "urn:ietf:params:oauth:token-type:access_token",
            "token_type": "Bearer",
            "expires_in": 300,
        }))
    } else {
        ResponseTemplate::new(status).set_body_json(serde_json::json!({
            "error": "invalid_grant",
        }))
    };
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(TOKEN_EXCHANGE))
        .respond_with(answer)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": OWN_TOKEN,
            "token_type": "Bearer",
            "expires_in": 300,
        })))
        .mount(&server)
        .await;
    server
}

/// A node answering the query sent under `token`, and `401` to every other.
async fn node(token: &str) -> Result<Server, Box<dyn Error>> {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(has_header(
            header::AUTHORIZATION.as_str(),
            format!("Bearer {token}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(ONE_ROW.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    Ok(server)
}

/// The patient query, sent as `authorization`.
fn patient_post(authorization: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, authorization.parse()?);
    Ok(request)
}

/// The forms the authorization server received for a token exchange.
async fn exchanges(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .filter(|form| form.contains(TOKEN_EXCHANGE))
        .collect())
}

/// A person of the node's organisation reaches node A under the token the
/// onward issuer exchanged the caller's for, naming the node as its
/// resource; the caller's own token and the gateway's own token never reach
/// the node (RFC 8693 §2.1, RFC 8707 §2, N25).
// conformance: CP-17
#[tokio::test]
async fn a_person_reaches_the_node_under_the_exchanged_token() -> TestResult {
    let exchanged = onward_token()?;
    let server = authorization_server(&exchanged, 200).await;
    let a = node(&exchanged).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a.uri(), &lane(&server.uri(), "persons", ""))?;
    let authorization = bearer_as(CALLER)?;

    let (status, text) = call(app, patient_post(&authorization)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let forms = exchanges(&server).await?;
    assert_eq!(1, forms.len(), "one exchange: {forms:?}");
    let caller = authorization
        .strip_prefix("Bearer ")
        .ok_or("a bearer credential")?;
    let form = forms.first().ok_or("an exchange")?;
    assert!(
        form.contains(&format!("subject_token={caller}")),
        "the caller's token is the subject: {form}"
    );
    assert!(
        form.contains("resource=https%3A%2F%2Fcdr-a.example.org%2Fopenehr"),
        "the node is the resource: {form}"
    );
    let sent: Vec<String> = a
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .filter_map(|request| request.headers.get(header::AUTHORIZATION.as_str()))
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect();
    assert_eq!(vec![format!("Bearer {exchanged}")], sent);
    Ok(())
}

/// A token endpoint that refuses the exchange fails node A, and nothing is
/// sent to the node: never the gateway's own token in its place.
// conformance: CP-17
#[tokio::test]
async fn a_refused_exchange_fails_the_node_with_nothing_sent() -> TestResult {
    let server = authorization_server("unused", 400).await;
    let a = node(OWN_TOKEN).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a.uri(), &lane(&server.uri(), "persons", ""))?;

    let (status, text) = call(app, patient_post(&bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(
        text.contains("invalid_grant"),
        "the node's report names the token endpoint's code: {text}"
    );
    assert_eq!(1, exchanges(&server).await?.len(), "the exchange was asked");
    assert!(
        a.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "node A was sent nothing"
    );
    Ok(())
}

/// A person is never exchanged at a `federation` issuer: the call fails
/// node A before any token request is sent.
// conformance: CP-17
#[tokio::test]
async fn a_caller_the_issuer_does_not_carry_is_refused_before_anything_is_sent() -> TestResult {
    let server = authorization_server("unused", 200).await;
    let a = node(OWN_TOKEN).await?;
    let dir = tempfile::tempdir()?;
    let tables = federation_lane(&server.uri());
    let app = gateway(dir.path(), &a.uri(), &tables)?;

    let (status, text) = call(app, patient_post(&bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(
        exchanges(&server).await?.is_empty(),
        "no exchange was asked"
    );
    assert!(
        a.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "node A was sent nothing"
    );
    Ok(())
}

/// Resolves `tables` as the whole configuration's credentials and returns
/// the refusal.
fn refusal(tables: &str) -> Result<ConfigError, Box<dyn Error>> {
    match Config::from_sources(Some(&signed(tables)), &BTreeMap::new())?.resolve() {
        Ok(_) => Err(format!("accepted: {tables}").into()),
        Err(error) => Ok(error),
    }
}

/// An onward table needs the `oauth` client it authenticates with.
#[test]
fn an_onward_table_without_its_oauth_client_is_refused() -> TestResult {
    let tables = lane("https://as.example.org", "persons", "");
    let start = tables
        .find("[credentials.\"node-a-pub\".onward]")
        .ok_or("the onward table")?;
    let error = refusal(tables.get(start..).ok_or("the onward part")?)?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "credentials.node-a-pub.oauth"),
        "{error:?}"
    );
    Ok(())
}

/// An onward table beside a Basic user names two schemes.
#[test]
fn an_onward_table_beside_a_basic_user_is_refused() -> TestResult {
    let tables = format!(
        "{}\n[credentials.\"node-a-pub\"]\nuser = \"synthetic-user\"\npassword = \"synthetic-password\"\n",
        lane("https://as.example.org", "persons", "")
    );
    let error = refusal(&tables)?;
    assert!(
        matches!(&error, ConfigError::Credentials(CredentialsFault::Scheme { section }) if section == "credentials.node-a-pub"),
        "{error:?}"
    );
    Ok(())
}

/// An onward table with no audience is refused by `ferrohealth-smart`,
/// naming the table.
#[test]
fn an_onward_table_with_no_audience_is_refused() -> TestResult {
    let tables = lane("https://as.example.org", "persons", "")
        .replace(&format!("audiences = [\"{NODE}\"]\n"), "");
    let error = refusal(&tables)?;
    assert!(
        matches!(&error, ConfigError::Smart { section, .. } if section == "credentials.node-a-pub.onward"),
        "{error:?}"
    );
    Ok(())
}

/// `tables` with node A's credentials opted in to organisations.
fn opted_in(tables: &str) -> String {
    format!("[credentials.\"node-a-pub\"]\nonward_organisations = true\n\n{tables}")
}

/// A lane opts in to another party's organisation only at a `federation`
/// issuer, and only with an onward table: a `persons` lane and an `oauth`
/// grant alone refuse the switch at load.
#[test]
fn onward_organisations_needs_a_federation_lane() -> TestResult {
    let persons = lane("https://as.example.org", "persons", "");
    let start = persons
        .find("[credentials.\"node-a-pub\".onward]")
        .ok_or("the onward table")?;
    let oauth_alone = persons.get(..start).ok_or("the oauth part")?;
    for tables in [opted_in(&persons), opted_in(oauth_alone)] {
        let error = refusal(&tables)?;
        assert!(
            matches!(&error, ConfigError::Credentials(CredentialsFault::OnwardOrganisations { section }) if section == "credentials.node-a-pub"),
            "{error:?}"
        );
    }
    Ok(())
}

/// A `federation` lane takes the switch, and loads without it as before.
#[test]
fn a_federation_lane_takes_onward_organisations() -> TestResult {
    let federation = federation_lane("https://as.example.org");
    for tables in [opted_in(&federation), federation] {
        Config::from_sources(Some(&signed(&tables)), &BTreeMap::new())?.resolve()?;
    }
    Ok(())
}

/// A canonical `CODE_PHRASE`-coded text of the openEHR terminology.
fn coded(value: &str, code: &str) -> String {
    format!(
        r#"{{"_type":"DV_CODED_TEXT","value":"{value}","defining_code":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"openehr"}},"code_string":"{code}"}}}}"#
    )
}

/// A `CONTRIBUTION` of one created `COMPOSITION` whose audits
/// state a committer of the client's own, with `attestations` on the
/// version when given.
fn contribution(attestations: &str) -> String {
    let audit = format!(
        r#"{{"_type":"UPDATE_AUDIT","change_type":{},"committer":{{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}}}}"#,
        coded("creation", "249")
    );
    let data = format!(
        r#"{{"_type":"COMPOSITION","name":{{"_type":"DV_TEXT","value":"Synthetic note"}},"archetype_node_id":"openEHR-EHR-COMPOSITION.report.v1","language":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"ISO_639-1"}},"code_string":"en"}},"territory":{{"_type":"CODE_PHRASE","terminology_id":{{"_type":"TERMINOLOGY_ID","value":"ISO_3166-1"}},"code_string":"NL"}},"category":{},"composer":{{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}}}}"#,
        coded("event", "433")
    );
    format!(
        r#"{{"versions":[{{"lifecycle_state":{},"commit_audit":{audit},{attestations}"data":{data}}}],"audit":{audit}}}"#,
        coded("complete", "532")
    )
}

/// `body` posted as a `CONTRIBUTION` to node A's EHR, naming node A as its
/// target, as `authorization`.
fn contribution_post(body: String, authorization: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(
        Request::post(format!("/v1/ehr/{}/contribution", crate::facade::EHR_A))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, authorization)
            .header("openEHR-federation-endpoint", "node-a-pub")
            .body(Body::from(body))?,
    )
}

/// A node A that takes every `CONTRIBUTION` with `201`.
async fn committing_node() -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/ehr/{}/contribution",
            crate::facade::EHR_A
        )))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    server
}

/// The JSON bodies node A received.
#[expect(
    clippy::disallowed_types,
    reason = "a test reads the JSON a stub node received"
)]
async fn received(server: &Server) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| Ok(serde_json::from_slice(&request.body)?))
        .collect()
}

/// The placeholder FerroEHR takes where ITS-REST requires a committer and
/// the node derives it: the onward token's `iss` and `sub`, nothing else.
#[expect(
    clippy::disallowed_types,
    reason = "a test reads the JSON a stub node received"
)]
fn placeholder() -> serde_json::Value {
    serde_json::json!({
        "_type": "PARTY_IDENTIFIED",
        "identifiers": [{"_type": "DV_IDENTIFIER", "issuer": ISSUER, "id": CALLER}],
    })
}

/// A `CONTRIBUTION` sent through an onward lane names, in its `audit` and in
/// every version's `commit_audit`, the placeholder of the onward token the
/// node authenticates, and keeps every other member as the client sent it
/// (ITS-REST 1.1.0 `UpdateAudit`, `UpdateVersion`).
#[tokio::test]
#[expect(
    clippy::disallowed_types,
    reason = "a test reads the JSON a stub node received"
)]
async fn a_contribution_names_the_onward_tokens_placeholder_as_its_committer() -> TestResult {
    let exchanged = onward_token()?;
    let server = authorization_server(&exchanged, 200).await;
    let a = committing_node().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a.uri(), &lane(&server.uri(), "persons", ""))?;
    let sent = contribution("");

    let (status, text) = call(app, contribution_post(sent.clone(), &bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let [body] = received(&a).await?.try_into().map_err(|_n| "one request")?;
    assert_eq!(placeholder(), body["audit"]["committer"]);
    assert_eq!(
        placeholder(),
        body["versions"][0]["commit_audit"]["committer"]
    );
    let original: serde_json::Value = serde_json::from_str(&sent)?;
    assert_eq!(original["versions"][0]["data"], body["versions"][0]["data"]);
    assert_eq!(
        original["audit"]["change_type"],
        body["audit"]["change_type"]
    );
    Ok(())
}

/// The `attestations` member of a version, one `UPDATE_ATTESTATION` its
/// client states a committer of its own for (ITS-REST 1.1.0
/// `UpdateAttestation`).
fn attestation() -> String {
    format!(
        r#""attestations":[{{"_type":"UPDATE_ATTESTATION","change_type":{},"committer":{{"_type":"PARTY_SELF"}},"reason":{{"_type":"DV_TEXT","value":"signed"}},"is_pending":false}}],"#,
        coded("creation", "249")
    )
}

/// A person's attestation takes the same placeholder (ITS-REST 1.1.0
/// `UpdateAttestation`).
#[tokio::test]
async fn a_persons_attestation_names_the_placeholder() -> TestResult {
    let exchanged = onward_token()?;
    let server = authorization_server(&exchanged, 200).await;
    let a = committing_node().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a.uri(), &lane(&server.uri(), "persons", ""))?;
    let sent = contribution(&attestation());
    let (status, text) = call(app, contribution_post(sent, &bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let [body] = received(&a).await?.try_into().map_err(|_n| "one request")?;
    assert_eq!(
        placeholder(),
        body["versions"][0]["attestations"][0]["committer"]
    );
    Ok(())
}

/// An attestation through a `federation` lane is refused before any token
/// is asked for and nothing reaches the node, which refuses every
/// attestation by an external person.
#[tokio::test]
async fn an_attestation_through_a_federation_lane_is_refused_before_sending() -> TestResult {
    let server = authorization_server("unused", 200).await;
    let a = committing_node().await;
    let dir = tempfile::tempdir()?;
    let tables = federation_lane(&server.uri());
    let app = gateway(dir.path(), &a.uri(), &tables)?;
    let sent = contribution(&attestation());
    let (status, text) = call(app, contribution_post(sent, &bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert!(
        exchanges(&server).await?.is_empty(),
        "no token was asked for"
    );
    assert!(received(&a).await?.is_empty(), "node A was sent nothing");
    Ok(())
}

/// A header commit through an onward lane is forwarded as the client sent
/// it: the gateway writes no committer of its own into it.
#[tokio::test]
async fn a_header_commit_carries_no_committer_of_the_gateways() -> TestResult {
    let exchanged = onward_token()?;
    let server = authorization_server(&exchanged, 200).await;
    let a = Server::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a.uri(), &lane(&server.uri(), "persons", ""))?;
    let uid = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
    let request = Request::put(format!(
        "/v1/ehr/{}/composition/8849182c-82ad-4088-a07f-48ead4180515",
        crate::facade::EHR_A
    ))
    .header(header::CONTENT_TYPE, "application/json")
    .header(header::AUTHORIZATION, bearer_as(CALLER)?)
    .header(header::IF_MATCH, format!("\"{uid}\""))
    .header("openehr-audit-details", "change_type.code_string=\"251\"")
    .header("openEHR-federation-endpoint", "node-a-pub")
    .body(Body::from(r#"{"_type":"COMPOSITION"}"#))?;

    let _answer = call(app, request).await?;
    for sent in a.received_requests().await.ok_or("recording is on")? {
        let audit: Vec<&str> = sent
            .headers
            .get_all("openehr-audit-details")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect();
        assert!(
            audit.iter().all(|line| !line.contains("committer")),
            "{audit:?}"
        );
    }
    Ok(())
}

/// The `Authorization` values `server` received.
async fn authorizations(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .filter_map(|request| request.headers.get(header::AUTHORIZATION.as_str()))
        .filter_map(|value| value.to_str().ok().map(str::to_owned))
        .collect())
}

/// No inbound request reaches node A under the token of the endpoint's
/// client credentials grant, though the authorization server would issue
/// one: a person's query and write go under the exchanged token, and a
/// system caller, its token's `sub` its own client (RFC 9068 §2.2), is
/// refused patient data at the guard and every other read at the lane, with
/// nothing sent (§13.1, N25).
// conformance: CP-17
#[tokio::test]
async fn no_inbound_request_reaches_the_node_under_the_client_credentials_token() -> TestResult {
    let exchanged = onward_token()?;
    let server = authorization_server(&exchanged, 200).await;
    let a = node(&exchanged).await?;
    Mock::given(method("POST"))
        .and(path(format!(
            "/v1/ehr/{}/contribution",
            crate::facade::EHR_A
        )))
        .respond_with(ResponseTemplate::new(201))
        .mount(&a)
        .await;
    let dir = tempfile::tempdir()?;
    let tables = lane(&server.uri(), "persons", "");

    let app = gateway(dir.path(), &a.uri(), &tables)?;
    let (status, text) = call(app, patient_post(&bearer_as(CALLER)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let app = gateway(dir.path(), &a.uri(), &tables)?;
    let write = contribution_post(contribution(""), &bearer_as(CALLER)?)?;
    let (status, text) = call(app, write).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let sent = authorizations(&a).await?;
    assert_eq!(vec![format!("Bearer {exchanged}"); 2], sent);

    let app = gateway(dir.path(), &a.uri(), &tables)?;
    let mut claims = crate::support::caller::claims();
    "synthetic-backend".clone_into(&mut claims.sub);
    "synthetic-backend".clone_into(&mut claims.client_id);
    let system = format!("Bearer {}", issuer().mint(&claims)?);
    let (status, text) = call(app, patient_post(&system)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    assert!(text.contains("natural-person-required"), "{text}");
    let app = gateway(dir.path(), &a.uri(), &tables)?;
    let templates = Request::get("/v1/definition/template/adl1.4")
        .header(header::AUTHORIZATION, &system)
        .header("openEHR-federation-endpoint", "node-a-pub")
        .body(Body::empty())?;
    let (status, text) = call(app, templates).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert_eq!(
        2,
        authorizations(&a).await?.len(),
        "the system was sent nothing"
    );
    assert!(
        !authorizations(&a)
            .await?
            .contains(&format!("Bearer {OWN_TOKEN}")),
        "the gateway's own token never reached the node"
    );
    Ok(())
}
