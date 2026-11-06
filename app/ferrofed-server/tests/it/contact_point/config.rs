// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A contact point as written: an `[[auth.issuer]]` of kind `federation`
//! whose `[[auth.federation]]` table sets `contact_point = true`, with its
//! `[[federation.contact_point]]`. A table naming no contact point, a
//! contact point without its table, a country neither a Member State nor
//! added, a correlation header that is no field name or carries a
//! credential, and the removed `[trust.issuer.national_contact_point]` each
//! refuse to start, naming the key.

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthFault;
use ferrofed_server::config::error::Error as ConfigError;

/// The contact point's issuer.
const NCP: &str = "https://ncp.example.test";

/// The contact point's `[[auth.issuer]]` and `[[auth.federation]]`, its
/// `countries` `countries`, with `federation` added to the latter, beside
/// the FerroFED store a contact point needs.
fn issuer(countries: &str, federation: &str) -> String {
    format!(
        "{}\n[auth]\n\n[[auth.issuer]]\nissuer = \"{NCP}\"\nkind = \"federation\"\naudience = [\"urn:example:gateway\"]\nsender_constraint = \"required\"\n\n[[auth.federation]]\nissuer = \"{NCP}\"\ncontact_point = true\nmapping = \"ehdsi\"\ncountries = [{countries}]\nfields.\"professional.country_code\" = [{{ constant = \"LU\" }}]\nfields.\"professional.issuing_authority_name\" = [{{ constant = \"hp-authority\" }}]\n{federation}",
        crate::support::store_table()
    )
}

/// The contact point of [`issuer`] in `LU` with the provider's issuing
/// authority configured, and its `[[federation.contact_point]]` with
/// `extra`.
fn declaring(extra: &str) -> String {
    format!(
        "{}\n[[federation.contact_point]]\nissuer = \"{NCP}\"\n{extra}",
        issuer(
            "\"LU\"",
            "fields.\"provider.issuing_authority_name\" = [{ constant = \"hcp-authority\" }]\n"
        )
    )
}

/// The refusal of `text`, or `None` when it resolves.
fn refusal(text: &str) -> Result<Option<ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?
        .resolve()
        .err())
}

/// The `Error::Auth` of `text`, as its key and fault.
fn fault(text: &str) -> Result<(String, AuthFault), Box<dyn Error>> {
    match refusal(text)? {
        Some(ConfigError::Auth { key, fault }) => Ok((key, fault)),
        other => Err(format!("{other:?}").into()),
    }
}

#[test]
fn a_contact_point_with_its_table_is_declared() -> Result<(), Box<dyn Error>> {
    let settings = Config::from_sources(
        Some(&declaring("correlation_header = \"Ncp-Correlation-Id\"\n")),
        &BTreeMap::new(),
    )?
    .resolve()?;
    let [issuer] = settings.server.auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    assert!(issuer.verification.is_contact_point());
    let declared = issuer
        .national_contact_point
        .as_ref()
        .ok_or("declared a contact point")?;
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
fn an_issuer_that_is_no_federation_issuer_is_no_contact_point() -> Result<(), Box<dyn Error>> {
    let text = "[auth]\n\n[[auth.issuer]]\nissuer = \"https://idp.example.test\"\nkind = \"persons\"\naudience = [\"urn:example:gateway\"]\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    let [issuer] = settings.server.auth.issuers.as_slice() else {
        return Err("one issuer".into());
    };
    assert!(issuer.national_contact_point.is_none());
    Ok(())
}

#[test]
fn a_contact_point_without_its_table_is_refused() -> Result<(), Box<dyn Error>> {
    let text = issuer(
        "\"LU\"",
        "fields.\"provider.issuing_authority_name\" = [{ constant = \"hcp-authority\" }]\n",
    );
    assert_eq!(
        (
            String::from("auth.issuer[0]"),
            AuthFault::ContactPointUndeclared
        ),
        fault(&text)?
    );
    Ok(())
}

#[test]
fn a_table_naming_no_contact_point_is_refused() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}\n[[federation.contact_point]]\nissuer = \"https://idp.example.test\"\n",
        declaring("")
    );
    assert_eq!(
        (
            String::from("federation.contact_point[1].issuer"),
            AuthFault::NotContactPoint
        ),
        fault(&text)?
    );
    let twice = format!(
        "{}\n[[federation.contact_point]]\nissuer = \"{NCP}\"\n",
        declaring("")
    );
    assert_eq!(
        (
            String::from("federation.contact_point[1].issuer"),
            AuthFault::DuplicateIssuer
        ),
        fault(&twice)?
    );
    Ok(())
}

