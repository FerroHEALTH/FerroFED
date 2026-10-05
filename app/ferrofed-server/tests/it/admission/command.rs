// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission command's refusals and its exit codes.

use ferrofed_server::{EXIT_CONFIG, EXIT_USAGE};
use ferrofed_testkit::unreachable;

use crate::facade::registry;
use crate::run::binary;

use super::{SYSTEM_A, SYSTEM_B, TestResult, UNREACHABLE_B, configuration};

#[test]
fn a_registry_with_a_shared_system_id_refuses_the_check() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    let shared =
        registry("http://127.0.0.1:9", "http://127.0.0.1:9", "").replace(SYSTEM_B, SYSTEM_A);
    std::fs::write(&document, shared)?;
    let output = binary(
        &["admission", "check", "--endpoint", "node-a-pub"],
        &configuration(&document, "", ""),
    )?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(SYSTEM_A),
        "the shared system_id is named: {stderr}"
    );
    assert!(output.stdout.is_empty(), "nothing was checked");
    Ok(())
}

#[tokio::test]
async fn the_binary_exits_one_on_a_failed_condition() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(unreachable::BASE, UNREACHABLE_B, ""))?;
    let text = configuration(&document, "", "");
    let output = tokio::task::spawn_blocking(move || {
        binary(&["admission", "check", "--endpoint", "node-a-pub"], &text)
            .map_err(|error| error.to_string())
    })
    .await??;
    assert_eq!(Some(1), output.status.code());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[fail] ehr_id generation"), "{stdout}");
    Ok(())
}

#[test]
fn an_endpoint_the_registry_does_not_hold_is_a_usage_refusal() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(unreachable::BASE, UNREACHABLE_B, ""))?;
    let output = binary(
        &["admission", "check", "--endpoint", "node-c-pub"],
        &configuration(&document, "", ""),
    )?;
    assert_eq!(Some(i32::from(EXIT_USAGE)), output.status.code());
    assert!(String::from_utf8_lossy(&output.stderr).contains("node-c-pub"));
    Ok(())
}

#[test]
fn a_configuration_without_a_registry_cannot_check_admission() -> TestResult {
    let output = binary(&["admission", "check", "--endpoint", "node-a-pub"], "")?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    Ok(())
}
