// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of a federated answer, which the page shows as it
//! stands.
//!
//! The answer renders on the server: its tables need no code in the browser,
//! which shows the HTML the server wrote. Leptos escapes every text and
//! attribute value it renders, and the Content-Security-Policy admits no
//! inline script besides.

use leptos::either::Either;
use leptos::prelude::*;

use crate::components::icon;
use crate::query::model::{
    ColumnLine, EndpointLine, PAGE_ROWS, QueryAnswer, ROWS_TABLE, RenderedAnswer,
};
use crate::styles::{self, Tone};
use crate::views::model::Refusal;

/// The HTML of `answer`.
#[must_use]
pub fn html(answer: &QueryAnswer) -> String {
    answered(answer).to_html()
}

/// What the page shows of a query `refused` kept from running: the
/// gateway's status and code, or what is wrong with the form, as the error
/// state of the results card.
#[must_use]
pub fn refused(refused: &Refusal) -> RenderedAnswer {
    let status = match refused {
        Refusal::Gateway { status, .. } => Some(*status),
        Refusal::NotAuthenticated { .. } => Some(http::StatusCode::UNAUTHORIZED.as_u16()),
        Refusal::Invalid { .. } => None,
    };
    RenderedAnswer {
        status,
        complete: false,
        refused: true,
        rows: 0,
        // NOTE: the Leptos book, `reactivity/working_with_signals`: the icon is a
        // component, which renders under a reactive owner of its own here.
        html: Owner::new().with(|| refusal_state(refused).to_html()),
        raw: String::new(),
    }
}

/// The error state of a refused query: the `refused` pill and the one line
/// that says why, with the gateway's status and code.
fn refusal_state(refused: &Refusal) -> AnyView {
    let line = crate::views::refusal(refused);
    view! {
        <div class="query-refusal">
            <span class="text-danger-fg">{icon(icondata_lu::LuCircleAlert, "18")}</span>
            <span class=Tone::Danger.pill()>"refused"</span>
            {line}
        </div>
    }
    .into_any()
}

/// A federated answer: its summary, its completeness in words, every node,
/// and the rows.
fn answered(answer: &QueryAnswer) -> AnyView {
    let summary = summary_section(answer);
    let outcome = outcome_section(answer);
    let nodes = nodes_section(&answer.endpoints);
    let details = details_section(&answer.endpoints);
    let rows = rows_section(answer);
    view! { <div class="query-answer">{summary} {outcome} {nodes} {details} {rows}</div> }
        .into_any()
}

/// Whether `status` is one a node in scope ends with once it has answered
/// (§11.1, §11.3: `not-resolved` and `consent-denied` are answers).
fn answered_status(status: &str) -> bool {
    matches!(status, "active" | "not-resolved" | "consent-denied")
}

/// Whether `status` is one of a node that was never in scope (§11.1).
fn out_of_scope(status: &str) -> bool {
    matches!(status, "excluded" | "not-localized")
}

/// The tone of a node's §11.1 status: answered, failed, an answer of no
/// rows, or never in scope.
fn node_tone(status: &str) -> Tone {
    match status {
        "active" => Tone::Ok,
        "offline" | "time-out" | "node-error" => Tone::Danger,
        "not-resolved" | "consent-denied" => Tone::Warn,
        _ => Tone::Neutral,
    }
}

/// The strip over the answer: its rows, its completeness, the nodes that
/// answered of those in scope, and the time the gateway took.
fn summary_section(answer: &QueryAnswer) -> AnyView {
    let (tone, word) = if !answer.succeeded {
        (Tone::Danger, format!("failed {}", answer.status))
    } else if answer.complete {
        (Tone::Ok, String::from("complete"))
    } else {
        (Tone::Warn, String::from("partial"))
    };
    let in_scope = answer
        .endpoints
        .iter()
        .filter(|endpoint| !out_of_scope(&endpoint.status))
        .count();
    let replied = answer
        .endpoints
        .iter()
        .filter(|endpoint| answered_status(&endpoint.status))
        .count();
    let nodes = if in_scope == 1 {
        format!("{replied} of 1 node answered")
    } else {
        format!("{replied} of {in_scope} nodes answered")
    };
    let elapsed = answer.elapsed_ms.map(|elapsed| {
        view! {
            <span aria-hidden="true">"·"</span>
            <span class="tabular-nums">{format!("{elapsed} ms")}</span>
        }
    });
    let dedup = answer.dedup.clone().map(|mode| {
        let suppressed = answer
            .suppressed_rows
            .map(|rows| format!(", {rows} rows suppressed"))
            .unwrap_or_default();
        view! {
            <span aria-hidden="true">"·"</span>
            <span>{format!("Deduplication: {mode}{suppressed}.")}</span>
        }
    });
    view! {
        <div class="summary-strip">
            <span class="font-semibold text-ink tabular-nums">{counted(answer.rows.len())}</span>
            <span class=tone.pill()>{word}</span>
            <span aria-hidden="true">"·"</span>
            <span class="tabular-nums">{nodes}</span>
            {elapsed}
            {dedup}
            <span aria-hidden="true">"·"</span>
            {completeness(answer.complete)}
        </div>
    }
    .into_any()
}

