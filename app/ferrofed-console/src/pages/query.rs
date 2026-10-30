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

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{Body, Column, DataTable, Sorting, cell, data_table_wide};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::fanout::{Answer, Fanout, FanoutNode, fanout};
use ferrohealth_ui::view::form::{Field, INPUT, SELECT, check, columns, fieldset, keyed_check};
use ferrohealth_ui::view::repeat::{Fields, ROW, Repeat, RepeatAdd, RepeatRow, RowEnd, repeat};
use ferrohealth_ui::view::surface::{
    Header, SectionHead, disclosure, page_heading, panel_section, panel_sections_card,
};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::template::list::HeadingCount;
use openehr_federation::status::EndpointStatus;

use crate::act::fields::Fields as Posted;
use crate::act::query::{
    ADD, PARAMETER_NAME, PARAMETER_VALUE, ParameterRow, QueryAnswer, QueryForm, QueryOptions,
    QueryPost, REMOVE, Ran, succeeded,
};
use crate::app::QUERY;
use crate::editors::{AQL, aql_editor};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{fault_view, h, opt, refused_view, t};
use crate::read::Stop;

/// The id of the query form.
const FORM: &str = "query-form";

/// The path of the Stored query tab.
pub const STORED_TAB: &str = "/query/stored";

/// A key no row of `rows` holds.
#[must_use]
pub fn fresh_key(rows: &[ParameterRow]) -> String {
    ferrohealth_ui::view::repeat::fresh_key(|key| rows.iter().any(|row| row.key == key))
}

/// The query console, on the tab the path names, with what the form
/// `posted` to it gave.
pub async fn page(posted: Option<Posted>) -> Html {
    let stored = crate::pages::param("tab") == "stored";
    if !stored && !crate::pages::param("tab").is_empty() {
        return crate::pages::errors::not_found();
    }
    let fresh = {
        let mut form = QueryForm {
            kind: String::from(if stored { "stored" } else { "aql" }),
            name: crate::pages::query_param("stored").unwrap_or_default(),
            version: crate::pages::query_param("version").unwrap_or_default(),
            ..QueryForm::default()
        };
        form.parameters.push(ParameterRow {
            key: String::from("1"),
            ..ParameterRow::default()
        });
        form
    };
    // NOTE: no specification governs this: our own design; the answer carries the form as
    // the server read it, so the page draws the form from the answer, not the posted fields.
    let (form, ran) = match posted {
        Some(fields) => match crate::act::query::run_query(&fields).await {
            Ok(QueryPost { form, ran }) => (form, ran),
            Err(stop) => (fresh, Some(Ran::from(stop))),
        },
        None => (fresh, None),
    };
    let options = options_now().await.unwrap_or_default();
    crate::app::title(tr(Msg::QueryTitle)).then(console(&form, stored, &options, ran))
}

/// What the gateway's self-description offers a query, as the operator of
/// the request reads it; nothing when it cannot be read.
///
/// # Errors
/// Returns the [`Stop`] of a missing session.
pub async fn options_now() -> Result<QueryOptions, Stop> {
    let (state, token) = crate::read::signed_in()?;
    Ok(state
        .gateway()
        .self_description(&token)
        .await
        .ok()
        .map(|description| QueryOptions::of(&description))
        .unwrap_or_default())
}

