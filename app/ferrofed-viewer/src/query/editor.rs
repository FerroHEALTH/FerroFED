// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The editor card: the query, as AQL or a stored query, its parameters,
//! its window and the federation options the gateway declares.
//!
//! The answer renders on the page that asked, so the form runs a query only
//! once the bundle has hydrated it: its submit button stays disabled until
//! then, and the server function refuses a plain form post before it asks
//! the gateway anything ([`super::run::run_query`]). A submission dispatches
//! the action with the fields read from the form's own data, which keeps the
//! parsing of the URL-encoded form out of the bundle.

use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use super::tabs::{TabPair, tab_list};
use crate::components::icon;
use crate::query::model::{QueryForm, QueryOptionsView};
use crate::query::run::RunQuery;
use crate::styles;

/// The name of each parameter row's name field, which the form posts once
/// per row.
const PARAMETER_NAME: &str = "parameter-name";

/// The name of each parameter row's value field.
const PARAMETER_VALUE: &str = "parameter-value";

/// What a parameter's name may be: an AQL parameter name, with or without
/// its `$` (AQL, Parameters).
const NAME_PATTERN: &str = r"\$?[A-Za-z0-9_]+";

/// How many parameter rows the editor holds.
const PARAMETER_ROWS: u32 = 8;

/// The editor card, with the choices `offered` names.
pub(super) fn editor_section(
    action: ServerAction<RunQuery>,
    offered: &QueryOptionsView,
) -> AnyView {
    let form = NodeRef::<leptos::html::Form>::new();
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let query = leptos_router::hooks::use_query_map();
    let chosen = move |key: &str| query.read().get(key).unwrap_or_default();
    // NOTE: no specification governs this: our own design; a stored query's detail
    // opens the console with a full load, so the tab starts from the URL it names.
    let stored = RwSignal::new(query.with_untracked(|query| {
        query
            .get(crate::views::stored::CONSOLE_NAME)
            .is_some_and(|name| !name.is_empty())
    }));
    let submit = move |event: leptos::ev::SubmitEvent| {
        let data = form
            .get_untracked()
            .and_then(|form| leptos::web_sys::FormData::new_with_form(&form).ok());
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post, which the server refuses.
        if let Some(data) = data {
            event.prevent_default();
            action.dispatch(RunQuery {
                form: submitted(&data),
            });
        }
    };
    let kinds = tab_list(
        TabPair {
            label: "Query kind",
            tabs: ["query-tab-aql", "query-tab-stored"],
            panels: ["query-panel-aql", "query-panel-stored"],
            names: ["AQL", "Stored query"],
        },
        stored,
    )
    .into_any();
    let query = query_section(stored, chosen);
    let parameters = parameters_section();
    let window = window_section();
    let federation = federation_section(offered);
    view! {
        <form
            method="post"
            action=RunQuery::url()
            node_ref=form
            on:submit=submit
            class=format!("{} query-editor", styles::CARD)
        >
            <input
                type="hidden"
                name="form[kind]"
                value=move || if stored.get() { "stored" } else { "aql" }
            />
            <div class="grid lg:grid-cols-[minmax(0,1fr)_24rem]">
                <div class="min-w-0 px-5 pt-3 pb-5">{kinds} {query}</div>
                <div class="min-w-0 border-t border-line p-5 lg:border-t-0 lg:border-l">
                    {parameters} {window} {federation}
                </div>
            </div>
            <div class="run-bar">
                <button type="submit" disabled=move || !hydrated.get() class=styles::BUTTON_PRIMARY>
                    {icon(icondata_lu::LuPlay, "16")}
                    "Run query"
                </button>
                <p role="status" class=styles::HELP>
                    {move || {
                        if hydrated.get() {
                            ""
                        } else {
                            "The form is ready once the page has loaded."
                        }
                    }}
                </p>
            </div>
        </form>
    }
    .into_any()
}

/// The query `data` holds, by the field names the form posts: the parameter
/// rows as `name=value` lines, and each ticked target in one list.
fn submitted(data: &leptos::web_sys::FormData) -> QueryForm {
    // NOTE: XHR standard, `FormData.get`: a field the form does not send, a
    // clear checkbox among them, is null, which is the empty field it means.
    let field = |name: &str| data.get(name).as_string().unwrap_or_default();
    let all = |name: &str| -> Vec<String> {
        data.get_all(name)
            .iter()
            .filter_map(|value| value.as_string())
            .collect()
    };
    let names = all(PARAMETER_NAME);
    let values = all(PARAMETER_VALUE);
    let parameters: Vec<String> = names
        .iter()
        .zip(&values)
        .filter(|(name, value)| !name.trim().is_empty() || !value.trim().is_empty())
        .map(|(name, value)| format!("{}={value}", name.trim()))
        .collect();
    QueryForm {
        kind: field("form[kind]"),
        aql: field("form[aql]"),
        name: field("form[name]"),
        version: field("form[version]"),
        parameters: parameters.join("\n"),
        offset: field("form[offset]"),
        fetch: field("form[fetch]"),
        partial: field("form[partial]"),
        dedup: field("form[dedup]"),
        endpoints: all("form[endpoints]").join(", "),
        organisation: all("form[organisation]").join(", "),
    }
}

