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
            "client_auth = \"client_secret_post\"\n",
            "client_secret = \"synthetic-onward-secret\"\n",
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
        onward = ISSUER,
        kind = kind,
        node = NODE,
        extra = extra,
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
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n{}\n{tables}",
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
    let federation = format!(
        "[credentials.\"node-a-pub\".onward.federation]\ncontact_point = false\nexchanges_from = [\"{ISSUER}\"]\nperson_mapping = \"sub\"\ncountries = [\"NL\"]\nissuing_authorities = [\"synthetic-authority\"]\nprovider_identifier_prefixes = [\"urn:oid:2.999\"]\n"
    );
    let tables = lane(&server.uri(), "federation", "").replace(
        "[credentials.\"node-a-pub\".onward.keys]",
        &format!("{federation}\n[credentials.\"node-a-pub\".onward.keys]"),
    );
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
