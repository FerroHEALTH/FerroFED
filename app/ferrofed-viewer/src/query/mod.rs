// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console.
//!
//! An AQL query, or a stored query by name, runs through the gateway as the
//! signed-in operator, and every endpoint's status and whether the answer is
//! complete show beside the rows (§9, §11.1, §11.4, N37).
//!
//! The form posts to the server function of [`run`] in a request body, so
//! nothing the operator enters, a patient named in a parameter among it,
//! reaches a URL, the browser history or a log (N33). The answer renders in
//! the page that sent it, and an incomplete one says so in words before its
//! rows.

pub mod model;
// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod run;

use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;
use leptos_meta::Title;

use crate::query::model::{QueryForm, QueryOptionsView};
use crate::query::run::RunQuery;
use crate::views::model::Outcome;
use crate::views::{fault, refusal, titled};

/// The path of the query console.
pub const QUERY: &str = "/query";

/// The query console.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn QueryPage() -> impl IntoView {
    let options = Resource::new_blocking(|| (), |()| run::query_options());
    let action = ServerAction::<RunQuery>::new();
    let answer = answer_section(action);
    view! {
        <Title text=titled("Query") />
        <h1>"Query"</h1>
        <Suspense fallback=|| {
            view! { <p>"Loading what the gateway offers a query."</p> }
        }>
            {move || Suspend::new(async move {
                match options.await {
                    Ok(Outcome::Shown(offered)) => form_section(action, &offered),
                    Ok(Outcome::Refused(refused)) => refusal(&refused),
                    Err(error) => fault(&error),
                }
            })}
        </Suspense>
        {answer}
    }
}

/// The query form, with the choices the gateway offers.
///
/// The answer renders on the page that asked, so the form runs a query only
/// once the bundle has hydrated it: its submit button stays disabled until
/// then, and the server function refuses a plain form post before it asks
/// the gateway anything ([`run::run_query`]). A submission dispatches the
/// action with the fields read from the form's own data, which keeps the
/// parsing of the URL-encoded form out of the bundle.
fn form_section(action: ServerAction<RunQuery>, offered: &QueryOptionsView) -> AnyView {
    let what = what_section();
    let federation = federation_section(offered);
    let form = NodeRef::<leptos::html::Form>::new();
    // NOTE: the Leptos book, `ssr/24_hydration_bugs`: an effect runs only in the
    // browser, once hydrated, so the server and the hydration render alike.
    let hydrated = RwSignal::new(false);
    Effect::new(move |_| hydrated.set(true));
    let submit = move |event: leptos::ev::SubmitEvent| {
        let data = form
            .get_untracked()
            .and_then(|form| leptos::web_sys::FormData::new_with_form(&form).ok());
        // NOTE: no specification governs this: our own design; form data the
        // browser cannot give leaves the plain post to run the query instead.
        if let Some(data) = data {
            event.prevent_default();
            action.dispatch(RunQuery {
                form: submitted(&data),
            });
        }
    };
    view! {
        <form method="post" action=RunQuery::url() node_ref=form on:submit=submit>
            {what}
            {federation}
            <button type="submit" disabled=move || !hydrated.get()>
                "Run the query"
            </button>
            <p role="status">
                {move || {
                    if hydrated.get() { "" } else { "The form is ready once the page has loaded." }
                }}
            </p>
        </form>
    }
    .into_any()
}

/// The query form `data` holds, by the field names the form posts.
fn submitted(data: &leptos::web_sys::FormData) -> QueryForm {
    // NOTE: XHR standard, `FormData.get`: a field the form does not send, a
    // clear checkbox among them, is null, which is the empty field it means.
    let field = |name: &str| data.get(name).as_string().unwrap_or_default();
    QueryForm {
        kind: field("form[kind]"),
        aql: field("form[aql]"),
        name: field("form[name]"),
        version: field("form[version]"),
        parameters: field("form[parameters]"),
        offset: field("form[offset]"),
        fetch: field("form[fetch]"),
        partial: field("form[partial]"),
        dedup: field("form[dedup]"),
        endpoints: field("form[endpoints]"),
        organisation: field("form[organisation]"),
    }
}

