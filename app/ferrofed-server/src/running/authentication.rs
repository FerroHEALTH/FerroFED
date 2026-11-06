// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings of who may call, in the running configuration: `[auth]`,
//! `[trust]` and `[[federation.contact_point]]`, which only the file sets,
//! so a revision never holds them.
//!
//! The document lists the product scope, each `[[auth.issuer]]` by its
//! issuer, kind, key set file and audience, with the client secret of its
//! introspection when it has one, each `[[trust.issuer]]` by its issuer, and
//! each national contact point's `[[federation.contact_point]]` (Regulation
//! (EU) 2025/327 Art 23) by its issuer, correlation header and added
//! countries. An issuer is keyed by its position, as the reload names it
//! (`trust.issuer[0].issuer`).
//! A secret is named by its file and never by a value, and a Basic user's
//! password hash is not listed. Every change but an issuer's audience
//! applies at the next reload. No specification governs this: our own
//! design.

use ferrofed_operator_api::running::{SecretSetting, Section, Setting, Source};

use super::{Layers, at, indexed, provenance_key, sourced};

/// The keys of a `[[federation.contact_point]]`, in the order the book's
/// table lists them.
const CONTACT_POINT: [&str; 3] = ["issuer", "correlation_header", "additional_countries"];

/// The keys of an `[[auth.issuer]]` the document lists that apply at the
/// next reload.
const ISSUER: [&str; 3] = ["issuer", "kind", "jwks_file"];

/// The settings of `[auth]`, `[trust]` and `[[federation.contact_point]]` as
/// `layers` set them.
pub(super) fn settings(layers: &Layers) -> Vec<Setting> {
    let mut settings = vec![listed(layers, "auth.product_scope", false)];
    match at(&layers.file_now, "auth.issuer") {
        Some(toml::Value::Array(issuers)) => {
            for (index, _) in issuers.iter().enumerate() {
                let prefix = format!("auth.issuer[{index}]");
                for key in ISSUER {
                    settings.push(listed(layers, &format!("{prefix}.{key}"), false));
                }
                settings.push(listed(layers, &format!("{prefix}.audience"), true));
                if indexed(&layers.file_now, &format!("{prefix}.introspection")).is_some() {
                    settings.push(secret(
                        layers,
                        &format!("{prefix}.introspection.client.client_secret"),
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
        settings.push(listed(
            layers,
            &format!("trust.issuer[{index}].issuer"),
            false,
        ));
    }
    let contact_points = match at(&layers.file_now, "federation.contact_point") {
        Some(toml::Value::Array(tables)) => tables.len(),
        _ => 0,
    };
    for index in 0..contact_points {
        for key in CONTACT_POINT {
            settings.push(listed(
                layers,
                &format!("federation.contact_point[{index}].{key}"),
                false,
            ));
        }
    }
    settings
}

/// Where the value at `key` comes from: the override that set it to what
/// runs, the file otherwise, which alone sets `[auth]` and `[trust]`, and
/// nothing when it is unset.
fn origin(layers: &Layers, key: &str, runs: Option<&toml::Value>, secret: bool) -> Source {
    let by = if secret {
        layers.provenance.secret_source(&provenance_key(key))
    } else {
        layers.provenance.source(&provenance_key(key))
    };
    match sourced(layers, key, runs, by) {
        Source::Revision => Source::File,
        other => other,
    }
}

/// The setting at `key`, which waits for the next start when `restart`.
fn listed(layers: &Layers, key: &str, restart: bool) -> Setting {
    let tree = if restart {
        &layers.file_boot
    } else {
        &layers.file_now
    };
    let runs = indexed(tree, key);
    let next = restart
        .then(|| indexed(&layers.file_now, key))
        .flatten()
        .filter(|now| Some(*now) != runs)
        .cloned();
    Setting {
        key: key.to_owned(),
        section: Section::Authentication,
        value: runs.cloned(),
        source: origin(layers, key, runs, false),
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
    let inline = indexed(&layers.file_now, key);
    let reference = indexed(&layers.file_now, &file);
    let source = match reference {
        Some(_) => origin(layers, &file, reference, true),
        None => origin(layers, key, inline, true),
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
    use ferrofed_operator_api::running::{Section, Source};

    /// The tree `text` writes.
    fn tree(text: &str) -> toml::Table {
        toml::from_str(text).expect("the test tree parses")
    }

    /// A file declaring one introspected issuer, with its client secret
    /// inline, its trust entry, and its `[[federation.contact_point]]`.
    const FILE: &str = r#"
[auth]
product_scope = "__ferrofed.operator"

[[auth.issuer]]
issuer = "https://ncp.example.org"
kind = "persons"
audience = ["urn:example:gw"]
jwks_file = "/run/ferrofed/ncp-jwks.json"

[auth.issuer.introspection.client]
client_id = "ferrofed"
client_secret = "SENTINEL-SECRET-9k2"

[[auth.basic.users]]
user = "scraper"
password_hash = "SENTINEL-HASH-4f1"

[[trust.issuer]]
issuer = "https://ncp.example.org"

[[federation.contact_point]]
issuer = "https://ncp.example.org"
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
                    "federation.contact_point[0].issuer",
                    "https://ncp.example.org",
                ),
                (
                    "federation.contact_point[0].correlation_header",
                    "ncp-correlation-id",
                ),
                ("auth.issuer[0].issuer", "https://ncp.example.org"),
                ("auth.issuer[0].kind", "persons"),
                ("auth.product_scope", "__ferrofed.operator"),
            ] {
                let setting = document.setting(key).expect(key);
                assert_eq!(Section::Authentication, setting.section, "{key}");
                assert_eq!(Some(toml::Value::from(value)), setting.value, "{key}");
                assert_eq!(Source::File, setting.source, "{key}");
                assert!(!setting.editable, "{key}: the file alone sets it");
            }
            let countries = document
                .setting("federation.contact_point[0].additional_countries")
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
            .setting("auth.issuer[0].introspection.client.client_secret")
            .expect("listed");
        assert_eq!(None, secret.value);
        assert!(secret.secret.as_ref().is_some_and(|held| held.set));
    }

    #[test]
    fn a_changed_audience_waits_for_the_next_start() {
        let mut layers = layers(false);
        layers.file_now = tree(&FILE.replace("urn:example:gw", "urn:example:other"));
        let document = of(&layers);
        let audience = document.setting("auth.issuer[0].audience").expect("listed");
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
    fn a_file_with_no_contact_point_lists_no_contact_point_key() {
        let mut layers = layers(false);
        layers.file_now = tree(
            "[[auth.issuer]]\nissuer = \"https://idp.example.org\"\n\n[auth.issuer.introspection.client]\nclient_secret_file = \"/run/secret\"\n\n[[trust.issuer]]\nissuer = \"https://idp.example.org\"\n",
        );
        let document = of(&layers);
        assert!(document.setting("trust.issuer[0].issuer").is_some());
        assert!(
            document
                .settings
                .iter()
                .all(|setting| !setting.key.contains("contact_point"))
        );
        let secret = document
            .setting("auth.issuer[0].introspection.client.client_secret")
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
