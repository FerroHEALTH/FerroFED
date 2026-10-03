// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The purpose of use (§13.4, CP-17 inbound half): required by default, read
//! from the IHE IUA extension or from RFC 9396 `authorization_details`, and
//! optional only where the deployment declares it so.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use ferrofed_server::auth::Refusal;
use ferrofed_testkit::issuer::{ACT_REASON, AuthorizationDetail, Coding, Extensions};

use super::{Gateway, TestResult, assert_admitted, assert_refused, bearing, claims, minted, query};
use crate::support;

/// §13.4, CP-17 inbound half: a token with no purpose of use is a `403`
/// that reaches no node.
#[tokio::test]
async fn a_token_without_a_purpose_of_use_is_403() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut purposeless = claims();
    purposeless.extensions = None;
    let request = bearing(query()?, &minted(&purposeless)?)?;
    assert_refused(&gateway, request, Refusal::PurposeOfUse).await
}

/// §13.4: an IUA coding with no code declares no purpose.
#[tokio::test]
async fn an_iua_coding_without_a_code_declares_none() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut coded = claims();
    let mut extensions = Extensions::treatment();
    extensions.ihe_iua.purpose_of_use = vec![Coding {
        system: ACT_REASON.to_owned(),
        code: String::new(),
    }];
    coded.extensions = Some(extensions);
    let request = bearing(query()?, &minted(&coded)?)?;
    assert_refused(&gateway, request, Refusal::PurposeOfUse).await
}

/// §13.4: a purpose of use in RFC 9396 `authorization_details` is read as
/// well as one in the IUA extension.
#[tokio::test]
async fn a_purpose_in_authorization_details_is_admitted() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut rar = claims();
    rar.extensions = None;
    rar.authorization_details = Some(vec![AuthorizationDetail {
        kind: String::from("example"),
        purpose_of_use: format!("{ACT_REASON}|TREAT"),
    }]);
    assert_admitted(&gateway, bearing(query()?, &minted(&rar)?)?).await
}

/// §13.4: a deployment that declares the purpose optional admits a token
/// without one.
#[tokio::test]
async fn a_deployment_that_declares_it_optional_admits_a_token_without_one() -> TestResult {
    let mut auth = support::auth();
    auth.purpose_required = false;
    let gateway = Gateway::with(auth).await?;
    let mut purposeless = claims();
    purposeless.extensions = None;
    assert_admitted(&gateway, bearing(query()?, &minted(&purposeless)?)?).await
}
