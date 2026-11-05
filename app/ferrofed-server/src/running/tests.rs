// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use ferrofed_operator_api::revision::RevisionId;
use ferrofed_operator_api::running::{RunningConfig, Section, Setting, Source};

use std::collections::BTreeMap;

use super::{Layers, of};
use crate::config::Config;

/// The synthetic password of a test's inline connection string, which no
/// document may carry.
const SECRET: &str = "synthetic-secret-71";

/// The tree `text` writes.
fn tree(text: &str) -> toml::Table {
    toml::from_str(text).expect("the test tree reads")
}

/// The document `layers` give, and its `store.url`, a secret that is set.
fn store_url(layers: &Layers) -> (RunningConfig, Setting) {
    let document = of(layers);
    let url = document.setting("store.url").expect("listed").clone();
    assert!(
        url.secret.as_ref().is_some_and(|secret| secret.set),
        "a secret that is set"
    );
    (document, url)
}

#[test]
fn a_setting_nothing_sets_runs_its_default() {
    let document = of(&Layers::default());
    assert_eq!("off", document.ui);
    assert_eq!(None, document.revision);
    let timeout = document
        .setting("federation.per_node_timeout_ms")
        .expect("listed");
    assert_eq!(Some(toml::Value::Integer(10_000)), timeout.value);
    assert_eq!(timeout.default, timeout.value);
    assert_eq!(Source::Default, timeout.source);
    assert!(timeout.restart);
    assert!(!timeout.editable, "nothing is edited under off");
    let selection = document
        .setting("federation.node_selection")
        .expect("listed");
    assert_eq!(None, selection.value, "node_selection has no default");
}

#[test]
fn a_value_names_the_file_or_the_override_it_comes_from() {
    let loaded = Config::loaded_from_sources(
        Some("[federation]\nper_node_timeout_ms = 2500\noverall_timeout_ms = 3000\n"),
        &BTreeMap::from([
            (
                String::from("FERROFED__FEDERATION__OVERALL_TIMEOUT_MS"),
                String::from("4000"),
            ),
            (String::from("PATH"), String::from("/usr/bin")),
        ]),
    )
    .expect("the configuration reads");
    let boot = loaded.table_with_secrets().clone();
    let layers = Layers {
        now: boot.clone(),
        boot: boot.clone(),
        provenance: loaded.provenance().clone(),
        file_boot: boot.clone(),
        file_now: boot,
        ..Layers::default()
    };
    let document = of(&layers);
    let per_node = document
        .setting("federation.per_node_timeout_ms")
        .expect("listed");
    assert_eq!(Some(toml::Value::Integer(2500)), per_node.value);
    assert_eq!(Source::File, per_node.source);
    let overall = document
        .setting("federation.overall_timeout_ms")
        .expect("listed");
    assert_eq!(Some(toml::Value::Integer(4000)), overall.value);
    assert_eq!(Source::Environment, overall.source);
}

#[test]
fn a_restart_setting_runs_the_boot_value_and_names_the_next_while_a_live_one_runs_the_new() {
    let layers = Layers {
        boot: tree(
            "[federation]\nper_node_timeout_ms = 2500\n\n[federation.consent]\ndisclose = true\n",
        ),
        now: tree(
            "[federation]\nper_node_timeout_ms = 3000\n\n[federation.consent]\ndisclose = false\n",
        ),
        ..Layers::default()
    };
    let document = of(&layers);
    let per_node = document
        .setting("federation.per_node_timeout_ms")
        .expect("listed");
    assert_eq!(Some(toml::Value::Integer(2500)), per_node.value);
    assert_eq!(Some(toml::Value::Integer(3000)), per_node.next);
    let disclose = document
        .setting("federation.consent.disclose")
        .expect("listed");
    assert!(!disclose.restart);
    assert_eq!(Some(toml::Value::Boolean(false)), disclose.value);
    assert_eq!(None, disclose.next);
}

#[test]
fn under_managed_a_value_comes_from_the_revision_and_a_scalar_is_editable() {
    let held = tree(
        "[federation]\nbest_effort = false\n\n[access_log]\ntemplates = { \"t.v1\" = { categories = [\"lab\"] } }\n",
    );
    let layers = Layers {
        boot: held.clone(),
        now: held,
        revision: RevisionId::new(7),
        managed: true,
        ..Layers::default()
    };
    let document = of(&layers);
    assert_eq!("managed", document.ui);
    assert_eq!(RevisionId::new(7), document.revision);
    let best_effort = document.setting("federation.best_effort").expect("listed");
    assert_eq!(Source::Revision, best_effort.source);
    assert!(best_effort.editable);
    let templates = document.setting("access_log.templates").expect("listed");
    assert!(!templates.editable, "a table is changed by an import");
    assert_eq!(Source::Revision, templates.source);
    assert_eq!(7, Section::ALL.len());
    for section in Section::ALL {
        assert!(document.of(section).count() > 0, "{section:?}");
    }
}

#[test]
fn a_secret_is_named_by_its_file_and_its_value_never_reaches_the_document() {
    let inline = tree(&format!(
        "[store]\nurl = \"postgres://ferrofed:{SECRET}@db.example.org/stored?sslmode=require\"\n"
    ));
    let layers = Layers {
        now: inline.clone(),
        boot: inline,
        ..Layers::default()
    };
    let (document, url) = store_url(&layers);
    let secret = url.secret.as_ref().expect("a secret");
    assert_eq!(None, secret.reference);
    assert_eq!(None, url.value);
    assert!(!url.editable);
    let written = serde_json::to_string(&document).expect("the document writes");
    assert!(!written.contains(SECRET), "{written}");
    assert!(!written.contains("ferrofed:"), "{written}");

    let by_file = tree("[store]\nurl_file = \"/run/secrets/store-url\"\n");
    let layers = Layers {
        now: by_file.clone(),
        boot: by_file,
        managed: true,
        ..Layers::default()
    };
    let (_, url) = store_url(&layers);
    let secret = url.secret.as_ref().expect("a secret");
    assert_eq!(Some("/run/secrets/store-url"), secret.reference.as_deref());
    assert_eq!(Source::Revision, url.source);
    assert!(!url.editable, "a secret is never edited as a setting");

    let unset = of(&Layers::default());
    let url = unset.setting("store.url").expect("listed");
    assert_eq!(
        Some(false),
        url.secret.as_ref().map(|secret| secret.set),
        "an unset secret says so"
    );
    assert_eq!(Source::Default, url.source);
}
