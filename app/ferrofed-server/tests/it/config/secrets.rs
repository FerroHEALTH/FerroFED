// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials sections and their `_file` secrets, never echoed by a refusal.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::basic::BasicFault;
use ferrofed_server::config::error::credentials::CredentialsFault;
use ferrofed_server::config::settings::Scheme;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use std::collections::BTreeMap;
use std::error::Error as StdError;

use super::{FULL, env, everything, refusal, secret_file};

/// An `oauth` table beside a credentials section `[credentials."a"]`.
const OAUTH: &str = "[credentials.\"a\".oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-client-secret\"\n";

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
        "Debug output carries the client secret"
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
    let mut environment = env("FERROFED__CREDENTIALS__NODE_A__USER", "gateway");
    environment.extend(env("FERROFED__CREDENTIALS__NODE_A__PASSWORD", "synthetic"));
    let settings = Config::from_sources(None, &environment)?.resolve()?;
    assert!(
        matches!(
            settings.credentials.get(&EndpointId::new("node_a")?),
            Some(Scheme::Basic { .. })
        ),
        "node_a should resolve to a basic scheme"
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
        "[credentials.\"hospital-a\"]\nuser = \"gateway\"\npassword_file = {:?}\n",
        file.path()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    match settings.credentials.get(&EndpointId::new("hospital-a")?) {
        Some(Scheme::Basic { password, .. }) => {
            assert_eq!("synthetic-from-file", password.expose());
        }
        other => return Err(format!("a basic scheme: {other:?}").into()),
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
        "[credentials.\"hospital-a\".oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic\"\nclient_secret_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Smart { section, .. } if section == "credentials.hospital-a.oauth"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("synthetic"), "{error}");
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
    let error = refusal(&format!(
        "[credentials.\"a\"]\nuser = \"u\"\npassword = \"p\"\n{OAUTH}"
    ))?;
    assert!(
        matches!(error, Error::Credentials(CredentialsFault::Scheme { .. })),
        "{error:?}"
    );
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
    assert!(
        matches!(error, Error::Credentials(CredentialsFault::NoScheme { .. })),
        "{error:?}"
    );
    let error = refusal("[credentials.\"node a\"]\nuser = \"u\"\npassword = \"t\"\n")?;
    assert!(
        matches!(
            error,
            Error::Credentials(CredentialsFault::EndpointId { .. })
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_file_that_cannot_be_read_or_is_empty_refuses_to_boot() -> Result<(), Box<dyn StdError>>
{
    let error = refusal(
        "[credentials.\"a\"]\nuser = \"u\"\npassword_file = \"/nonexistent/ferrofed/password\"\n",
    )?;
    assert!(
        matches!(&error, Error::Secret { key, .. } if key == "credentials.a.password_file"),
        "{error:?}"
    );
    let file = secret_file("  \n")?;
    let error = refusal(&format!(
        "[credentials.\"a\"]\nuser = \"u\"\npassword_file = {:?}\n",
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
    let error = refusal(&format!(
        "[credentials.\"a\"]\nuser = \"u\"\npassword = \"synthetic-secret\"\n{OAUTH}"
    ))?;
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
    let error = refusal("[credentials.\"a\"]\nuser = \"u\"\npassword = 73194426\n")?;
    assert!(matches!(error, Error::Parse { .. }), "not a parse refusal");
    assert!(
        !everything(&error).contains("73194426"),
        "the refusal echoes the inline secret"
    );
    assert!(
        error.to_string().contains("credentials.\"a\".password"),
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
    let error = refusal(
        "[credentials.\"a\"]\nuser = \"gateway\"\npassword = \"Qz7left\\u0000Qz7right\"\n",
    )?;
    assert!(
        matches!(&error, Error::Credentials(CredentialsFault::Basic {
                key,
                fault: BasicFault::ControlCharacter,
                source: InvalidCredentials::ControlCharacter(BasicPart::Password),
            }) if key == "credentials.a.password"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");

    #[cfg(feature = "binding-ihe")]
    {
        let manager = "[[pixm.manager]]\nurl = \"http://127.0.0.1:9\"\n\
                       [pixm.manager.credentials]\nuser = \"gateway\"\npassword = \"Qz7left\\nQz7right\"\n";
        let error = refusal(manager)?;
        assert!(
            matches!(&error, Error::Credentials(CredentialsFault::Basic { key, .. })
                if key == "pixm.manager[0].credentials.password"),
            "a PIX Manager credential is held to the same rule: {error:?}"
        );
        assert!(!everything(&error).contains("Qz7"), "{error}");
    }
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
        matches!(&error, Error::Credentials(CredentialsFault::Basic {
                key,
                fault: BasicFault::Colon,
                source: InvalidCredentials::ColonInUserId,
            }) if key == "credentials.a.user"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");
    Ok(())
}