/// What the status and `meta.federation.complete` say, in words (§11.4).
fn outcome_section(answer: &QueryAnswer) -> AnyView {
    let status = answer.status;
    // NOTE: §11.4: under all-or-nothing a node not answering is a 504 and a
    // node error a 424; any other failing status gets no reading of its own.
    let why = match status {
        504 => " A node in scope was asked and did not answer, so the gateway returned no rows.",
        424 => " A node in scope answered with an error, so the gateway returned no rows.",
        _ => " The gateway returned no rows.",
    };
    let failed = (!answer.succeeded).then(|| {
        view! {
            <p role="alert">
                <strong>{format!("The gateway failed the query: {status}.")}</strong>
                {why}
                " The nodes below say what each one did."
            </p>
        }
    });
    failed.into_any()
}

/// What `meta.federation.complete` says, in words, for the end of the
/// summary strip, whose pill names it in one word (§11.4, N37).
fn completeness(complete: bool) -> impl IntoView {
    if complete {
        Either::Left(view! {
            <span role="status" class="complete-note">
                "Every node in scope answered."
            </span>
        })
    } else {
        Either::Right(view! {
            <span role="alert" class="incomplete-note">
                "Not every node in scope answered (meta.federation.complete is false), "
                "so these rows are not the whole answer."
            </span>
        })
    }
}

/// One chip per node: its id, its status, its rows and its latency (§9.5,
/// §11.1).
fn nodes_section(endpoints: &[EndpointLine]) -> AnyView {
    let chips = endpoints
        .iter()
        .map(|endpoint| {
            let rows = endpoint.row_count.map(|count| {
                view! { <span class="text-muted tabular-nums">{counted_u64(count)}</span> }
            });
            let latency = endpoint.latency_ms.map(|latency| {
                view! { <span class="text-muted tabular-nums">{format!("{latency} ms")}</span> }
            });
            view! {
                <li class="node-chip">
                    <span class=format!(
                        "{} font-medium text-ink",
                        styles::MONO,
                    )>{endpoint.id.clone()}</span>
                    <span class=node_tone(&endpoint.status).pill()>{endpoint.status.clone()}</span>
                    {rows}
                    {latency}
                </li>
            }
        })
        .collect_view();
    view! {
        <ul class="node-chips" aria-label="Nodes">
            {chips}
        </ul>
    }
    .into_any()
}

/// Every endpoint's record in full, its organisation and error with it,
/// open when a node reported an error.
fn details_section(endpoints: &[EndpointLine]) -> AnyView {
    let open = endpoints.iter().any(|endpoint| endpoint.error.is_some());
    let table = endpoints_table(endpoints);
    view! {
        <details class="endpoint-details" open=open>
            <summary>"Endpoint details"</summary>
            {table}
        </details>
    }
    .into_any()
}

