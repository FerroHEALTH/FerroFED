// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Stored queries tab: the stored versions grouped by name, and the
//! form that stores a new version, which is immutable once stored (§12.7,
//! N44).

use ferrofed_registry::console::{QueryRegistry, QueryResource};
use leptos::prelude::*;

use crate::configure::render::Loaded;
use crate::configure::render::members::{GROUP, owner};
use crate::configure::render::parts::{
    Text, confirmed, error_line, form, head, hidden, text, trail,
};
use crate::locale::{Msg, tr};
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
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let registry = loaded.resources.stored_query_registry;
    let store = (registry == QueryRegistry::Writable).then(|| {
        view! {
            <a href="/configuration/stored-queries/new" class=Variant::Primary.class()>
                {icon(Glyph::PLUS, "16")}
                {tr(Msg::StoreANewVersion)}
            </a>
        }
    });
    let header = head(
        ("queries-title", tr(Msg::StoredQueries)),
        (
            tr(Msg::StoredVersionImmutableChangeNewVersion),
            "console-managed-resources",
        ),
        store,
    );
    if registry == QueryRegistry::Absent {
        return view! {
            {header}
            <p class="fui-muted">{tr(Msg::GatewayOffersNoStoredQueryRegistry)}</p>
        }
        .into_any();
    }
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        view! { <p role="note">{tr(Msg::StoredQueriesComeFromReadOnly)}</p> }
    });
    let groups = grouped(&loaded.resources.stored_queries);
    let rows = if groups.is_empty() {
        view! {
            <tr>
                <td colspan="3" class="fui-muted">
                    {tr(Msg::NoVersionStoredYet)}
                </td>
            </tr>
        }
        .into_any()
    } else {
        groups
            .into_iter()
            .map(|(name, versions)| {
                let badges = versions
                    .iter()
                    .map(|query| badge(Tone::Neutral, &query.version))
                    .collect_view();
                let held = versions.first().map(|query| owner(query.owner));
                view! {
                    <tr>
                        <th scope="row">
                            <code>{name.to_owned()}</code>
                        </th>
                        <td data-label=tr(Msg::Versions)>
                            <div class="fui-cluster">{badges}</div>
                        </td>
                        <td data-label=tr(Msg::Owner)>{held}</td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        {header}
        {read_only}
        {crate::pages::table(&tr(Msg::StoredQueryVersionsName), &query_columns(), rows)}
    }
    .into_any()
}

/// The form that stores a new version, its confirmation saying that the
/// version cannot be changed afterwards.
pub(super) fn form_page(loaded: &Loaded<'_>) -> AnyView {
    let posted = loaded
        .drafted(&["store-query"])
        .cloned()
        .unwrap_or_default();
    let names = grouped(&loaded.resources.stored_queries)
        .into_iter()
        .map(|(name, _)| view! { <option value=name.to_owned()></option> })
        .collect_view();
    let body = view! {
        {hidden("action", String::from("store-query"))}
        <fieldset class=GROUP>
            <legend>{tr(Msg::Version)}</legend>
            <div class="fui-field-row fui-field-row-halves">
                {text(
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
                )}
                {text(
                    ("cfg-version", "form[version]"),
                    tr(Msg::Version),
                    posted.version.clone(),
                    Text {
                        required: true,
                        mono: true,
                        hint: Some(tr(Msg::MajorMinorPatchSuch10)),
                        ..Text::default()
                    },
                )}
            </div>
            <datalist id="cfg-names">{names}</datalist>
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::Aql)}</legend>
            <label for="cfg-aql" class="fui-sr-only">
                {tr(Msg::Aql)}
            </label>
            <textarea
                id="cfg-aql"
                name="form[aql]"
                rows="14"
                spellcheck="false"
                required
                class=format!("{TEXTAREA} fui-mono")
                aria-describedby="cfg-aql-error"
            >
                {posted.aql.clone()}
            </textarea>
            {error_line("cfg-aql")}
        </fieldset>
        <div class="fui-actions">
            {confirmed(
                "store-confirm",
                (Glyph::SAVE, tr(Msg::StoreTheVersion), Variant::Primary),
                (tr(Msg::StoreVersionStoredVersionCannotChanged), tr(Msg::Store)),
                (),
            )} <a href="/configuration/stored-queries" class=Variant::Secondary.class()>
                {tr(Msg::Cancel)}
            </a>
        </div>
    };
    view! {
        {trail(
            (tr(Msg::StoredQueries), "/configuration/stored-queries"),
            tr(Msg::StoreANewVersion),
        )}
        {head(
            ("query-title", tr(Msg::StoreANewVersion)),
            (tr(Msg::GatewayStoresOnceEveryReplicaServes), "console-managed-resources"),
            (),
        )}
        {form("fui-stack", body)}
    }
    .into_any()
}
