// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The binary is thin over the library, so a test drives the real run path,
//! and the boot refusals are checked on the real binary too.

use ferrofed_server::{EXIT_CONFIG, EXIT_USAGE};
use std::error::Error as StdError;
use std::io::Write;
use std::process::{Command, ExitCode, Output};

/// Runs the library entry point with `argv`.
fn run(argv: &[&str]) -> ExitCode {
    ferrofed_server::run(argv.iter().map(|word| (*word).to_owned()))
}

/// Renders `code` the way `ExitCode` renders itself, so two are comparable.
fn rendered(code: ExitCode) -> String {
    format!("{code:?}")
}

/// Runs the real binary with `args` and a configuration file holding `toml`.
fn binary(args: &[&str], toml: &str) -> Result<Output, Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(toml.as_bytes())?;
    let output = Command::new(env!("CARGO_BIN_EXE_ferrofed"))
        .args(args)
        .arg("--config")
        .arg(file.path())
        .env_remove("FERROFED_CONFIG")
        .output()?;
    Ok(output)
}

#[test]
fn no_job_at_all_is_a_usage_refusal() {
    assert_eq!(
        rendered(ExitCode::from(EXIT_USAGE)),
        rendered(run(&["ferrofed"])),
        "the binary does nothing without a job"
    );
}

#[test]
fn help_and_version_are_printed_and_exit_zero() {
    assert_eq!(
        rendered(ExitCode::SUCCESS),
        rendered(run(&["ferrofed", "--help"]))
    );
    assert_eq!(
        rendered(ExitCode::SUCCESS),
        rendered(run(&["ferrofed", "--version"]))
    );
}

#[test]
fn a_configuration_file_that_does_not_exist_exits_seventy_eight() {
    assert_eq!(
        rendered(ExitCode::from(EXIT_CONFIG)),
        rendered(run(&[
            "ferrofed",
            "serve",
            "--config",
            "/nonexistent/ferrofed.toml"
        ])),
        "a refused configuration is EX_CONFIG, never a generic failure"
    );
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn config_check_accepts_a_valid_file_and_binds_nothing() -> Result<(), Box<dyn StdError>> {
    let output = binary(
        &["config", "check"],
        "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"a\"]\nbearer_token = \"synthetic\"\n",
    )?;
    assert_eq!(Some(0), output.status.code());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("valid"), "{stdout}");
    assert!(
        !stdout.contains("synthetic"),
        "a secret is never printed: {stdout}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_binary_refuses_to_boot_on_an_unknown_key_and_on_a_doubly_set_secret()
-> Result<(), Box<dyn StdError>> {
    let mut secret = tempfile::NamedTempFile::new()?;
    secret.write_all(b"synthetic-from-file")?;
    let doubled = format!(
        "[credentials.\"a\"]\nbearer_token = \"synthetic-inline\"\nbearer_token_file = {:?}\n",
        secret.path()
    );
    for (toml, names) in [
        ("[server]\nlisten_port = 8080\n", "listen_port"),
        (doubled.as_str(), "credentials.a.bearer_token"),
    ] {
        for job in [&["serve"][..], &["config", "check"][..]] {
            let output = binary(job, toml)?;
            assert_eq!(
                Some(i32::from(EXIT_CONFIG)),
                output.status.code(),
                "{job:?} refuses {names}"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains(names), "{job:?} names {names}: {stderr}");
            assert!(
                !stderr.contains("synthetic-inline") && !stderr.contains("synthetic-from-file"),
                "the refusal quotes no secret: {stderr}"
            );
        }
    }
    Ok(())
}
