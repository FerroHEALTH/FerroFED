// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `[[xcpd.gateway]]` marked `myhealth_eu = true` is a MyHealth@EU route,
//! an eHDSI link. A fixed assertion never serves it, and boot refuses one
//! beside it, naming the gateway (eHDSI Messaging Profile 10.0.0 §1.5, SAML
//! Profile 10.0.0 §2.1). It is `https` with mutual TLS under its own trust
//! roots (Section II Security Services 10.0.0 §5.9.1, §5.9.2), and its
//! certificate chain is checked for revocation before a discovery is sent,
//! so a revoked chain fails the query closed and asks no member (X.509
//! Certificates Profile 7.0.0 §3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use ferrofed_server::config::Config;
use ferrofed_server::config::error;
use ferrofed_server::config::error::binding::{BindingFault, MyHealthEuNeed};
use ferrofed_testkit::revocation::{RevocationFront, Source, Standing};
use ferrofed_testkit::xcpd::{Answer, Community, PATH, RespondingGateway};
use http::StatusCode;

use crate::facade::{Answer as Federated, body, patient_query, post};
use crate::support::{call, injected};
use crate::xcpd::{COMMUNITIES, asked_counts, config, gateway, members, urls};

type TestResult = Result<(), Box<dyn Error>>;

/// The device line of every `[[xcpd.gateway]]` the shared `config` writes.
const DEVICE: &str = "device = \"2.999.50.1\"\n";

/// `text` with every gateway marked as a MyHealth@EU route.
fn marked(text: &str) -> String {
    text.replace(DEVICE, &format!("{DEVICE}myhealth_eu = true\n"))
}

/// The `[xcpd]` keys of a client identity and trust roots written into
/// `dir` from `identity` and `roots`.
fn link_keys(dir: &Path, identity: &str, roots: &str) -> Result<String, Box<dyn Error>> {
    let identity_file = dir.join("client.pem");
    let roots_file = dir.join("roots.pem");
    std::fs::write(&identity_file, identity)?;
    std::fs::write(&roots_file, roots)?;
    Ok(format!(
        "tls = {{ client_identity_file = {}, trust_roots_file = {} }}\n",
        toml::Value::String(identity_file.display().to_string()),
        toml::Value::String(roots_file.display().to_string()),
    ))
}

/// What configuration load answers for the marked `[xcpd]` over one
/// gateway at `url`, under `profile`, with the `[xcpd]` keys `extra`.
fn resolved(
    dir: &Path,
    url: &str,
    profile: &str,
    extra: &str,
) -> Result<Result<(), error::Error>, Box<dyn Error>> {
    let text = marked(&config(
        dir,
        [
            "https://a.example.org",
            "https://b.example.org",
            "https://c.example.org",
        ],
        &[url.to_owned()],
        profile,
        extra,
    )?);
    Ok(
        Config::from_sources(Some(&injected(&text)), &BTreeMap::new())?
            .resolve()
            .map(|_settings| ()),
    )
}

const HTTPS: &str = "https://xcpd.example.org/RespondingGateway";

#[test]
fn a_fixed_assertion_beside_a_myhealth_eu_gateway_refuses_to_boot_naming_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let assertion = dir.path().join("assertion.xml");
    std::fs::write(&assertion, "<saml:Assertion/>")?;
    let mut extra = link_keys(dir.path(), "identity", "roots")?;
    writeln!(
        extra,
        "assertion_file = {}",
        toml::Value::String(assertion.display().to_string())
    )?;
    for profile in ["production", "development"] {
        match resolved(dir.path(), HTTPS, profile, &extra)? {
            Err(error::Error::Binding(BindingFault::FixedAssertionOnMyHealthEu { gateway })) => {
                assert_eq!("xcpd.gateway[0]", gateway);
            }
            other => {
                return Err(format!("a fixed assertion never serves eHDSI: {other:?}").into());
            }
        }
    }
    Ok(())
}

#[test]
fn a_myhealth_eu_gateway_without_a_client_identity_refuses_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let roots = dir.path().join("roots.pem");
    std::fs::write(&roots, "roots")?;
    let extra = format!(
        "tls = {{ trust_roots_file = {} }}\n",
        toml::Value::String(roots.display().to_string())
    );
    match resolved(dir.path(), HTTPS, "production", &extra)? {
        Err(error::Error::Binding(BindingFault::MyHealthEuLink { gateway, needs })) => {
            assert_eq!("xcpd.gateway[0]", gateway);
            assert_eq!(MyHealthEuNeed::ClientIdentity, needs);
            Ok(())
        }
        other => Err(format!("an eHDSI link is mutual TLS: {other:?}").into()),
    }
}

