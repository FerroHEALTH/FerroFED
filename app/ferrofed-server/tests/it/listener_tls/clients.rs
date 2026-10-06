// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A listener with a client CA admits a client whose certificate that CA
//! signed and refuses one that presents none or another CA's, and `ferrofed
//! healthcheck` presents `healthcheck_identity_file` to it.

use std::sync::Arc;

use ferrofed_server::healthcheck::READINESS;
use ferrofed_server::listener::certificates::Certificates;
use ferrofed_testkit::listener::ListenerCertificates;
use http::StatusCode;

use super::{TestResult, client, ferrofed, presented, quoted, ready_over, tls_files, written};

#[tokio::test(flavor = "multi_thread")]
async fn only_a_client_certificate_the_client_ca_signed_is_admitted() -> TestResult {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let files = set.write(dir.path())?;
    let address = ready_over(Arc::new(Certificates::load(tls_files(&files, true))?)).await?;

    let (status, _) = presented(
        &client(set.ca(), Some(set.client_identity()))?,
        address,
        READINESS,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the proxy's certificate is admitted"
    );

    let anonymous = presented(&client(set.ca(), None)?, address, READINESS).await;
    assert!(
        anonymous.is_err(),
        "a client with no certificate is refused"
    );

    let stranger = ListenerCertificates::generate()?;
    let other = presented(
        &client(set.ca(), Some(stranger.client_identity()))?,
        address,
        READINESS,
    )
    .await;
    assert!(other.is_err(), "another CA's certificate is refused");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_healthcheck_presents_its_identity_to_a_listener_that_requires_one() -> TestResult {
    let dir = tempfile::tempdir()?;
    let set = ListenerCertificates::generate()?;
    let files = set.write(dir.path())?;
    let address = ready_over(Arc::new(Certificates::load(tls_files(&files, true))?)).await?;
    let config = dir.path().join("ferrofed.toml");
    let base = format!(
        "[server]\nlisten = \"{address}\"\n\n[server.tls]\ncertificate_file = {}\nkey_file = {}\nclient_ca_file = {}\n",
        quoted(&files.certificate),
        quoted(&files.key),
        quoted(&files.client_ca),
    );

    std::fs::write(
        &config,
        format!(
            "{base}healthcheck_identity_file = {}\n",
            quoted(&files.client_identity)
        ),
    )?;
    let output = tokio::task::spawn_blocking({
        let config = config.clone();
        move || ferrofed(&["healthcheck"], &config)
    })
    .await??;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    assert!(written(&output).contains("ready"), "{}", written(&output));

    std::fs::write(&config, &base)?;
    let output = tokio::task::spawn_blocking(move || ferrofed(&["healthcheck"], &config)).await??;
    assert_eq!(
        Some(1),
        output.status.code(),
        "no identity, no answer: {}",
        written(&output)
    );
    assert_eq!(1, written(&output).lines().count(), "{}", written(&output));
    Ok(())
}
