// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant as the authorizer of the NVI client, against the harness
//! Nuts node and the harness Localization Service, which answers only the
//! `DPoP`-bound token the node issued: the search carries the token with a
//! proof of the grant's key over the request (the IG's GFI-004 and GFI-005;
//! RFC 9449 §7.1); the token is cached and replaced as it nears its end; a
//! refused grant sends the service nothing; a token the service refuses is
//! dropped; a nonce it demands is answered once (RFC 9449 §9); and the
//! pseudonym reaches neither the authorization server nor a proof (Annex B
//! §B.1, §B.7). Every identifier, key and credential is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::grant::nl::nuts::{NutsAuthorizer, NutsGrant};
use ferrofed_engine::onward::{Clock, SystemClock};
use ferrofed_testkit::dpop;
use ferrofed_testkit::nvi::LocalizationService;
use http::StatusCode;
use nl_generic_functions::identification::{PseudoBsn, Ura};
use nl_generic_functions::nuts_auth::NutsClient;
use nl_generic_functions::nvi::NviClient;
use nl_generic_functions::nvi::error::NviError;
use secrecy::SecretString;
use url::Url;

use crate::nuts::{Setup, setup};
use crate::onward::ManualClock;

type TestResult = Result<(), Box<dyn Error>>;

/// The pseudonym every search asks about, visibly synthetic.
const PSEUDONYM: &str = "pbsn-synthetic-nuts-0001";

/// The care provider the service names for it.
const URA: &str = "ura-test-0001";

/// A timeout no harness answer comes near.
const PROMPT: Duration = Duration::from_secs(5);

/// The authorizer of `grant` for the NVI, reading `clock`.
fn authorizer(
    grant: NutsGrant,
    clock: Arc<dyn Clock>,
) -> Result<Arc<NutsAuthorizer>, Box<dyn Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(Arc::new(NutsAuthorizer::new(
        "nl_gf.nvi",
        grant,
        NutsClient::new(http),
        PROMPT,
        clock,
    )))
}

/// The harness service answering only the token `setup`'s node issued, with
/// the pseudonym indexed, and the NVI client of it authorized by `setup`'s
/// grant reading `clock`.
async fn service(
    setup: &Setup,
    clock: Arc<dyn Clock>,
) -> Result<(LocalizationService, NviClient), Box<dyn Error>> {
    let nvi = LocalizationService::start_guarded(setup.authority.dpop_bound()).await;
    nvi.index(PSEUDONYM, URA);
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let client = NviClient::new(Url::parse(&nvi.base())?, http)?
        .with_authorizer(authorizer(setup.grant.clone(), clock)?);
    Ok((nvi, client))
}

fn patient() -> Result<PseudoBsn, Box<dyn Error>> {
    Ok(PseudoBsn::new(SecretString::from(PSEUDONYM))?)
}

/// The search reaches the service with the token the Nuts node issued,
/// under the `DPoP` scheme, and a proof of the grant's key over the search
/// endpoint and the token (GFI-005; RFC 9449 §4.2, §7.1).
#[tokio::test]
async fn the_search_carries_the_dpop_bound_nuts_token() -> TestResult {
    let setup = setup().await?;
    let (nvi, client) = service(&setup, Arc::new(SystemClock)).await?;

    let localization = client.localize(&patient()?, PROMPT).await?;
    assert_eq!(
        vec![URA],
        localization
            .custodians()
            .iter()
            .map(Ura::as_str)
            .collect::<Vec<_>>()
    );
    assert_eq!(1, setup.authority.issued());
    let [authorization] = nvi
        .headers("authorization")
        .await
        .try_into()
        .map_err(|all| format!("one search: {all:?}"))?;
    let token = authorization
        .strip_prefix("DPoP ")
        .ok_or("the DPoP scheme")?;
    let [proof] = nvi
        .headers(dpop::HEADER)
        .await
        .try_into()
        .map_err(|_all| "one proof")?;
    let verified = dpop::verify(
        &proof,
        "GET",
        &format!("{}/DocumentReference", nvi.base()),
        Some(token),
    )?;
    assert_eq!(setup.prover.thumbprint(), verified.jkt);
    Ok(())
}

