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
const QUERY_COLUMNS: [Column; 3] = [
    Column::text("name", "Name"),
    Column::text("versions", "Versions"),
    Column::text("owner", "Owner"),
];

/// The Stored queries tab.
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let registry = loaded.resources.stored_query_registry;
    let store = (registry == QueryRegistry::Writable).then(|| {
        view! {
            <a href="/configuration/stored-queries/new" class=Variant::Primary.class()>
                {icon(Glyph::PLUS, "16")}
                "Store a new version"
            </a>
        }
    });
    let header = head(
        ("queries-title", String::from("Stored queries")),
        (
            "A stored version is immutable: a change is a new version, and none is edited or removed.",
            "console-managed-resources",
        ),
        store,
    );
    if registry == QueryRegistry::Absent {
        return view! {
            {header}
            <p class="fui-muted">"The gateway offers no stored-query registry."</p>
        }
        .into_any();
    }
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        view! {
            <p role="note">
                "The stored queries come from the read-only definition files: change the files."
            </p>
        }
    });
    let groups = grouped(&loaded.resources.stored_queries);
    let rows = if groups.is_empty() {
        view! {
            <tr>
                <td colspan="3" class="fui-muted">
                    "No version is stored yet."
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
                        <td data-label="Versions">
                            <div class="fui-cluster">{badges}</div>
                        </td>
                        <td data-label="Owner">{held}</td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        {header}
        {read_only}
        {crate::views::table("Stored-query versions by name", &QUERY_COLUMNS, rows)}
    }
    .into_any()
}

/// The form that stores a new version, its confirmation saying that the
/// version cannot be changed afterwards.
pub(super) fn form_page(loaded: &Loaded<'_>) -> AnyView {
    let names = grouped(&loaded.resources.stored_queries)
        .into_iter()
        .map(|(name, _)| view! { <option value=name.to_owned()></option> })
        .collect_view();
    let body = view! {
        {hidden("action", String::from("store-query"))}
        <fieldset class=GROUP>
            <legend>"Version"</legend>
            <div class="fui-field-row fui-field-row-halves">
                {text(
                    ("cfg-name", "form[name]"),
                    String::from("Qualified name"),
                    String::new(),
                    Text {
                        required: true,
                        mono: true,
                        list: Some("cfg-names"),
                        hint: Some("A namespace and a name, such as org.example::vital-signs."),
                        ..Text::default()
                    },
                )}
                {text(
                    ("cfg-version", "form[version]"),
                    String::from("Version"),
                    String::new(),
                    Text {
                        required: true,
                        mono: true,
                        hint: Some("major.minor.patch, such as 1.0.0."),
                        ..Text::default()
                    },
                )}
            </div>
            <datalist id="cfg-names">{names}</datalist>
        </fieldset>
        <fieldset class=GROUP>
            <legend>"AQL"</legend>
            <label for="cfg-aql" class="fui-sr-only">
                "AQL"
            </label>
            <textarea
                id="cfg-aql"
                name="form[aql]"
                rows="14"
                spellcheck="false"
                required
                class=format!("{TEXTAREA} fui-mono")
                aria-describedby="cfg-aql-error"
            ></textarea>
            {error_line("cfg-aql")}
        </fieldset>
        <div class="fui-action-bar">
            {confirmed(
                String::from("store-confirm"),
                (String::from("Store the version"), Variant::Primary),
                (
                    String::from(
                        "Store this version? A stored version cannot be changed or removed afterwards.",
                    ),
                    "Store",
                ),
                (),
            )} <a href="/configuration/stored-queries" class=Variant::Secondary.class()>
                "Cancel"
            </a>
        </div>
    };
    view! {
        {trail(("Stored queries", "/configuration/stored-queries"), String::from("Store a new version"))}
        {head(
            ("query-title", String::from("Store a new version")),
            (
                "The gateway stores it at once and every replica serves it.",
                "console-managed-resources",
            ),
            (),
        )}
        {form("main", "fui-stack", body)}
    }
    .into_any()
}
