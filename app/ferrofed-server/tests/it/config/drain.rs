// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The drain delay and the drain: the drain defaults to the request timeout,
//! and one shorter is refused. No specification governs the process model:
//! our own design.

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::time::Duration;

use super::{env, refusal};

/// Resolves `text` with no environment.
fn resolved(
    text: &str,
) -> Result<ferrofed_server::config::settings::ServerSettings, Box<dyn StdError>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()?
        .server)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unset_drain_takes_the_request_timeout_and_the_delay_is_zero() -> Result<(), Box<dyn StdError>>
{
    let server = resolved("[server]\nrequest_timeout_ms = 45000\n")?;
    assert_eq!(Duration::from_secs(45), server.shutdown_timeout);
    assert_eq!(Duration::ZERO, server.drain_delay);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_drain_shorter_than_the_request_timeout_refuses_to_boot_naming_both_keys()
-> Result<(), Box<dyn StdError>> {
    for shutdown_ms in [1, 10_000, 29_999] {
        let error = refusal(&format!(
            "[server]\nrequest_timeout_ms = 30000\nshutdown_timeout_ms = {shutdown_ms}\n"
        ))?;
        assert!(
            matches!(
                error,
                Error::Drain {
                    shutdown_ms: refused,
                    request_ms: 30_000,
                } if refused == shutdown_ms
            ),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("server.shutdown_timeout_ms")
                && message.contains("server.request_timeout_ms"),
            "the refusal names both keys: {message}"
        );
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_drain_as_long_as_the_request_timeout_or_longer_is_accepted() -> Result<(), Box<dyn StdError>> {
    for shutdown_ms in [30_000_u64, 45_000] {
        let server = resolved(&format!(
            "[server]\nrequest_timeout_ms = 30000\nshutdown_timeout_ms = {shutdown_ms}\n"
        ))?;
        assert_eq!(Duration::from_millis(shutdown_ms), server.shutdown_timeout);
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_drain_delay_is_read_from_the_file_and_the_environment() -> Result<(), Box<dyn StdError>> {
    let server = resolved("[server]\ndrain_delay_ms = 5000\n")?;
    assert_eq!(Duration::from_secs(5), server.drain_delay);
    let server = Config::from_sources(
        Some("[server]\ndrain_delay_ms = 5000\n"),
        &env("FERROFED__SERVER__DRAIN_DELAY_MS", "7000"),
    )?
    .resolve()?
    .server;
    assert_eq!(Duration::from_secs(7), server.drain_delay);
    let error = refusal("[server]\ndrain_delay_ms = \"five seconds\"\n")?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    Ok(())
}
