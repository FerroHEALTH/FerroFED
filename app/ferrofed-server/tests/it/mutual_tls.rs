// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Mutual TLS to the identity services `[[pixm.manager]]`, `[pdqm]`,
//! `[pmir]` and `[registry.mcsd]` name (#507): each harness server sits
//! behind the testkit's mutual-TLS front, the gateway presents the client
//! identity of `client_identity_file` and trusts the front by
//! `trust_roots_file`, and a gateway without the identity is refused at the
//! handshake and fails closed.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use ferrofed_registry::health::Observed;
use ferrofed_server::config::{self, Config};
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_testkit::mcsd::HarnessDirectory;
use ferrofed_testkit::pmir::PatientIdentityRegistry;
use ferrofed_testkit::tls::MutualTls;
use http::StatusCode;
use url::Url;

use crate::facade::{Answer, body, gateway_within, node_answering, post, registry, statuses};
use crate::pdqm::{ASK_ALL, gateway, local_query, manager, supplier};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The `http://host:port` origin of `base`.
fn origin(base: &str) -> Result<String, Box<dyn Error>> {
    let url = Url::parse(base)?;
    Ok(format!(
        "http://{}:{}",
        url.host_str().ok_or("a host")?,
        url.port_or_known_default().ok_or("a port")?
    ))
}

/// `base` reached through `front` instead of its own origin.
fn behind(front: &MutualTls, base: &str) -> Result<String, Box<dyn Error>> {
    let url = Url::parse(base)?;
    Ok(format!("{}{}", front.origin(), url.path()))
}

/// The TLS keys of a table that presents `front`'s client identity and
/// trusts it, the files written into `dir` under `name`.
fn tls_keys(dir: &Path, name: &str, front: &MutualTls) -> Result<String, Box<dyn Error>> {
    let identity = dir.join(format!("{name}-client.pem"));
    let roots = dir.join(format!("{name}-roots.pem"));
    std::fs::write(&identity, front.client_identity())?;
    std::fs::write(&roots, front.trust_roots())?;
    Ok(format!(
        "client_identity_file = {}\ntrust_roots_file = {}\n",
        toml::Value::String(identity.display().to_string()),
        toml::Value::String(roots.display().to_string())
    ))
}

/// The trust keys alone: the gateway trusts the front and presents nothing.
fn roots_only(dir: &Path, name: &str, front: &MutualTls) -> Result<String, Box<dyn Error>> {
    let roots = dir.join(format!("{name}-roots.pem"));
    std::fs::write(&roots, front.trust_roots())?;
    Ok(format!(
        "trust_roots_file = {}\n",
        toml::Value::String(roots.display().to_string())
    ))
}

#[tokio::test]
async fn pixm_and_pdqm_resolve_over_mutual_tls() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pdq = supplier().await?;
    let pix_front = MutualTls::front(&pix.uri())?;
    let pdq_front = MutualTls::front(pdq.origin())?;
    let dir = tempfile::tempdir()?;
    let tables = format!(
        "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n{}\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n\n[pdqm]\nurl = \"{}\"\nmaster = \"{}\"\ntimeout_ms = 4000\n{}\n[pdqm.namespaces]\n\"{}\" = \"{}\"\n",
        pix_front.origin(),
        tls_keys(dir.path(), "pix", &pix_front)?,
        crate::pdqm::DOMAIN_A,
        crate::pdqm::DOMAIN_B,
        behind(&pdq_front, &pdq.base_url())?,
        crate::pdqm::MASTER,
        tls_keys(dir.path(), "pdq", &pdq_front)?,
        crate::pdqm::LOCAL,
        crate::pdqm::LOCAL,
    );
    // A handshake can take seconds where the platform verifier asks the system,
    // so the budgets leave room for two of them.
    let app = gateway_within(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &tables,
        (4_000, 9_000),
    )?;
    let (status, text) = call(app, post(body(&local_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer)
    );
    assert!(
        pdq_front.handshakes() >= 1,
        "the Supplier saw the client certificate"
    );
    assert!(
        pix_front.handshakes() >= 1,
        "the Manager saw the client certificate"
    );
    assert_eq!(1, pdq.searches());
    Ok(())
}

#[tokio::test]
async fn a_pix_manager_that_requires_a_client_certificate_refuses_a_gateway_without_one()
-> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let pix = manager().await;
    let pix_front = MutualTls::front(&pix.uri())?;
    let dir = tempfile::tempdir()?;
    let tables = format!(
        "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n{}\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n",
        pix_front.origin(),
        roots_only(dir.path(), "pix", &pix_front)?,
        crate::pdqm::DOMAIN_A,
        crate::pdqm::DOMAIN_B,
    );
    let (app, _state) = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ASK_ALL,
        &tables,
    )?;
    let query =
        crate::facade::patient_query().replace(crate::facade::PATIENT, crate::pdqm::MASTER_ID);
    let (status, text) = call(app, post(body(&query)?)?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "a Manager the gateway cannot reach fails the query: {text}"
    );
    assert_eq!(0, pix_front.handshakes(), "no handshake completed");
    assert!(pix_front.refused() >= 1, "the handshake was refused");
    assert_eq!(0, pix.received_requests().await.ok_or("recording")?.len());
    Ok(())
}

