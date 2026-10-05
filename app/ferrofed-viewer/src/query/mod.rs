// SPDX-FileCopyrightText: Vernum Projecten B.V.
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

// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
pub mod model;
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod run;

use leptos::form::ActionForm;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::query::model::{QueryAnswer, QueryOptionsView};
use crate::query::run::RunQuery;
use crate::views::{refusal, titled};

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
                    Ok(offered) => form_section(action, &offered),
                    Err(error) => refusal(&error),
                }
            })}
        </Suspense>
        {answer}
    }
}

/// The query form, with the choices the gateway offers.
fn form_section(action: ServerAction<RunQuery>, offered: &QueryOptionsView) -> AnyView {
    let what = what_section();
    let federation = federation_section(offered);
    view! {
        <ActionForm action=action>
            {what} {federation} <button type="submit">"Run the query"</button>
        </ActionForm>
    }
    .into_any()
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
            <Show when=move || pending.get()>
                <p role="status">"Running the query."</p>
            </Show>
            {move || {
                value
                    .with(|answer| match answer {
                        None => view! { <p>"No query has run yet."</p> }.into_any(),
                        Some(Ok(answer)) => answered(answer),
                        Some(Err(error)) => refusal(error),
                    })
            }}
        </section>
    }
    .into_any()
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
    let failed = (!answer.succeeded).then(|| {
        view! {
            <p role="alert">
                <strong>{format!("The gateway failed the query: {status}.")}</strong>
                " A node in scope did not answer, so the gateway returned no rows. "
                "The endpoints below say which node and why."
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
            <caption>{format!("{} rows", answer.rows.len())}</caption>
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
    }
}
