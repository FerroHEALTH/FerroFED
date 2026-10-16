// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Stored queries tab: the stored versions grouped by name, and the
//! form that stores a new version, which is immutable once stored (§12.7,
//! N44).

use ferrofed_registry::console::{QueryRegistry, QueryResource};
use leptos::prelude::*;

use crate::configure::render::Loaded;
use crate::configure::render::members::{GROUP, GROUP_TITLE, owner};
use crate::configure::render::parts::{Text, confirmed, error_line, form, head, hidden, text};
use crate::styles::{self, Button};

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

/// The Stored queries tab.
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let registry = loaded.resources.stored_query_registry;
    let store = (registry == QueryRegistry::Writable).then(|| {
        view! {
            <a href="/configuration/stored-queries/new" class=styles::BUTTON_PRIMARY>
                {crate::components::icon(icondata_lu::LuPlus, "16")}
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
            <p class="m-0 text-muted">"The gateway offers no stored-query registry."</p>
        }
        .into_any();
    }
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        view! {
            <p class="mt-0 text-sm" role="note">
                "The stored queries come from the read-only definition files: change the files."
            </p>
        }
    });
    let groups = grouped(&loaded.resources.stored_queries);
    let rows = if groups.is_empty() {
        view! {
            <tr>
                <td colspan="3" class="text-muted">
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
                    .map(|query| {
                        view! { <span class="badge tone-neutral font-mono">{query.version.clone()}</span> }
                    })
                    .collect_view();
                let held = versions.first().map(|query| owner(query.owner));
                view! {
                    <tr>
                        <th scope="row">
                            <code>{name.to_owned()}</code>
                        </th>
                        <td>
                            <div class="flex flex-wrap gap-1">{badges}</div>
                        </td>
                        <td>{held}</td>
                    </tr>
                }
            })
            .collect_view()
            .into_any()
    };
    view! {
        {header}
        {read_only}
        <div class=styles::TABLE_SCROLL tabindex="0" role="region" aria-labelledby="queries-title">
            <table>
                <caption class="sr-only">"Stored-query versions by name"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Name"</th>
                        <th scope="col">"Versions"</th>
                        <th scope="col">"Owner"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
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
            <legend class=GROUP_TITLE>"Version"</legend>
            <div class="grid grid-cols-1 gap-x-4 sm:grid-cols-2">
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
            <legend class=GROUP_TITLE>"AQL"</legend>
            <label for="cfg-aql" class="sr-only">
                "AQL"
            </label>
            <textarea
                id="cfg-aql"
                name="form[aql]"
                rows="14"
                spellcheck="false"
                required
                class="min-h-64 w-full"
                aria-describedby="cfg-aql-error"
            ></textarea>
            {error_line("cfg-aql")}
        </fieldset>
        <div class="flex flex-wrap items-center gap-3 border-t border-line pt-4">
            {confirmed(
                String::from("store-confirm"),
                (String::from("Store the version"), Button::Primary),
                (
                    String::from(
                        "Store this version? A stored version cannot be changed or removed afterwards.",
                    ),
                    "Store",
                ),
                (),
            )} <a href="/configuration/stored-queries" class=styles::BUTTON_SECONDARY>
                "Cancel"
            </a>
        </div>
    };
    view! {
        <p class="mt-0 mb-2 text-sm">
            <a href="/configuration/stored-queries">"Stored queries"</a>
        </p>
        {head(
            ("query-title", String::from("Store a new version")),
            (
                "The gateway stores it at once and every replica serves it.",
                "console-managed-resources",
            ),
            (),
        )}
        {form("main", "", body)}
    }
    .into_any()
}
