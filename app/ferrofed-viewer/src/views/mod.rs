// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator views: the members and their health, the integrity
//! incidents and the `creating_system_id` routing table, the stored
//! queries, and the gateway's self-description.
//!
//! Each view reads the gateway's own public surface as the signed-in
//! operator, through a server function of [`load`]. Its route renders
//! `SsrMode::Async`: the server waits for every answer and sends the whole
//! page at once, so the view is in the HTML the browser receives and needs
//! no script to be read (the Leptos book, `ssr/23_ssr_modes`). None renders
//! a patient identifier or clinical data (§5.4.1, N33).

// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod load;
pub mod model;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::views::model::{
    FederationView, IntegrityView, MembersView, Outcome, PAGE_SIZE, Refusal, StoredView, ViewError,
};

/// The path of the members view.
pub const MEMBERS: &str = "/members";

/// The path of the integrity view.
pub const INTEGRITY: &str = "/integrity";

/// The path of the stored-query view.
pub const STORED_QUERIES: &str = "/stored-queries";

/// The path of the self-description view.
pub const FEDERATION: &str = "/federation";

/// Every view path, each of which needs a signed-in operator.
pub const PATHS: [&str; 5] = [
    MEMBERS,
    INTEGRITY,
    STORED_QUERIES,
    FEDERATION,
    crate::query::QUERY,
];

/// The page title of a view called `section`.
pub(crate) fn titled(section: &str) -> String {
    format!("{section} · {}", crate::app::PRODUCT)
}

/// The offset of the page the URL names in its `offset` query parameter.
fn offset_in_url() -> impl Fn() -> u64 + Send + Sync + Clone + 'static {
    let query = use_query_map();
    // NOTE: no specification governs this: our own design; an offset that is
    // no number is the first page, as a link with none is.
    move || {
        query
            .read()
            .get("offset")
            .and_then(|offset| offset.parse().ok())
            .unwrap_or(0)
    }
}

/// Where a page of `shown` rows from `offset` of `total` sits, with a link to
/// the page before it and the page after it under `path`.
fn pager(path: &'static str, offset: u64, shown: usize, total: u64) -> AnyView {
    let shown = u64::try_from(shown).unwrap_or(u64::MAX);
    let end = offset.saturating_add(shown);
    let place = if shown == 0 {
        format!("No row on this page, of {total} in all.")
    } else {
        format!("Rows {} to {end} of {total}.", offset.saturating_add(1))
    };
    let previous = (offset > 0).then(|| {
        let to = offset.saturating_sub(PAGE_SIZE);
        view! {
            <a href=format!("{path}?offset={to}") rel="prev">
                "Previous page"
            </a>
        }
    });
    let next = (end < total).then(|| {
        view! {
            <a href=format!("{path}?offset={end}") rel="next">
                "Next page"
            </a>
        }
    });
    view! {
        <nav aria-label="Pages">
            <p>{place}</p>
            {previous}
            " "
            {next}
        </nav>
    }
    .into_any()
}

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
        <h1>"Members"</h1>
        <Suspense fallback=|| {
            view! { <p>"Loading the members."</p> }
        }>{move || Suspend::new(async move { shown(loaded.await, members_section) })}</Suspense>
    }
}

/// The members table and the other services.
fn members_section(view: MembersView) -> AnyView {
    let rows = view
        .members
        .into_iter()
        .map(|member| {
            view! {
                <tr>
                    <th scope="row">{member.endpoint_id}</th>
                    <td>{member.organisation}</td>
                    <td>{member.status}</td>
                    <td>{member.health}</td>
                    <td>{member.node_id.unwrap_or_default()}</td>
                    <td>{member.system_id.unwrap_or_default()}</td>
                    <td>{member.product.unwrap_or_default()}</td>
                    <td>
                        {member
                            .latency_ms_p50
                            .map(|latency| format!("{latency} ms"))
                            .unwrap_or_default()}
                    </td>
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
                    <th scope="row">{key}</th>
                    <td>{state}</td>
                </tr>
            }
        })
        .collect_view();
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
                    <th scope="col">"Median latency"</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
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
    }
    .into_any()
}

/// The integrity incidents and the `creating_system_id` routing table.
#[component]
pub fn IntegrityPage() -> impl IntoView {
    let loaded = Resource::new_blocking(offset_in_url(), load::integrity);
    view! {
        <Title text=titled("Integrity") />
        <h1>"Integrity"</h1>
        <Transition fallback=|| {
            view! { <p>"Loading the incidents."</p> }
        }>{move || Suspend::new(async move { shown(loaded.await, integrity_section) })}</Transition>
    }
}

