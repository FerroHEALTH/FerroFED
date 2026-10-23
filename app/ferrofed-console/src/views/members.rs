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

use ferrohealth_ui::icon::Glyph;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::table::model::filled_columns;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::table::view::{Column, NUM};
use ferrohealth_ui::view::button::{Variant, link_button};
use ferrohealth_ui::view::empty::{Busy, busy, empty_state};
use ferrohealth_ui::view::surface::page_header;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::view::surface::{Crumb, Header, card, definitions, page_heading, stack};
#[cfg(not(target_arch = "wasm32"))]
use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;

use crate::locale::{Msg, tr, tr_with};
#[cfg(not(target_arch = "wasm32"))]
use crate::views::model::{MemberRow, MembersView};
use crate::views::{MEMBERS, load, shown, titled};
#[cfg(not(target_arch = "wasm32"))]
use crate::views::{has, replica_line, state_pill, table, table_unstacked};

/// The members and their health.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn MembersPage() -> impl IntoView {
    let loaded = Resource::new_blocking(|| (), |()| load::members());
    view! {
        <Title text=titled(&tr(Msg::Members)) />
        {page_header(tr(Msg::Members), Some(tr(Msg::EachMemberEndpointHealthOneReplica)), ())}
        <Suspense fallback=|| {
            busy(tr(Msg::LoadingTheMembers), Busy::Section)
        }>{move || Suspend::new(async move { shown(loaded.await, drawn) })}</Suspense>
    }
}

/// The markup the server rendered, shown as the HTML it is.
pub(crate) fn drawn(html: String) -> AnyView {
    view! { <div inner_html=html></div> }.into_any()
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
        .map(|latency| tr_with(Msg::Ms7, &[("latency", (latency).to_string().into())]))
        .unwrap_or_default()
}

/// The columns of the members table.
#[cfg(not(target_arch = "wasm32"))]
fn member_columns() -> [Column; 8] {
    [
        Column::text("endpoint", tr(Msg::Endpoint)),
        Column::text("organisation", tr(Msg::Organisation)),
        Column::text("membership", tr(Msg::Membership)),
        Column::text("health", tr(Msg::Health)),
        Column::text("node", tr(Msg::Node)),
        Column::text("system_id", tr(Msg::SystemId)),
        Column::text("product", tr(Msg::Product)),
        Column::text("latency", tr(Msg::MedianLatency)).numeric(),
    ]
}

/// The columns of a member's endpoints table.
#[cfg(not(target_arch = "wasm32"))]
fn endpoint_columns() -> [Column; 4] {
    [
        Column::text("endpoint", tr(Msg::Endpoint)),
        Column::text("membership", tr(Msg::Membership)),
        Column::text("health", tr(Msg::Health)),
        Column::text("latency", tr(Msg::MedianLatency)).numeric(),
    ]
}

/// The columns of the other dependencies.
#[cfg(not(target_arch = "wasm32"))]
fn service_columns() -> [Column; 2] {
    [
        Column::text("dependency", tr(Msg::Dependency)),
        Column::text("state", tr(Msg::State)),
    ]
}

/// The columns of `columns` some of `members` fills: Product and Median
/// latency only when a member reports one.
#[cfg(not(target_arch = "wasm32"))]
fn filled(columns: &[Column], members: &[MemberRow]) -> Vec<Column> {
    filled_columns(columns, members, &[], |member, key| match key {
        "product" => member.product.is_some(),
        "latency" => member.latency_ms_p50.is_some(),
        _ => true,
    })
}

/// The members table and the other services.
#[cfg(not(target_arch = "wasm32"))]
fn members_section(view: MembersView) -> AnyView {
    let columns = filled(&member_columns(), &view.members);
    let (products, timed) = (has(&columns, "product"), has(&columns, "latency"));
    let rows = view
        .members
        .iter()
        .map(|member| {
            view! {
                <tr>
                    <th scope="row" class="fui-mono">
                        {endpoint_name(member)}
                    </th>
                    <td data-label=tr(Msg::Organisation)>{member.organisation.clone()}</td>
                    <td data-label=tr(Msg::Membership)>{state_pill(&member.status)}</td>
                    <td data-label=tr(Msg::Health)>{state_pill(&member.health)}</td>
                    <td data-label=tr(Msg::Node) class="fui-mono">
                        {member.node_id.clone().unwrap_or_default()}
                    </td>
                    <td data-label=tr(Msg::SystemId) class="fui-mono">
                        {member.system_id.clone().unwrap_or_default()}
                    </td>
                    {products
                        .then(|| {
                            view! {
                                <td data-label=tr(
                                    Msg::Product,
                                )>{member.product.clone().unwrap_or_default()}</td>
                            }
                        })}
                    {timed
                        .then(|| {
                            view! {
                                <td data-label=tr(Msg::MedianLatency) class=NUM>
                                    {latency(member)}
                                </td>
                            }
                        })}
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
                    <th scope="row" class="fui-mono">
                        {key}
                    </th>
                    <td data-label=tr(Msg::State)>{state_pill(&state)}</td>
                </tr>
            }
        })
        .collect_view();
    let answered_by = replica_line(view.answered_by);
    stack(view! {
        {answered_by}
        {table(&tr(Msg::MemberEndpoints), &columns, rows.into_any())}
        {table_unstacked(&tr(Msg::OtherDependencies), &service_columns(), services.into_any())}
    })
    .into_any()
}