/// The query itself: AQL text or a stored query, and its parameters.
fn what_section() -> AnyView {
    view! {
        <fieldset>
            <legend>"What to run"</legend>
            <p>
                <input type="radio" id="query-kind-aql" name="form[kind]" value="aql" checked />
                <label for="query-kind-aql">"An AQL query"</label>
                " "
                <input type="radio" id="query-kind-stored" name="form[kind]" value="stored" />
                <label for="query-kind-stored">"A stored query"</label>
            </p>
            <p>
                <label for="query-aql">"AQL"</label>
                <textarea
                    id="query-aql"
                    name="form[aql]"
                    rows="6"
                    autocomplete="off"
                    spellcheck="false"
                ></textarea>
            </p>
            <p>
                <label for="query-name">"Stored query name"</label>
                <input type="text" id="query-name" name="form[name]" autocomplete="off" />
                " "
                <label for="query-version">"Version (optional)"</label>
                <input type="text" id="query-version" name="form[version]" autocomplete="off" />
            </p>
            <p>
                <label for="query-parameters">"Parameters, one name=value per line"</label>
                <textarea
                    id="query-parameters"
                    name="form[parameters]"
                    rows="3"
                    autocomplete="off"
                    spellcheck="false"
                    aria-describedby="query-parameters-hint"
                ></textarea>
            </p>
            <p id="query-parameters-hint">
                "Name a patient here, as a parameter the query reads, never in the AQL text. "
                "Each value is sent as a string, in the request body."
            </p>
            <p>
                <label for="query-offset">"Offset"</label>
                <input type="text" id="query-offset" name="form[offset]" inputmode="numeric" />
                " "
                <label for="query-fetch">"Fetch"</label>
                <input type="text" id="query-fetch" name="form[fetch]" inputmode="numeric" />
            </p>
        </fieldset>
    }
    .into_any()
}

/// The federation headers a client may send: targeting, dedup and the
/// completeness opt-in (§8.4, §10, §11.4).
fn federation_section(offered: &QueryOptionsView) -> AnyView {
    let endpoints = if offered.endpoints.is_empty() {
        String::from("The gateway declares no member endpoint.")
    } else {
        format!("Declared: {}.", offered.endpoints.join(", "))
    };
    let organisations = if offered.organisations.is_empty() {
        String::from("The gateway declares no organisation.")
    } else {
        format!("Declared: {}.", offered.organisations.join(", "))
    };
    let modes = offered
        .dedup_modes
        .iter()
        .map(|mode| {
            let label = mode.clone();
            view! { <option value=mode.clone()>{label}</option> }
        })
        .collect_view();
    let partial = if offered.best_effort {
        view! {
            <input type="checkbox" id="query-partial" name="form[partial]" value="partial" />
            <label for="query-partial">
                "Best effort: answer with the rows that came back when a node does not answer"
            </label>
        }
        .into_any()
    } else {
        view! {
            <input
                type="checkbox"
                id="query-partial"
                name="form[partial]"
                value="partial"
                disabled
            />
            <label for="query-partial">"Best effort, which this gateway does not offer"</label>
        }
        .into_any()
    };
    view! {
        <fieldset>
            <legend>"Federation"</legend>
            <p>
                <label for="query-endpoints">"Target endpoints, comma-separated"</label>
                <input
                    type="text"
                    id="query-endpoints"
                    name="form[endpoints]"
                    aria-describedby="query-endpoints-hint"
                />
            </p>
            <p id="query-endpoints-hint">{endpoints}</p>
            <p>
                <label for="query-organisation">"Target organisation"</label>
                <input
                    type="text"
                    id="query-organisation"
                    name="form[organisation]"
                    aria-describedby="query-organisation-hint"
                />
            </p>
            <p id="query-organisation-hint">{organisations}</p>
            <p>
                <label for="query-dedup">"Deduplication"</label>
                <select id="query-dedup" name="form[dedup]">
                    <option value="">"The gateway's default"</option>
                    {modes}
                </select>
            </p>
            <p>{partial}</p>
        </fieldset>
    }
    .into_any()
}

