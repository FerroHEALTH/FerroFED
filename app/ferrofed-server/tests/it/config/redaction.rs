// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! No rendering of a configuration type shows a credential: every type that
//! holds one is formatted with `{:?}` and `{:#?}` from a configuration
//! carrying sentinel secrets, inline, from the environment and from `_file`
//! siblings, and none of the sentinels may appear.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Debug;

use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::ihe::pixm::ManagerConfig;
use ferrofed_registry::document::Document;
use ferrofed_registry::secret::{REDACTED, Secret, SecretUrl};
use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Scheme;
use ihe_iti::pixm::Invocation;

use super::secret_file;
use crate::run::binary;

type TestResult = Result<(), Box<dyn Error>>;

/// An inline bearer token.
const BEARER: &str = "Qz7bearerSentinel";
/// An inline basic password.
const PASSWORD: &str = "Qz7passwordSentinel";
/// A bearer token set through the environment.
const ENV_BEARER: &str = "Qz7envSentinel";
/// A basic password read from a `_file` sibling.
const FILE_PASSWORD: &str = "Qz7fileSentinel";
/// The user name in the userinfo of a URL.
const URL_USER: &str = "Qz7urlUserSentinel";
/// The password in the userinfo of a URL.
const URL_PASSWORD: &str = "Qz7urlPasswordSentinel";
/// The password in a PostgreSQL connection URL.
const PG_PASSWORD: &str = "Qz7pgPasswordSentinel";

/// Every sentinel; no rendering may hold any of them.
const SENTINELS: [&str; 7] = [
    BEARER,
    PASSWORD,
    ENV_BEARER,
    FILE_PASSWORD,
    URL_USER,
    URL_PASSWORD,
    PG_PASSWORD,
];

/// Asserts that neither `{:?}` nor `{:#?}` of `value` holds a sentinel.
fn redacted(name: &str, value: &impl Debug) {
    for rendered in [format!("{value:?}"), format!("{value:#?}")] {
        for sentinel in SENTINELS {
            // NOTE: a failure message never echoes the rendering, which would
            // put the leaked secret into the test log it keeps it out of.
            assert!(
                !rendered.contains(sentinel),
                "the Debug output of {name} carries a secret"
            );
        }
    }
}

/// Asserts what [`redacted`] does, and that the rendering shows the
/// placeholder where the secret was.
fn shows_placeholder(name: &str, value: &impl Debug) {
    redacted(name, value);
    for rendered in [format!("{value:?}"), format!("{value:#?}")] {
        assert!(rendered.contains(REDACTED), "{name} shows the placeholder");
    }
}

/// A PIX Manager URL that carries userinfo, which the configuration holds
/// but refuses to resolve.
fn pix_url_with_userinfo() -> String {
    format!("https://{URL_USER}:{URL_PASSWORD}@pix.example.org/fhir")
}

/// A PIX Manager URL without userinfo.
const PIX_URL: &str = "https://pix.example.org/fhir";

/// An OTLP collector URL that carries userinfo.
fn otlp_url() -> String {
    format!("http://{URL_USER}:{URL_PASSWORD}@127.0.0.1:4317")
}

/// A PostgreSQL connection URL that carries a password.
fn postgres_url() -> String {
    format!("postgres://ferrofed:{PG_PASSWORD}@db.example.org:5432/ferrofed")
}

/// The configuration under test: one endpoint with an inline bearer token,
/// one with an inline basic password, one whose token the environment sets,
/// one whose password a `_file` sibling holds, a PIX Manager at `pix_url`
/// whose credentials are inline, an OTLP collector URL with userinfo, and
/// the tables of `extra`.
fn configuration(
    password_file: &str,
    pix_url: &str,
    extra: &str,
) -> Result<Config, Box<dyn Error>> {
    let path = toml::Value::String(password_file.to_owned());
    let otlp = otlp_url();
    let text = format!(
        "[credentials.\"node-a-pub\"]\nbearer_token = \"{BEARER}\"\n\n\
         [credentials.\"node-b-pub\"]\nuser = \"gateway\"\npassword = \"{PASSWORD}\"\n\n\
         [credentials.\"node-d-pub\"]\nuser = \"gateway\"\npassword_file = {path}\n\n\
         [metrics]\notlp_endpoint = \"{otlp}\"\n\n\
         [audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{pix_url}\"\n\
         members = {{ \"node-a\" = \"urn:oid:2.999.1\" }}\n\n\
         [pixm.manager.credentials]\nbearer_token = \"{BEARER}\"\n\n{extra}"
    );
    let env = BTreeMap::from([(
        String::from("FERROFED__CREDENTIALS__NODE-C-PUB__BEARER_TOKEN"),
        ENV_BEARER.to_owned(),
    )]);
    Ok(Config::from_sources(Some(&text), &env)?)
}

