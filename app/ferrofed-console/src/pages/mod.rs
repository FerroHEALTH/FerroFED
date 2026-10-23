// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages, each drawn on the server from what it reads of the
//! gateway, with FerroUI's templates and components.
//!
//! A page reads inside a `Suspense`, and its route renders
//! `SsrMode::Async`, so the server sends it whole. A read the gateway
//! refused draws its status and stable code in words, and a fault of the
//! console says what failed with the way to try again; neither draws an
//! empty view (FerroUI `leptos-ui.md` §4).

pub mod about;
pub mod configuration;
pub mod dashboard;
pub mod describe;
pub mod errors;
pub mod integrity;
pub mod node;
pub mod organisation;
pub mod organisations;
pub mod prefs;
pub mod query;
pub mod search;
pub mod services;
pub mod sign_in;
pub mod stored;
pub mod topology;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, link_button};
use ferrohealth_ui::view::refusal::{Issue, Refusal as Refused, Severity, fault, refusal};
use ferrohealth_ui::view::surface::disclosure;
use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_params_map};

use crate::locale::{Msg, tr, tr_with};
use crate::read::{Fault, Refusal, Stop};

/// The words of a read that stopped short of its view: the gateway's
/// refusal with its status and stable code, or the console's fault with
/// the way to try again.
#[must_use]
pub fn stopped(stop: &Stop) -> AnyView {
    match stop {
        Stop::Refused(refused) => refused_view(refused),
        Stop::Fault(failed) => fault_view(failed),
    }
}

/// The gateway's refusal `refused`, or the console's of input it cannot
/// send.
#[must_use]
pub fn refused_view(refused: &Refusal) -> AnyView {
    if let Refusal::Invalid { reason } = refused {
        return ferrohealth_ui::view::notice::notice(
            ferrohealth_ui::tone::Tone::Bad,
            ferrohealth_ui::view::notice::Announce::Alert,
            None,
            reason.clone(),
        )
        .into_any();
    }
    let (status, summary, code) = match refused {
        Refusal::Gateway { status, code } => {
            (Some(*status), tr(Msg::RefusedByGateway), code.clone())
        }
        Refusal::NotAuthenticated { code } => {
            (Some(401), tr(Msg::RefusedNotAuthenticated), code.clone())
        }
        Refusal::Invalid { reason } => (None, reason.clone(), None),
    };
    let issues = code
        .map(|code| {
            vec![Issue {
                severity: Severity::Error,
                code,
                text: None,
                place: String::new(),
                details: Vec::new(),
            }]
        })
        .unwrap_or_default();
    let sign_in = matches!(refused, Refusal::NotAuthenticated { .. }).then(|| {
        view! {
            <p>
                <a href=crate::app::SIGN_IN rel="external">
                    {tr(Msg::SignInAgain)}
                </a>
            </p>
        }
    });
    view! {
        {refusal(
            &Refused {
                status,
                summary,
                issues,
                request: None,
                evidence: None,
            },
        )}
        {sign_in}
    }
    .into_any()
}

/// The console's own fault `failed`, with the way to read the page again.
#[must_use]
pub fn fault_view(failed: &Fault) -> AnyView {
    let (words, detail) = match failed {
        Fault::Unreachable => (tr(Msg::FaultUnreachable), tr(Msg::FaultUnreachableDetail)),
        Fault::Unreadable { status } => (
            tr(Msg::FaultUnreadable),
            tr_with(
                Msg::FaultUnreadableDetail,
                &[("status", u32::from(*status).into())],
            ),
        ),
        Fault::SignedOut => (tr(Msg::FaultSignedOut), tr(Msg::FaultSignedOutDetail)),
        Fault::Unavailable => (tr(Msg::FaultUnavailable), tr(Msg::FaultUnavailableDetail)),
    };
    let here = use_location().pathname.get_untracked();
    view! {
        {fault(words)}
        <p class="fui-actions">
            {link_button(here, Variant::Secondary, Some(Glyph::REFRESH_CW), tr(Msg::TryAgain))}
        </p>
        {disclosure(tr(Msg::TechnicalDetail), view! { <p class="fui-small">{detail}</p> })}
    }
    .into_any()
}

/// The route's parameter `name`, empty when it has none.
#[must_use]
pub fn param(name: &str) -> String {
    use_params_map().with_untracked(|params| params.get(name).unwrap_or_default())
}

/// The value of the query parameter `name` of the request, if it has one.
#[must_use]
pub fn query_param(name: &str) -> Option<String> {
    leptos_router::hooks::use_query_map().with_untracked(|query| query.get(name))
}

