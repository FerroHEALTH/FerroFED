// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mutual-TLS front: a client presenting the identity it issued reaches
//! the origin behind it, and a client without one is refused at the
//! handshake and never reaches the origin.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use ferrofed_testkit::tls::MutualTls;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

async fn origin() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/metadata"))
        .respond_with(ResponseTemplate::new(200).set_body_string("reached"))
        .mount(&server)
        .await;
    server
}

fn client(front: &MutualTls, identity: bool) -> Result<reqwest::Client, Box<dyn Error>> {
    let mut builder = reqwest::Client::builder().tls_certs_merge(
        reqwest::Certificate::from_pem_bundle(front.trust_roots().as_bytes())?,
    );
    if identity {
        builder = builder.identity(reqwest::Identity::from_pem(
            front.client_identity().as_bytes(),
        )?);
    }
    Ok(builder.build()?)
}

#[tokio::test]
async fn a_client_with_the_issued_identity_reaches_the_origin() -> TestResult {
    let server = origin().await;
    let front = MutualTls::front(&server.uri())?;
    let answer = client(&front, true)?
        .get(format!("{}/fhir/metadata", front.origin()))
        .send()
        .await?;
    assert_eq!(http::StatusCode::OK, answer.status());
    assert_eq!("reached", answer.text().await?);
    assert_eq!(1, front.handshakes());
    Ok(())
}

#[tokio::test]
async fn a_client_without_an_identity_is_refused_at_the_handshake() -> TestResult {
    let server = origin().await;
    let front = MutualTls::front(&server.uri())?;
    let answer = client(&front, false)?
        .get(format!("{}/fhir/metadata", front.origin()))
        .send()
        .await;
    assert!(answer.is_err(), "{answer:?}");
    assert_eq!(0, front.handshakes());
    assert_eq!(
        0,
        server.received_requests().await.ok_or("recording")?.len()
    );
    Ok(())
}
