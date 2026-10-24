// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings of who may call, in the running configuration: `[auth]` and
//! `[trust]`, which only the file sets, so a revision never holds them.
//!
//! The document lists the audience and the product scope, each `[[auth.issuer]]`
//! by its issuer and key set file, and each `[[trust.issuer]]` by its issuer,
//! the introspection secret it authenticates with, and, for an issuer declared
//! a national contact point (Regulation (EU) 2025/327 Art 23), every key of
//! its `[trust.issuer.national_contact_point]`: the claim each Implementing
//! Regulation (EU) 2026/2099 Annex attribute is read from, the correlation
//! header and the added countries. An issuer is keyed by its position, as the
//! reload names it (`trust.issuer[0].issuer`). A secret is named by its file
//! and never by a value, and a Basic user's password hash is not listed. Every
//! change but the audience's applies at the next reload. No specification
//! governs this: our own design.

use ferrofed_registry::running::{SecretSetting, Section, Setting, Source};

use super::{Layers, at};

/// The keys of `[trust.issuer.national_contact_point]`, in the order the
/// book's table lists them.
const CONTACT_POINT: [&str; 8] = [
    "family_name",
    "given_name",
    "country_code",
    "professional_issuing_authority",
    "provider_issuing_authority",
    "provider_address",
    "correlation_header",
    "additional_countries",
];

/// The keys of an `[[auth.issuer]]` the document lists.
const ISSUER: [&str; 2] = ["issuer", "jwks_file"];

/// The settings of `[auth]` and `[trust]` as `layers` set them.
pub(super) fn settings(layers: &Layers) -> Vec<Setting> {
    let mut settings = vec![
        listed(layers, "auth.audience", true),
        listed(layers, "auth.product_scope", false),
    ];
    match at(&layers.file_now, "auth.issuer") {
        Some(toml::Value::Array(issuers)) => {
            for (index, _) in issuers.iter().enumerate() {
                for key in ISSUER {
                    settings.push(listed(
                        layers,
                        &format!("auth.issuer[{index}].{key}"),
                        false,
                    ));
                }
            }
        }
        Some(_) => settings.push(listed(layers, "auth.issuer", false)),
        None => {}
    }
    let trusted = match at(&layers.file_now, "trust.issuer") {
        Some(toml::Value::Array(issuers)) => issuers.len(),
        _ => 0,
    };
    for index in 0..trusted {
        let prefix = format!("trust.issuer[{index}]");
        settings.push(listed(layers, &format!("{prefix}.issuer"), false));
        settings.push(secret(layers, &format!("{prefix}.client_secret")));
        if value(
            &layers.file_now,
            &format!("{prefix}.national_contact_point"),
        )
        .is_some()
        {
            for key in CONTACT_POINT {
                settings.push(listed(
                    layers,
                    &format!("{prefix}.national_contact_point.{key}"),
                    false,
                ));
            }
        }
    }
    settings
}

/// The value at `key` of `tree`, a dotted key whose segments may name an
/// array element as `name[index]`.
fn value<'a>(tree: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut value: Option<&toml::Value> = None;
    for segment in key.split('.') {
        let (name, index) = match segment.split_once('[') {
            Some((name, rest)) => (name, rest.strip_suffix(']')?.parse::<usize>().ok()),
            None => (segment, None),
        };
        let table = match value {
            None => tree,
            Some(held) => held.as_table()?,
        };
        let held = table.get(name)?;
        value = Some(match index {
            Some(index) => held.as_array()?.get(index)?,
            None => held,
        });
    }
    value
}

/// Where the value at `key` comes from: the environment when an override
/// set it to what runs, the file otherwise, and nothing when it is unset.
fn origin(layers: &Layers, key: &str, runs: Option<&toml::Value>) -> Source {
    match runs {
        None => Source::Default,
        Some(runs) if value(&layers.environment, key) == Some(runs) => Source::Environment,
        Some(_) => Source::File,
    }
}

/// The setting at `key`, which waits for the next start when `restart`.
fn listed(layers: &Layers, key: &str, restart: bool) -> Setting {
    let tree = if restart {
        &layers.file_boot
    } else {
        &layers.file_now
    };
    let runs = value(tree, key);
    let next = restart
        .then(|| value(&layers.file_now, key))
        .flatten()
        .filter(|now| Some(*now) != runs)
        .cloned();
    Setting {
        key: key.to_owned(),
        section: Section::Authentication,
        value: runs.cloned(),
        source: origin(layers, key, runs),
        default: None,
        restart,
        next,
        secret: None,
        editable: false,
    }
}