/// The `[stored_queries]` table of a PostgreSQL store, with the registry
/// document it needs.
fn postgres_store() -> String {
    let url = postgres_url();
    format!(
        "[registry]\ndocument = \"/nonexistent/registry.toml\"\n\n\
         [stored_queries]\nbackend = \"postgres\"\nurl = \"{url}\"\n"
    )
}

#[test]
fn no_configuration_type_renders_a_credential_as_written() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let config = configuration(
        &file.path().display().to_string(),
        &pix_url_with_userinfo(),
        &postgres_store(),
    )?;
    assert_eq!(4, config.credentials.len(), "every section is read");
    shows_placeholder("Config", &config);
    for (endpoint, credentials) in &config.credentials {
        redacted(endpoint, credentials);
    }
    let pixm = config.pixm.as_ref().ok_or("the [pixm] table is read")?;
    redacted("Pixm", pixm);
    for manager in &pixm.manager {
        redacted("PixManager", manager);
        shows_placeholder("PixManager.url", &manager.url);
    }
    shows_placeholder("Metrics", &config.metrics);
    shows_placeholder("StoredQueries", &config.stored_queries);
    let composed = ManagerConfig {
        tls: ferrofed_identity::fhir::Tls::default(),
        base: SecretUrl::new(pix_url_with_userinfo()),
        auth: Authentication::Basic {
            user: String::from("gateway"),
            password: Secret::new(PASSWORD).to_secret_string(),
        },
        members: BTreeMap::new(),
        invocation: Invocation::Post,
    };
    shows_placeholder("ManagerConfig", &composed);
    Ok(())
}

#[cfg(feature = "postgres")]
#[test]
fn a_resolved_postgres_store_never_renders_its_password() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let settings = configuration(
        &file.path().display().to_string(),
        PIX_URL,
        &postgres_store(),
    )?
    .resolve()?;
    shows_placeholder("Settings", &settings);
    let store = settings
        .stored_queries
        .as_ref()
        .ok_or("the store resolves")?;
    shows_placeholder("Store", store);
    Ok(())
}

#[test]
fn no_resolved_settings_type_renders_a_credential() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let config = configuration(&file.path().display().to_string(), PIX_URL, "")?;
    let settings = config.resolve()?;
    shows_placeholder("MetricsSettings", &settings.metrics);
    shows_placeholder("Settings", &settings);
    for (endpoint, scheme) in &settings.credentials {
        shows_placeholder(endpoint.as_str(), scheme);
    }
    let pixm = settings.pixm.as_ref().ok_or("the PIXm settings resolve")?;
    redacted("PixmSettings", pixm);
    for manager in &pixm.managers {
        redacted("PixManagerSettings", manager);
        let auth = match &manager.credentials {
            Some(Scheme::Bearer(token)) => Authentication::Bearer(token.to_secret_string()),
            _ => return Err("the PIX Manager resolves to its bearer token".into()),
        };
        let composed = ManagerConfig {
            tls: ferrofed_identity::fhir::Tls::default(),
            base: manager.url.clone(),
            auth,
            members: BTreeMap::new(),
            invocation: Invocation::Get,
        };
        redacted("ManagerConfig", &composed);
    }
    Ok(())
}

#[test]
fn every_secret_still_resolves_to_its_value() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let settings = configuration(&file.path().display().to_string(), PIX_URL, "")?.resolve()?;
    let mut tokens = BTreeMap::new();
    for (endpoint, scheme) in &settings.credentials {
        let value = match scheme {
            Scheme::Bearer(token) => token.expose(),
            Scheme::Basic { password, .. } => password.expose(),
            _ => return Err("a scheme this test does not know".into()),
        };
        tokens.insert(endpoint.as_str(), value);
    }
    let expected = BTreeMap::from([
        ("node-a-pub", BEARER),
        ("node-b-pub", PASSWORD),
        ("node-c-pub", ENV_BEARER),
        ("node-d-pub", FILE_PASSWORD),
    ]);
    assert_eq!(expected, tokens, "each endpoint holds its own secret");
    let pixm = settings.pixm.as_ref().ok_or("the PIXm settings resolve")?;
    let url = pixm.managers.first().ok_or("one Manager")?.url.expose();
    assert_eq!(PIX_URL, url, "the Manager URL is kept as written");
    let otlp = settings
        .metrics
        .otlp_endpoint
        .as_ref()
        .ok_or("an OTLP push")?;
    assert_eq!(
        format!("{}/", otlp_url()),
        otlp.expose(),
        "the collector URL is kept for connecting"
    );
    Ok(())
}

