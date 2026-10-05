// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An IHE FHIR service reached with the OAuth 2.0 client-credentials grant
//! (RFC 6749 §4.4; IHE IUA ITI-71 §3.71.4.1.2.1), through the real
//! configuration path, against the harness token endpoint: the PIX Manager
//! is asked with a token the grant obtained and incorporated as a bearer
//! token (ITI-72 §3.72.4.2), the token is reused while it is fresh, a `401`
//! gets one more request with a fresh token (§3.72.4.3), a refused grant
//! fails the query before any node is asked, and `config check` names every
//! key it refuses and never the secret.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_server::EXIT_CONFIG;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::grant::GrantFault;
use ferrofed_server::config::settings::Scheme;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::TokenEndpoint;
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{
    Answer, EHR_A, EHR_B, PATIENT, body, gateway, node_answering, patient_query, post, registry,
    statuses,
};
use crate::run::binary;
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The `ehr_id` domains of node A and node B at the PIX Manager.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

const OPERATION: &str = "/fhir/Patient/$ihe-pix";

/// The client the Manager's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-pix-consumer";

/// The synthetic client secret; no part of it may reach a refusal.
const SECRET: &str = "Qz7synthetic-pix-secret";

/// The scope the token is asked for, as the Manager's server defines it.
const SCOPE: &str = "*";

/// A PIX Manager that answers ITI-83 with both members' `ehr_id`s to a
/// request carrying a token `endpoint` issued and still accepts, and `401`
/// to every other.
async fn manager_requiring(endpoint: &TokenEndpoint) -> Server {
    let pix = Server::start().await;
    let answer = format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}},{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_B}","value":"{EHR_B}"}}}}]}}"#
    );
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .and(endpoint.bearer())
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/fhir+json"),
        )
        .with_priority(1)
        .mount(&pix)
        .await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&pix)
        .await;
    pix
}

/// The `[pixm]` table of one Manager at `pix`, authenticated by the grant
/// at `token_url` with the secret in `secret_file`.
fn pixm(pix: &str, token_url: &str, secret_file: &Path) -> String {
    let file = toml::Value::String(secret_file.display().to_string());
    format!(
        "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{pix}/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\"node-b\" = \"{DOMAIN_B}\"\n\n[pixm.manager.credentials.oauth2]\ngrant = \"client_credentials\"\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"client_secret_basic\"\nclient_secret_file = {file}\nscope = \"{SCOPE}\"\n"
    )
}

/// Writes [`SECRET`] to a file under `dir`, with the newline an editor adds.
fn secret_file(dir: &Path) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let file = dir.join("pix-consumer-secret");
    std::fs::write(&file, format!("{SECRET}\n"))?;
    Ok(file)
}

/// The number of requests `server` received.
async fn asked(server: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .len())
}

// conformance: CP-3
#[tokio::test]
async fn the_manager_is_asked_with_a_token_the_grant_obtained_and_refreshed() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret(SECRET);
    endpoint.expect_scope(SCOPE);
    let pix = manager_requiring(&endpoint).await;
    let dir = tempfile::tempdir()?;
    let tables = pixm(&pix.uri(), &endpoint.token_url(), &secret_file(dir.path())?);
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &tables,
    )?;

    for _ in 0..2 {
        let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        let answer: Answer = serde_json::from_str(&text)?;
        assert_eq!(
            vec![("node-a-pub", "active"), ("node-b-pub", "active")],
            statuses(&answer),
            "{text}"
        );
    }
    assert_eq!(1, endpoint.issued(), "a fresh token is reused");
    assert_eq!(vec!["client_secret_basic"], endpoint.secret_methods());

    endpoint.revoke_all();
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "a refused token is replaced and the ITI-83 request sent once more: {text}"
    );
    assert_eq!(2, endpoint.issued());
    assert!(
        endpoint
            .forms()
            .iter()
            .flatten()
            .all(|(_, value)| !value.contains(PATIENT) && !value.contains(SECRET)),
        "no token request carries the patient identifier or the secret in its body"
    );
    Ok(())
}

#[tokio::test]
async fn a_grant_the_token_endpoint_refuses_fails_the_query_424_and_no_node_is_asked() -> TestResult
{
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret("another synthetic secret");
    let pix = manager_requiring(&endpoint).await;
    let dir = tempfile::tempdir()?;
    let tables = pixm(&pix.uri(), &endpoint.token_url(), &secret_file(dir.path())?);
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &tables,
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(!text.contains(SECRET), "{text}");
    assert_eq!(0, endpoint.issued());
    assert_eq!(0, asked(&pix).await?, "nothing is sent without a token");
    assert_eq!(0, asked(&a).await? + asked(&b).await?, "no node is asked");
    Ok(())
}

/// A `[pixm]` table whose grant carries `oauth2`, under `profile`, asking
/// for [`SCOPE`] unless `oauth2` names a scope.
fn manager_with(profile: &str, oauth2: &str) -> String {
    let scope = if oauth2.contains("scope =") {
        String::new()
    } else {
        format!("scope = \"{SCOPE}\"\n")
    };
    format!(
        "profile = \"{profile}\"\n\n[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"{DOMAIN_A}\"\n\n[pixm.manager.credentials.oauth2]\ngrant = \"client_credentials\"\nclient_id = \"{CLIENT_ID}\"\n{scope}{oauth2}"
    )
}

