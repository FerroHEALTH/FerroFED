// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of `ferrofed conformance run`, offline: a run that may not
//! write, a patient outside the example arc, a deployment that is not a
//! development one without the acknowledgement, seed files that are not the
//! vendored synthetic content, and a token file with no token are each
//! refused with the usage exit before any node is asked anything. A gateway
//! or a node reached over plain http beyond the host, or over any http
//! outside the development profile, is refused with the configuration exit;
//! the gateway client follows no redirect; the caller's token reaches no
//! report file, no log line and no node; and `--node-profile` reports
//! CP-18, CP-19 and CP-27 against every member, through its node client.
//!
//! The run against real nodes is in `e2e::conformance_run`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::process::{ExitCode, Output};

use ferrofed_server::config::Config;
use ferrofed_server::conformance::client::{Gateway, HttpGateway, post_aql};
use ferrofed_server::conformance::fixture::SyntheticPatient;
use ferrofed_server::conformance::run::RunOptions;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_server::{EXIT_CONFIG, EXIT_USAGE};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use secrecy::SecretString;
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use crate::support::{Logs, asked, auth_toml, signed};

/// A token no part of a run may print, log or forward.
const SENTINEL: &str = "sentinel-token-that-never-appears";

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

/// Runs the real `ferrofed conformance run` with `flags`, for its stderr.
async fn binary(config: &str, flags: Vec<String>) -> Result<Output, Box<dyn Error>> {
    let config = config.to_owned();
    Ok(tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_ferrofed"))
            .args(["--config", &config, "conformance", "run"])
            .args(flags)
            .env_remove("FERROFED_CONFIG")
            .output()
    })
    .await??)
}

/// Asserts that `output` is the configuration exit naming `named`, and that
/// neither stream carries [`SENTINEL`].
fn refused_naming(output: &Output, named: &str) {
    let (stdout, stderr) = (
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        Some(i32::from(EXIT_CONFIG)),
        output.status.code(),
        "{stderr}"
    );
    assert!(
        stderr.contains(named),
        "the refusal names {named}: {stderr}"
    );
    assert!(
        !stdout.contains(SENTINEL) && !stderr.contains(SENTINEL),
        "the token is never printed"
    );
}

#[tokio::test]
async fn an_http_gateway_beyond_the_host_is_refused_under_every_profile() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "profile = \"development\"")?;
    let token = dir.path().join("token");
    std::fs::write(&token, SENTINEL)?;
    let output = binary(
        &config,
        flags(
            "urn:oid:2.999.1.1",
            &token,
            DEMO_DATA,
            &["--allow-writes", "--gateway", "http://gw.example.org/fed/"],
        ),
    )
    .await?;
    refused_naming(&output, "--gateway");
    nobody_asked(&a, &b).await
}

#[tokio::test]
async fn a_node_reached_in_cleartext_is_refused_outside_development() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "")?;
    let token = dir.path().join("token");
    std::fs::write(&token, SENTINEL)?;
    let output = binary(
        &config,
        flags(
            "urn:oid:2.999.1.1",
            &token,
            DEMO_DATA,
            &[
                "--allow-writes",
                "--i-understand-this-writes-synthetic-data-to-the-nodes",
                "--gateway",
                "https://gw.example.org/fed/",
            ],
        ),
    )
    .await?;
    refused_naming(&output, "endpoint");
    nobody_asked(&a, &b).await
}

#[tokio::test]
async fn the_gateway_client_follows_no_redirect() -> TestResult {
    let (gateway, elsewhere) = (Server::start().await, Server::start().await);
    Mock::given(any())
        .respond_with(
            ResponseTemplate::new(StatusCode::TEMPORARY_REDIRECT.as_u16())
                .insert_header("location", format!("{}/v1/query/aql", elsewhere.uri())),
        )
        .mount(&gateway)
        .await;
    let client = HttpGateway::new(gateway.uri().parse()?, SecretString::from(SENTINEL))?;

    let reply = client
        .send(post_aql("SELECT c/uid/value FROM COMPOSITION c", &[])?)
        .await?;
    assert_eq!(
        StatusCode::TEMPORARY_REDIRECT,
        reply.status,
        "the 307 is the answer read"
    );
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&elsewhere).await?,
        "the token never travels to the origin a redirect names"
    );
    assert_eq!(
        1,
        asked(&gateway).await?.len(),
        "the gateway was asked once"
    );
    Ok(())
}