#[test]
fn a_pix_manager_url_with_userinfo_is_refused_without_quoting_it() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let config = configuration(
        &file.path().display().to_string(),
        &pix_url_with_userinfo(),
        "",
    )?;
    let Err(refused) = config.resolve() else {
        return Err("a PIX Manager URL with userinfo was accepted".into());
    };
    assert!(
        matches!(
            &refused,
            ferrofed_server::config::error::Error::UrlCredentials { key, section }
                if key == "pixm.manager[0].url" && section == "pixm.manager[0].credentials"
        ),
        "the refusal names the key and the section"
    );
    let rendered = format!("{refused}\n{refused:?}");
    for sentinel in [URL_USER, URL_PASSWORD] {
        assert!(!rendered.contains(sentinel), "the refusal quotes the URL");
    }
    let output = binary(
        &["config", "check"],
        &format!(
            "[audit]\ndestination = \"log\"\n\n[[pixm.manager]]\nurl = \"{}\"\n",
            pix_url_with_userinfo()
        ),
    )?;
    assert_eq!(
        Some(i32::from(ferrofed_server::EXIT_CONFIG)),
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pixm.manager[0].url"),
        "config check names the key"
    );
    for sentinel in [URL_USER, URL_PASSWORD] {
        assert!(!stderr.contains(sentinel), "config check quotes the URL");
    }
    Ok(())
}

#[cfg(feature = "postgres")]
#[test]
fn a_postgres_url_file_resolves_into_its_redacting_type() -> TestResult {
    let file = secret_file(&format!("{}\n", postgres_url()))?;
    let path = toml::Value::String(file.path().display().to_string());
    let settings = Config::from_sources(
        Some(&format!(
            "[registry]\ndocument = \"/nonexistent/registry.toml\"\n\n\
             [stored_queries]\nbackend = \"postgres\"\nurl_file = {path}\n"
        )),
        &BTreeMap::new(),
    )?
    .resolve()?;
    let store = settings
        .stored_queries
        .as_ref()
        .ok_or("the store resolves")?;
    shows_placeholder("Store", store);
    let ferrofed_server::config::stored_queries::Store::Postgres(url) = store else {
        return Err("a PostgreSQL store".into());
    };
    assert_eq!(
        postgres_url(),
        url.expose(),
        "read from the file and trimmed"
    );
    Ok(())
}

#[test]
fn a_registry_document_never_renders_endpoint_userinfo() -> TestResult {
    let document: Document = toml::from_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n\
         [[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n\
         [[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\n\
         url = \"https://{URL_USER}:{URL_PASSWORD}@cdr-a.example.org/openehr\"\n\
         connection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    ))?;
    shows_placeholder("Document", &document);
    for endpoint in &document.endpoints {
        redacted("EndpointDoc", endpoint);
    }
    Ok(())
}

#[test]
fn a_connection_url_and_a_secret_render_and_serialize_redacted() -> TestResult {
    let url = SecretUrl::new(format!(
        "postgres://ferrofed:{PG_PASSWORD}@db.example.org:5432/ferrofed"
    ));
    shows_placeholder("SecretUrl", &url);
    assert_eq!(
        "postgres://***@db.example.org:5432/ferrofed",
        url.to_string()
    );
    let serialized = toml::Value::try_from(&url)?.to_string();
    assert!(
        !serialized.contains(PG_PASSWORD),
        "Serialize redacts the URL"
    );

    let secret = Secret::new(BEARER);
    shows_placeholder("Secret", &secret);
    assert_eq!(REDACTED, secret.to_string());
    let serialized = toml::Value::try_from(&secret)?.to_string();
    assert!(!serialized.contains(BEARER), "Serialize redacts the secret");
    Ok(())
}

#[test]
fn config_check_prints_no_credential() -> TestResult {
    let file = secret_file(&format!("{FILE_PASSWORD}\n"))?;
    let path = toml::Value::String(file.path().display().to_string());
    let text = format!(
        "[credentials.\"node-a-pub\"]\nbearer_token = \"{BEARER}\"\n\n\
         [credentials.\"node-b-pub\"]\nuser = \"gateway\"\npassword = \"{PASSWORD}\"\n\n\
         [credentials.\"node-d-pub\"]\nuser = \"gateway\"\npassword_file = {path}\n"
    );
    let output = binary(&["config", "check"], &text)?;
    assert!(output.status.success(), "the configuration is accepted");
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for sentinel in SENTINELS {
        assert!(
            !printed.contains(sentinel),
            "config check prints a credential"
        );
    }
    Ok(())
}
