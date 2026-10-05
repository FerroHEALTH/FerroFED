// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's client of the gateway, against a stub gateway: the
//! self-description of §7a.2 read into its typed body, sent as the operator,
//! and every other answer a typed refusal.

use std::error::Error;
use std::time::Duration;

use ferrofed_viewer::config::settings::GatewaySettings;
use ferrofed_viewer::gateway::{AccessToken, Gateway, GatewayError};
use http::StatusCode;
use openehr_its::rest::client::ClientError;
use secrecy::SecretString;
use url::Url;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// The vendored specification page that carries the §7a.2 example.
const REST_FACADE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/rest-facade.adoc"
);

/// The one `[source,json]` example of the §7a.2 page: an `OPTIONS {base}/`
/// body.
fn options_example() -> Result<String, Box<dyn Error>> {
    let page = std::fs::read_to_string(REST_FACADE)?;
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "[source,json]" && lines.next().map(str::trim) == Some("----") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "----").collect();
            return Ok(body.join("\n"));
        }
    }
    Err("the page carries no JSON example".into())
}

/// A client of the stub gateway at `base`.
fn gateway(base: &str) -> Result<Gateway, Box<dyn Error>> {
    Ok(Gateway::new(&GatewaySettings {
        base: Url::parse(base)?,
        timeout: Duration::from_secs(5),
    })?)
}

/// A synthetic operator token.
fn token() -> AccessToken {
    AccessToken::new(SecretString::from("synthetic-operator-token"))
}

// §7a.2, N30: the self-description is `OPTIONS {base}/`.
#[tokio::test]
async fn the_self_description_is_read_as_the_operator() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/fed/"))
        .and(header("authorization", "Bearer synthetic-operator-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(options_example()?),
        )
        .expect(1)
        .mount(&server)
        .await;
    let gateway = gateway(&format!("{}/fed", server.uri()))?;
    let description = gateway.self_description(&token()).await?;
    let expected: openehr_federation::options::OptionsRoot =
        serde_json::from_str(&options_example()?)?;
    assert_eq!(expected.federation.id, description.federation.id);
    assert_eq!(
        expected.federation.spec_version,
        description.federation.spec_version
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_self_description_carries_the_gateway_status() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let refused = gateway(&server.uri())?
        .self_description(&token())
        .await
        .expect_err("a 404 is no self-description");
    assert!(
        matches!(&refused, GatewayError::Status { status, .. } if *status == StatusCode::NOT_FOUND),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_operator_token_is_a_typed_refusal() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let refused = gateway(&server.uri())?
        .self_description(&token())
        .await
        .expect_err("a 401 is no self-description");
    assert!(
        matches!(
            &refused,
            GatewayError::Call {
                source: ClientError::Unauthorized { .. }
            }
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_body_that_is_not_a_self_description_is_refused() -> Result<(), Box<dyn Error>> {
    let server = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(r#"{"federation":{}}"#),
        )
        .mount(&server)
        .await;
    let refused = gateway(&server.uri())?
        .self_description(&token())
        .await
        .expect_err("an incomplete body is refused");
    assert!(matches!(refused, GatewayError::Call { .. }), "{refused:?}");
    Ok(())
}

#[test]
fn the_its_rest_surface_is_under_the_v1_segment_of_the_base() -> Result<(), Box<dyn Error>> {
    for (base, api) in [
        (
            "https://gateway.example.org",
            "https://gateway.example.org/v1",
        ),
        (
            "https://gateway.example.org/fed",
            "https://gateway.example.org/fed/v1",
        ),
        (
            "https://gateway.example.org/fed/",
            "https://gateway.example.org/fed/v1",
        ),
    ] {
        let gateway = gateway(base)?;
        assert_eq!(api, gateway.its(&token())?.base().as_str(), "{base}");
    }
    Ok(())
}
