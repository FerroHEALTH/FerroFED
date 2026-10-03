// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals that name their key, line and file, and never echo a value.

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use std::error::Error as StdError;

use super::{BROKEN_DEV_ROWS, env, everything, refusal, secret_file};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unknown_key_refuses_to_boot_and_the_message_names_it() -> Result<(), Box<dyn StdError>> {
    for (text, key) in [
        ("[server]\nlisten_port = 8080\n", "server.listen_port"),
        (
            "[telemetry]\nlogged_query_parameters = [\"q\"]\n",
            "telemetry.logged_query_parameters",
        ),
        (
            "[credentials.\"a\"]\ntoken = \"synthetic\"\n",
            "credentials.\"a\".token",
        ),
        ("[unknown]\nkey = 1\n", "unknown"),
    ] {
        let error = refusal(text)?;
        assert!(matches!(error, Error::Parse { .. }), "{error:?}");
        let message = error.to_string();
        assert!(message.contains(key), "the refusal names {key}: {message}");
    }
    Ok(())
}

#[test]
fn an_unknown_key_set_through_the_environment_refuses_to_boot() {
    let outcome = Config::from_sources(None, &env("FERROFED__SERVER__LISTEN_PORT", "8080"));
    assert!(matches!(outcome, Err(Error::Parse { .. })), "{outcome:?}");
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_bad_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten = \"not an address\"\n")?;
    assert!(
        matches!(&error, Error::Listen { key, .. } if key == "server.listen"),
        "{error:?}"
    );
    for key in [
        "request_timeout_ms",
        "shutdown_timeout_ms",
        "body_limit_bytes",
    ] {
        let error = refusal(&format!("[server]\n{key} = 0\n"))?;
        assert!(
            matches!(&error, Error::Zero { key: named } if named == &format!("server.{key}")),
            "{error:?}"
        );
    }
    let error = refusal("[server]\nrequest_timeout_ms = \"thirty\"\n")?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let error = refusal("[telemetry]\nformat = \"xml\"\n")?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let error = refusal("[telemetry]\nfilter = \"info,=,,\"\n")?;
    assert!(matches!(error, Error::Filter { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_malformed_dev_row_never_echoes_its_identifier() -> Result<(), Box<dyn StdError>> {
    for text in BROKEN_DEV_ROWS {
        let error = refusal(text)?;
        assert!(matches!(error, Error::Parse { .. }), "not a parse refusal");
        // NOTE: the messages never print the rendering, which would put the
        // identifier into the test log the test keeps it out of.
        let rendered = everything(&error);
        assert!(
            !rendered.contains("SENTINEL-7319"),
            "a parse refusal echoes the [dev] identifier"
        );
        assert!(
            error.to_string().contains("at dev"),
            "the refusal locates the fault in the [dev] table"
        );
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_malformed_line_outside_dev_names_its_key_and_position() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten = \"127.0.0.1:8080\nshutdown_timeout_ms = 10\n")?;
    let message = error.to_string();
    assert!(
        message.contains("at server.listen") && message.contains("line 2"),
        "the refusal names server.listen on line 2: {message}"
    );
    assert!(
        !message.contains("127.0.0.1:8080"),
        "the refusal quotes the source line"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_environment_fault_names_its_key_and_no_line() -> Result<(), Box<dyn StdError>> {
    let Err(error) = Config::from_sources(None, &env("FERROFED__SERVER__LISTEN_PORT", "8080"))
    else {
        return Err("the override was accepted".into());
    };
    let message = error.to_string();
    assert!(
        message.contains("after the environment overrides")
            && message.contains("at server.listen_port")
            && !message.contains("line "),
        "the refusal names the key and no position: {message}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_parse_fault_names_the_file_it_is_in() -> Result<(), Box<dyn StdError>> {
    let file = secret_file("[server]\nlisten_port = 8080\n")?;
    let Err(error) = Config::load(Some(file.path())) else {
        return Err("the configuration was accepted".into());
    };
    let message = error.to_string();
    assert!(
        message.contains(&file.path().display().to_string()),
        "the refusal names the file: {message}"
    );
    Ok(())
}