#[tokio::test]
async fn a_run_writes_the_callers_token_to_no_report_and_no_log() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "profile = \"development\"")?;
    let text = std::fs::read_to_string(&config)?;
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let token = crate::support::token()?;
    let out = dir.path().join("report");
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "trace", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let summary = ferrofed_server::conformance::run::run(
        &settings,
        RunOptions {
            gateway: None,
            token: SecretString::from(token.clone()),
            patient: SyntheticPatient::new(
                "urn:oid:2.999.1.1",
                SecretString::from("ffd-test-0038".to_owned()),
            )?,
            seed_data: DEMO_DATA.into(),
            out: out.clone(),
            node_profile: false,
        },
    )
    .await?;
    drop(guard);

    let described = summary
        .report
        .row("gateway", "CP-23")
        .ok_or("CP-23 is reported")?;
    assert_eq!(
        1, described.passed,
        "the self-description answered the run's token, so the token was used"
    );
    for file in ["report.md", "report.tsv", "node-profile.tsv", "written.tsv"] {
        let written = std::fs::read_to_string(out.join(file))?;
        assert!(!written.contains(&token), "{file} never carries the token");
    }
    let logged = logs.text();
    assert!(
        !logged.is_empty(),
        "the in-process gateway logged its requests"
    );
    assert!(!logged.contains(&token), "no log line carries the token");
    assert!(
        !format!("{summary:?}").contains(&token),
        "nor does the summary's Debug"
    );
    for node in [&a, &b] {
        for request in node.received_requests().await.ok_or("recording is on")? {
            assert!(
                request.headers.values().all(|value| !value
                    .as_bytes()
                    .windows(token.len())
                    .any(|w| w == token.as_bytes())),
                "the caller's token never reaches a node (§13.1): {} {}",
                request.method,
                request.url.path()
            );
            assert!(
                request.method == http::Method::GET,
                "with no cross-reference the run seeds nothing, and a node is only read: {} {}",
                request.method,
                request.url.path()
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_node_profile_reports_each_point_against_every_member() -> TestResult {
    let (a, b) = (Server::start().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let config = deployment(dir.path(), &a, &b, "profile = \"development\"")?;
    let text = std::fs::read_to_string(&config)?;
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    let summary = ferrofed_server::conformance::run::run(
        &settings,
        RunOptions {
            gateway: None,
            token: SecretString::from(crate::support::token()?),
            patient: SyntheticPatient::new(
                "urn:oid:2.999.1.1",
                SecretString::from("ffd-test-0038".to_owned()),
            )?,
            seed_data: DEMO_DATA.into(),
            out: dir.path().join("report"),
            node_profile: true,
        },
    )
    .await?;

    let findings = summary.report.findings_tsv();
    for node in ["node node-a", "node node-b"] {
        for point in ["CP-18", "CP-19", "CP-27", "CP-33a"] {
            assert!(
                findings
                    .lines()
                    .any(|line| line.starts_with(&format!("{node}\t{point}\t"))),
                "{point} is reported against {node}: {findings}"
            );
        }
    }
    for line in findings.lines().filter(|line| line.contains("\tCP-19\t")) {
        assert!(
            line.contains("\tnot-observable\t") && line.contains("arranges no consent refusal"),
            "a run arranges no consent refusal, and says so: {line}"
        );
    }
    for point in ["CP-18", "CP-19", "CP-27"] {
        let row = summary
            .report
            .row("node", point)
            .ok_or("the point is reported")?;
        assert!(
            row.result.starts_with("node-"),
            "{point} reads from the findings, never as a gateway pass: {row:?}"
        );
    }
    for node in [&a, &b] {
        let requests = node.received_requests().await.ok_or("recording is on")?;
        assert!(
            requests
                .iter()
                .any(|request| request.url.path() == "/v1/query/aql"),
            "the checks reached the member through its node client"
        );
        for request in &requests {
            let seen = format!(
                "{} {:?} {}",
                request.url,
                request.headers,
                String::from_utf8_lossy(&request.body)
            );
            assert!(
                !seen.contains("ffd-test-0038"),
                "no check carries the run's patient (N33): {seen}"
            );
        }
    }
    Ok(())
}