/// The two panels of the query kind tabs: the AQL text, or a stored query
/// by name and version.
///
/// A link from a stored query's detail names the query and its version in
/// the URL, never its AQL or a parameter, and `chosen` reads them there
/// ([`crate::views::stored::console_href`]).
fn query_section(
    stored: RwSignal<bool>,
    chosen: impl Fn(&str) -> String + Copy + Send + Sync + 'static,
) -> AnyView {
    view! {
        <div
            role="tabpanel"
            id="query-panel-aql"
            aria-labelledby="query-tab-aql"
            hidden=move || stored.get()
            class="pt-4"
        >
            <label for="query-aql">"AQL"</label>
            <textarea
                id="query-aql"
                name="form[aql]"
                rows="14"
                autocomplete="off"
                spellcheck="false"
                placeholder="SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c"
                aria-describedby="query-aql-help"
                class="query-aql"
            ></textarea>
            <p id="query-aql-help" class=format!("{} mt-1", styles::HELP)>
                "Name a patient as a parameter, never in the AQL text."
            </p>
        </div>
        <div
            role="tabpanel"
            id="query-panel-stored"
            aria-labelledby="query-tab-stored"
            hidden=move || !stored.get()
            class="pt-4"
        >
            <div class="grid gap-4 sm:grid-cols-[minmax(0,2fr)_minmax(0,1fr)]">
                <div class=styles::FIELD>
                    <label for="query-name">"Stored query name"</label>
                    <input
                        type="text"
                        id="query-name"
                        name="form[name]"
                        autocomplete="off"
                        spellcheck="false"
                        placeholder="org.example::vital_signs"
                        value=move || chosen(crate::views::stored::CONSOLE_NAME)
                        class=styles::MONO
                    />
                </div>
                <div class=styles::FIELD>
                    <label for="query-version">"Version (optional)"</label>
                    <input
                        type="text"
                        id="query-version"
                        name="form[version]"
                        autocomplete="off"
                        placeholder="1.0.0"
                        value=move || chosen(crate::views::stored::CONSOLE_VERSION)
                        class=styles::MONO
                    />
                </div>
            </div>
        </div>
    }
    .into_any()
}

/// The parameters: name and value rows, one shown to start with, and a
/// button that shows the next, up to [`PARAMETER_ROWS`].
///
/// Every row is in the page from the start and a row not yet added is
/// hidden, so the browser keeps no list of rows of its own; a row left
/// empty is not sent.
fn parameters_section() -> AnyView {
    let shown = RwSignal::new(1_u32);
    let add = move |_| shown.update(|shown| *shown = shown.saturating_add(1).min(PARAMETER_ROWS));
    let full = move || shown.get() == PARAMETER_ROWS;
    let rows = (0..PARAMETER_ROWS)
        .map(|index| parameter_row(index, shown))
        .collect_view();
    view! {
        <fieldset class=styles::GROUP>
            <legend>"Parameters"</legend>
            <ul class="m-0 flex list-none flex-col gap-2 p-0">{rows}</ul>
            <button
                type="button"
                class=format!("{} mt-2", styles::BUTTON_SECONDARY)
                disabled=full
                on:click=add
            >
                {icon(icondata_lu::LuPlus, "16")}
                "Add parameter"
            </button>
            <p id="query-parameters-help" class=format!("{} mt-2", styles::HELP)>
                "Name a patient here. Each value is sent as a string, in the request body."
            </p>
        </fieldset>
    }
    .into_any()
}

