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

use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::form::{INPUT, SELECT, SWITCH, field, keyed_field};
use ferrohealth_ui::view::layout::{FieldRow, action_bar, field_row, main_aside};
use ferrohealth_ui::view::surface::stack;
use ferrohealth_ui::widget::editor::{Editor, code_editor};
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use super::tabs::{TabPair, tab_list};
use crate::query::model::{QueryForm, QueryOptionsView};
use crate::query::run::RunQuery;

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
    let query = query_section(stored, chosen, hydrated);
    let parameters = parameters_section();
    let window = window_section();
    let federation = federation_section(offered);
    let run_bar = action_bar(view! {
        <button type="submit" disabled=move || !hydrated.get() class=Variant::Primary.class()>
            {label(Some(Glyph::PLAY), "Run query")}
        </button>
        <p role="status" class="fui-help">
            {move || {
                if hydrated.get() {
                    ""
                } else {
                    "The form is ready once the page has loaded."
                }
            }}
        </p>
    });
    let main = view! {
        {kinds}
        {query}
    };
    let aside = stack(view! {
        {parameters}
        {window}
        {federation}
    });
    view! {
        <form
            method="post"
            action=RunQuery::url()
            node_ref=form
            on:submit=submit
            class="fui-card"
        >
            <input
                type="hidden"
                name="form[kind]"
                value=move || if stored.get() { "stored" } else { "aql" }
            />
            {main_aside(main, aside)}
            {run_bar}
        </form>
    }
    .into_any()
}

/// The id and name of the AQL editor's text area.
const AQL: &str = "query-aql";

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
        aql: field(AQL),
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
    hydrated: RwSignal<bool>,
) -> AnyView {
    let aql = RwSignal::new(String::new());
    let editor = code_editor(
        Editor {
            id: AQL,
            language: Language::Aql,
            text: aql.into(),
            marks: Signal::stored(Vec::new()),
            editable: hydrated.into(),
        },
        move |text| aql.set(text),
    );
    view! {
        <div
            role="tabpanel"
            id="query-panel-aql"
            aria-labelledby="query-tab-aql"
            hidden=move || stored.get()
            class="fui-tab-panel"
        >
            {field(
                AQL,
                "AQL",
                Some("Name a patient as a parameter, never in the AQL text."),
                editor,
            )}
        </div>
        <div
            role="tabpanel"
            id="query-panel-stored"
            aria-labelledby="query-tab-stored"
            hidden=move || !stored.get()
            class="fui-tab-panel"
        >
            {field_row(
                FieldRow::NarrowWide,
                view! {
                    {field(
                        "query-version",
                        "Version (optional)",
                        None,
                        view! {
                            <input
                                type="text"
                                id="query-version"
                                name="form[version]"
                                autocomplete="off"
                                placeholder="1.0.0"
                                value=move || chosen(crate::views::stored::CONSOLE_VERSION)
                                class=format!("{INPUT} fui-mono")
                            />
                        },
                    )}
                    {field(
                        "query-name",
                        "Stored query name",
                        None,
                        view! {
                            <input
                                type="text"
                                id="query-name"
                                name="form[name]"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="org.example::vital_signs"
                                value=move || chosen(crate::views::stored::CONSOLE_NAME)
                                class=format!("{INPUT} fui-mono")
                            />
                        },
                    )}
                },
            )}
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
        <fieldset class="fui-fieldset">
            <legend class="fui-label">"Parameters"</legend>
            <ul class="fui-divided">{rows}</ul>
            <div class="fui-cluster">
                <button type="button" class=Variant::Secondary.class() disabled=full on:click=add>
                    {label(Some(Glyph::PLUS), "Add parameter")}
                </button>
            </div>
            <p id="query-parameters-help" class="fui-help">
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
        <li class="fui-field-row fui-field-row-narrow-wide" hidden=waiting>
            {keyed_field(
                name_id.clone(),
                "Name",
                None,
                view! {
                    <input
                        type="text"
                        id=name_id
                        name=PARAMETER_NAME
                        autocomplete="off"
                        spellcheck="false"
                        pattern=NAME_PATTERN
                        title="Letters, digits and underscores"
                        class=format!("{INPUT} fui-mono")
                    />
                },
            )}
            {keyed_field(
                value_id.clone(),
                "Value",
                None,
                view! {
                    <input
                        type="text"
                        id=value_id
                        name=PARAMETER_VALUE
                        autocomplete="off"
                        spellcheck="false"
                        aria-describedby="query-parameters-help"
                        class=format!("{INPUT} fui-mono")
                    />
                },
            )}
        </li>
    }
}

/// The ITS-REST window of the answer, `offset` and `fetch`, on one row.
fn window_section() -> AnyView {
    field_row(
        FieldRow::Halves,
        view! {
            {field(
                "query-offset",
                "Offset",
                Some("The rows to skip. Empty starts at the first row."),
                view! {
                    <input
                        type="text"
                        id="query-offset"
                        name="form[offset]"
                        class=INPUT
                        placeholder="0"
                        inputmode="numeric"
                        autocomplete="off"
                    />
                },
            )}
            {field(
                "query-fetch",
                "Fetch",
                Some("The rows to return. Empty leaves it to the gateway."),
                view! {
                    <input
                        type="text"
                        id="query-fetch"
                        name="form[fetch]"
                        class=INPUT
                        inputmode="numeric"
                        autocomplete="off"
                    />
                },
            )}
        },
    )
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
        <details id="federation-options" class="fui-disclosure">
            <summary>"Federation options"</summary>
            <div class="fui-form">
                {endpoints}
                {organisations}
                {field(
                    "query-dedup",
                    "Deduplication",
                    None,
                    view! {
                        <select id="query-dedup" name="form[dedup]" class=SELECT>
                            <option value="">"The gateway's default"</option>
                            {modes}
                        </select>
                    },
                )}
                <div class="fui-check">
                    <input
                        type="checkbox"
                        role="switch"
                        id="query-partial"
                        name="form[partial]"
                        value="partial"
                        class=SWITCH
                        disabled=disabled
                        aria-describedby="query-partial-help"
                    />
                    <span class="fui-check-text">
                        <label for="query-partial">"Best effort"</label>
                        <span id="query-partial-help" class="fui-help">
                            {hint}
                        </span>
                    </span>
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
                <label for=label class="fui-chip-check">
                    <input type="checkbox" id=input name=group.name value=id.clone() />
                    <span class="fui-mono">{id.clone()}</span>
                </label>
            }
        })
        .collect_view();
    view! {
        <fieldset class="fui-fieldset fui-choice">
            <legend class="fui-label">{group.legend}</legend>
            <div class="fui-chip-checks">{chips}</div>
            <p class="fui-help">{help}</p>
        </fieldset>
    }
    .into_any()
}