/// The answer of the last query run, or what kept it from running.
fn answer_section(action: ServerAction<RunQuery>) -> AnyView {
    let value = action.value();
    let pending = action.pending();
    view! {
        <section aria-labelledby="query-answer" aria-busy=move || pending.get().to_string()>
            <h2 id="query-answer">"Answer"</h2>
            <p role="status">{move || if pending.get() { "Running the query." } else { "" }}</p>
            {move || {
                value
                    .with(|answer| match answer {
                        None => view! { <p>"No query has run yet."</p> }.into_any(),
                        Some(Ok(rendered)) => {
                            let html = rendered.html.clone();
                            view! { <div inner_html=html></div> }.into_any()
                        }
                        Some(Err(error)) => fault(error),
                    })
            }}
        </section>
    }
    .into_any()
}

/// The server's rendering of a federated answer, which the page shows as
/// it stands.
///
/// The answer renders on the server: its tables need no code in the browser,
/// which shows the HTML the server wrote. Leptos escapes every text and
/// attribute value it renders, and the Content-Security-Policy admits no
/// inline script besides.
#[cfg(not(target_arch = "wasm32"))]
pub mod answer {
    use leptos::prelude::*;

    use crate::query::model::{QueryAnswer, RenderedAnswer};
    use crate::views::model::Refusal;

    /// The HTML of `answer`.
    #[must_use]
    pub fn html(answer: &QueryAnswer) -> String {
        answered(answer).to_html()
    }

