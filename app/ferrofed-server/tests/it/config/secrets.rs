// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials sections and their `_file` secrets, never echoed by a refusal.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::{BasicFault, Error};
use ferrofed_server::config::settings::Scheme;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use std::collections::BTreeMap;
use std::error::Error as StdError;

use super::{FULL, env, everything, refusal, secret_file};

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_resolved_secret_never_reaches_debug_output() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    let rendered = format!("{settings:?}");
    // NOTE: a failure message never echoes the rendering, which would put the
    // leaked secret into the test log it is meant to keep it out of.
    assert!(
        !rendered.contains("synthetic-token"),
        "Debug output carries the bearer token"
    );
    assert!(
        !rendered.contains("synthetic-password"),
        "Debug output carries the basic password"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_environment_override_adds_a_credentials_section_with_no_file() -> Result<(), Box<dyn StdError>>
{
    let settings = Config::from_sources(
        None,
        &env("FERROFED__CREDENTIALS__NODE_A__BEARER_TOKEN", "synthetic"),
    )?
    .resolve()?;
    assert!(
        matches!(
            settings.credentials.get(&EndpointId::new("node_a")?),
            Some(Scheme::Bearer(_))
        ),
        "node_a should resolve to a bearer scheme"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_is_read_from_its_file_sibling_and_trimmed() -> Result<(), Box<dyn StdError>> {
    let file = secret_file("synthetic-from-file\n")?;
    let text = format!(
        "[credentials.\"hospital-a\"]\nbearer_token_file = {:?}\n",
        file.path()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    match settings.credentials.get(&EndpointId::new("hospital-a")?) {
        Some(Scheme::Bearer(token)) => assert_eq!("synthetic-from-file", token.expose()),
        other => return Err(format!("a bearer scheme: {other:?}").into()),
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_set_inline_and_through_its_file_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let file = secret_file("synthetic-from-file")?;
    let text = format!(
        "[credentials.\"hospital-a\"]\nbearer_token = \"synthetic\"\nbearer_token_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Conflict { key } if key == "credentials.hospital-a.bearer_token"),
        "{error:?}"
    );
    let file = secret_file("synthetic-password")?;
    let text = format!(
        "[credentials.\"clinic-b\"]\nuser = \"gateway\"\npassword = \"synthetic\"\npassword_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Conflict { key } if key == "credentials.clinic-b.password"),
        "{error:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_credentials_section_must_name_exactly_one_complete_scheme() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[credentials.\"a\"]\nbearer_token = \"t\"\nuser = \"u\"\n")?;
    assert!(matches!(error, Error::Scheme { .. }), "{error:?}");
    let error = refusal("[credentials.\"a\"]\nuser = \"u\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.password"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\npassword = \"p\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.user"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\n")?;
    assert!(matches!(error, Error::NoScheme { .. }), "{error:?}");
    let error = refusal("[credentials.\"node a\"]\nbearer_token = \"t\"\n")?;
    assert!(matches!(error, Error::EndpointId { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_file_that_cannot_be_read_or_is_empty_refuses_to_boot() -> Result<(), Box<dyn StdError>>
{
    let error =
        refusal("[credentials.\"a\"]\nbearer_token_file = \"/nonexistent/ferrofed/token\"\n")?;
    assert!(
        matches!(&error, Error::Secret { key, .. } if key == "credentials.a.bearer_token_file"),
        "{error:?}"
    );
    let file = secret_file("  \n")?;
    let error = refusal(&format!(
        "[credentials.\"a\"]\nbearer_token_file = {:?}\n",
        file.path()
    ))?;
    assert!(matches!(error, Error::EmptySecret { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_refusal_never_quotes_a_secret() -> Result<(), Box<dyn StdError>> {
    let error =
        refusal("[credentials.\"a\"]\nbearer_token = \"synthetic-secret\"\nuser = \"u\"\n")?;
    let mut rendered = error.to_string();
    let mut cause = std::error::Error::source(&error);
    while let Some(source) = cause {
        rendered.push_str(&source.to_string());
        cause = source.source();
    }
    assert!(!rendered.contains("synthetic-secret"), "{rendered}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_of_the_wrong_type_is_never_echoed() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[credentials.\"a\"]\nbearer_token = 73194426\n")?;
    assert!(matches!(error, Error::Parse { .. }), "not a parse refusal");
    assert!(
        !everything(&error).contains("73194426"),
        "the refusal echoes the inline secret"
    );
    assert!(
        error.to_string().contains("credentials.\"a\".bearer_token"),
        "the refusal names the key"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_credential_the_authorization_header_cannot_carry_is_refused_by_its_key()
-> Result<(), Box<dyn StdError>> {
    let error = refusal("[credentials.\"a\"]\nbearer_token = \"Qz7left\\rQz7right\"\n")?;
    assert!(
        matches!(&error, Error::Authorization { key, .. } if key == "credentials.a.bearer_token"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");

    let error = refusal(
        "[credentials.\"a\"]\nuser = \"gateway\"\npassword = \"Qz7left\\u0000Qz7right\"\n",
    )?;
    assert!(
        matches!(&error, Error::Basic {
                key,
                fault: BasicFault::ControlCharacter,
                source: InvalidCredentials::ControlCharacter(BasicPart::Password),
            } if key == "credentials.a.password"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");

    let manager = "[[pixm.manager]]\nurl = \"http://127.0.0.1:9\"\n\
                   [pixm.manager.credentials]\nbearer_token = \"Qz7left\\nQz7right\"\n";
    let error = refusal(manager)?;
    assert!(
        matches!(&error, Error::Authorization { key, .. }
            if key == "pixm.manager[0].credentials.bearer_token"),
        "a PIX Manager credential is held to the same rule: {error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");
    Ok(())
}

/// A bearer token is the `b64token` of RFC 6750 §2.1, so a printable token
/// outside that set is refused as the node client would refuse it.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_bearer_token_that_is_no_b64token_is_refused_by_its_key() -> Result<(), Box<dyn StdError>> {
    for token in [
        "Qz7left Qz7right",
        "Qz7left\\\"Qz7right",
        "Qz7left=Qz7right",
        "=",
    ] {
        let error = refusal(&format!(
            "[credentials.\"a\"]\nbearer_token = \"{token}\"\n"
        ))?;
        assert!(
            matches!(&error, Error::Authorization {
                    key,
                    source: InvalidCredentials::NotB64Token,
                } if key == "credentials.a.bearer_token"),
            "{token}: {error:?}"
        );
        assert!(!everything(&error).contains("Qz7"), "{error}");
    }
    let accepted = Config::from_sources(
        Some("[credentials.\"a\"]\nbearer_token = \"Az-._~+/9==\"\n"),
        &BTreeMap::new(),
    )?
    .resolve()?;
    assert!(
        matches!(
            accepted.credentials.get(&EndpointId::new("a")?),
            Some(Scheme::Bearer(_))
        ),
        "every b64token character and its padding is accepted"
    );
    Ok(())
}

/// A basic user-id carries no colon (RFC 7617 §2), named by the user key.
#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_basic_user_holding_a_colon_is_refused_by_its_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[credentials.\"a\"]\nuser = \"gate:way\"\npassword = \"Qz7left\"\n")?;
    assert!(
        matches!(&error, Error::Basic {
                key,
                fault: BasicFault::Colon,
                source: InvalidCredentials::ColonInUserId,
            } if key == "credentials.a.user"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");
    Ok(())
}