/// The path of the page being drawn.
#[must_use]
pub fn here() -> String {
    use_location().pathname.get_untracked()
}

/// Sets the answer's status to `status`, for a page that is a refusal or
/// a fault of its own.
pub fn set_status(status: http::StatusCode) {
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(status);
    }
}

/// A time as the gateway writes it (RFC 3339), relative to now in a list
/// ("13 s ago"), with the full timestamp as its tooltip (family rule 11).
#[must_use]
pub fn relative(at: &str) -> AnyView {
    let Ok(then) = at.parse::<jiff::Timestamp>() else {
        return view! { <span>{at.to_owned()}</span> }.into_any();
    };
    let seconds = u64::try_from(jiff::Timestamp::now().duration_since(then).as_secs()).unwrap_or(0);
    let words = if seconds < 60 {
        tr_with(Msg::AgoSeconds, &[("count", seconds.into())])
    } else if seconds < 3600 {
        tr_with(Msg::AgoMinutes, &[("count", seconds.div_euclid(60).into())])
    } else if seconds < 86_400 {
        tr_with(Msg::AgoHours, &[("count", seconds.div_euclid(3600).into())])
    } else {
        tr_with(
            Msg::AgoDays,
            &[("count", seconds.div_euclid(86_400).into())],
        )
    };
    view! {
        <time datetime=at.to_owned() title=at.to_owned()>
            {words}
        </time>
    }
    .into_any()
}

/// A state as its shape, its colour and its word (family rule 12); an
/// excluded part is a muted word with no shape.
// TODO(#1233): draw FerroUI's F12 state once the library exposes it.
#[must_use]
pub fn state(health: crate::read::federation::Health) -> AnyView {
    let word = health.word();
    if health == crate::read::federation::Health::Excluded {
        return view! { <span class="fui-muted">{word}</span> }.into_any();
    }
    ferrohealth_ui::view::status::pill(health.tone(), &word).into_any()
}

/// Who made or applied a revision, in words.
#[must_use]
pub fn author(author: &ferrofed_registry::revision::Author) -> String {
    use ferrofed_registry::revision::Author;
    match author {
        Author::Operator { subject, .. } => subject.clone(),
        Author::File => tr(Msg::AuthorFile),
        Author::DevelopmentLoopback => tr(Msg::AuthorLoopback),
        Author::Gateway => tr(Msg::AuthorGateway),
        _ => tr(Msg::AuthorUnknown),
    }
}

/// A median latency in milliseconds, or a muted dash when none is measured.
#[must_use]
pub fn median(ms: Option<u64>) -> AnyView {
    match ms {
        Some(ms) => tr_with(Msg::Milliseconds, &[("count", ms.into())]).into_any(),
        None => view! {
            <span class="fui-muted" title=tr(Msg::NoMeasure)>
                "\u{2013}"
            </span>
        }
        .into_any(),
    }
}

/// What owns a member entry, as a provenance tag (F4) where the console
/// cannot change it.
#[must_use]
pub fn owner_tag(owner: ferrofed_registry::console::Owner) -> AnyView {
    use ferrofed_registry::console::Owner;
    use ferrohealth_ui::view::tag::Tag;
    match owner {
        Owner::Console => Tag::new(tr(Msg::OwnerConsole)).view(),
        Owner::File => Tag::new(tr(Msg::OwnerFile))
            .locked(tr(Msg::OwnerFileTitle))
            .view(),
        Owner::Directory => Tag::new(tr(Msg::OwnerDirectory))
            .locked(tr(Msg::OwnerDirectoryTitle))
            .view(),
        Owner::Gateway => Tag::new(tr(Msg::OwnerGateway))
            .locked(tr(Msg::OwnerGatewayTitle))
            .view(),
        Owner::Store => Tag::new(tr(Msg::OwnerStore)).view(),
        _ => Tag::new(tr(Msg::OwnerOther)).view(),
    }
}

/// A data table captioned `caption` over `columns` holding `rows`, with no
/// sortable column.
#[must_use]
pub fn table(
    caption: &str,
    columns: &[ferrohealth_ui::table::view::Column],
    rows: AnyView,
) -> AnyView {
    ferrohealth_ui::table::view::data_table(ferrohealth_ui::table::view::DataTable {
        caption,
        columns,
        sorting: ferrohealth_ui::table::view::Sorting::default(),
        body: ferrohealth_ui::table::view::Body::Rows(rows),
    })
    .into_any()
}
