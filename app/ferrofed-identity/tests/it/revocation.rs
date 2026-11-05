// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The revocation check of an eHDSI link, the family's
//! `ferrohealth_server::revocation`, against a harness CA chain with a
//! CRL and an OCSP responder: every certificate the peer presents is checked
//! before any data is sent, a revoked one or one whose revocation source
//! cannot be reached fails the connection, and a valid one passes (eHDSI
//! X.509 Certificates Profile 7.0.0 §3.2).

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::mock::{Server, recorded};
use ferrofed_testkit::revocation::{RevocationFront, Source, Standing};
use ferrohealth_secret::secret::Secret;
use ferrohealth_server::revocation::{RevocationCheck, RevocationError};
use ferrohealth_server::section::TlsVersion;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The budget of one check.
const WITHIN: Duration = Duration::from_secs(5);

/// An origin answering `GET /ping`, and the front over it whose server
/// certificate names `source` and stands as `standing`.
async fn link(
    source: Source,
    standing: Standing,
) -> Result<(Server, RevocationFront), Box<dyn Error>> {
    let origin = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(200).set_body_string("pong"))
        .mount(&origin)
        .await;
    let front = RevocationFront::start(&origin.uri(), source, standing).await?;
    Ok((origin, front))
}

/// The check over the front's trust roots, presenting its client identity.
fn check(front: &RevocationFront) -> Result<RevocationCheck, Box<dyn Error>> {
    Ok(RevocationCheck::from_pem(
        front.trust_roots(),
        Some(&Secret::new(front.client_identity())),
        TlsVersion::Tls12,
    )?)
}

/// The front's `/ping` URL.
fn ping(front: &RevocationFront) -> Result<Url, Box<dyn Error>> {
    Ok(Url::parse(&format!("{}/ping", front.origin()))?)
}

/// How many requests reached `origin`.
async fn reached(origin: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(recorded(origin).await?.len())
}

/// Clears the front and asks it once, returning the answer's status.
async fn cleared_and_asked(front: &RevocationFront) -> Result<http::StatusCode, Box<dyn Error>> {
    let check = check(front)?;
    let url = ping(front)?;
    check.clear(&url, WITHIN).await?;
    Ok(check.http_client()?.get(url).send().await?.status())
}

#[tokio::test]
async fn a_good_certificate_by_its_crl_passes() -> TestResult {
    let (origin, front) = link(Source::Crl, Standing::Good).await?;
    assert_eq!(http::StatusCode::OK, cleared_and_asked(&front).await?);
    assert_eq!(1, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn a_good_certificate_by_its_ocsp_responder_passes() -> TestResult {
    let (origin, front) = link(Source::Ocsp, Standing::Good).await?;
    assert_eq!(http::StatusCode::OK, cleared_and_asked(&front).await?);
    assert_eq!(1, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn every_certificate_of_the_chain_is_checked() -> TestResult {
    let (_origin, front) = link(Source::Crl, Standing::Good).await?;
    check(&front)?.clear(&ping(&front)?, WITHIN).await?;
    // The server certificate's CRL and the intermediate's, the root's own.
    assert_eq!(2, front.status_requests().await?);
    Ok(())
}

#[tokio::test]
async fn a_certificate_its_crl_revokes_fails_and_nothing_is_sent() -> TestResult {
    let (origin, front) = link(Source::Crl, Standing::Revoked).await?;
    match check(&front)?.clear(&ping(&front)?, WITHIN).await {
        Err(RevocationError::Revoked { depth: 0 }) => {}
        other => return Err(format!("a revoked certificate fails: {other:?}").into()),
    }
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn a_certificate_its_ocsp_responder_revokes_fails_and_nothing_is_sent() -> TestResult {
    let (origin, front) = link(Source::Ocsp, Standing::Revoked).await?;
    match check(&front)?.clear(&ping(&front)?, WITHIN).await {
        Err(RevocationError::Revoked { depth: 0 }) => {}
        other => return Err(format!("a revoked certificate fails: {other:?}").into()),
    }
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn an_unreachable_crl_fails_closed() -> TestResult {
    let (origin, front) = link(Source::UnreachableCrl, Standing::Good).await?;
    match check(&front)?.clear(&ping(&front)?, WITHIN).await {
        Err(RevocationError::Unavailable { depth: 0, .. }) => {}
        other => return Err(format!("an unknown status fails closed: {other:?}").into()),
    }
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn an_unreachable_ocsp_responder_fails_closed() -> TestResult {
    let (origin, front) = link(Source::UnreachableOcsp, Standing::Good).await?;
    match check(&front)?.clear(&ping(&front)?, WITHIN).await {
        Err(RevocationError::Unavailable { depth: 0, .. }) => {}
        other => return Err(format!("an unknown status fails closed: {other:?}").into()),
    }
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn the_client_sends_nothing_over_a_chain_no_check_cleared() -> TestResult {
    let (origin, front) = link(Source::Crl, Standing::Good).await?;
    let check = check(&front)?;
    let refused = check.http_client()?.get(ping(&front)?).send().await;
    assert!(
        refused.is_err(),
        "an unchecked chain is refused: {refused:?}"
    );
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[tokio::test]
async fn a_revoked_chain_stays_refused_by_the_client() -> TestResult {
    let (origin, front) = link(Source::Ocsp, Standing::Revoked).await?;
    let check = check(&front)?;
    let url = ping(&front)?;
    let checked = check.clear(&url, WITHIN).await;
    assert!(checked.is_err(), "the chain is revoked: {checked:?}");
    let refused = check.http_client()?.get(url).send().await;
    assert!(refused.is_err(), "a revoked chain is refused: {refused:?}");
    assert_eq!(0, reached(&origin).await?);
    Ok(())
}

#[test]
fn material_that_does_not_read_is_refused() {
    let identity = Secret::new("-----BEGIN PRIVATE KEY-----\nQz7\n-----END PRIVATE KEY-----\n");
    assert!(RevocationCheck::from_pem("", Some(&identity), TlsVersion::Tls12).is_err());
}