/// The secret at `key`: whether it is set and the file it is read from,
/// never its value.
fn secret(layers: &Layers, key: &str) -> Setting {
    let file = format!("{key}_file");
    let inline = value(&layers.file_now, key);
    let reference = value(&layers.file_now, &file);
    let source = match reference {
        Some(_) => origin(layers, &file, reference),
        None => origin(layers, key, inline),
    };
    Setting {
        key: key.to_owned(),
        section: Section::Authentication,
        value: None,
        source,
        default: None,
        restart: false,
        next: None,
        secret: Some(SecretSetting {
            set: inline.is_some() || reference.is_some(),
            reference: reference.and_then(toml::Value::as_str).map(str::to_owned),
        }),
        editable: false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Layers, of};
    use ferrofed_registry::running::{Section, Source};

    /// The tree `text` writes.
    fn tree(text: &str) -> toml::Table {
        toml::from_str(text).expect("the test tree parses")
    }

    /// A file declaring one key-set issuer and one trusted issuer declared a
    /// national contact point, with an introspection secret inline.
    const FILE: &str = r#"
[auth]
audience = ["urn:example:gw"]
product_scope = "__ferrofed.operator"

[[auth.issuer]]
issuer = "https://ncp.example.org"
jwks_file = "/run/ferrofed/ncp-jwks.json"

[[auth.basic.users]]
user = "scraper"
password_hash = "SENTINEL-HASH-4f1"

[[trust.issuer]]
issuer = "https://ncp.example.org"
client_secret = "SENTINEL-SECRET-9k2"

[trust.issuer.national_contact_point]
family_name = "ncp_family_name"
given_name = "ncp_given_name"
country_code = "ncp_country_code"
professional_issuing_authority = "ncp_hp_issuing_authority"
provider_issuing_authority = "ncp_hcp_issuing_authority"
provider_address = "ncp_hcp_address"
correlation_header = "ncp-correlation-id"
additional_countries = ["XA"]
"#;

    fn layers(managed: bool) -> Layers {
        let file = tree(FILE);
        Layers {
            boot: if managed {
                toml::Table::new()
            } else {
                file.clone()
            },
            now: if managed {
                toml::Table::new()
            } else {
                file.clone()
            },
            file_boot: file.clone(),
            file_now: file,
            managed,
            ..Layers::default()
        }
    }

    #[test]
    fn every_contact_point_key_is_listed_from_the_file_in_either_mode() {
        for managed in [false, true] {
            let document = of(&layers(managed));
            for (key, value) in [
                ("trust.issuer[0].issuer", "https://ncp.example.org"),
                (
                    "trust.issuer[0].national_contact_point.family_name",
                    "ncp_family_name",
                ),
                (
                    "trust.issuer[0].national_contact_point.correlation_header",
                    "ncp-correlation-id",
                ),
                ("auth.issuer[0].issuer", "https://ncp.example.org"),
                ("auth.product_scope", "__ferrofed.operator"),
            ] {
                let setting = document.setting(key).expect(key);
                assert_eq!(Section::Authentication, setting.section, "{key}");
                assert_eq!(Some(toml::Value::from(value)), setting.value, "{key}");
                assert_eq!(Source::File, setting.source, "{key}");
                assert!(!setting.editable, "{key}: the file alone sets it");
            }
            let countries = document
                .setting("trust.issuer[0].national_contact_point.additional_countries")
                .expect("listed");
            assert_eq!(
                Some(toml::Value::Array(vec![toml::Value::from("XA")])),
                countries.value
            );
        }
    }

    #[test]
    fn no_secret_and_no_password_hash_is_ever_a_value() {
        let document = of(&layers(false));
        let written = serde_json::to_string(&document).expect("the document writes");
        assert!(!written.contains("SENTINEL-SECRET-9k2"), "{written}");
        assert!(!written.contains("SENTINEL-HASH-4f1"), "{written}");
        let secret = document
            .setting("trust.issuer[0].client_secret")
            .expect("listed");
        assert_eq!(None, secret.value);
        assert!(secret.secret.as_ref().is_some_and(|held| held.set));
    }

    #[test]
    fn a_changed_audience_waits_for_the_next_start() {
        let mut layers = layers(false);
        layers.file_now = tree(&FILE.replace("urn:example:gw", "urn:example:other"));
        let document = of(&layers);
        let audience = document.setting("auth.audience").expect("listed");
        assert!(audience.restart);
        assert_eq!(
            Some(toml::Value::Array(vec![toml::Value::from(
                "urn:example:gw"
            )])),
            audience.value
        );
        assert_eq!(
            Some(toml::Value::Array(vec![toml::Value::from(
                "urn:example:other"
            )])),
            audience.next
        );
    }

    #[test]
    fn an_issuer_not_declared_a_contact_point_lists_no_contact_point_key() {
        let mut layers = layers(false);
        layers.file_now = tree(
            "[[trust.issuer]]\nissuer = \"https://idp.example.org\"\nclient_secret_file = \"/run/secret\"\n",
        );
        let document = of(&layers);
        assert!(document.setting("trust.issuer[0].issuer").is_some());
        assert!(
            document
                .settings
                .iter()
                .all(|setting| !setting.key.contains("national_contact_point"))
        );
        let secret = document
            .setting("trust.issuer[0].client_secret")
            .expect("listed");
        assert_eq!(
            Some("/run/secret"),
            secret
                .secret
                .as_ref()
                .and_then(|held| held.reference.as_deref())
        );
    }
}
