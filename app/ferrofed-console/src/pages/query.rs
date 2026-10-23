// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console: an AQL query, or a stored query by name, run through
//! the gateway as the operator.
//!
//! Binder tabs choose AQL or a stored query; under them sit the editor or
//! the name and version, the parameters as repeatable rows (F7), the row
//! window and the federation options behind a disclosure, and Run. The
//! form posts to this page, which answers with the fan-out outcome (F5),
//! every endpoint's part said in words, an incomplete answer said before
//! its rows (§11.4), the rows, and the raw result set. Before a run the
//! answer is one quiet line.

use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Sorting, cell, data_table_wide};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::fanout::{Answer, Fanout, FanoutNode, fanout};
use ferrohealth_ui::view::form::{Field, INPUT, SELECT, check, columns, fieldset, keyed_check};
use ferrohealth_ui::view::surface::{
    Header, SectionHead, disclosure, page_heading, panel_section, panel_sections_card,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;
use ferrohealth_ui::widget::repeat::{Fields, ROW, Repeat, RepeatAdd, RepeatRow, RowEnd, repeat};
use leptos::prelude::*;
use openehr_federation::status::EndpointStatus;

use crate::act::query::{
    ADD, PARAMETER_NAME, PARAMETER_VALUE, ParameterRow, QueryAnswer, QueryForm, QueryOptions,
    QueryPost, REMOVE, Ran, succeeded,
};
use crate::app::QUERY;
use crate::islands::{AQL, AqlEditor};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::{fault_view, refused_view};

/// The id of the query form.
const FORM: &str = "query-form";

/// The path of the Stored query tab.
pub const STORED_TAB: &str = "/query/stored";

/// A key no row of `rows` holds.
#[must_use]
pub fn fresh_key(rows: &[ParameterRow]) -> String {
    ferrohealth_ui::widget::repeat::fresh_key(|key| rows.iter().any(|row| row.key == key))
}

/// The query console, on the tab the path names.
#[must_use]
pub fn page() -> AnyView {
    let stored = crate::pages::param("tab") == "stored";
    if !stored && !crate::pages::param("tab").is_empty() {
        return crate::pages::errors::not_found();
    }
    let posted = use_context::<QueryPost>();
    let mut form = posted
        .as_ref()
        .map(|post| post.form.clone())
        .unwrap_or_default();
    if posted.is_none() {
        form.kind = String::from(if stored { "stored" } else { "aql" });
        form.name = crate::pages::query_param("stored").unwrap_or_default();
        form.version = crate::pages::query_param("version").unwrap_or_default();
        form.parameters.push(ParameterRow {
            key: String::from("1"),
            ..ParameterRow::default()
        });
    }
    let ran = posted.and_then(|post| post.ran);
    let body = Suspend::new(async move {
        let options = match crate::read::signed_in() {
            Ok((state, token)) => state
                .gateway()
                .self_description(&token)
                .await
                .ok()
                .map(|description| QueryOptions::of(&description)),
            Err(_) => None,
        };
        console(&form, stored, &options.unwrap_or_default(), ran)
    });
    view! {
        {crate::app::title(tr(Msg::QueryTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}

/// The console with `form` filled in, on the Stored query tab when
/// `stored`, offering `options`, with what the run `ran` gave.
fn console(form: &QueryForm, stored: bool, options: &QueryOptions, ran: Option<Ran>) -> AnyView {
    let heading = page_heading(
        Header {
            title: tr(Msg::QueryTitle).into(),
            subtitle: Some(tr(Msg::QuerySubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    )
    .into_any();
    let action = if stored { STORED_TAB } else { QUERY };
    let query = if stored {
        stored_fields(form)
    } else {
        aql_field(form)
    };
    let editor = view! {
        <form id=FORM method="post" action=action class="fui-form fui-stack">
            <input type="hidden" name="kind" value=if stored { "stored" } else { "aql" } />
            {query}
            {parameters(form)}
            {window(form)}
            {federation_options(form, options)}
            <div class="fui-actions">
                <button type="submit" class=Variant::Primary.class()>
                    {label(Some(Glyph::PLAY), tr(Msg::QueryRun))}
                </button>
            </div>
        </form>
    }
    .into_any();
    let tabs = binder(
        tr(Msg::QueryTitle),
        vec![
            Tab::new("aql", QUERY, tr(Msg::QueryTabAql)),
            Tab::new("stored", STORED_TAB, tr(Msg::QueryTabStored)),
        ],
        if stored { "stored" } else { "aql" },
        editor,
    );
    let answer = match ran {
        None => view! { <p class="fui-muted">{tr(Msg::QueryBeforeRun)}</p> }.into_any(),
        Some(Ran::Refused(refused)) => refused_view(&refused),
        Some(Ran::Fault(failed)) => fault_view(&failed),
        Some(Ran::Answered(answer)) => answered(&answer),
    };
    view! {
        {heading}
        <div class="fui-stack">
            {tabs} <section aria-labelledby="query-answer-title" class="fui-stack">
                <h2 id="query-answer-title" class="fui-sr-only">
                    {tr(Msg::QueryAnswerTitle)}
                </h2>
                {answer}
            </section>
        </div>
    }
    .into_any()
}

/// The AQL editor.
fn aql_field(form: &QueryForm) -> AnyView {
    let text = form.aql.clone();
    Field::new(AQL, tr(Msg::QueryAqlLabel))
        .help(tr(Msg::QueryAqlHelp))
        .view(view! { <AqlEditor text=text /> })
}

/// The stored query's name and version.
fn stored_fields(form: &QueryForm) -> AnyView {
    let name = Field::new("query-name", tr(Msg::QueryNameLabel))
        .help(tr(Msg::QueryNameHelp))
        .view(view! {
            <input
                type="text"
                id="query-name"
                name="name"
                class=INPUT
                value=form.name.clone()
                autocomplete="off"
                spellcheck="false"
            />
        });
    let version = Field::new("query-version", tr(Msg::QueryVersionLabel))
        .help(tr(Msg::QueryVersionHelp))
        .view(view! {
            <input
                type="text"
                id="query-version"
                name="version"
                class=INPUT
                value=form.version.clone()
                autocomplete="off"
            />
        });
    columns(view! {
        {name}
        {version}
    })
    .into_any()
}

/// One parameter row, its fields keyed by `key`.
fn parameter_row(key: &str, row: &ParameterRow) -> RepeatRow {
    let name_id = format!("{PARAMETER_NAME}{key}");
    let value_id = format!("{PARAMETER_VALUE}{key}");
    let fields = view! {
        {Field::new(name_id.clone(), tr(Msg::QueryParameterNameLabel))
            .label_hidden()
            .view(
                view! {
                    <input
                        type="text"
                        id=name_id.clone()
                        name=name_id
                        class=INPUT
                        value=row.name.clone()
                        autocomplete="off"
                        spellcheck="false"
                    />
                },
            )}
        {Field::new(value_id.clone(), tr(Msg::QueryParameterValueLabel))
            .label_hidden()
            .view(
                view! {
                    <input
                        type="text"
                        id=value_id.clone()
                        name=value_id
                        class=INPUT
                        value=row.value.clone()
                        autocomplete="off"
                    />
                },
            )}
    }
    .into_any();
    RepeatRow {
        key: key.to_owned(),
        fields,
        end: RowEnd::Remove {
            label: tr(Msg::QueryParameterRemove),
            name: String::from(REMOVE),
            value: key.to_owned(),
        },
        after: None,
        heading: None,
    }
}

/// The parameters as repeatable rows.
fn parameters(form: &QueryForm) -> AnyView {
    let rows = form
        .parameters
        .iter()
        .map(|row| parameter_row(&row.key, row))
        .collect();
    let list = repeat(Repeat {
        id: String::from("query-parameters"),
        label: tr(Msg::QueryParameters),
        fields: Fields::Two,
        head: vec![
            tr(Msg::QueryParameterNameLabel),
            tr(Msg::QueryParameterValueLabel),
        ],
        rows,
        token: ROW,
        blank: Some(parameter_row(ROW, &ParameterRow::default())),
        add: Some(RepeatAdd {
            label: tr(Msg::QueryParameterAdd),
            name: String::from(ADD),
            value: String::from("1"),
        }),
        empty: tr(Msg::QueryParametersNone),
        form: None,
    });
    fieldset(
        tr(Msg::QueryParameters),
        view! {
            <p class="fui-help">{tr(Msg::QueryParametersHelp)}</p>
            {list}
        },
    )
    .into_any()
}

/// The row window: the first row and how many.
fn window(form: &QueryForm) -> AnyView {
    let offset = Field::new("query-offset", tr(Msg::QueryOffset)).view(view! {
        <input
            type="text"
            inputmode="numeric"
            id="query-offset"
            name="offset"
            class=INPUT
            value=form.offset.clone()
        />
    });
    let fetch = Field::new("query-fetch", tr(Msg::QueryFetch)).view(view! {
        <input
            type="text"
            inputmode="numeric"
            id="query-fetch"
            name="fetch"
            class=INPUT
            value=form.fetch.clone()
        />
    });
    columns(view! {
        {offset}
        {fetch}
    })
    .into_any()
}

/// A group of check boxes named `name`, one per value of `values`, those in
/// `chosen` checked.
fn checks(name: &'static str, values: &[String], chosen: &[String]) -> AnyView {
    values
        .iter()
        .map(|value| {
            let id = format!("query-{name}-{value}");
            let checked = chosen.contains(value);
            keyed_check(
                id.clone(),
                value.clone(),
                view! { <input type="checkbox" id=id name=name value=value.clone() checked=checked /> },
            )
        })
        .collect_view()
        .into_any()
}

/// The federation options the gateway declares, behind a disclosure.
fn federation_options(form: &QueryForm, options: &QueryOptions) -> AnyView {
    let dedup = (!options.dedup_modes.is_empty()).then(|| {
        let modes = options
            .dedup_modes
            .iter()
            .map(|mode| {
                let chosen = form.dedup == *mode;
                view! {
                    <option value=mode.clone() selected=chosen>
                        {mode.clone()}
                    </option>
                }
            })
            .collect_view();
        Field::new("query-dedup", tr(Msg::QueryDedup)).view(view! {
            <select id="query-dedup" name="dedup" class=SELECT>
                <option value="">{tr(Msg::QueryDedupDefault)}</option>
                {modes}
            </select>
        })
    });
    let partial = options.best_effort.then(|| {
        let checked = !form.partial.trim().is_empty();
        check(
            "query-partial",
            tr(Msg::QueryPartial),
            view! { <input type="checkbox" id="query-partial" name="partial" value="on" checked=checked /> },
        )
    });
    let body = view! {
        <div class="fui-stack">
            {fieldset(
                tr(Msg::QueryEndpoints),
                view! {
                    <p class="fui-help">{tr(Msg::QueryEndpointsHelp)}</p>
                    {checks("endpoint", &options.endpoints, &form.endpoints)}
                },
            )}
            {fieldset(
                tr(Msg::QueryOrganisations),
                checks("organisation", &options.organisations, &form.organisations),
            )} {dedup} {partial}
        </div>
    };
    let summary = if form.endpoints.is_empty() && form.organisations.is_empty() {
        tr(Msg::QueryOptionsEvery)
    } else {
        tr_with(
            Msg::QueryOptionsChosen,
            &[(
                "count",
                Arg::from(form.endpoints.len() + form.organisations.len()),
            )],
        )
    };
    disclosure(summary, body).into_any()
}

/// How an endpoint of `status` answered, and its word (§11.1).
fn status_words(status: EndpointStatus) -> (Answer, String) {
    let (answer, msg) = match status {
        EndpointStatus::Active => (Answer::Answered, Msg::StatusActive),
        EndpointStatus::Offline => (Answer::Failed, Msg::StatusOffline),
        EndpointStatus::TimeOut => (Answer::Failed, Msg::StatusTimeOut),
        EndpointStatus::NodeError => (Answer::Failed, Msg::StatusNodeError),
        EndpointStatus::NotResolved => (Answer::Skipped, Msg::StatusNotResolved),
        EndpointStatus::ConsentDenied => (Answer::Skipped, Msg::StatusConsentDenied),
        EndpointStatus::Excluded => (Answer::Skipped, Msg::StatusExcluded),
        EndpointStatus::NotLocalized => (Answer::Skipped, Msg::StatusNotLocalized),
        _ => (Answer::Failed, Msg::StatusOther),
    };
    (answer, tr(msg))
}

/// The fan-out outcome of an answer of `status` whose endpoints did as
/// `endpoints` says: each endpoint's part in words.
#[must_use]
pub fn outcome(endpoints: &[crate::act::query::EndpointLine], status: u16) -> AnyView {
    let nodes: Vec<FanoutNode> = endpoints
        .iter()
        .map(|endpoint| {
            let (state, word) = status_words(endpoint.status);
            let measure = match (endpoint.row_count, endpoint.latency_ms) {
                (Some(rows), Some(ms)) => tr_with(
                    Msg::FanoutRowsMs,
                    &[("rows", Arg::from(rows)), ("ms", Arg::from(ms))],
                ),
                (Some(rows), None) => tr_with(Msg::FanoutRows, &[("rows", Arg::from(rows))]),
                (None, Some(ms)) => tr_with(Msg::Milliseconds, &[("count", Arg::from(ms))]),
                (None, None) => tr(Msg::FanoutNotAskedMeasure),
            };
            FanoutNode {
                name: endpoint.id.clone(),
                answer: state,
                word,
                term: Some(endpoint.status.as_str().to_owned()),
                measure,
                reason: endpoint.error.clone(),
            }
        })
        .collect();
    let asked = nodes
        .iter()
        .filter(|node| node.answer != Answer::Skipped)
        .count();
    let answered = nodes
        .iter()
        .filter(|node| node.answer == Answer::Answered)
        .count();
    let missing: Vec<String> = nodes
        .iter()
        .filter(|node| node.answer == Answer::Failed)
        .map(|node| node.name.clone())
        .collect();
    fanout(Fanout {
        label: tr(Msg::FanoutLabel),
        line: tr_with(
            Msg::FanoutLine,
            &[
                ("answered", Arg::from(answered)),
                ("asked", Arg::from(asked)),
                ("status", Arg::from(u32::from(status))),
            ],
        ),
        missing: tr_with(
            Msg::FanoutMissing,
            &[("endpoints", missing.join(", ").into())],
        ),
        nodes,
    })
}

/// The answer the gateway gave: the fan-out outcome, the rows and the raw
/// result set.
fn answered(answer: &QueryAnswer) -> AnyView {
    let columns: Vec<Column> = answer
        .columns
        .iter()
        .enumerate()
        .map(|(at, name)| Column::owned(format!("c{at}"), name.clone()))
        .collect();
    let count = u64::try_from(answer.rows.len()).unwrap_or(u64::MAX);
    let rows = answer
        .rows
        .iter()
        .map(|row| {
            let cells = row
                .iter()
                .zip(columns.iter())
                .map(|(value, column)| cell(column, value.clone()))
                .collect_view();
            view! { <tr>{cells}</tr> }
        })
        .collect_view()
        .into_any();
    let table = if answer.rows.is_empty() {
        view! {
            <p class="fui-panel-body fui-muted">
                {tr(
                    if succeeded(answer.status) { Msg::QueryNoRows } else { Msg::QueryNoRowsFailed },
                )}
            </p>
        }
        .into_any()
    } else {
        data_table_wide(DataTable {
            caption: &tr(Msg::QueryRows),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
        .into_any()
    };
    let raw = code_section(
        CodeHead {
            title: tr(Msg::QueryRaw),
            tags: vec![String::from("JSON")],
            digest: None,
            copy: None,
            actions: None,
        },
        &Code {
            label: tr(Msg::QueryRaw),
            language: ferrohealth_ui::highlight::Language::Json,
            text: answer.raw.clone(),
            ids: None,
            current: None,
            diagnostics: Vec::new(),
        },
    );
    let window = answer.elapsed_ms.map(|ms| {
        tr_with(
            Msg::QueryWindow,
            &[("count", Arg::from(count)), ("ms", Arg::from(ms))],
        )
    });
    view! {
        {outcome(&answer.endpoints, answer.status)}
        {panel_sections_card(
            tr(Msg::QueryAnswerTitle),
            view! {
                {panel_section(
                    Some(SectionHead {
                        title: tr(Msg::QueryRows),
                        count: Some(HeadingCount::All(count)),
                        actions: None,
                    }),
                    view! {
                        {window
                            .map(|window| {
                                view! { <p class="fui-small fui-muted fui-panel-body">{window}</p> }
                            })}
                        {table}
                    },
                )}
                {panel_section(None, disclosure(tr(Msg::QueryRaw), raw))}
            },
        )}
    }
    .into_any()
}