/// The table of `endpoints`, each with its status, latency, rows and
/// error (§9.5, §11.1).
pub(crate) fn endpoints_table(endpoints: &[EndpointLine]) -> AnyView {
    let rows = endpoints
        .iter()
        .map(|endpoint| {
            view! {
                <tr>
                    <th scope="row">{endpoint.id.clone()}</th>
                    <td>{endpoint.status.clone()}</td>
                    <td>
                        {endpoint
                            .latency_ms
                            .map(|latency| format!("{latency} ms"))
                            .unwrap_or_default()}
                    </td>
                    <td>{endpoint.row_count.map(|count| count.to_string()).unwrap_or_default()}</td>
                    <td>{endpoint.organisation.clone().unwrap_or_default()}</td>
                    <td>{endpoint.error.clone().unwrap_or_default()}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class=styles::TABLE_SCROLL tabindex="0">
            <table>
                <caption>"Every endpoint the gateway reports"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Endpoint"</th>
                        <th scope="col">"Status"</th>
                        <th scope="col">"Latency"</th>
                        <th scope="col">"Rows"</th>
                        <th scope="col">"Organisation"</th>
                        <th scope="col">"Error"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
    .into_any()
}

/// The heading of the column at `index`: its `name`, or its `path` when the
/// name is only a position (`#0`) or empty, and `#index` when it has neither
/// (§9.4, ITS-REST `RESULT_SET.columns`).
fn heading(index: usize, column: &ColumnLine) -> String {
    let positional = column.name.is_empty()
        || column
            .name
            .strip_prefix('#')
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()));
    if !positional {
        return column.name.clone();
    }
    match column.path.as_deref().filter(|path| !path.is_empty()) {
        Some(path) => path.to_owned(),
        None if column.name.is_empty() => format!("#{index}"),
        None => column.name.clone(),
    }
}

/// How many rows `count` is, in words.
fn counted(count: usize) -> String {
    if count == 1 {
        String::from("1 row")
    } else {
        format!("{count} rows")
    }
}

/// How many rows a node's `count` is, in words.
fn counted_u64(count: u64) -> String {
    if count == 1 {
        String::from("1 row")
    } else {
        format!("{count} rows")
    }
}

/// The rows, under the columns as the gateway renders them (§9.4), in a box
/// they scroll in under a header that stays, [`PAGE_ROWS`] to a `tbody`, of
/// which the page shows one at a time.
fn rows_section(answer: &QueryAnswer) -> AnyView {
    if answer.rows.is_empty() {
        return view! { <p class="no-rows">"No rows."</p> }.into_any();
    }
    let header = answer
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let title = column.path.clone().unwrap_or_default();
            view! {
                <th scope="col" title=title>
                    {heading(index, column)}
                </th>
            }
        })
        .collect_view();
    let size = usize::try_from(PAGE_ROWS).unwrap_or(usize::MAX);
    let pages = answer
        .rows
        .chunks(size)
        .enumerate()
        .map(|(index, page)| {
            let rows = page
                .iter()
                .map(|row| {
                    let cells = row
                        .iter()
                        .map(|cell| view! { <td>{cell.clone()}</td> })
                        .collect_view();
                    view! { <tr>{cells}</tr> }
                })
                .collect_view();
            if index == 0 {
                Either::Left(view! { <tbody>{rows}</tbody> })
            } else {
                Either::Right(view! { <tbody hidden>{rows}</tbody> })
            }
        })
        .collect_view();
    view! {
        <div class=styles::TABLE_SCROLL role="region" aria-label="Rows" tabindex="0">
            <table id=ROWS_TABLE>
                <caption>{counted(answer.rows.len())}</caption>
                <thead>
                    <tr>{header}</tr>
                </thead>
                {pages}
            </table>
        </div>
    }
    .into_any()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::answered;
    use crate::query::model::{ColumnLine, EndpointLine, QueryAnswer};
    use crate::views::model::Refusal;
    use leptos::prelude::RenderHtml as _;

    fn endpoint(id: &str, status: &str, error: Option<&str>) -> EndpointLine {
        EndpointLine {
            id: id.to_owned(),
            status: status.to_owned(),
            latency_ms: Some(118),
            row_count: None,
            organisation: Some(String::from("Org A")),
            error: error.map(str::to_owned),
        }
    }

    fn answer(status: u16, complete: bool) -> QueryAnswer {
        QueryAnswer {
            status,
            succeeded: status == 200,
            complete,
            endpoints: vec![
                endpoint("node_1", "active", None),
                endpoint("node_2", "time-out", Some("no answer in time")),
            ],
            dedup: Some(String::from("none")),
            suppressed_rows: None,
            columns: vec![ColumnLine {
                name: String::from("composition_id"),
                path: Some(String::from("c/uid/value")),
            }],
            rows: vec![vec![String::from("8849182a::cdr1.rso.nl::1")]],
            elapsed_ms: Some(142),
        }
    }

    #[test]
    fn an_incomplete_answer_says_so_in_words_before_its_rows() {
        let html = answered(&answer(200, false)).to_html();
        let flag = html
            .find("Not every node in scope answered (meta.federation.complete is false)")
            .expect("the flag");
        let rows = html.find("8849182a::cdr1.rso.nl::1").expect("the rows");
        assert!(flag < rows, "{html}");
        assert!(html.contains(r#"role="alert""#), "{html}");
        assert!(html.contains("<td>time-out</td>"), "{html}");
        assert!(html.contains("<td>no answer in time</td>"), "{html}");
        assert!(html.contains("<tbody>"), "{html}");
        assert!(html.contains(r#"<th scope="col""#), "{html}");
    }

    // The answer reaches the page through `inner_html`, so every value a
    // node or the gateway controls must arrive escaped, as text or as a
    // double-quoted attribute value, never as markup: in the summary strip,
    // the node chips, the endpoint details and every page of rows.
    #[test]
    fn hostile_values_in_an_answer_render_escaped() {
        let mut hostile = answer(200, false);
        hostile.columns = vec![ColumnLine {
            name: String::from("</th><script>x</script>"),
            path: Some(String::from(r#"" onmouseover="alert(1)"#)),
        }];
        hostile.rows = (0..30)
            .map(|index| {
                if index % 2 == 0 {
                    vec![String::from("<script>alert(1)</script>")]
                } else {
                    vec![String::from("<img src=x onerror=alert(1)>")]
                }
            })
            .collect();
        hostile.endpoints = vec![EndpointLine {
            id: String::from(r#"<b>node</b>" onfocus="x"#),
            status: String::from("<i>time-out</i>"),
            latency_ms: Some(9),
            row_count: Some(2),
            organisation: Some(String::from("<b>Org</b> & co")),
            error: Some(String::from("<b>late</b> & gone")),
        }];
        hostile.dedup = Some(String::from("<i>mode</i>"));
        let html = answered(&hostile).to_html();
        for escaped in [
            "&lt;script&gt;alert(1)&lt;/script&gt;",
            "&lt;img src=x onerror=alert(1)&gt;",
            "&lt;/th&gt;&lt;script&gt;x&lt;/script&gt;",
            "&quot; onmouseover=&quot;alert(1)",
            "&lt;b&gt;Org&lt;/b&gt; &amp; co",
            "&lt;b&gt;late&lt;/b&gt; &amp; gone",
            "&lt;i&gt;mode&lt;/i&gt;",
            "&lt;b&gt;node&lt;/b&gt;\" onfocus=\"x",
            "&lt;i&gt;time-out&lt;/i&gt;",
        ] {
            assert!(html.contains(escaped), "{escaped}: {html}");
        }
        for raw in [
            "<script",
            "<img",
            "onmouseover=\"",
            "<b>",
            "<i>",
            "</th><script",
        ] {
            assert!(!html.contains(raw), "{raw}: {html}");
        }
        assert_eq!(1, html.matches("<tbody hidden>").count(), "{html}");
    }

    #[test]
    fn a_complete_answer_says_every_node_answered() {
        let html = answered(&answer(200, true)).to_html();
        assert!(html.contains("Every node in scope answered."), "{html}");
        assert!(
            !html.contains("Not every node in scope answered (meta.federation.complete is false)"),
            "{html}"
        );
        assert!(!html.contains("failed the query"), "{html}");
    }

    #[test]
    fn the_summary_strip_names_the_rows_the_completeness_the_nodes_and_the_time() {
        let html = answered(&answer(200, false)).to_html();
        assert!(
            html.contains(r#"<span class="pill tone-warn">partial</span>"#),
            "{html}"
        );
        assert!(html.contains("1 of 2 nodes answered"), "{html}");
        assert!(html.contains("142 ms"), "{html}");
        let html = answered(&answer(200, true)).to_html();
        assert!(
            html.contains(r#"<span class="pill tone-ok">complete</span>"#),
            "{html}"
        );
        let mut failed = answer(504, false);
        failed.rows.clear();
        let html = answered(&failed).to_html();
        assert!(
            html.contains(r#"<span class="pill tone-danger">failed 504</span>"#),
            "{html}"
        );
    }

    // A node reported excluded was never in scope, so it is not counted
    // among the nodes the answer waited for (§11.1).
    #[test]
    fn a_node_out_of_scope_is_not_counted_as_one_that_did_not_answer() {
        let mut directed = answer(200, true);
        directed.endpoints = vec![
            endpoint("node_1", "active", None),
            endpoint("node_2", "not-resolved", None),
            endpoint("node_3", "excluded", None),
        ];
        let html = answered(&directed).to_html();
        assert!(html.contains("2 of 2 nodes answered"), "{html}");
        for (id, tone) in [("node_1", "ok"), ("node_2", "warn"), ("node_3", "neutral")] {
            assert!(
                html.contains(&format!(r#"{id}</span><span class="pill tone-{tone}">"#)),
                "{id}: {html}"
            );
        }
    }

    #[test]
    fn rows_beyond_a_page_wait_in_hidden_pages() {
        let mut long = answer(200, true);
        long.rows = (0..60).map(|index| vec![format!("row-{index}")]).collect();
        let html = answered(&long).to_html();
        assert!(html.contains("<caption>60 rows</caption>"), "{html}");
        // The endpoint details' table and the first page of rows.
        assert_eq!(2, html.matches("<tbody>").count(), "{html}");
        assert_eq!(2, html.matches("<tbody hidden>").count(), "{html}");
        let first = html.find("row-24").expect("the first page");
        let hidden = html.find("<tbody hidden>").expect("the second page");
        assert!(first < hidden, "{html}");
        assert!(hidden < html.find("row-25").expect("row 26"), "{html}");
    }

    #[test]
    fn a_failed_all_or_nothing_answer_shows_its_status() {
        let mut failed = answer(504, false);
        failed.rows.clear();
        let html = answered(&failed).to_html();
        assert!(
            html.contains("The gateway failed the query: 504."),
            "{html}"
        );
        assert!(
            html.contains("Not every node in scope answered (meta.federation.complete is false)"),
            "{html}"
        );
        assert!(html.contains("No rows."), "{html}");
        assert!(html.contains("was asked and did not answer"), "{html}");
    }

    #[test]
    fn each_failing_status_is_read_as_section_11_4_reads_it() {
        let mut failed = answer(424, false);
        failed.rows.clear();
        let html = answered(&failed).to_html();
        assert!(html.contains("answered with an error"), "{html}");
        failed.status = 502;
        let html = answered(&failed).to_html();
        assert!(
            html.contains("The gateway failed the query: 502."),
            "{html}"
        );
        assert!(!html.contains("did not answer"), "{html}");
        assert!(!html.contains("answered with an error"), "{html}");
    }

    // A refusal reaches the page through `inner_html` as the answer does,
    // so the code and the reason the gateway or the form gave arrive
    // escaped, never as markup, in its badges as in its line.
    #[test]
    fn a_refusal_renders_its_status_and_code_escaped() {
        let refused = super::refused(&Refusal::Gateway {
            status: 400,
            code: Some(String::from(r#"<script>x</script>" onmouseover="y"#)),
        });
        assert_eq!(Some(400), refused.status);
        assert!(!refused.complete);
        assert!(refused.refused);
        assert_eq!(0, refused.rows);
        assert!(refused.raw.is_empty());
        let html = &refused.html;
        assert!(html.contains(r#"<p role="alert">"#), "{html}");
        assert!(
            html.contains("The gateway refused this view: 400 ("),
            "{html}"
        );
        assert!(
            html.contains(r#"<span class="pill tone-danger">refused</span>"#),
            "{html}"
        );
        assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"), "{html}");
        assert!(!html.contains("<script"), "{html}");
        let invalid = super::refused(&Refusal::Invalid {
            reason: String::from("<b>The offset</b> is a whole number from 0."),
        });
        assert_eq!(None, invalid.status);
        assert!(invalid.refused);
        assert!(
            invalid.html.contains("&lt;b&gt;The offset"),
            "{}",
            invalid.html
        );
        assert!(!invalid.html.contains("<b>"), "{}", invalid.html);
    }

    // A node names a column `#0` when the query gave it no alias, so the
    // header reads its path; the path reaches the header escaped.
    #[test]
    fn a_column_named_by_position_is_headed_by_its_path() {
        let mut positional = answer(200, true);
        positional.columns = vec![
            ColumnLine {
                name: String::from("#0"),
                path: Some(String::from("c/uid/value")),
            },
            ColumnLine {
                name: String::from("#1"),
                path: Some(String::from("</th><script>x</script>")),
            },
            ColumnLine {
                name: String::new(),
                path: None,
            },
            ColumnLine {
                name: String::from("#3"),
                path: None,
            },
        ];
        positional.rows = vec![vec![String::from("a"); 4]];
        let html = answered(&positional).to_html();
        assert!(html.contains(">c/uid/value</th>"), "{html}");
        assert!(
            html.contains(">&lt;/th&gt;&lt;script&gt;x&lt;/script&gt;</th>"),
            "{html}"
        );
        assert!(html.contains(">#2</th>"), "{html}");
        assert!(html.contains(">#3</th>"), "{html}");
        assert!(!html.contains(">#0</th>"), "{html}");
        assert!(!html.contains("<script"), "{html}");
    }

    #[test]
    fn one_row_is_one_row() {
        let html = answered(&answer(200, true)).to_html();
        assert!(html.contains("<caption>1 row</caption>"), "{html}");
    }
}