/// The pseudonym reaches the Localization Service alone: no form sent to
/// the authorization server and no proof names it (Annex B §B.7; RFC 9449
/// §4.2 `htu` carries no query).
// conformance: CP-26
#[tokio::test]
async fn the_pseudonym_reaches_neither_the_authorization_server_nor_a_proof() -> TestResult {
    let setup = setup().await?;
    let (nvi, client) = service(&setup, Arc::new(SystemClock)).await?;
    client.localize(&patient()?, PROMPT).await?;

    for form in setup.authority.forms() {
        for (name, value) in form {
            assert!(
                !value.contains(PSEUDONYM),
                "the {name} form names the pseudonym"
            );
        }
    }
    for proof in nvi.headers(dpop::HEADER).await {
        let claims = String::from_utf8(ferrofed_testkit::nuts::payload(&proof)?)?;
        assert!(!claims.contains(PSEUDONYM), "a proof names the pseudonym");
    }
    Ok(())
}

/// The token is served from the cache until 30 seconds before the lifetime
/// it was issued with ends, and then replaced.
#[tokio::test]
async fn the_token_is_cached_and_replaced_as_it_nears_its_end() -> TestResult {
    let setup = setup().await?;
    let clock = ManualClock::new();
    let (_nvi, client) = service(&setup, clock.clone()).await?;

    client.localize(&patient()?, PROMPT).await?;
    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(1, setup.authority.issued(), "the token is cached");

    clock.advance(Duration::from_secs(269))?;
    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(1, setup.authority.issued(), "still fresh at 269 of 300 s");

    clock.advance(Duration::from_secs(1))?;
    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(2, setup.authority.issued(), "replaced 30 s before its end");
    Ok(())
}

/// A grant the authorization server refuses sends the service nothing, and
/// the failure names neither the credential nor the presentation (RFC 6749
/// §5.2; Nuts RFC021 §4.5).
#[tokio::test]
async fn a_refused_grant_sends_the_service_nothing() -> TestResult {
    let setup = setup().await?;
    setup
        .authority
        .refuse(400, "invalid_request", "the presentation was refused");
    let (nvi, client) = service(&setup, Arc::new(SystemClock)).await?;

    let Err(error) = client.localize(&patient()?, PROMPT).await else {
        return Err("a refused grant localized".into());
    };
    assert!(matches!(error, NviError::Unauthenticated(_)), "{error:?}");
    assert!(
        nvi.searches().await.is_empty(),
        "the service was sent nothing"
    );
    let mut rendered = format!("{error} {error:?}");
    let mut cause = error.source();
    while let Some(inner) = cause {
        write!(rendered, " {inner} {inner:?}")?;
        cause = inner.source();
    }
    for part in setup.credential.split('.').filter(|part| part.len() > 8) {
        assert!(!rendered.contains(part), "the failure names the credential");
    }
    for form in setup.authority.forms() {
        for (name, value) in form {
            if name == "assertion" {
                for part in value.split('.').filter(|part| part.len() > 8) {
                    assert!(
                        !rendered.contains(part),
                        "the failure names the presentation"
                    );
                }
            }
        }
    }
    assert!(
        !rendered.contains(PSEUDONYM),
        "the failure names the pseudonym"
    );
    Ok(())
}

/// A token the service no longer accepts fails that search with the
/// service's `401`, and is dropped, so the next search obtains another.
#[tokio::test]
async fn a_token_the_service_refuses_is_dropped() -> TestResult {
    let setup = setup().await?;
    let (_nvi, client) = service(&setup, Arc::new(SystemClock)).await?;
    client.localize(&patient()?, PROMPT).await?;
    setup.authority.revoke_all();

    let Err(error) = client.localize(&patient()?, PROMPT).await else {
        return Err("a revoked token localized".into());
    };
    assert!(
        matches!(error, NviError::Rejected { status } if status == StatusCode::UNAUTHORIZED),
        "{error:?}"
    );
    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(2, setup.authority.issued(), "a new token after the 401");
    Ok(())
}

/// A nonce the service demands is put in a new proof and the search sent
/// once more, and every later proof to the service names it (RFC 9449 §9).
#[tokio::test]
async fn a_nonce_the_service_demands_is_answered_once() -> TestResult {
    let setup = setup().await?;
    let (nvi, client) = service(&setup, Arc::new(SystemClock)).await?;
    nvi.require_nonce("synthetic-nonce-1");

    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(
        2,
        nvi.searches().await.len(),
        "the challenge and the resend"
    );
    client.localize(&patient()?, PROMPT).await?;
    assert_eq!(3, nvi.searches().await.len(), "the nonce is kept");
    assert_eq!(
        1,
        setup.authority.issued(),
        "the token is kept across the challenge"
    );
    Ok(())
}
