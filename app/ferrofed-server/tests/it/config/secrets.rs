// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials sections and their `_file` secrets, never echoed by a refusal.

use ferrofed_registry::id::EndpointId;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::basic::BasicFault;
use ferrofed_server::config::error::credentials::CredentialsFault;
use ferrofed_server::config::settings::Scheme;
use ferrohealth_secret::file::SecretError;
use openehr_its::rest::client::{BasicPart, InvalidCredentials};
use std::collections::BTreeMap;
use std::error::Error as StdError;

use super::{env, everything, full, refusal, secret_file};

/// An `oauth` table beside a credentials section `[credentials."a"]`.
const OAUTH: &str = "[credentials.\"a\".credentials.oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-client-secret\"\n";

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_resolved_secret_never_reaches_debug_output() -> Result<(), Box<dyn StdError>> {
    let full = full();
    let settings = Config::from_sources(Some(full.as_str()), &BTreeMap::new())?.resolve()?;
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
    let mut environment = env(
        "FERROFED__CREDENTIALS__NODE_A__CREDENTIALS__USER",
        "gateway",
    );
    environment.extend(env(
        "FERROFED__CREDENTIALS__NODE_A__CREDENTIALS__PASSWORD",
        "synthetic",
    ));
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
        "[credentials.\"hospital-a\"]\ncredentials.user = \"gateway\"\ncredentials.password_file = {:?}\n",
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
        "[credentials.\"hospital-a\".credentials.oauth]\ntoken_endpoint = \"https://as.example.org/token\"\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic\"\nclient_secret_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Smart { section, .. } if section == "credentials.hospital-a.credentials.oauth"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("synthetic"), "{error}");
    let file = secret_file("synthetic-password")?;
    let text = format!(
        "[credentials.\"clinic-b\"]\ncredentials.user = \"gateway\"\ncredentials.password = \"synthetic\"\ncredentials.password_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::SecretFile(SecretError::Both { key }) if key == "credentials.clinic-b.credentials.password"),
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
        "[credentials.\"a\"]\ncredentials.user = \"u\"\ncredentials.password = \"p\"\n{OAUTH}"
    ))?;
    assert!(
        matches!(error, Error::Credentials(CredentialsFault::Scheme { .. })),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\ncredentials.user = \"u\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.credentials.password"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\ncredentials.password = \"p\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.credentials.user"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\n")?;
    assert!(
        matches!(error, Error::Credentials(CredentialsFault::NoScheme { .. })),
        "{error:?}"
    );
    let error = refusal(
        "[credentials.\"node a\"]\ncredentials.user = \"u\"\ncredentials.password = \"t\"\n",
    )?;
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
        "[credentials.\"a\"]\ncredentials.user = \"u\"\ncredentials.password_file = \"/nonexistent/ferrofed/password\"\n",
    )?;
    assert!(
        matches!(&error, Error::SecretFile(SecretError::Read { key, .. }) if key == "credentials.a.credentials.password"),
        "{error:?}"
    );
    let file = secret_file("\n")?;
    let error = refusal(&format!(
        "[credentials.\"a\"]\ncredentials.user = \"u\"\ncredentials.password_file = {:?}\n",
        file.path()
    ))?;
    assert!(
        matches!(error, Error::SecretFile(SecretError::Empty { .. })),
        "{error:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_refusal_never_quotes_a_secret() -> Result<(), Box<dyn StdError>> {
    let error = refusal(&format!(
        "[credentials.\"a\"]\ncredentials.user = \"u\"\ncredentials.password = \"synthetic-secret\"\n{OAUTH}"
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
    let error = refusal(
        "[credentials.\"a\"]\ncredentials.user = \"u\"\ncredentials.password = 73194426\n",
    )?;
    assert!(matches!(error, Error::Load { .. }), "not a parse refusal");
    assert!(
        !everything(&error).contains("73194426"),
        "the refusal echoes the inline secret"
    );
    assert_eq!(
        Some("credentials.a.credentials.password"),
        crate::support::refused_key(&error),
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
        "[credentials.\"a\"]\ncredentials.user = \"gateway\"\ncredentials.password = \"Qz7left\\u0000Qz7right\"\n",
    )?;
    assert!(
        matches!(&error, Error::Credentials(CredentialsFault::Basic {
                key,
                fault: BasicFault::ControlCharacter,
                source: InvalidCredentials::ControlCharacter(BasicPart::Password),
            }) if key == "credentials.a.credentials.password"),
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
    let error = refusal(
        "[credentials.\"a\"]\ncredentials.user = \"gate:way\"\ncredentials.password = \"Qz7left\"\n",
    )?;
    assert!(
        matches!(&error, Error::Credentials(CredentialsFault::Basic {
                key,
                fault: BasicFault::Colon,
                source: InvalidCredentials::ColonInUserId,
            }) if key == "credentials.a.credentials.user"),
        "{error:?}"
    );
    assert!(!everything(&error).contains("Qz7"), "{error}");
    Ok(())
}
