// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

use ferrofed_registry::revision::RevisionId;
use ferrofed_registry::running::{Section, Source};

use super::{Layers, environment, of, set};

/// The tree `text` writes.
fn tree(text: &str) -> toml::Table {
    toml::from_str(text).expect("the test tree reads")
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
    let boot = tree("[federation]\nper_node_timeout_ms = 2500\noverall_timeout_ms = 4000\n");
    let layers = Layers {
        now: boot.clone(),
        boot,
        environment: environment([
            (
                String::from("FERROFED__FEDERATION__OVERALL_TIMEOUT_MS"),
                String::from("4000"),
            ),
            (String::from("PATH"), String::from("/usr/bin")),
        ]),
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
    let inline = tree(
        "[stored_queries]\nbackend = \"postgres\"\nurl = \"postgres://ferrofed:synthetic-secret-71@db.example.org/stored\"\n",
    );
    let layers = Layers {
        now: inline.clone(),
        boot: inline,
        ..Layers::default()
    };
    let document = of(&layers);
    let url = document.setting("stored_queries.url").expect("listed");
    let secret = url.secret.as_ref().expect("a secret");
    assert!(secret.set);
    assert_eq!(None, secret.reference);
    assert_eq!(None, url.value);
    assert!(!url.editable);
    let written = serde_json::to_string(&document).expect("the document writes");
    assert!(!written.contains("synthetic-secret-71"), "{written}");
    assert!(!written.contains("ferrofed:"), "{written}");

    let by_file = tree(
        "[stored_queries]\nbackend = \"postgres\"\nurl_file = \"/run/secrets/stored-queries-url\"\n",
    );
    let layers = Layers {
        now: by_file.clone(),
        boot: by_file,
        managed: true,
        ..Layers::default()
    };
    let document = of(&layers);
    let url = document.setting("stored_queries.url").expect("listed");
    let secret = url.secret.as_ref().expect("a secret");
    assert!(secret.set);
    assert_eq!(
        Some("/run/secrets/stored-queries-url"),
        secret.reference.as_deref()
    );
    assert_eq!(Source::Revision, url.source);
    assert!(!url.editable, "a secret is never edited as a setting");

    let unset = of(&Layers::default());
    let url = unset.setting("stored_queries.url").expect("listed");
    assert_eq!(
        Some(false),
        url.secret.as_ref().map(|secret| secret.set),
        "an unset secret says so"
    );
    assert_eq!(Source::Default, url.source);
}

#[test]
fn a_dotted_key_is_set_through_its_parents_and_never_through_a_value() {
    let mut tree = toml::Table::new();
    assert!(set(
        &mut tree,
        "federation.consent.disclose",
        toml::Value::Boolean(false)
    ));
    assert_eq!(
        Some(&toml::Value::Boolean(false)),
        super::at(&tree, "federation.consent.disclose")
    );
    let mut flat = self::tree("federation = 3\n");
    assert!(!set(
        &mut flat,
        "federation.best_effort",
        toml::Value::Boolean(true)
    ));
}
