// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator views: the members and their health with a page per
//! member, the integrity incidents, the `creating_system_id` routing table,
//! the stored queries, and the gateway's self-description.
//!
//! Each view reads the gateway's own public surface as the signed-in
//! operator, through a server function of [`load`]. Its route renders
//! `SsrMode::Async`: the server waits for every answer and sends the whole
//! page at once, so the view is in the HTML the browser receives and needs
//! no script to be read (the Leptos book, `ssr/23_ssr_modes`). None renders
//! a patient identifier or clinical data (§5.4.1, N33). Each view that reads
//! the operator surface names the replica that answered it, because the
//! health, the incidents and the routing table are each process's own.

// The browser half of each server function, which the server macro writes,
// is a network call that awaits nothing, and the lint names the macro alone.
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod describe;
pub mod integrity;
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod load;
pub mod members;
pub mod model;
pub mod routing;
#[cfg_attr(
    target_arch = "wasm32",
    expect(
        clippy::unused_async_trait_impl,
        reason = "written by the server_fn #[server] macro for the browser half, whose call awaits nothing here (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
pub mod stored;

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::views::model::{Outcome, PAGE_SIZE, Refusal, ViewError};

/// The path of the members view.
pub const MEMBERS: &str = "/members";

/// The path of the integrity view.
pub const INTEGRITY: &str = "/integrity";

/// The path of the routing view.
pub const ROUTING: &str = "/routing";

/// The path of the stored-query view.
pub const STORED_QUERIES: &str = "/stored-queries";

/// The path of the self-description view.
pub const FEDERATION: &str = "/federation";

/// Every view path, each of which needs a signed-in operator.
pub const PATHS: [&str; 7] = [
    MEMBERS,
    INTEGRITY,
    ROUTING,
    STORED_QUERIES,
    FEDERATION,
    crate::query::QUERY,
    crate::configure::CONFIGURATION,
];

/// Whether `path`, trimmed of a trailing slash and in lower case, names a
/// view that needs a signed-in operator.
///
/// That is one of [`PATHS`], the page of one member under [`MEMBERS`], or
/// the detail of a stored query under [`STORED_QUERIES`].
#[must_use]
pub fn needs_session(path: &str) -> bool {
    PATHS.contains(&path)
        || [MEMBERS, STORED_QUERIES].into_iter().any(|parent| {
            path.strip_prefix(parent)
                .is_some_and(|rest| rest.starts_with('/'))
        })
}

/// The page title of a view called `section`.
pub(crate) fn titled(section: &str) -> String {
    format!("{section} · {}", crate::app::PRODUCT)
}

/// The offset of the page the URL names in its `offset` query parameter.
pub(crate) fn offset_in_url() -> impl Fn() -> u64 + Send + Sync + Clone + 'static {
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
pub(crate) fn pager(path: &'static str, offset: u64, shown: usize, total: u64) -> AnyView {
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

/// The line naming the replica, or the replicas, that answered a view;
/// nothing when the replica has no name.
pub(crate) fn replica_line(line: Option<String>) -> AnyView {
    line.map(|line| view! { <p>{line}</p> }).into_any()
}

/// The section `section` draws of a view `loaded`, the notice of the refusal
/// that kept it, or the notice of the console's fault: never an empty view.
pub(crate) fn shown<T>(
    loaded: Result<Outcome<T>, ViewError>,
    section: impl FnOnce(T) -> AnyView,
) -> AnyView {
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

#[cfg(test)]
mod tests {
    use super::needs_session;

    #[test]
    fn every_view_each_members_page_and_each_stored_query_needs_a_session_and_nothing_else_does() {
        for path in super::PATHS {
            assert!(needs_session(path), "{path}");
        }
        assert!(needs_session("/members/node-a"));
        assert!(needs_session("/members/node-a/more"));
        assert!(!needs_session("/membersx"));
        assert!(needs_session("/stored-queries/org.example::compositions"));
        assert!(!needs_session("/stored-queriesx"));
        assert!(!needs_session("/"));
        assert!(!needs_session("/about"));
    }
}
