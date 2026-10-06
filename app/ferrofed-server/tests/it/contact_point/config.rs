// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth.issuer.national_contact_point]` as written: it declares the issuer
//! a national contact point and names every claim the Annex attributes IHE
//! IUA has no claim for are read from; a claim left unnamed, or a
//! correlation header that is no field name or carries a credential,
//! refuses to start, naming its key.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;

use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthFault;
use ferrofed_server::config::error::Error as ConfigError;

/// One trusted issuer declared a national contact point, its table
/// `table`.
fn declaring(table: &str) -> String {
    format!(
        "[auth]\naudience = \"urn:example:gateway\"\n\n[[auth.issuer]]\nissuer = \"https://ncp.example.test\"\njwks_uri = \"https://ncp.example.test/jwks\"\n\n[auth.issuer.national_contact_point]\n{table}"
    )
}

/// Every claim name of the table, with the claim it names.
const NAMES: [(&str, &str); 6] = [
    ("family_name", "f"),
    ("given_name", "g"),
    ("country_code", "c"),
    ("professional_issuing_authority", "pa"),
    ("provider_issuing_authority", "ha"),
    ("provider_address", "addr"),
];

/// Every claim name set but `skipped`, and `extra`.
fn naming(skipped: Option<&str>, extra: &str) -> String {
    let mut lines = String::new();
    for (name, claim) in NAMES {
        if Some(name) != skipped {
            writeln!(lines, "{name} = \"{claim}\"").unwrap_or_default();
        }
    }
    declaring(&format!("{lines}{extra}"))
}

/// Every claim name set, and `extra`.
fn complete(extra: &str) -> String {
    naming(None, extra)
}

/// The refusal of `text`, or `None` when it resolves.
fn refusal(text: &str) -> Result<Option<ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err())
}

#[test]
fn a_complete_table_declares_the_issuer_a_contact_point() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(
        Some(&complete("correlation_header = \"Ncp-Correlation-Id\"\n")),
        &BTreeMap::new(),
    )?
    .resolve()?;
    let [issuer] = settings.server.auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    let declared = issuer
        .national_contact_point
        .as_ref()
        .ok_or("declared a contact point")?;
    assert_eq!("f", declared.claims.family_name);
    assert_eq!(
        Some("ncp-correlation-id"),
        declared
            .correlation_header
            .as_ref()
            .map(http::HeaderName::as_str)
    );
    Ok(())
}

#[test]
fn an_issuer_without_the_table_is_no_contact_point() -> Result<(), Box<dyn Error>> {
    let text = "[auth]\naudience = \"urn:example:gateway\"\n\n[[auth.issuer]]\nissuer = \"https://idp.example.test\"\njwks_uri = \"https://idp.example.test/jwks\"\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let [issuer] = settings.server.auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    assert!(issuer.national_contact_point.is_none());
    Ok(())
}

#[test]
fn every_claim_name_is_required_and_a_missing_one_is_named() -> Result<(), Box<dyn Error>> {
    for (missing, _) in NAMES {
        match refusal(&naming(Some(missing), ""))? {
            Some(ConfigError::Missing { key }) => assert_eq!(
                format!("auth.issuer[0].national_contact_point.{missing}"),
                key
            ),
            other => return Err(format!("{missing}: {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_correlation_header_that_carries_a_credential_is_refused() -> Result<(), Box<dyn Error>> {
    for header in ["Authorization", "cookie", "DPoP", "proxy-authorization"] {
        match refusal(&complete(&format!("correlation_header = \"{header}\"\n")))? {
            Some(ConfigError::Auth { key, fault }) => assert_eq!(
                (
                    "auth.issuer[0].national_contact_point.correlation_header",
                    AuthFault::CorrelationCredential
                ),
                (key.as_str(), fault),
                "{header}"
            ),
            other => return Err(format!("{header}: {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn a_correlation_header_that_is_no_field_name_is_refused() -> Result<(), Box<dyn Error>> {
    match refusal(&complete("correlation_header = \"not a header\"\n"))? {
        Some(ConfigError::Auth { key, fault }) => {
            assert_eq!(
                (
                    "auth.issuer[0].national_contact_point.correlation_header",
                    AuthFault::HeaderName
                ),
                (key.as_str(), fault)
            );
            Ok(())
        }
        other => Err(format!("{other:?}").into()),
    }
}

#[test]
fn an_unknown_key_in_the_table_is_refused() -> Result<(), Box<dyn Error>> {
    let outcome = Config::from_sources(Some(&complete("disclose = true\n")), &BTreeMap::new());
    let Err(error) = outcome else {
        return Err("the configuration was accepted".into());
    };
    let message = error.to_string();
    assert!(
        message.contains("national_contact_point.disclose"),
        "the refusal names the key: {message}"
    );
    Ok(())
}