/// The incident counts, the recent incidents and the routing table.
fn integrity_section(view: IntegrityView) -> AnyView {
    let counts = view
        .counts
        .into_iter()
        .map(|(kind, count)| {
            view! {
                <tr>
                    <th scope="row">{kind}</th>
                    <td>{count}</td>
                </tr>
            }
        })
        .collect_view();
    let recent = if view.recent.is_empty() {
        view! { <p>"No incident since the gateway started."</p> }.into_any()
    } else {
        let rows = view
            .recent
            .into_iter()
            .map(|incident| {
                view! {
                    <tr>
                        <td>{incident.at}</td>
                        <th scope="row">{incident.kind}</th>
                        <td>{incident.description}</td>
                    </tr>
                }
            })
            .collect_view();
        view! {
            <table>
                <caption>"Recent incidents, newest first"</caption>
                <thead>
                    <tr>
                        <th scope="col">"At"</th>
                        <th scope="col">"Kind"</th>
                        <th scope="col">"Description"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        }
        .into_any()
    };
    let routes_pager = pager(
        INTEGRITY,
        view.routes_offset,
        view.routes.len(),
        view.routes_total,
    );
    let routes = view
        .routes
        .into_iter()
        .map(|route| {
            view! {
                <tr>
                    <th scope="row">{route.creating_system_id}</th>
                    <td>{route.source}</td>
                    <td>{route.node.unwrap_or_default()}</td>
                    <td>{route.endpoint.unwrap_or_default()}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table>
            <caption>"Incidents since the gateway started"</caption>
            <thead>
                <tr>
                    <th scope="col">"Kind"</th>
                    <th scope="col">"Count"</th>
                </tr>
            </thead>
            <tbody>{counts}</tbody>
        </table>
        {recent}
        <table>
            <caption>"The creating_system_id routing table"</caption>
            <thead>
                <tr>
                    <th scope="col">"creating_system_id"</th>
                    <th scope="col">"Source"</th>
                    <th scope="col">"Node"</th>
                    <th scope="col">"Endpoint"</th>
                </tr>
            </thead>
            <tbody>{routes}</tbody>
        </table>
        {routes_pager}
    }
    .into_any()
}

/// The stored queries the gateway holds.
#[component]
pub fn StoredQueriesPage() -> impl IntoView {
    let loaded = Resource::new_blocking(offset_in_url(), load::stored_queries);
    view! {
        <Title text=titled("Stored queries") />
        <h1>"Stored queries"</h1>
        <Transition fallback=|| {
            view! { <p>"Loading the stored queries."</p> }
        }>{move || Suspend::new(async move { shown(loaded.await, stored_section) })}</Transition>
    }
}

/// Every held version with its text.
fn stored_section(view: StoredView) -> AnyView {
    if view.total == 0 {
        return view! { <p>"The gateway holds no stored query."</p> }.into_any();
    }
    let stored_pager = pager(
        STORED_QUERIES,
        view.offset,
        view.definitions.len(),
        view.total,
    );
    let rows = view
        .definitions
        .into_iter()
        .map(|definition| {
            view! {
                <tr>
                    <th scope="row">{definition.name}</th>
                    <td>{definition.version}</td>
                    <td>{definition.saved}</td>
                    <td>
                        <pre>{definition.aql}</pre>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table>
            <caption>"Held versions"</caption>
            <thead>
                <tr>
                    <th scope="col">"Name"</th>
                    <th scope="col">"Version"</th>
                    <th scope="col">"Stored"</th>
                    <th scope="col">"AQL"</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
        {stored_pager}
    }
    .into_any()
}

/// The gateway's self-description, `OPTIONS {base}/`.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn FederationPage() -> impl IntoView {
    let loaded = Resource::new_blocking(|| (), |()| load::federation());
    view! {
        <Title text=titled("Self-description") />
        <h1>"Self-description"</h1>
        <Suspense fallback=|| {
            view! { <p>"Loading the self-description."</p> }
        }>{move || Suspend::new(async move { shown(loaded.await, federation_section) })}</Suspense>
    }
}

/// The headline facts and the whole `federation` object.
fn federation_section(view: FederationView) -> AnyView {
    view! {
        <dl>
            <dt>"Federation"</dt>
            <dd>{view.id}</dd>
            <dt>"Specification version"</dt>
            <dd>{view.spec_version}</dd>
            <dt>"Member endpoints"</dt>
            <dd>{view.endpoints}</dd>
        </dl>
        <pre>{view.document}</pre>
    }
    .into_any()
}

/// The section `section` draws of a view `loaded`, the notice of the refusal
/// that kept it, or the notice of the console's fault: never an empty view.
fn shown<T>(loaded: Result<Outcome<T>, ViewError>, section: fn(T) -> AnyView) -> AnyView {
    match loaded {
        Ok(Outcome::Shown(view)) => section(view),
        Ok(Outcome::Refused(refused)) => refusal(&refused),
        Err(error) => fault(&error),
    }
}

/// The inline notice of an expected refusal: the gateway's status and
/// stable error code, or what is wrong with the input.
pub(crate) fn refusal(refused: &Refusal) -> AnyView {
    match refused {
        Refusal::NotAuthenticated { code } => {
            let code = code.clone().unwrap_or_else(|| String::from("no code"));
            view! {
                <p role="alert">
                    {format!("The gateway did not accept your sign-in ({code}). ")}
                    <a href=crate::app::SIGN_IN rel="external">
                        "Sign in again"
                    </a>
                </p>
            }
            .into_any()
        }
        Refusal::Gateway { status, code } => {
            let code = code.clone().unwrap_or_else(|| String::from("no code"));
            let hint = if code == "scope-insufficient" {
                " Your access token may carry no operator scope."
            } else {
                ""
            };
            view! { <p role="alert">{format!("The gateway refused this view: {status} ({code}).{hint}")}</p> }
            .into_any()
        }
        Refusal::Invalid { reason } => {
            let reason = reason.clone();
            view! { <p role="alert">{reason}</p> }.into_any()
        }
    }
}

/// The inline notice of a view the console could not serve: never an empty
/// view.
pub(crate) fn fault(error: &ViewError) -> AnyView {
    match error {
        ViewError::SignedOut => view! {
            <p role="alert">
                "Sign in to see this view. " <a href=crate::app::SIGN_IN rel="external">
                    "Sign in"
                </a>
            </p>
        }
        .into_any(),
        ViewError::Unreadable { .. }
        | ViewError::Unreachable
        | ViewError::PlainPost
        | ViewError::Unavailable
        | ViewError::Fetch { .. } => {
            let text = error.to_string();
            view! { <p role="alert">{text}</p> }.into_any()
        }
    }
}