/// The parameter row `index`, hidden until `shown` counts it.
fn parameter_row(index: u32, shown: RwSignal<u32>) -> impl IntoView {
    let name_id = format!("query-parameter-name-{index}");
    let value_id = format!("query-parameter-value-{index}");
    let waiting = move || index >= shown.get();
    view! {
        <li class="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)] gap-2" hidden=waiting>
            <div class="flex min-w-0 flex-col gap-1">
                <label for=name_id.clone() class="text-xs text-muted">
                    "Name"
                </label>
                <input
                    type="text"
                    id=name_id
                    name=PARAMETER_NAME
                    autocomplete="off"
                    spellcheck="false"
                    pattern=NAME_PATTERN
                    title="Letters, digits and underscores"
                    class=format!("{} w-full", styles::MONO)
                />
            </div>
            <div class="flex min-w-0 flex-col gap-1">
                <label for=value_id.clone() class="text-xs text-muted">
                    "Value"
                </label>
                <input
                    type="text"
                    id=value_id
                    name=PARAMETER_VALUE
                    autocomplete="off"
                    spellcheck="false"
                    aria-describedby="query-parameters-help"
                    class=format!("{} w-full", styles::MONO)
                />
            </div>
        </li>
    }
}
/// The ITS-REST window of the answer, `offset` and `fetch`, on one row.
fn window_section() -> AnyView {
    view! {
        <div class="mt-5 flex flex-wrap gap-3">
            <div class="flex w-28 flex-col gap-1">
                <label for="query-offset">"Offset"</label>
                <input
                    type="text"
                    id="query-offset"
                    name="form[offset]"
                    inputmode="numeric"
                    autocomplete="off"
                />
            </div>
            <div class="flex w-28 flex-col gap-1">
                <label for="query-fetch">"Fetch"</label>
                <input
                    type="text"
                    id="query-fetch"
                    name="form[fetch]"
                    inputmode="numeric"
                    autocomplete="off"
                />
            </div>
        </div>
    }
    .into_any()
}

/// The federation headers a client may send, behind one disclosure:
/// targeting, dedup and the completeness opt-in (§8.4, §10, §11.4).
fn federation_section(offered: &QueryOptionsView) -> AnyView {
    let endpoints = chips(Chips {
        legend: "Endpoints",
        prefix: "query-endpoint",
        name: "form[endpoints]",
        ids: &offered.endpoints,
        none: "The gateway declares no member endpoint.",
    });
    let organisations = chips(Chips {
        legend: "Organisations",
        prefix: "query-organisation",
        name: "form[organisation]",
        ids: &offered.organisations,
        none: "The gateway declares no organisation.",
    });
    let modes = offered
        .dedup_modes
        .iter()
        .map(|mode| {
            let label = mode.clone();
            view! { <option value=mode.clone()>{label}</option> }
        })
        .collect_view();
    let (disabled, hint) = if offered.best_effort {
        (
            false,
            "Answer with the rows that came back when a node does not answer.",
        )
    } else {
        (true, "This gateway does not offer best effort.")
    };
    view! {
        <details class="federation-options">
            <summary>{icon(icondata_lu::LuSlidersHorizontal, "16")} "Federation options"</summary>
            <div class="flex flex-col gap-4 pt-3">
                {endpoints} {organisations} <div class="flex flex-col gap-1">
                    <label for="query-dedup">"Deduplication"</label>
                    <select id="query-dedup" name="form[dedup]">
                        <option value="">"The gateway's default"</option>
                        {modes}
                    </select>
                </div> <div class="flex items-start gap-3">
                    <input
                        type="checkbox"
                        role="switch"
                        id="query-partial"
                        name="form[partial]"
                        value="partial"
                        class="switch"
                        disabled=disabled
                        aria-describedby="query-partial-help"
                    />
                    <div class="min-w-0">
                        <label for="query-partial">"Best effort"</label>
                        <p id="query-partial-help" class=styles::HELP>
                            {hint}
                        </p>
                    </div>
                </div>
            </div>
        </details>
    }
    .into_any()
}

/// One group of target chips.
#[derive(Clone, Copy)]
struct Chips<'a> {
    /// What the group names.
    legend: &'static str,
    /// The start of each chip's id.
    prefix: &'static str,
    /// The field name each chip posts.
    name: &'static str,
    /// What the gateway declares, one chip each.
    ids: &'a [String],
    /// What the group says when the gateway declares none.
    none: &'static str,
}

/// The chips of `group`: a checkbox and its label for each identifier the
/// gateway declares, which the form posts as the targeting header (§8.4).
fn chips(group: Chips<'_>) -> AnyView {
    let help = if group.ids.is_empty() {
        group.none
    } else {
        "Leave all clear to let the gateway choose."
    };
    let chips = group
        .ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let input = format!("{}-{index}", group.prefix);
            let label = input.clone();
            view! {
                <li>
                    <label for=label class="chip">
                        <input type="checkbox" id=input name=group.name value=id.clone() />
                        <span class=styles::MONO>{id.clone()}</span>
                    </label>
                </li>
            }
        })
        .collect_view();
    view! {
        <fieldset class=styles::GROUP>
            <legend>{group.legend}</legend>
            <ul class="m-0 flex list-none flex-wrap gap-2 p-0">{chips}</ul>
            <p class=styles::HELP>{help}</p>
        </fieldset>
    }
    .into_any()
}
