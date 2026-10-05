// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of `ferrofed conformance run`, offline: a run that may not
//! write, a patient outside the example arc, a deployment that is not a
//! development one without the acknowledgement, seed files that are not the
//! vendored synthetic content, and a token file with no token are each
//! refused with the usage exit before any node is asked anything.
//!
//! The run against real nodes is in `e2e::conformance_run`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

use ferrofed_server::EXIT_USAGE;
use ferrofed_testkit::mock::Server;

use crate::support::{asked, auth_toml, signed};

type TestResult = Result<(), Box<dyn Error>>;

/// The vendored demo data a run's seed files are read from.
const DEMO_DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-ref/docker/demo-data"
);

/// Renders `code` the way `ExitCode` renders itself, so two are comparable.
fn rendered(code: ExitCode) -> String {
    format!("{code:?}")
}

/// Runs `ferrofed conformance run` with `flags` after the subcommand, on a
/// thread with no runtime, as `main` does.
async fn run(config: &str, flags: Vec<String>) -> Result<ExitCode, Box<dyn Error>> {
    let mut argv = vec![
        "ferrofed".to_owned(),
        "--config".to_owned(),
        config.to_owned(),
        "conformance".to_owned(),
        "run".to_owned(),
    ];
    argv.extend(flags);
    Ok(tokio::task::spawn_blocking(move || ferrofed_server::command::run(argv)).await?)
}

/// The patient, token and seed flags of a run, the token read from `token`
/// and the seed files from `seed`.
fn flags(namespace: &str, token: &Path, seed: &str, extra: &[&str]) -> Vec<String> {
    let mut flags: Vec<String> = [
        "--patient-namespace",
        namespace,
        "--patient-value",
        "ffd-test-0038",
        "--token-file",
        &token.display().to_string(),
        "--seed-data",
        seed,
        "--out",
        &token.with_file_name("report").display().to_string(),
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();
    flags.extend(extra.iter().map(|word| (*word).to_owned()));
    flags
}

/// A deployment over two mock nodes, written to `dir`, with `top` as its
/// top-level keys; returns the configuration file.
fn deployment(dir: &Path, a: &Server, b: &Server, top: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, crate::facade::registry(&a.uri(), &b.uri(), ""))?;
    let text = format!(
        "{top}\n\n[registry]\ndocument = {}\n\n[federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n{}",
        toml::Value::String(document.display().to_string()),
        auth_toml()?
    );
    let file = dir.join("ferrofed.toml");
    std::fs::write(&file, signed(&text))?;
    Ok(file.display().to_string())
}

/// Asserts that neither mock node was asked anything.
async fn nobody_asked(a: &Server, b: &Server) -> TestResult {
    assert_eq!(Vec::<(String, String)>::new(), asked(a).await?, "node A");
    assert_eq!(Vec::<(String, String)>::new(), asked(b).await?, "node B");
    Ok(())
}

#[tokio::test]
async fn a_run_without_allow_writes_is_refused_before_the_configuration_is_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let token = dir.path().join("token");
    std::fs::write(&token, "a-token")?;
    let code = run(
        "/nonexistent/ferrofed.toml",
        flags("urn:oid:2.999.1.1", &token, DEMO_DATA, &[]),
    )
    .await?;
    assert_eq!(
        rendered(ExitCode::from(EXIT_USAGE)),
        rendered(code),
        "the writes are refused before the missing configuration is noticed"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_outside_the_example_arc_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let token = dir.path().join("token");
    std::fs::write(&token, "a-token")?;
    for namespace in ["urn:oid:2.16.840.1.113883.2.4.6.3", "urn:oid:2.9990.1"] {
        let code = run(
            "/nonexistent/ferrofed.toml",
            flags(namespace, &token, DEMO_DATA, &["--allow-writes"]),
        )
        .await?;
        assert_eq!(
            rendered(ExitCode::from(EXIT_USAGE)),
            rendered(code),
            "{namespace} is no synthetic patient's namespace"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_deployment_that_is_not_a_development_one_needs_the_acknowledgement() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "")?;
    let token = dir.path().join("token");
    std::fs::write(&token, "a-token")?;
    let code = run(
        &config,
        flags("urn:oid:2.999.1.1", &token, DEMO_DATA, &["--allow-writes"]),
    )
    .await?;
    assert_eq!(
        rendered(ExitCode::from(EXIT_USAGE)),
        rendered(code),
        "a production profile is refused without the acknowledgement"
    );
    nobody_asked(&a, &b).await
}

#[tokio::test]
async fn seed_files_that_are_not_the_vendored_content_are_refused() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "profile = \"development\"")?;
    let token = dir.path().join("token");
    std::fs::write(&token, "a-token")?;
    let seed = dir.path().join("seed");
    std::fs::create_dir_all(&seed)?;
    for (file, _) in ferrofed_server::conformance::fixture::DIGESTS {
        std::fs::write(seed.join(file), "{\"not\": \"the vendored file\"}")?;
    }
    let code = run(
        &config,
        flags(
            "urn:oid:2.999.1.1",
            &token,
            &seed.display().to_string(),
            &["--allow-writes"],
        ),
    )
    .await?;
    assert_eq!(
        rendered(ExitCode::from(EXIT_USAGE)),
        rendered(code),
        "a run writes the vendored synthetic content and nothing else"
    );
    nobody_asked(&a, &b).await
}

#[tokio::test]
async fn a_token_file_holding_no_token_is_refused() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "profile = \"development\"")?;
    let token = dir.path().join("token");
    std::fs::write(&token, "  \n")?;
    let code = run(
        &config,
        flags("urn:oid:2.999.1.1", &token, DEMO_DATA, &["--allow-writes"]),
    )
    .await?;
    assert_eq!(rendered(ExitCode::from(EXIT_USAGE)), rendered(code));
    nobody_asked(&a, &b).await
}
