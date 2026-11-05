// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A responding gateway marked as a MyHealth@EU route is an eHDSI link: its
//! certificate chain is checked for revocation before the discovery is
//! sent, a revoked chain or an unreachable source fails the discovery with
//! nothing sent, and a gateway with no mark is reached as before (eHDSI
//! X.509 Certificates Profile 7.0.0 §3.2).

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::Tls;
use ferrofed_identity::ihe::xcpd::{
    GatewayConfig, Transport, XcpdConfig, XcpdConfigError, XcpdLocalizer,
};
use ferrofed_identity::revocation::RevocationCheck;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::localizer::{Localization, Localizer, LocalizerError};
use ferrofed_testkit::revocation::{RevocationFront, Source, Standing};
use ferrofed_testkit::xcpd::{PATH, RespondingGateway};
use ferrohealth_secret::secret_url::SecretUrl;
use secrecy::SecretString;

use crate::support::registry;
use crate::xcpd::{COMMUNITY_A, holds, members, node, patient};

type TestResult = Result<(), Box<dyn Error>>;

/// A stub gateway holding the patient in `node-a`'s community, behind a
/// front whose certificate names `source` and stands as `standing`.
async fn behind(
    source: Source,
    standing: Standing,
) -> Result<(RespondingGateway, RevocationFront), Box<dyn Error>> {
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let origin = stub
        .endpoint()
        .strip_suffix(PATH)
        .ok_or("the stub serves its path")?
        .to_owned();
    let front = RevocationFront::start(&origin, source, standing).await?;
    Ok((stub, front))
}

/// The localizer over `front`, the gateway marked as a MyHealth@EU route
/// when `myhealth_eu` is set, with `revocation` as its check.
fn localizer(
    front: &RevocationFront,
    myhealth_eu: bool,
    revocation: Option<RevocationCheck>,
) -> Result<XcpdLocalizer, Box<dyn Error>> {
    let identity = SecretString::from(front.client_identity());
    let config = XcpdConfig {
        sender_device: "2.999.40.1".to_owned(),
        home_community: Some("2.999.40".to_owned()),
        gateways: vec![GatewayConfig {
            endpoint: SecretUrl::new(format!("{}{PATH}", front.origin())),
            device: "2.999.50.1".to_owned(),
            community: None,
            myhealth_eu,
        }],
        communities: BTreeMap::from([
            (COMMUNITY_A.to_owned(), node("node-a")?),
            ("2.999.60".to_owned(), node("node-b")?),
        ]),
        namespaces: BTreeMap::new(),
        transport: Transport::Encrypted,
        tls: Tls::from_pem(Some(&identity), Some(front.trust_roots()))?,
        revocation,
    };
    Ok(XcpdLocalizer::from_config(config, None, &registry())?)
}

/// The check over `front`'s roots, presenting its client identity.
fn check(front: &RevocationFront) -> Result<RevocationCheck, Box<dyn Error>> {
    Ok(RevocationCheck::from_pem(
        &SecretString::from(front.client_identity()),
        front.trust_roots(),
    )?)
}

/// What `localizer` answers for the test patient.
async fn localize(localizer: &XcpdLocalizer) -> Result<Localization, Box<dyn Error>> {
    Ok(localizer
        .localize(
            &patient()?,
            &members()?,
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_secs(5),
        )
        .await)
}

#[tokio::test]
async fn a_marked_gateway_with_a_good_chain_is_asked() -> TestResult {
    let (stub, front) = behind(Source::Ocsp, Standing::Good).await?;
    let localizer = localizer(&front, true, Some(check(&front)?))?;
    match localize(&localizer).await? {
        Localization::Candidates(found) => {
            assert!(found.contains(&node("node-a")?));
        }
        other => return Err(format!("a good chain is asked: {other:?}").into()),
    }
    assert_eq!(1, stub.requests().await?.len());
    Ok(())
}

#[tokio::test]
async fn a_marked_gateway_with_a_revoked_chain_fails_closed_and_is_sent_nothing() -> TestResult {
    for source in [Source::Crl, Source::Ocsp] {
        let (stub, front) = behind(source, Standing::Revoked).await?;
        let localizer = localizer(&front, true, Some(check(&front)?))?;
        let outcome = localize(&localizer).await?;
        assert!(
            matches!(
                outcome,
                Localization::Unavailable(LocalizerError::Backend(_))
            ),
            "a revoked chain fails the discovery ({source:?}): {outcome:?}"
        );
        assert!(
            stub.requests().await?.is_empty(),
            "nothing is sent ({source:?})"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_marked_gateway_whose_revocation_source_is_unreachable_fails_closed() -> TestResult {
    for source in [Source::UnreachableCrl, Source::UnreachableOcsp] {
        let (stub, front) = behind(source, Standing::Good).await?;
        let localizer = localizer(&front, true, Some(check(&front)?))?;
        let outcome = localize(&localizer).await?;
        assert!(
            matches!(
                outcome,
                Localization::Unavailable(LocalizerError::Backend(_))
            ),
            "an unknown status fails the discovery ({source:?}): {outcome:?}"
        );
        assert!(
            stub.requests().await?.is_empty(),
            "nothing is sent ({source:?})"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_gateway_with_no_mark_is_reached_as_before() -> TestResult {
    let (stub, front) = behind(Source::Crl, Standing::Revoked).await?;
    let localizer = localizer(&front, false, None)?;
    match localize(&localizer).await? {
        Localization::Candidates(found) => {
            assert!(found.contains(&node("node-a")?));
        }
        other => return Err(format!("an unmarked link checks no revocation: {other:?}").into()),
    }
    assert_eq!(1, stub.requests().await?.len());
    Ok(())
}

#[tokio::test]
async fn a_marked_gateway_without_a_revocation_check_is_refused() -> TestResult {
    let (_stub, front) = behind(Source::Crl, Standing::Good).await?;
    match localizer(&front, true, None) {
        Err(error) => match error.downcast_ref::<XcpdConfigError>() {
            Some(XcpdConfigError::Unchecked { index: 0 }) => Ok(()),
            other => Err(format!("refused for its missing check: {other:?}").into()),
        },
        Ok(_) => Err("a MyHealth@EU gateway is never reached unchecked".into()),
    }
}
