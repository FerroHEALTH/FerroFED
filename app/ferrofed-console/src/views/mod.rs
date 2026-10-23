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

use ferrohealth_ui::table::view::{Body, Column, DataTable, Step, data_table, step_label};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::pill;
use ferrohealth_ui::view::surface::page_meta;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

use crate::locale::{Msg, tr, tr_with};
use crate::views::model::{Outcome, PAGE_SIZE, Refusal, ViewError};

/// The tone of the state word `word`, written in either case.
///
/// Up, complete and active take the good tone; down, failing, refused and
/// error the bad one; unknown, degraded, partial and pending the warning
/// one; any other word, which is no state, the neutral one.
#[must_use]
pub fn state_tone(word: &str) -> Tone {
    match word.to_ascii_lowercase().as_str() {
        "up" | "complete" | "ok" | "active" => Tone::Ok,
        "down" | "failing" | "refused" | "error" => Tone::Bad,
        "unknown" | "degraded" | "partial" | "pending" => Tone::Warn,
        _ => Tone::Neutral,
    }
}

/// A status pill: a dot and the state `word`, in the tone the word has.
#[must_use]
pub fn state_pill(word: &str) -> impl IntoView + use<> {
    pill(state_tone(word), word)
}

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
        tr_with(Msg::NoRowPageAll, &[("total", (total).to_string().into())])
    } else {
        tr_with(
            Msg::RowsToOf2,
            &[
                ("offset", (offset.saturating_add(1)).to_string().into()),
                ("end", (end).to_string().into()),
                ("total", (total).to_string().into()),
            ],
        )
    };
    let previous = step(
        Step::Previous,
        (offset > 0).then(|| {
            (
                format!("{path}?offset={}", offset.saturating_sub(PAGE_SIZE)),
                "prev",
            )
        }),
    );
    let next = step(
        Step::Next,
        (end < total).then(|| (format!("{path}?offset={end}"), "next")),
    );
    view! {
        <nav class="fui-pager" aria-label=tr(Msg::Pages)>
            <p role="status">{place}</p>
            <div class="fui-pager-steps">{previous} {next}</div>
        </nav>
    }
    .into_any()
}

/// One step of [`pager`]: a link to `to` with its `rel`, or a step that
/// cannot be taken, which says so in words.
fn step(step: Step, to: Option<(String, &'static str)>) -> AnyView {
    match to {
        Some((href, rel)) => view! {
            <a href=href rel=rel class=Variant::Secondary.small()>
                {step_label(step, true)}
            </a>
        }
        .into_any(),
        None => view! {
            <span class=Variant::Secondary.small() aria-disabled="true">
                {step_label(step, false)}
            </span>
        }
        .into_any(),
    }
}

/// The line naming the replica, or the replicas, that answered a view;
/// nothing when the replica has no name.
pub(crate) fn replica_line(line: Option<String>) -> AnyView {
    page_meta(line).into_any()
}

/// The library's data table under `caption`, headed by `columns`, over
/// `rows`, its rows stacked as cards on a phone: every body cell but the
/// row's name carries its column's `data-label`. The console's tables do not
/// sort.
pub(crate) fn table(caption: &str, columns: &[Column], rows: AnyView) -> AnyView {
    data_table(DataTable {
        caption,
        columns,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The data table [`table`] draws, as a grid at every width: for a table of
/// two short columns, which fits a phone as it is.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn table_unstacked(caption: &str, columns: &[Column], rows: AnyView) -> AnyView {
    ferrohealth_ui::table::view::data_table_unstacked(DataTable {
        caption,
        columns,
        sort: None,
        sort_href: &|_| String::new(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// Whether `columns` holds the column keyed `key`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn has(columns: &[Column], key: &str) -> bool {
    columns.iter().any(|column| column.key == key)
}

/// The alert `body` says, in the refusal's tone.
fn alert(body: impl IntoView + 'static) -> AnyView {
    notice(Tone::Bad, Announce::Alert, None, body).into_any()
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
            let code = code.clone().unwrap_or_else(|| tr(Msg::NoCode));
            alert(view! {
                {tr_with(Msg::GatewayDidNotAcceptYourSign2, &[("code", (code).clone().into())])}
                <a href=crate::app::SIGN_IN rel="external">
                    {tr(Msg::SignInAgain)}
                </a>
            })
        }
        Refusal::Gateway { status, code } => {
            let code = code.clone().unwrap_or_else(|| tr(Msg::NoCode));
            let hint = if code == "scope-insufficient" {
                tr(Msg::YourAccessTokenMayCarryNo)
            } else {
                String::new()
            };
            alert(tr_with(
                Msg::GatewayRefusedView,
                &[
                    ("status", (status).to_string().into()),
                    ("code", (code).clone().into()),
                    ("hint", (hint).clone().into()),
                ],
            ))
        }
        Refusal::Invalid { reason } => alert(reason.clone()),
    }
}

/// The inline notice of a view the console could not serve: never an empty
/// view.
pub(crate) fn fault(error: &ViewError) -> AnyView {
    match error {
        ViewError::SignedOut => alert(view! {
            {tr(Msg::SignSeeView)}
            <a href=crate::app::SIGN_IN rel="external">
                {tr(Msg::SignIn)}
            </a>
        }),
        ViewError::Unreadable { .. }
        | ViewError::Unreachable
        | ViewError::PlainPost
        | ViewError::Unavailable
        | ViewError::Fetch { .. } => alert(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use ferrohealth_ui::tone::Tone;

    use super::{needs_session, state_tone};

    #[test]
    fn a_pill_names_its_tone_by_the_word_in_either_case() {
        assert_eq!(Tone::Ok, state_tone("UP"));
        assert_eq!(Tone::Ok, state_tone("complete"));
        assert_eq!(Tone::Bad, state_tone("DOWN"));
        assert_eq!(Tone::Bad, state_tone("failing"));
        assert_eq!(Tone::Bad, state_tone("refused"));
        assert_eq!(Tone::Warn, state_tone("partial"));
        assert_eq!(Tone::Warn, state_tone("UNKNOWN"));
        assert_eq!(Tone::Warn, state_tone("degraded"));
        assert_eq!(Tone::Neutral, state_tone("withdrawn"));
        assert_eq!(Tone::Neutral, state_tone("registry-invalid"));
        assert_eq!(Tone::Neutral, state_tone("not reported"));
    }

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
