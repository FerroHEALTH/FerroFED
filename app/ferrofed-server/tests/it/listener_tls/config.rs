// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `config check` over `[server.tls]` and `[metrics.tls]`: it reads every
//! file as `serve` does, accepts a usable pair, and refuses each fault with
//! exit code 78 naming its key, never quoting a file.

use std::path::Path;

use ferrofed_server::EXIT_CONFIG;
use ferrofed_testkit::listener::{ListenerCertificates, ListenerFiles};

use super::{TestResult, ferrofed, quoted, written};
use crate::support::auth_toml;

/// A marker a refused file holds, which no output may repeat.
const MARKER: &str = "Qz7-synthetic-listener-file";

/// Runs `config check` over `tables`, with the suite's `[auth]`, written in
/// `dir`.
fn check(dir: &Path, tables: &str) -> Result<std::process::Output, Box<dyn std::error::Error>> {
    let config = dir.join("ferrofed.toml");
    std::fs::write(&config, format!("{tables}{}", auth_toml()?))?;
    Ok(ferrofed(&["config", "check"], &config)?)
}

/// The `[server.tls]` table over `files`, with `extra` keys appended.
fn server_tls(files: &ListenerFiles, extra: &str) -> String {
    format!(
        "[server.tls]\ncertificate_file = {}\nkey_file = {}\n{extra}",
        quoted(&files.certificate),
        quoted(&files.key),
    )
}

/// Asserts `output` is a refusal naming `key` that quotes no file.
fn refused(output: &std::process::Output, key: &str) {
    let text = written(output);
    assert_eq!(
        Some(i32::from(EXIT_CONFIG)),
        output.status.code(),
        "{key}: {text}"
    );
    assert!(text.contains(key), "names {key}: {text}");
    assert!(!text.contains(MARKER), "quotes no file: {text}");
    assert!(!text.contains("PRIVATE KEY"), "quotes no key: {text}");
}

#[test]
fn a_usable_certificate_and_key_pass_on_both_listeners() -> TestResult {
    let dir = tempfile::tempdir()?;
    let files = ListenerCertificates::generate()?.write(dir.path())?;
    let metrics = format!(
        "[metrics]\nlisten = \"127.0.0.1:9464\"\n\n[metrics.tls]\ncertificate_file = {}\nkey_file = {}\nclient_ca_file = {}\n",
        quoted(&files.certificate),
        quoted(&files.key),
        quoted(&files.client_ca),
    );
    let output = check(
        dir.path(),
        &format!(
            "{}\n{metrics}",
            server_tls(
                &files,
                &format!(
                    "client_ca_file = {}\nhealthcheck_identity_file = {}\n",
                    quoted(&files.client_ca),
                    quoted(&files.client_identity)
                )
            )
        ),
    )?;
    assert_eq!(Some(0), output.status.code(), "{}", written(&output));
    assert!(written(&output).contains("valid"), "{}", written(&output));
    Ok(())
}

#[test]
fn a_missing_key_or_certificate_is_refused_by_name() -> TestResult {
    let dir = tempfile::tempdir()?;
    let files = ListenerCertificates::generate()?.write(dir.path())?;
    let only_certificate = format!(
        "[server.tls]\ncertificate_file = {}\n",
        quoted(&files.certificate)
    );
    refused(
        &check(dir.path(), &only_certificate)?,
        "server.tls.key_file",
    );
    let only_key = format!("[server.tls]\nkey_file = {}\n", quoted(&files.key));
    refused(
        &check(dir.path(), &only_key)?,
        "server.tls.certificate_file",
    );
    let absent = format!(
        "[server.tls]\ncertificate_file = {}\nkey_file = {}\n",
        quoted(&dir.path().join("absent.crt")),
        quoted(&files.key)
    );
    refused(&check(dir.path(), &absent)?, "server.tls.certificate_file");
    Ok(())
}

#[test]
fn files_that_hold_no_pem_or_do_not_match_are_refused_without_quoting_them() -> TestResult {
    let dir = tempfile::tempdir()?;
    let files = ListenerCertificates::generate()?.write(dir.path())?;
    let other = tempfile::tempdir()?;
    let stranger = ListenerCertificates::generate()?.write(other.path())?;

    let garbage = dir.path().join("garbage.pem");
    std::fs::write(&garbage, MARKER)?;
    let no_key = format!(
        "[server.tls]\ncertificate_file = {}\nkey_file = {}\n",
        quoted(&files.certificate),
        quoted(&garbage)
    );
    refused(&check(dir.path(), &no_key)?, "server.tls.key_file");
    let no_ca = server_tls(&files, &format!("client_ca_file = {}\n", quoted(&garbage)));
    refused(&check(dir.path(), &no_ca)?, "server.tls.client_ca_file");

    let mismatched = format!(
        "[server.tls]\ncertificate_file = {}\nkey_file = {}\n",
        quoted(&files.certificate),
        quoted(&stranger.key)
    );
    refused(&check(dir.path(), &mismatched)?, "server.tls");
    Ok(())
}

#[test]
fn a_healthcheck_identity_the_healthcheck_never_presents_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let files = ListenerCertificates::generate()?.write(dir.path())?;
    let identity = format!(
        "healthcheck_identity_file = {}\n",
        quoted(&files.client_identity)
    );
    refused(
        &check(dir.path(), &server_tls(&files, &identity))?,
        "server.tls.healthcheck_identity_file",
    );
    let metrics = format!(
        "[metrics]\nlisten = \"127.0.0.1:9464\"\n\n[metrics.tls]\ncertificate_file = {}\nkey_file = {}\nclient_ca_file = {}\n{identity}",
        quoted(&files.certificate),
        quoted(&files.key),
        quoted(&files.client_ca),
    );
    refused(
        &check(dir.path(), &metrics)?,
        "metrics.tls.healthcheck_identity_file",
    );
    Ok(())
}