/// The page of one member: its endpoints, their health, and what the
/// gateway declares of its node.
#[component]
pub fn MemberPage() -> impl IntoView {
    let params = use_params_map();
    // NOTE: the Leptos book, appendix_life_cycle, "Signals can be used after they are
    // disposed"; the title is read after the route's owner can be gone, so a read
    // tries the params and falls back to the value the page rendered with.
    let rendered = params.with_untracked(|params| params.get("node").unwrap_or_default());
    let key = move || {
        params
            .try_with(|params| params.get("node"))
            .map_or_else(|| rendered.clone(), Option::unwrap_or_default)
    };
    let loaded = Resource::new_blocking(key.clone(), load::member);
    view! {
        <Title text=move || titled(&tr_with(Msg::Member, &[("value0", (key()).clone().into())])) />
        <Transition fallback=|| busy(
            tr(Msg::LoadingTheMember),
            Busy::Section,
        )>
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
        [] => tr(Msg::NotReported),
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
    let columns = filled(&endpoint_columns(), &endpoints);
    let timed = has(&columns, "latency");
    let rows = endpoints
        .iter()
        .map(|member| {
            view! {
                <tr>
                    <th scope="row" class="fui-mono">
                        {member.endpoint_id.clone()}
                    </th>
                    <td data-label=tr(Msg::Membership)>{state_pill(&member.status)}</td>
                    <td data-label=tr(Msg::Health)>{state_pill(&member.health)}</td>
                    {timed
                        .then(|| {
                            view! {
                                <td data-label=tr(Msg::MedianLatency) class=NUM>
                                    {latency(member)}
                                </td>
                            }
                        })}
                </tr>
            }
        })
        .collect_view();
    let mono = |text: String| view! { <span class="fui-mono">{text}</span> }.into_any();
    let facts = definitions(vec![
        (tr(Msg::Organisation), first.organisation.clone().into_any()),
        (
            tr(Msg::Node),
            mono(first.node_id.clone().unwrap_or_default()),
        ),
        (
            tr(Msg::SystemId),
            mono(
                first
                    .system_id
                    .clone()
                    .unwrap_or_else(|| tr(Msg::NotReported)),
            ),
        ),
        (
            tr(Msg::Product),
            first
                .product
                .clone()
                .unwrap_or_else(|| tr(Msg::NotReported))
                .into_any(),
        ),
        (tr(Msg::Endpoints), count.to_string().into_any()),
    ]);
    let heading = page_heading(
        Header {
            title: key.to_owned(),
            subtitle: Some(tr_with(
                Msg::MemberEndpointsTheirHealth,
                &[("organisation", (first.organisation).clone().into())],
            )),
            crumbs: vec![Crumb::new(tr(Msg::Members), MEMBERS)],
            mono: true,
        },
        state_pill(&state),
    );
    let answered_by = replica_line(view.answered_by);
    view! {
        {heading}
        {answered_by}
        <div class="fui-stack">
            {card(tr(Msg::Overview), facts)}
            {table(&tr(Msg::EndpointsOfThisMember), &columns, rows.into_any())}
        </div>
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
    view! {
        <div class="fui-centred">
            <h1 class="fui-sr-only">{tr(Msg::NoSuchMember)}</h1>
            <div class="fui-card">
                {empty_state(
                    Glyph::USERS,
                    tr(Msg::NoSuchMember),
                    tr(Msg::NoMemberAtAddress),
                    link_button(
                        String::from(MEMBERS),
                        Variant::Secondary,
                        Some(Glyph::ARROW_LEFT),
                        tr(Msg::AllMembers),
                    ),
                )}
            </div>
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

    #[test]
    fn a_column_no_member_fills_is_left_out() {
        leptos::prelude::Owner::new().with(|| {
            let mut view = hostile();
            let filled = members_html(view.clone());
            assert!(filled.contains(">Product</th>"), "{filled}");
            assert!(!filled.contains("Median latency"), "{filled}");
            for member in &mut view.members {
                member.product = None;
                member.latency_ms_p50 = Some(12);
            }
            let swapped = members_html(view.clone());
            assert!(!swapped.contains(">Product</th>"), "{swapped}");
            assert!(swapped.contains("Median latency"), "{swapped}");
            assert!(swapped.contains("12 ms"), "{swapped}");
            let detail = member_html("node-a", view).unwrap_or_default();
            assert!(detail.contains("Median latency"), "{detail}");
        });
    }
}
