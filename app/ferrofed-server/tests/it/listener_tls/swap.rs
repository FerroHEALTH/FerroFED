// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reload reads the certificate files again: a valid pair is presented
//! from the next handshake on, and a pair that does not form a
//! configuration is refused with the running certificate kept.

use std::sync::Arc;

use ferrofed_server::listener::certificates::{CertificateError, Certificates};
use ferrofed_testkit::listener::ListenerCertificates;
use http::StatusCode;

use super::{TestResult, client, presented, ready_over, tls_files};
use ferrofed_server::healthcheck::READINESS;

#[tokio::test(flavor = "multi_thread")]
async fn a_reload_presents_the_new_certificate_from_the_next_handshake() -> TestResult {
    let dir = tempfile::tempdir()?;
    let first = ListenerCertificates::generate()?;
    let files = first.write(dir.path())?;
    let certificates = Arc::new(Certificates::load(tls_files(&files, false))?);
    let address = ready_over(Arc::clone(&certificates)).await?;

    let (status, leaf) = presented(&client(first.ca(), None)?, address, READINESS).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(first.leaf(), leaf.as_slice(), "the first certificate");

    let second = ListenerCertificates::generate()?;
    second.write(dir.path())?;
    let (_, leaf) = presented(&client(first.ca(), None)?, address, READINESS).await?;
    assert_eq!(
        first.leaf(),
        leaf.as_slice(),
        "the files alone change nothing until a reload"
    );

    certificates.reload()?;
    let (status, leaf) = presented(&client(second.ca(), None)?, address, READINESS).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(second.leaf(), leaf.as_slice(), "the swapped certificate");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_that_does_not_match_is_refused_and_the_running_certificate_stays() -> TestResult {
    let dir = tempfile::tempdir()?;
    let first = ListenerCertificates::generate()?;
    let files = first.write(dir.path())?;
    let certificates = Arc::new(Certificates::load(tls_files(&files, false))?);
    let address = ready_over(Arc::clone(&certificates)).await?;

    let second = ListenerCertificates::generate()?;
    std::fs::write(&files.certificate, second.chain())?;
    let refused = certificates
        .reload()
        .expect_err("a certificate beside another certificate's key is refused");
    assert!(
        matches!(refused, CertificateError::Config { table: "server.tls", .. }),
        "{refused:?}"
    );

    std::fs::write(&files.key, "Qz7-synthetic-not-a-key")?;
    let refused = certificates
        .reload()
        .expect_err("a key file that holds no key is refused");
    let message = refused.to_string();
    assert!(message.contains("server.tls.key_file"), "{message}");
    assert!(
        !message.contains("Qz7"),
        "the file is never quoted: {message}"
    );

    let (status, leaf) = presented(&client(first.ca(), None)?, address, READINESS).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(first.leaf(), leaf.as_slice(), "the running certificate");
    Ok(())
}
