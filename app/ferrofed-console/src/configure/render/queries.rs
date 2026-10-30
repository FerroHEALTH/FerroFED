// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Stored queries tab: the stored versions grouped by name, and the
//! form that stores a new version, which is immutable once stored (§12.7,
//! N44).

use ferrofed_registry::console::{QueryRegistry, QueryResource};
use ferrohealth_ui::html::Html;

use crate::configure::render::Loaded;
use crate::configure::render::members::{GROUP, owner};
use crate::configure::render::parts::{
    Text, confirmed, error_line, form, head, hidden, text, trail,
};
use crate::locale::{Msg, tr};
use crate::pages::{h, opt, t};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::TEXTAREA;
use ferrohealth_ui::view::status::badge;

/// The stored versions grouped by name, in the order the gateway lists
/// them.
fn grouped(queries: &[QueryResource]) -> Vec<(&str, Vec<&QueryResource>)> {
    let mut groups: Vec<(&str, Vec<&QueryResource>)> = Vec::new();
    for query in queries {
        match groups.iter_mut().find(|(name, _)| *name == query.name) {
            Some((_, versions)) => versions.push(query),
            None => groups.push((&query.name, vec![query])),
        }
    }
    groups
}

/// The columns of the Stored queries tab.
fn query_columns() -> [Column; 3] {
    [
        Column::text("name", tr(Msg::Name)),
        Column::text("versions", tr(Msg::Versions)),
        Column::text("owner", tr(Msg::Owner)),
    ]
}

/// The Stored queries tab.
pub(super) fn tab(loaded: &Loaded<'_>) -> Html {
    let registry = loaded.resources.stored_query_registry;
    let store = (registry == QueryRegistry::Writable).then(|| {
        h(format!(
            "<a href=\"/configuration/stored-queries/new\" class=\"{}\">{}{}</a>",
            Variant::Primary.class(),
            icon(Glyph::PLUS, "16"),
            t(tr(Msg::StoreANewVersion)),
        ))
    });
    let header = head(
        ("queries-title", tr(Msg::StoredQueries)),
        (
            tr(Msg::StoredVersionImmutableChangeNewVersion),
            "console-managed-resources",
        ),
        opt(store),
    );
    if registry == QueryRegistry::Absent {
        return header.then(h(format!(
            "<p class=\"fui-muted\">{}</p>",
            t(tr(Msg::GatewayOffersNoStoredQueryRegistry))
        )));
    }
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        h(format!(
            "<p role=\"note\">{}</p>",
            t(tr(Msg::StoredQueriesComeFromReadOnly))
        ))
    });
    let groups = grouped(&loaded.resources.stored_queries);
    let rows = if groups.is_empty() {
        h(format!(
            "<tr><td colspan=\"3\" class=\"fui-muted\">{}</td></tr>",
            t(tr(Msg::NoVersionStoredYet))
        ))
    } else {
        Html::join(groups.into_iter().map(|(name, versions)| {
            let badges = Html::join(
                versions
                    .iter()
                    .map(|query| badge(Tone::Neutral, &query.version)),
            );
            let held = versions.first().map(|query| owner(query.owner));
            h(format!(
                "<tr><th scope=\"row\"><code>{}</code></th>\
                 <td data-label=\"{}\"><div class=\"fui-cluster\">{badges}</div></td>\
                 <td data-label=\"{}\">{}</td></tr>",
                t(name),
                t(tr(Msg::Versions)),
                t(tr(Msg::Owner)),
                opt(held),
            ))
        }))
    };
    Html::join([
        header,
        opt(read_only),
        crate::pages::table(&tr(Msg::StoredQueryVersionsName), &query_columns(), rows),
    ])
}

/// The form that stores a new version, its confirmation saying that the
/// version cannot be changed afterwards.
pub(super) fn form_page(loaded: &Loaded<'_>) -> Html {
    let posted = loaded
        .drafted(&["store-query"])
        .cloned()
        .unwrap_or_default();
    let names = Html::join(
        grouped(&loaded.resources.stored_queries)
            .into_iter()
            .map(|(name, _)| h(format!("<option value=\"{}\"></option>", t(name)))),
    );
    let name = text(
        ("cfg-name", "form[name]"),
        tr(Msg::QualifiedName),
        posted.name.clone(),
        Text {
            required: true,
            mono: true,
            list: Some("cfg-names"),
            hint: Some(tr(Msg::NamespaceNameSuchOrgExampleVital)),
            ..Text::default()
        },
    );
    let version = text(
        ("cfg-version", "form[version]"),
        tr(Msg::Version),
        posted.version.clone(),
        Text {
            required: true,
            mono: true,
            hint: Some(tr(Msg::MajorMinorPatchSuch10)),
            ..Text::default()
        },
    );
    let store = confirmed(
        "store-confirm",
        (Glyph::SAVE, tr(Msg::StoreTheVersion), Variant::Primary),
        (
            tr(Msg::StoreVersionStoredVersionCannotChanged),
            tr(Msg::Store),
        ),
        Html::empty(),
    );
    let body = Html::join([
        hidden("action", String::from("store-query")),
        h(format!(
            "<fieldset class=\"{GROUP}\"><legend>{}</legend>\
             <div class=\"fui-field-row fui-field-row-halves\">{name}{version}</div>\
             <datalist id=\"cfg-names\">{names}</datalist></fieldset>",
            t(tr(Msg::Version))
        )),
        h(format!(
            "<fieldset class=\"{GROUP}\"><legend>{aql}</legend>\
             <label for=\"cfg-aql\" class=\"fui-sr-only\">{aql}</label>\
             <textarea id=\"cfg-aql\" name=\"form[aql]\" rows=\"14\" spellcheck=\"false\" required \
             class=\"{TEXTAREA} fui-mono\" aria-describedby=\"cfg-aql-error\">{}</textarea>{}</fieldset>",
            t(&posted.aql),
            error_line("cfg-aql"),
            aql = t(tr(Msg::Aql)),
        )),
        h(format!(
            "<div class=\"fui-actions\">{store} <a href=\"/configuration/stored-queries\" class=\"{}\">{}</a></div>",
            Variant::Secondary.class(),
            t(tr(Msg::Cancel)),
        )),
    ]);
    Html::join([
        trail(
            (tr(Msg::StoredQueries), "/configuration/stored-queries"),
            tr(Msg::StoreANewVersion),
        ),
        head(
            ("query-title", tr(Msg::StoreANewVersion)),
            (
                tr(Msg::GatewayStoresOnceEveryReplicaServes),
                "console-managed-resources",
            ),
            Html::empty(),
        ),
        form("fui-stack", body),
    ])
}
