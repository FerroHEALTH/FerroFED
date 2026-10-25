// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The find in the top bar: organisations, nodes with their endpoints, a
//! node's `system_id` and stored queries, by name or id.
//!
//! A `system_id` is the creating system a record names, so it leads to its
//! node's Routes tab. The find takes routing ids and names alone and never a
//! patient identifier, so its words sit in the query (`GET /search?q=`) as any
//! filter's do. As you type, the panel under the field asks [`ANSWER`], the
//! server function [`find_answer`], which answers the FerroUI search
//! answer as JSON; the find page lists every match in the page.

use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::search::{Hit, HitGroup, Mark, SearchAnswer, Way, find_results};
use ferrohealth_ui::view::surface::{Header, page_heading};
use leptos::prelude::*;
use leptos::server_fn::codec::{GetUrl, Json};

use crate::app::{QUERY, STORED_QUERIES};
use crate::locale::{Msg, tr, tr_with};
use crate::read::federation::Federation;

/// The route the panel under the field asks as you type.
pub const ANSWER: &str = "/search/answer";

/// The words a find's query string `search` carries, trimmed, when there
/// are any.
#[must_use]
pub fn query_of(search: &str) -> Option<String> {
    url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes())
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Whether `text` holds `words`, ignoring case.
fn holds(text: &str, words: &str) -> bool {
    text.to_lowercase().contains(&words.to_lowercase())
}

/// The organisations `federation` holds that match `words`.
fn organisation_hits(federation: &Federation, words: &str) -> Vec<Hit> {
    federation
        .organisations
        .iter()
        .filter(|organisation| organisation.routable())
        .filter(|organisation| holds(organisation.shown(), words) || holds(&organisation.id, words))
        .map(|organisation| Hit {
            href: organisation.href(),
            name: organisation.shown().to_owned(),
            line: tr_with(
                Msg::SearchOrganisationLine,
                &[
                    ("nodes", Arg::from(organisation.nodes.len())),
                    ("state", organisation.health().word().into()),
                ],
            ),
            mark: Some(Mark::Dot(organisation.health().tone())),
            tag: Some(organisation.id.clone()),
        })
        .collect()
}

/// The nodes `federation` holds that match `words` by their id, an
/// endpoint's id or their `system_id`; a `system_id` leads to the node's
/// Routes tab.
fn node_hits(federation: &Federation, words: &str) -> Vec<Hit> {
    let mut nodes = Vec::new();
    for (organisation, node) in federation.nodes() {
        if !node.routable() || !organisation.routable() {
            continue;
        }
        let by_system = node
            .system_id
            .as_deref()
            .is_some_and(|system| holds(system, words));
        let by_endpoint = node
            .endpoints
            .iter()
            .any(|endpoint| holds(&endpoint.id, words));
        if !(holds(&node.id, words) || by_system || by_endpoint) {
            continue;
        }
        let (href, line) = if by_system {
            (
                format!("{}/routes", organisation.node_href(node)),
                tr_with(
                    Msg::SearchCreatingSystemLine,
                    &[("organisation", organisation.shown().to_owned().into())],
                ),
            )
        } else {
            (
                organisation.node_href(node),
                tr_with(
                    Msg::SearchNodeLine,
                    &[
                        ("organisation", organisation.shown().to_owned().into()),
                        ("state", node.health().word().into()),
                    ],
                ),
            )
        };
        nodes.push(Hit {
            href,
            name: node.id.clone(),
            line,
            mark: Some(Mark::Dot(node.health().tone())),
            tag: node.system_id.clone(),
        });
    }
    nodes
}

/// The hits of `words`, grouped by kind, and whether a read failed, so the
/// answer is not whole.
async fn groups(words: &str) -> (Vec<HitGroup>, bool) {
    let mut groups = Vec::new();
    let mut failed = false;
    if let Ok(federation) = crate::read::federation::read()
        .await
        .inspect_err(|_| failed = true)
    {
        for (kind, hits) in [
            (
                Msg::SearchKindOrganisations,
                organisation_hits(&federation, words),
            ),
            (Msg::SearchKindNodes, node_hits(&federation, words)),
        ] {
            if !hits.is_empty() {
                groups.push(HitGroup {
                    kind: tr(kind),
                    hits,
                });
            }
        }
    }
    if let Ok(queries) = crate::pages::stored::names()
        .await
        .inspect_err(|_| failed = true)
    {
        let hits: Vec<Hit> = queries
            .into_iter()
            .filter(|(name, _)| holds(name, words))
            .map(|(name, versions)| Hit {
                href: crate::pages::stored::href(&name),
                line: tr_with(Msg::SearchQueryLine, &[("count", Arg::from(versions))]),
                name,
                mark: Some(Mark::Glyph(ferrohealth_ui::icon::Glyph::DATABASE)),
                tag: None,
            })
            .collect();
        if !hits.is_empty() {
            groups.push(HitGroup {
                kind: tr(Msg::SearchKindQueries),
                hits,
            });
        }
    }
    (groups, failed)
}

/// What the find holds for `words`: every match, grouped by kind.
pub async fn answer(words: &str) -> SearchAnswer {
    let (groups, failed) = groups(words).await;
    let unfinished = failed.then(|| tr(Msg::SearchUnfinished));
    if groups.is_empty() {
        return SearchAnswer {
            query: words.to_owned(),
            none: Some((
                tr_with(Msg::SearchNothing, &[("words", words.to_owned().into())]),
                tr(Msg::SearchNothingLine),
            )),
            unfinished,
            more: Some(Way {
                href: String::from(QUERY),
                words: tr(Msg::SearchToQuery),
            }),
            ..SearchAnswer::default()
        };
    }
    SearchAnswer {
        query: words.to_owned(),
        head: None,
        groups,
        none: None,
        unfinished,
        more: Some(Way {
            href: String::from(STORED_QUERIES),
            words: tr(Msg::SearchAllQueries),
        }),
    }
}

/// Answers the panel under the field: what the find holds for the words
/// `q`, as JSON.
///
/// # Errors
/// Returns the error of a request with no live session.
#[server(prefix = "/search", endpoint = "answer", input = GetUrl, output = Json)]
pub async fn find_answer(
    /// The words typed.
    q: String,
) -> Result<SearchAnswer, ServerFnError> {
    if crate::read::signed_in().is_err() {
        if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
            response.set_status(http::StatusCode::UNAUTHORIZED);
        }
        return Err(ServerFnError::new("no live session"));
    }
    Ok(answer(q.trim()).await)
}

/// The find's page: the matches of the words the query carries.
#[must_use]
pub fn page() -> AnyView {
    let words = crate::pages::query_param("q")
        .map(|words| words.trim().to_owned())
        .filter(|words| !words.is_empty());
    let body = Suspend::new(async move {
        let heading = page_heading(
            Header {
                title: tr(Msg::SearchTitle).into(),
                subtitle: Some(
                    match &words {
                        Some(words) => {
                            tr_with(Msg::SearchSubtitle, &[("words", words.clone().into())])
                        }
                        None => tr(Msg::SearchEmptySubtitle),
                    }
                    .into(),
                ),
                ..Header::default()
            },
            ActionGroup::none(),
        )
        .into_any();
        let found = match &words {
            Some(words) => find_results(answer(words).await),
            None => view! { <p class="fui-muted">{tr(Msg::SearchNothingLine)}</p> }.into_any(),
        };
        view! {
            {heading}
            {found}
        }
    });
    view! {
        {crate::app::title(tr(Msg::SearchTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}
