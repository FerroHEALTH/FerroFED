// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members view, with every member endpoint and the other services the
//! gateway depends on, and the page of one member with its endpoints and
//! their health.
//!
//! A member's page is keyed by its `node_id`, or by the `endpoint_id` of an
//! endpoint that names no node: a routing id of the registry, never a
//! patient identifier (§5.4.1, N33). Both read the one members answer, so
//! the page of a member shows what the members view shows of it.

#[cfg(not(target_arch = "wasm32"))]
use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

#[cfg(not(target_arch = "wasm32"))]
use crate::components::{card, replica_meta, status_pill, table_card};
use crate::components::{empty_state, link_button, page_header};
use crate::styles;
#[cfg(not(target_arch = "wasm32"))]
use crate::views::model::{MemberRow, MembersView};
use crate::views::{MEMBERS, load, shown, titled};

/// The members and their health.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn MembersPage() -> impl IntoView {
    let loaded = Resource::new_blocking(|| (), |()| load::members());
    view! {
        <Title text=titled("Members") />
        {page_header(
            "Members",
            "Each member endpoint and its health, as one replica observes it.",
            (),
        )}
        <Suspense fallback=|| {
            view! { <p>"Loading the members."</p> }
        }>{move || Suspend::new(async move { shown(loaded.await, drawn) })}</Suspense>
    }
}

/// The markup the server rendered, shown as the HTML it is.
pub(crate) fn drawn(html: String) -> AnyView {
    view! { <div class="contents" inner_html=html></div> }.into_any()
}

/// The endpoint `member`'s id, a link to its member's page when it has one.
#[cfg(not(target_arch = "wasm32"))]
fn endpoint_name(member: &MemberRow) -> impl IntoView + use<> {
    let id = member.endpoint_id.clone();
    match member.detail.clone() {
        Some(href) => Either::Left(view! { <a href=href>{id}</a> }),
        None => Either::Right(id),
    }
}

/// The median latency of `member`, when the gateway reports one.
#[cfg(not(target_arch = "wasm32"))]
fn latency(member: &MemberRow) -> String {
    member
        .latency_ms_p50
        .map(|latency| format!("{latency} ms"))
        .unwrap_or_default()
}

/// The members table and the other services.
#[cfg(not(target_arch = "wasm32"))]
fn members_section(view: MembersView) -> AnyView {
    let rows = view
        .members
        .iter()
        .map(|member| {
            view! {
                <tr>
                    <th scope="row" class=styles::ID>
                        {endpoint_name(member)}
                    </th>
                    <td>{member.organisation.clone()}</td>
                    <td>{status_pill(member.status.clone())}</td>
                    <td>{status_pill(member.health.clone())}</td>
                    <td class=styles::MONO>{member.node_id.clone().unwrap_or_default()}</td>
                    <td class=styles::MONO>{member.system_id.clone().unwrap_or_default()}</td>
                    <td>{member.product.clone().unwrap_or_default()}</td>
                    <td class=styles::NUM>{latency(member)}</td>
                </tr>
            }
        })
        .collect_view();
    let services = view
        .services
        .into_iter()
        .map(|(key, state)| {
            view! {
                <tr>
                    <th scope="row" class=styles::ID>
                        {key}
                    </th>
                    <td>{status_pill(state)}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        {replica_meta(view.answered_by)}
        {table_card(
            "Member endpoints",
            view! {
                <table>
                    <caption>"Member endpoints"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Endpoint"</th>
                            <th scope="col">"Organisation"</th>
                            <th scope="col">"Membership"</th>
                            <th scope="col">"Health"</th>
                            <th scope="col">"Node"</th>
                            <th scope="col">"system_id"</th>
                            <th scope="col">"Product"</th>
                            <th scope="col" class=styles::NUM>
                                "Median latency"
                            </th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            },
        )}
        {table_card(
            "Other dependencies",
            view! {
                <table>
                    <caption>"Other dependencies"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Dependency"</th>
                            <th scope="col">"State"</th>
                        </tr>
                    </thead>
                    <tbody>{services}</tbody>
                </table>
            },
        )}
    }
    .into_any()
}

/// The page of one member: its endpoints, their health, and what the
/// gateway declares of its node.
#[component]
pub fn MemberPage() -> impl IntoView {
    let params = use_params_map();
    let key = move || params.read().get("node").unwrap_or_default();
    let loaded = Resource::new_blocking(key, load::member);
    view! {
        <Title text=move || titled(&format!("Member {}", key())) />
        <Transition fallback=|| {
            view! { <p>"Loading the member."</p> }
        }>
            {move || Suspend::new(async move {
                shown(
                    loaded.await,
                    |page| match page {
                        Some(html) => drawn(html),
                        None => missing(),
                    },
                )
            })}
        </Transition>
    }
}

/// The one state of a member whose endpoints report `words`: the word they
/// share, or `degraded` when they differ.
#[cfg(not(target_arch = "wasm32"))]
fn overall(words: &[&str]) -> String {
    match words {
        [first, rest @ ..] if rest.iter().all(|word| word == first) => (*first).to_owned(),
        [] => String::from("not reported"),
        _ => String::from("degraded"),
    }
}

/// The page of the member keyed `key` in `view`, or the page that says the
/// gateway declares no such member.
#[cfg(not(target_arch = "wasm32"))]
fn member_section(key: &str, view: MembersView) -> Option<AnyView> {
    let path = format!("{MEMBERS}/{key}");
    let endpoints: Vec<MemberRow> = view
        .members
        .into_iter()
        .filter(|member| member.detail.as_deref() == Some(path.as_str()))
        .collect();
    let first = endpoints.first().cloned()?;
    let words: Vec<&str> = endpoints.iter().map(|row| row.health.as_str()).collect();
    let state = overall(&words);
    let count = endpoints.len();
    let rows = endpoints
        .iter()
        .map(|member| {
            view! {
                <tr>
                    <th scope="row" class=styles::ID>
                        {member.endpoint_id.clone()}
                    </th>
                    <td>{status_pill(member.status.clone())}</td>
                    <td>{status_pill(member.health.clone())}</td>
                    <td class=styles::NUM>{latency(member)}</td>
                </tr>
            }
        })
        .collect_view();
    let facts = [
        ("Organisation", first.organisation.clone(), ""),
        ("Node", first.node_id.clone().unwrap_or_default(), styles::MONO),
        ("system_id", first.system_id.clone().unwrap_or_else(|| String::from("not reported")), styles::MONO),
        ("Product", first.product.clone().unwrap_or_else(|| String::from("not reported")), ""),
        ("Endpoints", count.to_string(), "tabular-nums"),
    ]
    .into_iter()
    .map(|(term, value, class)| {
        view! {
            <div class="grid grid-cols-1 gap-1 border-b border-line py-2 last:border-b-0 sm:grid-cols-[10rem_1fr]">
                <dt class="text-sm font-medium text-muted">{term}</dt>
                <dd class=format!("m-0 text-sm break-words {class}")>{value}</dd>
            </div>
        }
    })
    .collect_view();
    view! {
        <header class=styles::PAGE_HEADER>
            <div class="min-w-0">
                <p class="m-0 mb-1 text-sm">
                    <a href=MEMBERS>"Members"</a>
                </p>
                <h1 class=format!(
                    "{} break-all font-mono",
                    styles::PAGE_TITLE,
                )>{key.to_owned()}</h1>
                <p class=styles::PAGE_SUBTITLE>
                    {format!(
                        "A member of {}, with its endpoints and their health.",
                        first.organisation,
                    )}
                </p>
            </div>
            <div class="flex flex-wrap items-center gap-2">{status_pill(state)}</div>
        </header>
        {replica_meta(view.answered_by)}
        <div class="mb-6">{card("Overview", view! { <dl class="m-0">{facts}</dl> })}</div>
        {table_card(
            "Endpoints of this member",
            view! {
                <table>
                    <caption>"Endpoints of this member"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"Endpoint"</th>
                            <th scope="col">"Membership"</th>
                            <th scope="col">"Health"</th>
                            <th scope="col" class=styles::NUM>
                                "Median latency"
                            </th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            },
        )}
    }
    .into_any()
    .into()
}

/// The markup of the members view of `view`, which the page shows as HTML.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn members_html(view: MembersView) -> String {
    members_section(view).to_html()
}

/// The markup of the page of the member keyed `key` in `view`, or `None`
/// when the gateway declares no such member.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn member_html(key: &str, view: MembersView) -> Option<String> {
    member_section(key, view).map(RenderHtml::to_html)
}