/// The refusal `text` resolves to.
fn refusal(text: &str) -> Result<ConfigError, Box<dyn Error>> {
    match Config::from_sources(Some(text), &BTreeMap::new()).and_then(|config| config.resolve()) {
        Ok(_) => Err(format!("the configuration was accepted: {text}").into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn config_check_names_the_key_it_refuses_and_never_the_secret() -> TestResult {
    let key = "pixm.manager[0].credentials.oauth2";
    let cases = [
        (
            manager_with(
                "development",
                "token_endpoint = \"https://as.example.org/token\"\nclient_auth = \"client_secret_basic\"\n",
            ),
            format!("{key}.client_secret"),
        ),
        (
            manager_with(
                "production",
                &format!(
                    "token_endpoint = \"http://as.example.org/token\"\nclient_auth = \"client_secret_post\"\nclient_secret = \"{SECRET}\"\n"
                ),
            ),
            format!("{key}.token_endpoint"),
        ),
        (
            manager_with(
                "development",
                &format!(
                    "token_endpoint = \"https://as.example.org/token\"\nclient_auth = \"tls_client_auth\"\nclient_secret = \"{SECRET}\"\n"
                ),
            ),
            format!("{key}.client_auth"),
        ),
    ];
    for (toml, named) in cases {
        let output = binary(&["config", "check"], &toml)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            Some(i32::from(EXIT_CONFIG)),
            output.status.code(),
            "{named}: {stderr}"
        );
        assert!(stderr.contains(&named), "names {named}: {stderr}");
        assert!(!stderr.contains("Qz7"), "quotes no secret: {stderr}");
    }
    Ok(())
}

/// Whether a refusal is the one a case expects, naming the key it is given.
type Refused = fn(&ConfigError, &str) -> bool;

#[test]
fn a_service_grant_holds_each_key_to_its_rule() -> TestResult {
    let key = "pixm.manager[0].credentials.oauth2";
    let endpoint = "token_endpoint = \"https://as.example.org/token\"\n";
    let cases: [(&str, Refused, String); 5] = [
        (
            "client_auth = \"client_secret_basic\"\ndpop_key_file = \"/nonexistent/dpop.pem\"\nclient_secret = \"s\"\n",
            |error, key| matches!(error, ConfigError::GrantFault(GrantFault::NodeOnly { key: named }) if named == key),
            format!("{key}.dpop_key_file"),
        ),
        (
            "client_auth = \"private_key_jwt\"\nclient_secret = \"s\"\n",
            |error, key| matches!(error, ConfigError::GrantFault(GrantFault::SecretUnused { key: named }) if named == key),
            format!("{key}.client_secret"),
        ),
        (
            "client_auth = \"private_key_jwt\"\n",
            |error, key| matches!(error, ConfigError::GrantWithoutSigning { section } if section == key),
            key.to_owned(),
        ),
        (
            "client_auth = \"client_secret_basic\"\nclient_secret = \"s\"\nscope = \"say\\\"hi\\\"\"\n",
            |error, key| matches!(error, ConfigError::ServiceScope { key: named, .. } if named == key),
            format!("{key}.scope"),
        ),
        (
            "client_auth = \"client_secret_basic\"\n",
            |error, key| matches!(error, ConfigError::Missing { key: named } if named == key),
            format!("{key}.client_secret"),
        ),
    ];
    for (table, refused, named) in cases {
        let text = manager_with("development", &format!("{endpoint}{table}"));
        let error = refusal(&text)?;
        assert!(refused(&error, &named), "{named}: {error:?}");
        assert!(!format!("{error} {error:?}").contains(SECRET));
    }
    Ok(())
}

#[test]
fn a_service_grant_resolves_and_shows_no_secret() -> TestResult {
    let text = manager_with(
        "development",
        &format!(
            "token_endpoint = \"https://as.example.org/token\"\nclient_auth = \"client_secret_post\"\nclient_secret = \"{SECRET}\"\n"
        ),
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let credentials = settings
        .pixm
        .as_ref()
        .and_then(|pixm| pixm.managers.first())
        .and_then(|manager| manager.credentials.as_ref());
    assert!(
        matches!(credentials, Some(Scheme::ServiceGrant(_))),
        "{credentials:?}"
    );
    assert!(!format!("{settings:?}").contains(SECRET));
    Ok(())
}

#[test]
fn a_node_grant_takes_no_client_secret() -> TestResult {
    let node = |table: &str| {
        format!(
            "[credentials.\"node-a-pub\".oauth2]\ngrant = \"client_credentials\"\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"gateway\"\nscope = \"system/aql-*.s\"\n{table}"
        )
    };
    let error = refusal(&node("client_auth = \"client_secret_basic\"\n"))?;
    assert!(
        matches!(
            &error,
            ConfigError::GrantFault(GrantFault::SecretForNode { key })
                if key == "credentials.node-a-pub.oauth2.client_auth"
        ),
        "{error:?}"
    );
    let error = refusal(&node(&format!(
        "client_auth = \"private_key_jwt\"\nclient_secret = \"{SECRET}\"\n"
    )))?;
    assert!(
        matches!(
            &error,
            ConfigError::GrantFault(GrantFault::SecretForNode { key })
                if key == "credentials.node-a-pub.oauth2.client_secret"
        ),
        "{error:?}"
    );
    assert!(!format!("{error} {error:?}").contains(SECRET));
    Ok(())
}