/// The console with `form` filled in, on the Stored query tab when
/// `stored`, offering `options`, with what the run `ran` gave. The form
/// posts to the tab it sits on.
fn console(form: &QueryForm, stored: bool, options: &QueryOptions, ran: Option<Ran>) -> Html {
    let locale = current();
    let heading = page_heading(
        &locale,
        Header {
            title: tr(Msg::QueryTitle).into(),
            subtitle: Some(tr(Msg::QuerySubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let query = if stored {
        stored_fields(form)
    } else {
        aql_field(form)
    };
    let rows = parameters(form);
    let window = window(form);
    let federation = federation_options(form, options);
    let action = if stored { STORED_TAB } else { QUERY };
    let editor = h(format!(
        "<form method=\"post\" action=\"{action}\" id=\"{FORM}\" class=\"fui-form fui-stack\">\
         <input type=\"hidden\" name=\"kind\" value=\"{}\">{query}{rows}{window}{federation}\
         <div class=\"fui-actions\"><button type=\"submit\" class=\"{}\">{}</button></div></form>",
        if stored { "stored" } else { "aql" },
        Variant::Primary.class(),
        label(Some(Glyph::PLAY), tr(Msg::QueryRun)),
    ));
    let tabs = binder(
        &locale,
        tr(Msg::QueryTitle),
        vec![
            Tab::new("aql", QUERY, tr(Msg::QueryTabAql)),
            Tab::new("stored", STORED_TAB, tr(Msg::QueryTabStored)),
        ],
        if stored { "stored" } else { "aql" },
        editor,
    );
    let answer = match ran {
        None => h(format!(
            "<p class=\"fui-muted\">{}</p>",
            t(tr(Msg::QueryBeforeRun))
        )),
        Some(Ran::Refused(refused)) => refused_view(&refused),
        Some(Ran::Fault(failed)) => fault_view(&failed),
        Some(Ran::Answered(answer)) => answered(&answer),
    };
    heading.then(h(format!(
        "<div class=\"fui-stack\">{tabs} <section aria-labelledby=\"query-answer-title\" class=\"fui-stack\">\
         <h2 id=\"query-answer-title\" class=\"fui-sr-only\">{}</h2>{answer}</section></div>",
        t(tr(Msg::QueryAnswerTitle))
    )))
}

/// The AQL editor.
fn aql_field(form: &QueryForm) -> Html {
    Field::new(AQL, tr(Msg::QueryAqlLabel))
        .help(tr(Msg::QueryAqlHelp))
        .view(aql_editor(form.aql.clone()))
}

/// A text input `id`, named `name`, holding `value`.
fn input(id: &str, name: &str, value: &str, extra: &str) -> Html {
    h(format!(
        "<input type=\"text\" id=\"{}\" name=\"{}\" class=\"{INPUT}\" value=\"{}\"{extra}>",
        t(id),
        t(name),
        t(value)
    ))
}

/// The stored query's name and version.
fn stored_fields(form: &QueryForm) -> Html {
    let name = Field::new("query-name", tr(Msg::QueryNameLabel))
        .help(tr(Msg::QueryNameHelp))
        .view(input(
            "query-name",
            "name",
            &form.name,
            " autocomplete=\"off\" spellcheck=\"false\"",
        ));
    let version = Field::new("query-version", tr(Msg::QueryVersionLabel))
        .help(tr(Msg::QueryVersionHelp))
        .view(input(
            "query-version",
            "version",
            &form.version,
            " autocomplete=\"off\"",
        ));
    columns(name.then(version))
}

/// One parameter row, its fields keyed by `key`.
fn parameter_row(key: &str, row: &ParameterRow) -> RepeatRow {
    let name_id = format!("{PARAMETER_NAME}{key}");
    let value_id = format!("{PARAMETER_VALUE}{key}");
    let fields = Field::new(name_id.clone(), tr(Msg::QueryParameterNameLabel))
        .label_hidden()
        .view(input(
            &name_id,
            &name_id,
            &row.name,
            " autocomplete=\"off\" spellcheck=\"false\"",
        ))
        .then(
            Field::new(value_id.clone(), tr(Msg::QueryParameterValueLabel))
                .label_hidden()
                .view(input(
                    &value_id,
                    &value_id,
                    &row.value,
                    " autocomplete=\"off\"",
                )),
        );
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
fn parameters(form: &QueryForm) -> Html {
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
        h(format!(
            "<p class=\"fui-help\">{}</p>{list}",
            t(tr(Msg::QueryParametersHelp))
        )),
    )
}

/// The row window: the first row and how many.
fn window(form: &QueryForm) -> Html {
    let offset = Field::new("query-offset", tr(Msg::QueryOffset)).view(input(
        "query-offset",
        "offset",
        &form.offset,
        " inputmode=\"numeric\"",
    ));
    let fetch = Field::new("query-fetch", tr(Msg::QueryFetch)).view(input(
        "query-fetch",
        "fetch",
        &form.fetch,
        " inputmode=\"numeric\"",
    ));
    columns(offset.then(fetch))
}

/// A check box `id` named `name` with the value `value`, checked when
/// `checked`.
fn checkbox(id: &str, name: &str, value: &str, checked: bool) -> Html {
    h(format!(
        "<input type=\"checkbox\" id=\"{}\" name=\"{}\" value=\"{}\"{}>",
        t(id),
        t(name),
        t(value),
        if checked { " checked" } else { "" }
    ))
}

/// A group of check boxes named `name`, one per value of `values`, those in
/// `chosen` checked.
fn checks(name: &'static str, values: &[String], chosen: &[String]) -> Html {
    // NOTE: no specification governs this: our own design; a box is one of many under one
    // name, so the field is named with `[]` (`crate::act::fields`).
    let field = format!("{name}[]");
    Html::join(values.iter().map(|value| {
        let id = format!("query-{name}-{value}");
        let checked = chosen.contains(value);
        keyed_check(
            &id,
            value.clone(),
            checkbox(&id, &field, value, checked),
        )
    }))
}

/// The federation options the gateway declares, behind a disclosure.
fn federation_options(form: &QueryForm, options: &QueryOptions) -> Html {
    let dedup = (!options.dedup_modes.is_empty()).then(|| {
        let modes = Html::join(options.dedup_modes.iter().map(|mode| {
            let chosen = form.dedup == *mode;
            h(format!(
                "<option value=\"{mode}\"{}>{mode}</option>",
                if chosen { " selected" } else { "" },
                mode = t(mode),
            ))
        }));
        Field::new("query-dedup", tr(Msg::QueryDedup)).view(h(format!(
            "<select id=\"query-dedup\" name=\"dedup\" class=\"{SELECT}\"><option value=\"\">{}</option>{modes}</select>",
            t(tr(Msg::QueryDedupDefault))
        )))
    });
    let partial = options.best_effort.then(|| {
        let checked = !form.partial.trim().is_empty();
        check(
            "query-partial",
            tr(Msg::QueryPartial),
            checkbox("query-partial", "partial", "on", checked),
        )
    });
    let body = h(format!(
        "<div class=\"fui-stack\">{}{} {} {}</div>",
        fieldset(
            tr(Msg::QueryEndpoints),
            h(format!(
                "<p class=\"fui-help\">{}</p>{}",
                t(tr(Msg::QueryEndpointsHelp)),
                checks("endpoint", &options.endpoints, &form.endpoints)
            )),
        ),
        fieldset(
            tr(Msg::QueryOrganisations),
            checks("organisation", &options.organisations, &form.organisations),
        ),
        opt(dedup),
        opt(partial),
    ));
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
    disclosure(summary, body)
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
pub fn outcome(endpoints: &[crate::act::query::EndpointLine], status: u16) -> Html {
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
    fanout(
        &current(),
        &Fanout {
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
        },
    )
}

/// The answer the gateway gave: the fan-out outcome, the rows and the raw
/// result set.
fn answered(answer: &QueryAnswer) -> Html {
    let columns: Vec<Column> = answer
        .columns
        .iter()
        .enumerate()
        .map(|(at, name)| Column::owned(format!("c{at}"), name.clone()))
        .collect();
    let count = u64::try_from(answer.rows.len()).unwrap_or(u64::MAX);
    let rows = Html::join(answer.rows.iter().map(|row| {
        let cells = Html::join(
            row.iter()
                .zip(columns.iter())
                .map(|(value, column)| cell(column, t(value))),
        );
        h(format!("<tr>{cells}</tr>"))
    }));
    let table = if answer.rows.is_empty() {
        h(format!(
            "<p class=\"fui-panel-body fui-muted\">{}</p>",
            t(tr(if succeeded(answer.status) {
                Msg::QueryNoRows
            } else {
                Msg::QueryNoRowsFailed
            }))
        ))
    } else {
        data_table_wide(DataTable {
            caption: &tr(Msg::QueryRows),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })
    };
    let locale = current();
    let raw = code_section(
        &locale,
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
    outcome(&answer.endpoints, answer.status).then(panel_sections_card(
        tr(Msg::QueryAnswerTitle),
        panel_section(
            &locale,
            Some(SectionHead {
                title: tr(Msg::QueryRows),
                count: Some(HeadingCount::All(count)),
                actions: None,
            }),
            opt(window.map(|window| {
                h(format!(
                    "<p class=\"fui-small fui-muted fui-panel-body\">{}</p>",
                    t(window)
                ))
            }))
            .then(table),
        )
        .then(panel_section(&locale, None, disclosure(tr(Msg::QueryRaw), raw))),
    ))
}
