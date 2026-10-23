// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The find in the top bar: organisations, nodes with their endpoints, a
//! node's `system_id` and stored queries, by name or id.
//!
//! A `system_id` is the creating system a record names, so it leads to its
//! node's Routes tab. The find takes routing ids and names alone and never a
//! patient identifier, so its words sit in the query (`GET /search?q=`) as any
//! filter's do. The page under the top bar lists the same matches.

use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::search::{Hit, HitGroup, Mark, SearchAnswer, Way};
use ferrohealth_ui::view::surface::{Header, card, page_heading, stack};
use leptos::prelude::*;

use crate::app::{QUERY, STORED_QUERIES};
use crate::locale::{Msg, tr, tr_with};
use crate::read::federation::Federation;

/// The most hits of one kind the answer draws.
const SHOWN: usize = 8;

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
        .take(SHOWN)
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
        if nodes.len() == SHOWN {
            break;
        }
    }
    nodes
}

/// The hits of `words`, grouped by kind.
async fn groups(words: &str) -> Vec<HitGroup> {
    let mut groups = Vec::new();
    if let Ok(federation) = crate::read::federation::read().await {
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
    if let Ok(queries) = crate::pages::stored::names().await {
        let hits: Vec<Hit> = queries
            .into_iter()
            .filter(|(name, _)| holds(name, words))
            .take(SHOWN)
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
    groups
}

/// The answer under the find for `words`.
pub async fn answer(words: &str) -> Option<SearchAnswer> {
    let groups = groups(words).await;
    if groups.is_empty() {
        return Some(SearchAnswer {
            none: Some((
                tr_with(Msg::SearchNothing, &[("words", words.to_owned().into())]),
                tr(Msg::SearchNothingLine),
            )),
            more: Some(Way {
                href: String::from(QUERY),
                words: tr(Msg::SearchToQuery),
            }),
            ..SearchAnswer::default()
        });
    }
    Some(SearchAnswer {
        head: None,
        groups,
        none: None,
        more: Some(Way {
            href: String::from(STORED_QUERIES),
            words: tr(Msg::SearchAllQueries),
        }),
    })
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
            Some(words) => groups(words).await,
            None => Vec::new(),
        };
        let cards = if found.is_empty() {
            view! { <p class="fui-muted">{tr(Msg::SearchNothingLine)}</p> }.into_any()
        } else {
            found
                .into_iter()
                .map(|group| {
                    let items = group
                        .hits
                        .into_iter()
                        .map(|hit| {
                            view! {
                                <li>
                                    <a href=hit.href>{hit.name}</a>
                                    " "
                                    <span class="fui-small fui-muted">{hit.line}</span>
                                </li>
                            }
                        })
                        .collect_view();
                    card(group.kind, view! { <ul class="fui-stack">{items}</ul> })
                })
                .collect_view()
                .into_any()
        };
        view! {
            {heading}
            {stack(cards)}
        }
    });
    view! {
        {crate::app::title(tr(Msg::SearchTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}