#[test]
fn a_myhealth_eu_gateway_without_trust_roots_refuses_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let identity = dir.path().join("client.pem");
    std::fs::write(&identity, "identity")?;
    let extra = format!(
        "tls = {{ client_identity_file = {} }}\n",
        toml::Value::String(identity.display().to_string())
    );
    match resolved(dir.path(), HTTPS, "production", &extra)? {
        Err(error::Error::Binding(BindingFault::MyHealthEuLink { gateway, needs })) => {
            assert_eq!("xcpd.gateway[0]", gateway);
            assert_eq!(MyHealthEuNeed::TrustRoots, needs);
            Ok(())
        }
        other => Err(format!("an eHDSI link trusts its own roots: {other:?}").into()),
    }
}

#[test]
fn a_myhealth_eu_gateway_over_http_refuses_to_boot_even_in_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let extra = link_keys(dir.path(), "identity", "roots")?;
    match resolved(
        dir.path(),
        "http://xcpd.example.org/RespondingGateway",
        "development",
        &extra,
    )? {
        Err(error::Error::Binding(BindingFault::MyHealthEuLink { gateway, needs })) => {
            assert_eq!("xcpd.gateway[0]", gateway);
            assert_eq!(MyHealthEuNeed::Https, needs);
            Ok(())
        }
        other => Err(format!("an eHDSI link is TLS in every profile: {other:?}").into()),
    }
}

/// A stub gateway holding the patient in `node-a`'s community behind a
/// front whose chain names `source` and stands as `standing`.
async fn behind(
    source: Source,
    standing: Standing,
) -> Result<(RespondingGateway, RevocationFront), Box<dyn Error>> {
    let stub = RespondingGateway::answering(Answer::Holds(vec![Community::new(
        COMMUNITIES[0],
        "2.999.50.2",
        "PID-SYNTH-A",
    )]))
    .await;
    let origin = stub
        .endpoint()
        .strip_suffix(PATH)
        .ok_or("the stub serves its path")?
        .to_owned();
    let front = RevocationFront::start(&origin, source, standing).await?;
    Ok((stub, front))
}

#[tokio::test]
async fn a_revoked_chain_at_a_myhealth_eu_gateway_fails_the_query_closed() -> TestResult {
    let servers = members().await;
    let [a, b, c] = urls(&servers);
    let (stub, front) = behind(Source::Ocsp, Standing::Revoked).await?;
    let dir = tempfile::tempdir()?;
    let extra = link_keys(dir.path(), front.client_identity(), front.trust_roots())?;
    let app = gateway(&marked(&config(
        dir.path(),
        [&a, &b, &c],
        &[format!("{}{PATH}", front.origin())],
        "development",
        &extra,
    )?))?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Federated = serde_json::from_str(&text)?;
    for endpoint in &answer.meta.federation.endpoints {
        assert_eq!("not-localized", endpoint.status);
        assert!(endpoint.error.is_some(), "every member carries the error");
    }
    assert_eq!(
        [0, 0, 0],
        asked_counts(&servers).await?,
        "no member is asked"
    );
    assert!(
        stub.requests().await?.is_empty(),
        "nothing reaches the gateway"
    );
    Ok(())
}

#[tokio::test]
async fn a_good_chain_at_a_myhealth_eu_gateway_selects_the_members_asked() -> TestResult {
    let servers = members().await;
    let [a, b, c] = urls(&servers);
    let (stub, front) = behind(Source::Crl, Standing::Good).await?;
    let dir = tempfile::tempdir()?;
    let extra = link_keys(dir.path(), front.client_identity(), front.trust_roots())?;
    let app = gateway(&marked(&config(
        dir.path(),
        [&a, &b, &c],
        &[format!("{}{PATH}", front.origin())],
        "development",
        &extra,
    )?))?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        [1, 0, 0],
        asked_counts(&servers).await?,
        "node-a alone is asked"
    );
    assert_eq!(1, stub.requests().await?.len());
    Ok(())
}