#[test]
fn a_contact_point_with_no_provider_issuing_authority_is_refused() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}\n[[federation.contact_point]]\nissuer = \"{NCP}\"\n",
        issuer("\"LU\"", "")
    );
    match refusal(&text)? {
        Some(ConfigError::Missing { key }) => {
            assert_eq!(
                "auth.federation.fields.\"provider.issuing_authority_name\"",
                key
            );
            Ok(())
        }
        other => Err(format!("{other:?}").into()),
    }
}

#[test]
fn the_removed_trust_table_is_refused_naming_its_successor() -> Result<(), Box<dyn Error>> {
    let text = format!(
        "{}\n[[trust.issuer]]\nissuer = \"{NCP}\"\n\n[trust.issuer.national_contact_point]\nfamily_name = \"f\"\n",
        declaring("")
    );
    let (key, refused) = fault(&text)?;
    assert_eq!(
        (
            "trust.issuer[0].national_contact_point",
            AuthFault::ContactPointMoved
        ),
        (key.as_str(), refused)
    );
    assert!(refused.to_string().contains("[[federation.contact_point]]"));
    Ok(())
}

#[test]
fn a_correlation_header_that_carries_a_credential_is_refused() -> Result<(), Box<dyn Error>> {
    for header in ["Authorization", "cookie", "DPoP", "proxy-authorization"] {
        assert_eq!(
            (
                String::from("federation.contact_point[0].correlation_header"),
                AuthFault::CorrelationCredential
            ),
            fault(&declaring(&format!("correlation_header = \"{header}\"\n")))?,
            "{header}"
        );
    }
    Ok(())
}

#[test]
fn a_correlation_header_that_is_no_field_name_is_refused() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        (
            String::from("federation.contact_point[0].correlation_header"),
            AuthFault::HeaderName
        ),
        fault(&declaring("correlation_header = \"not a header\"\n"))?
    );
    Ok(())
}

#[test]
fn an_unknown_key_in_the_table_is_refused() -> Result<(), Box<dyn Error>> {
    let outcome = Config::from_sources(Some(&declaring("disclose = true\n")), &BTreeMap::new());
    let Err(error) = outcome else {
        return Err("the configuration was accepted".into());
    };
    let message = crate::support::chain(&error);
    assert!(
        message.contains("contact_point") && message.contains("disclose"),
        "the refusal names the key: {message}"
    );
    Ok(())
}

/// Implementing Regulation (EU) 2026/2099 Annex Table 1 names "the Member
/// State that issued" the data, and Regulation (EU) 2025/327 Art 24(3) lets
/// the Commission connect a third country's contact point: a country that
/// is neither a Member State nor added is refused, and one the deployment
/// adds is admitted for that contact point.
#[test]
fn a_country_is_a_member_state_or_one_the_deployment_adds() -> Result<(), Box<dyn Error>> {
    let authority =
        "fields.\"provider.issuing_authority_name\" = [{ constant = \"hcp-authority\" }]\n";
    for country in ["\"XA\"", "\"GB\""] {
        let text = format!(
            "{}\n[[federation.contact_point]]\nissuer = \"{NCP}\"\n",
            issuer(country, authority)
        );
        assert_eq!(
            (
                String::from("auth.federation.countries"),
                AuthFault::CountryCode
            ),
            fault(&text)?,
            "{country}"
        );
    }
    let added = format!(
        "{}\n[[federation.contact_point]]\nissuer = \"{NCP}\"\nadditional_countries = [\"XA\"]\n",
        issuer("\"XA\"", authority)
    );
    let settings = Config::from_sources(Some(&added), &BTreeMap::new())?.resolve()?;
    let declared = settings
        .server
        .auth
        .issuers
        .first()
        .and_then(|issuer| issuer.national_contact_point.as_ref())
        .ok_or("declared a contact point")?;
    assert!(declared.admits_country("XA"));
    assert!(declared.admits_country("LU"), "a Member State always");
    assert!(!declared.admits_country("XB"));
    Ok(())
}

#[test]
fn an_additional_country_out_of_form_or_a_member_state_is_refused() -> Result<(), Box<dyn Error>> {
    for code in ["xa", "XAA", "", "NL"] {
        assert_eq!(
            (
                String::from("federation.contact_point[0].additional_countries"),
                AuthFault::CountryCode
            ),
            fault(&declaring(&format!(
                "additional_countries = [\"{code}\"]\n"
            )))?,
            "{code}"
        );
    }
    Ok(())
}