#[tokio::test]
async fn the_registry_is_read_from_a_directory_over_mutual_tls() -> TestResult {
    let a = node_answering("uid-at-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-at-b::cdr-b.example.org::1").await;
    let directory = HarnessDirectory::start().await;
    directory.publish(&crate::registry_mcsd::members(&a.uri(), &b.uri()))?;
    let front = MutualTls::front(&origin(&directory.base())?)?;
    let dir = tempfile::tempdir()?;
    let text = crate::registry_mcsd::config(&behind(&front, &directory.base())?, 5_000).replace(
        "refresh_interval_s = 3600\n",
        &format!(
            "refresh_interval_s = 3600\n{}",
            tls_keys(dir.path(), "mcsd", &front)?
        ),
    );
    let gateway = crate::registry_mcsd::Gateway::boot_from(&text)?;
    let addresses = gateway.addresses()?;
    assert_eq!(2, addresses.len(), "{addresses:?}");
    assert!(
        front.handshakes() >= 1,
        "the directory saw the client certificate"
    );
    Ok(())
}

#[tokio::test]
async fn the_identity_feed_subscribes_over_mutual_tls() -> TestResult {
    let registry = PatientIdentityRegistry::start().await?;
    let front = MutualTls::front(&origin(&registry.base_url())?)?;
    let dir = tempfile::tempdir()?;
    let text = crate::pmir::text(
        dir.path(),
        &behind(&front, &registry.base_url())?,
        "http://127.0.0.1:9/pmir/feed",
        &tls_keys(dir.path(), "pmir", &front)?,
    )?;
    let gateway = crate::pmir::gateway(&text)?;
    let feed = gateway.state.identity_feed().ok_or("[pmir] is set")?;
    assert_eq!(Observed::Up, feed.check().await);
    assert_eq!(1, registry.subscriptions().len());
    assert!(
        front.handshakes() >= 1,
        "the Registry saw the client certificate"
    );
    Ok(())
}

/// What resolving the development configuration `text` refuses.
fn refusal(text: &str) -> Result<config::error::Error, Box<dyn Error>> {
    let text = format!("profile = \"development\"\n[audit]\ndestination = \"log\"\n\n{text}");
    match Config::from_sources(Some(&text), &BTreeMap::new()).and_then(|config| config.resolve()) {
        Ok(_) => Err(format!("the configuration was accepted: {text}").into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn a_tls_file_that_cannot_be_read_is_refused_naming_its_key() -> TestResult {
    let missing = "trust_roots_file = \"/nonexistent/ferrofed-roots.pem\"\n";
    for (table, key) in [
        (
            format!("[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n{missing}"),
            "pixm.manager[0].trust_roots_file",
        ),
        (
            format!(
                "[pdqm]\nurl = \"https://pdq.example.org/fhir/\"\nmaster = \"urn:oid:2.999.1\"\n{missing}\n[pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n"
            ),
            "pdqm.trust_roots_file",
        ),
        (
            format!(
                "[registry]\ndocument = \"registry.toml\"\n\n[pmir]\nurl = \"https://pmir.example.org/fhir/\"\ncallback_url = \"https://gateway.example.org/pmir/feed\"\nfeed_token = \"Qz7feedtoken\"\n{missing}"
            ),
            "pmir.trust_roots_file",
        ),
        (
            format!("[registry.mcsd]\nurl = \"https://directory.example.org/fhir\"\n{missing}"),
            "registry.mcsd.trust_roots_file",
        ),
    ] {
        let error = refusal(&table)?;
        assert!(
            matches!(&error, config::error::Error::Secret { key: named, .. } if named == key),
            "{key}: {error:?}"
        );
    }
    Ok(())
}

#[test]
fn an_identity_given_inline_and_as_a_file_is_refused() -> TestResult {
    let error = refusal(
        "[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\nclient_identity = \"Qz7inline\"\nclient_identity_file = \"/nonexistent/ferrofed-client.pem\"\n",
    )?;
    assert!(
        matches!(&error, config::error::Error::Conflict { key } if key == "pixm.manager[0].client_identity"),
        "{error:?}"
    );
    assert!(!format!("{error} {error:?}").contains("Qz7inline"));
    Ok(())
}

#[tokio::test]
async fn a_client_identity_that_is_no_pem_is_refused_without_its_content() -> TestResult {
    let dir = tempfile::tempdir()?;
    let identity = dir.path().join("client.pem");
    std::fs::write(&identity, "Qz7notpem")?;
    let tables = format!(
        "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\nclient_identity_file = {}\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n",
        toml::Value::String(identity.display().to_string()),
        crate::pdqm::DOMAIN_A,
        crate::pdqm::DOMAIN_B,
    );
    let settings = crate::pdqm::settings(
        dir.path(),
        &registry("https://cdr-a.example.org", "https://cdr-b.example.org", ""),
        ASK_ALL,
        &tables,
    )?;
    let built = Federation::load(&settings);
    let Err(error) = built else {
        return Err("a client identity that is no PEM is refused".into());
    };
    assert!(matches!(error, FederationError::Tls(_)), "{error:?}");
    assert!(!ferrofed_chain(&error).contains("Qz7notpem"));
    Ok(())
}

/// `error` and every cause behind it, as one line.
fn ferrofed_chain(error: &(dyn Error + 'static)) -> String {
    let mut line = format!("{error} {error:?}");
    let mut cause = error.source();
    while let Some(source) = cause {
        let _written: std::fmt::Result = write!(line, ": {source} {source:?}");
        cause = source.source();
    }
    line
}
