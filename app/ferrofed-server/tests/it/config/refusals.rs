// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals that name their key, line and file, and never echo a value.

use ferrofed_server::EXIT_CONFIG;
#[cfg(feature = "binding-ihe")]
use ferrofed_server::binding::ihe::pixm::PixmMethod;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrohealth_config::error::ConfigError;
#[cfg(feature = "binding-ihe")]
use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::error::server::ServerFault;

use super::{BROKEN_DEV_ROWS, env, everything, refusal, secret_file};
use crate::run::binary;

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unknown_key_refuses_to_boot_and_the_message_names_it() -> Result<(), Box<dyn StdError>> {
    for (text, key) in [
        ("[server]\nlisten_port = 8080\n", "server.listen_port"),
        (
            "[credentials.\"a\"]\ntoken = \"synthetic\"\n",
            "credentials.a.token",
        ),
        ("[unknown]\nkey = 1\n", "unknown"),
    ] {
        let error = refusal(text)?;
        assert!(matches!(error, Error::Load { .. }), "{error:?}");
        let message = crate::support::chain(&error);
        assert!(message.contains(key), "the refusal names {key}: {message}");
    }
    Ok(())
}

// NOTE: §5.4.3, a query value may carry the patient identifier, so a deployment cannot
// widen the logged parameters past the paging values.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_logged_query_parameter_refuses_to_boot_and_the_message_names_it()
-> Result<(), Box<dyn StdError>> {
    for list in ["[\"q\"]", "[\"offset\"]"] {
        let error = refusal(&format!("[telemetry]\nlogged_query_parameters = {list}\n"))?;
        assert!(
            matches!(error, Error::Server(ServerFault::LoggedQueryParameters)),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("telemetry.logged_query_parameters"),
            "the refusal names the key: {message}"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_key_set_through_the_environment_refuses_to_boot() {
    let outcome = Config::from_sources(None, &env("FERROFED__SERVER__LISTEN_PORT", "8080"));
    assert!(matches!(outcome, Err(Error::Load { .. })), "{outcome:?}");
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_bad_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten = \"not an address\"\n")?;
    assert_eq!(
        Some("server.listen"),
        crate::support::refused_key(&error),
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
    assert!(matches!(error, Error::Load { .. }), "{error:?}");
    let error = refusal("[telemetry]\nformat = \"xml\"\n")?;
    assert!(matches!(error, Error::Load { .. }), "{error:?}");
    let error = refusal("[telemetry]\nfilter = \"info,=,,\"\n")?;
    assert!(matches!(error, Error::Load { .. }), "{error:?}");
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
        assert!(matches!(error, Error::Load { .. }), "not a parse refusal");
        // NOTE: the messages never print the rendering, which would put the
        // identifier into the test log the test keeps it out of.
        let rendered = everything(&error);
        assert!(
            !rendered.contains("SENTINEL-7319"),
            "a parse refusal echoes the [dev] identifier"
        );
        assert!(
            matches!(
                &error,
                Error::Load { source, .. } if matches!(
                    source.as_ref(),
                    ConfigError::Syntax { position: Some((line, _)), .. } if *line >= 2
                )
            ),
            "the refusal locates the fault in the [dev] table, from its second line"
        );
    }
    Ok(())
}

/// FerroFED keeps no patient index (#1377): a configuration that still
/// sets the removed `[dev]` cross-reference is refused naming the table and
/// what replaces it, and never echoes a row's identifier.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_dev_table_is_refused_naming_the_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal(
        "profile = \"development\"\n\n[[dev.crossref]]\nnamespace = \"urn:oid:2.999.1\"\nvalue = \"SENTINEL-7320\"\nmember = \"node-a\"\nehr_id = \"7d44b88c-4199-4bad-97dc-d78268e01398\"\n",
    )?;
    assert!(matches!(error, Error::DevRemoved), "{error}");
    let shown = error.to_string();
    assert!(shown.contains("[dev]"), "{shown}");
    assert!(shown.contains("[[pixm.manager]]"), "{shown}");
    assert!(!everything(&error).contains("SENTINEL-7320"));
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_malformed_line_outside_dev_names_its_key_and_position() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten = \"127.0.0.1:8080\nshutdown_timeout_ms = 10\n")?;
    let message = crate::support::chain(&error);
    assert!(
        message.contains("line 2"),
        "the refusal names the line of server.listen: {message}"
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
    let message = crate::support::chain(&error);
    assert!(
        message.contains("server.listen_port")
            && message.contains("FERROFED__SERVER__LISTEN_PORT")
            && !message.contains("line "),
        "the refusal names the key, the variable that set it and no position: {message}"
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
    let Err(error) = Config::load(&crate::support::file_sources(file.path())) else {
        return Err("the configuration was accepted".into());
    };
    let message = crate::support::chain(&error);
    assert!(
        message.contains(&file.path().display().to_string()),
        "the refusal names the file: {message}"
    );
    Ok(())
}

/// A `[[pixm.manager]]` table asking with `method`, or the default when it
/// is empty.
#[cfg(feature = "binding-ihe")]
fn pix_manager(method: &str) -> String {
    format!("[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n{method}\n")
}

#[cfg(feature = "binding-ihe")]
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_pixm_method_other_than_get_or_post_refuses_to_boot_naming_its_key()
-> Result<(), Box<dyn StdError>> {
    for method in ["\"put\"", "\"GET\"", "\"\"", "1"] {
        let error = refusal(&pix_manager(&format!("method = {method}")))?;
        assert!(matches!(error, Error::Load { .. }), "{method}: {error:?}");
        let message = crate::support::chain(&error);
        assert!(
            message.contains("pixm.manager") && message.contains("method"),
            "the refusal names the key: {message}"
        );
    }
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_pixm_method_is_get_unless_post_is_named() -> Result<(), Box<dyn StdError>> {
    for (line, expected) in [
        ("", PixmMethod::Get),
        ("method = \"get\"", PixmMethod::Get),
        ("method = \"post\"", PixmMethod::Post),
    ] {
        let config = Config::from_sources(Some(&pix_manager(line)), &BTreeMap::new())?;
        let manager = config
            .pixm
            .as_ref()
            .and_then(|pixm| pixm.manager.first())
            .ok_or("one Manager")?;
        assert_eq!(expected, manager.method, "{line:?}");
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unknown_registry_format_is_refused() -> Result<(), Box<dyn StdError>> {
    let output = binary(
        &["config", "check"],
        "[registry]\ndocument = \"/nonexistent/registry.json\"\nformat = \"xml\"\n",
    )?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    Ok(())
}