/// The page for a member the gateway does not declare: `404`, and the way
/// back to the members.
fn missing() -> AnyView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::NOT_FOUND);
    }
    let body = view! { <p class=styles::EMPTY_BODY>"404: the gateway declares no member at this address."</p> };
    view! {
        <div class=styles::CENTRED>
            {empty_state(
                icondata_lu::LuUsers,
                "No such member",
                body,
                link_button(
                    MEMBERS,
                    styles::Button::Secondary,
                    icondata_lu::LuArrowLeft,
                    "All members",
                ),
            )}
        </div>
    }
    .into_any()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{member_html, members_html, overall};
    use crate::views::model::{MemberRow, MembersView};

    /// A value that breaks out of a text and out of an attribute.
    const HOSTILE: &str = r#""><script>alert(1)</script><img src=x onerror=alert(2)>"#;

    #[test]
    fn a_member_reads_as_the_state_its_endpoints_share_or_degraded() {
        assert_eq!("up", overall(&["up", "up"]));
        assert_eq!("degraded", overall(&["up", "down"]));
        assert_eq!("not reported", overall(&[]));
    }

    /// A members view whose every text the gateway names is `HOSTILE`,
    /// with one endpoint of node `node-a`.
    fn hostile() -> MembersView {
        MembersView {
            members: vec![MemberRow {
                endpoint_id: HOSTILE.to_owned(),
                organisation: HOSTILE.to_owned(),
                status: HOSTILE.to_owned(),
                node_id: Some(String::from("node-a")),
                system_id: Some(HOSTILE.to_owned()),
                product: Some(HOSTILE.to_owned()),
                latency_ms_p50: None,
                health: HOSTILE.to_owned(),
                detail: Some(String::from("/members/node-a")),
            }],
            services: vec![(HOSTILE.to_owned(), HOSTILE.to_owned())],
            answered_by: Some(HOSTILE.to_owned()),
        }
    }

    // The markup reaches the page through `inner_html`, so every value the
    // gateway names arrives escaped, and a key from the URL as well.
    #[test]
    fn hostile_values_render_escaped() {
        leptos::prelude::Owner::new().with(|| {
            let pages = [
                members_html(hostile()),
                member_html("node-a", hostile()).unwrap_or_default(),
            ];
            for html in pages {
                assert!(
                    html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
                    "{html}"
                );
                assert!(!html.contains("<script"), "{html}");
                assert!(!html.contains("<img"), "{html}");
            }
            assert_eq!(None, member_html(HOSTILE, hostile()));
        });
    }
}