    /// What the page shows of a query `refused` kept from running: the
    /// gateway's status and code, or what is wrong with the form.
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
            html: crate::views::refusal(refused).to_html(),
        }
    }

    /// A federated answer: its status, its completeness, every endpoint and the
    /// rows.
    fn answered(answer: &QueryAnswer) -> AnyView {
        let outcome = outcome_section(answer);
        let endpoints = endpoints_section(answer);
        let rows = rows_section(answer);
        view! {
            {outcome}
            {endpoints}
            {rows}
        }
        .into_any()
    }

    /// What the status and `meta.federation.complete` say, in words (§11.4).
    fn outcome_section(answer: &QueryAnswer) -> AnyView {
        let status = answer.status;
        // NOTE: §11.4: under all-or-nothing a node not answering is a 504 and a
        // node error a 424; any other failing status gets no reading of its own.
        let why = match status {
            504 => {
                " A node in scope was asked and did not answer, so the gateway returned no rows."
            }
            424 => " A node in scope answered with an error, so the gateway returned no rows.",
            _ => " The gateway returned no rows.",
        };
        let failed = (!answer.succeeded).then(|| {
            view! {
                <p role="alert">
                    <strong>{format!("The gateway failed the query: {status}.")}</strong>
                    {why}
                    " The endpoints below say what each one did."
                </p>
            }
        });
        let completeness = if answer.complete {
            view! {
                <p role="status" class="complete">
                    <strong>"Complete."</strong>
                    " Every node in scope answered."
                </p>
            }
            .into_any()
        } else {
            view! {
                <p role="alert" class="incomplete">
                    <strong>"Incomplete answer."</strong>
                    " Not every node in scope answered (meta.federation.complete is false), "
                    "so these rows are not the whole answer."
                </p>
            }
            .into_any()
        };
        let dedup = answer.dedup.clone().map(|mode| {
            let suppressed = answer
                .suppressed_rows
                .map(|rows| format!(", {rows} rows suppressed"))
                .unwrap_or_default();
            view! { <p>{format!("Deduplication: {mode}{suppressed}.")}</p> }
        });
        view! {
            {failed}
            {completeness}
            {dedup}
        }
        .into_any()
    }

    /// Every endpoint's status, latency, rows and error (§9.5, §11.1).
    fn endpoints_section(answer: &QueryAnswer) -> AnyView {
        let rows = answer
        .endpoints
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
        }
        .into_any()
    }

    /// How many rows `count` is, in words.
    fn counted(count: usize) -> String {
        if count == 1 {
            String::from("1 row")
        } else {
            format!("{count} rows")
        }
    }

    /// The rows, under the columns as the gateway renders them (§9.4).
    fn rows_section(answer: &QueryAnswer) -> AnyView {
        if answer.rows.is_empty() {
            return view! { <p>"No rows."</p> }.into_any();
        }
        let header = answer
            .columns
            .iter()
            .map(|column| {
                let title = column.path.clone().unwrap_or_default();
                view! {
                    <th scope="col" title=title>
                        {column.name.clone()}
                    </th>
                }
            })
            .collect_view();
        let rows = answer
            .rows
            .iter()
            .map(|row| {
                let cells = row
                    .iter()
                    .map(|cell| view! { <td>{cell.clone()}</td> })
                    .collect_view();
                view! { <tr>{cells}</tr> }
            })
            .collect_view();
        view! {
            <table>
                <caption>{counted(answer.rows.len())}</caption>
                <thead>
                    <tr>{header}</tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
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
            }
        }

        #[test]
        fn an_incomplete_answer_says_so_in_words_before_its_rows() {
            let html = answered(&answer(200, false)).to_html();
            let flag = html.find("Incomplete answer.").expect("the flag");
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
        // double-quoted attribute value, never as markup.
        #[test]
        fn hostile_values_in_an_answer_render_escaped() {
            let mut hostile = answer(200, false);
            hostile.columns = vec![ColumnLine {
                name: String::from("</th><script>x</script>"),
                path: Some(String::from(r#"" onmouseover="alert(1)"#)),
            }];
            hostile.rows = vec![
                vec![String::from("<script>alert(1)</script>")],
                vec![String::from("<img src=x onerror=alert(1)>")],
            ];
            hostile.endpoints = vec![EndpointLine {
                id: String::from("node_1"),
                status: String::from("time-out"),
                latency_ms: Some(9),
                row_count: None,
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
        }

        #[test]
        fn a_complete_answer_says_every_node_answered() {
            let html = answered(&answer(200, true)).to_html();
            assert!(html.contains("Every node in scope answered."), "{html}");
            assert!(!html.contains("Incomplete answer."), "{html}");
            assert!(!html.contains("failed the query"), "{html}");
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
            assert!(html.contains("Incomplete answer."), "{html}");
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
        // escaped, never as markup.
        #[test]
        fn a_refusal_renders_its_status_and_code_escaped() {
            let refused = super::refused(&Refusal::Gateway {
                status: 400,
                code: Some(String::from(r#"<script>x</script>" onmouseover="y"#)),
            });
            assert_eq!(Some(400), refused.status);
            assert!(!refused.complete);
            let html = &refused.html;
            assert!(html.contains(r#"<p role="alert">"#), "{html}");
            assert!(
                html.contains("The gateway refused this view: 400 ("),
                "{html}"
            );
            assert!(html.contains("&lt;script&gt;x&lt;/script&gt;"), "{html}");
            assert!(!html.contains("<script"), "{html}");
            let invalid = super::refused(&Refusal::Invalid {
                reason: String::from("<b>The offset</b> is a whole number from 0."),
            });
            assert_eq!(None, invalid.status);
            assert!(
                invalid.html.contains("&lt;b&gt;The offset"),
                "{}",
                invalid.html
            );
            assert!(!invalid.html.contains("<b>"), "{}", invalid.html);
        }

        #[test]
        fn one_row_is_one_row() {
            let html = answered(&answer(200, true)).to_html();
            assert!(html.contains("<caption>1 row</caption>"), "{html}");
        }
    }
}
