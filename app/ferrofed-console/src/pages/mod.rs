// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages, each drawn from what it reads of the gateway, with
//! FerroUI's templates and components, by the server for the first page and
//! by the browser for every page after it.
//!
//! A page asks its reads under a `Resource` inside a `Suspense`, and its
//! route renders `SsrMode::Async`, so the server sends it whole. A read the gateway
//! refused draws its status and stable code in words, and a fault of the
//! console says what failed with the way to try again; neither draws an
//! empty view, as FerroUI asks.

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
use leptos_router::hooks::use_location;

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

/// `draw`, mounted afresh each time `version` changes, such as an action's
/// version once it answered.
///
/// A form's repeatable rows that FerroUI's widget added in the page are
/// nodes the renderer does not know, so a form drawn again in place would
/// keep them beside the rows its answer names; a fresh mount drops them.
pub fn afresh<F, V>(version: impl Fn() -> usize + Send + Sync + 'static, draw: F) -> AnyView
where
    F: Fn() -> V + Clone + Send + Sync + 'static,
    V: IntoView + 'static,
{
    view! { <For each=move || [version()] key=|drawn| *drawn children=move |_| draw.clone() /> }
        .into_any()
}

/// The route's parameter `name`, empty when it has none.
#[must_use]
pub fn param(name: &str) -> String {
    // NOTE: no specification governs this: our own design; a lazy route group draws its
    // page as the address changes, before the router's parameters follow, so they are read
    // from the address itself.
    let path = use_location().pathname.get_untracked();
    param_of(&path, name)
}

/// The route parameter `name` the address path `path` names, decoded, empty
/// when the route has none.
#[must_use]
pub fn param_of(path: &str, name: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let at = match (parts.first().copied(), name) {
        (Some("organisations"), "org")
        | (Some("services"), "service")
        | (Some("query" | "self-description" | "configuration"), "tab")
        | (Some("preferences"), "section")
        | (Some("stored-queries"), "name") => 1,
        (Some("organisations"), "node")
        | (Some("stored-queries"), "tab")
        | (Some("configuration"), "item") => 2,
        (Some("organisations"), "tab") => 3,
        _ => return String::new(),
    };
    parts
        .get(at)
        .map(|segment| {
            url::form_urlencoded::parse(format!("v={}", segment.replace('+', "%2B")).as_bytes())
                .map(|(_, value)| value.into_owned())
                .next()
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// The value of the query parameter `name` of the address, if it has one.
#[must_use]
pub fn query_param(name: &str) -> Option<String> {
    query_values(name).into_iter().next_back()
}

/// Every value of the query parameter `name` of the address, in order.
#[must_use]
pub fn query_values(name: &str) -> Vec<String> {
    // NOTE: no specification governs this: our own design; read from the address
    // itself, as `param` is, so a page drawn as the address changes reads the new one.
    let search = use_location().search.get_untracked();
    url::form_urlencoded::parse(search.trim_start_matches('?').as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// The path of the page being drawn.
#[must_use]
pub fn here() -> String {
    use_location().pathname.get_untracked()
}

/// Sets the answer's status to `status`, for a page that is a refusal or
/// a fault of its own.
pub fn set_status(status: http::StatusCode) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(status);
    }
    #[cfg(target_arch = "wasm32")]
    let _: http::StatusCode = status;
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

/// The first character of a slot's marker in a message's text: a private
/// use character no message holds.
const SLOT_OPEN: char = '\u{E000}';

/// The last character of a slot's marker.
const SLOT_CLOSE: char = '\u{E001}';

/// The message `msg` with each placeable of `slots` drawn as its view.
///
/// The placeables `args` are filled in as text, and each of `slots`, such
/// as a link or a code span, is drawn where the message puts it, for
/// phrasing content such as a page subtitle (`Header::subtitle_view`).
#[must_use]
pub fn marked(
    msg: Msg,
    args: &[(&'static str, ferrohealth_ui::i18n::Arg)],
    slots: Vec<(&'static str, AnyView)>,
) -> AnyView {
    let mut all = args.to_vec();
    let mut views: Vec<Option<AnyView>> = Vec::with_capacity(slots.len());
    for (index, (name, slot)) in slots.into_iter().enumerate() {
        all.push((name, format!("{SLOT_OPEN}{index}{SLOT_CLOSE}").into()));
        views.push(Some(slot));
    }
    let text = tr_with(msg, &all);
    let mut parts: Vec<AnyView> = Vec::new();
    let mut pieces = text.split(SLOT_OPEN);
    if let Some(lead) = pieces.next() {
        parts.push(lead.to_owned().into_any());
    }
    for piece in pieces {
        let (index, rest) = piece.split_once(SLOT_CLOSE).unwrap_or(("", piece));
        // A marker the catalog never wrote is no slot: its text stays as written.
        match index
            .parse::<usize>()
            .ok()
            .and_then(|index| views.get_mut(index))
            .and_then(Option::take)
        {
            Some(slot) => parts.push(slot),
            None => parts.push(index.to_owned().into_any()),
        }
        parts.push(rest.to_owned().into_any());
    }
    parts.into_iter().collect_view().into_any()
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

/// A latency in milliseconds, or a muted dash when none is measured.
#[must_use]
pub fn latency(ms: Option<u64>) -> AnyView {
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

/// A latency fact: [`latency`] with `title` naming the statistic and its
/// window, shown on hover and read by assistive technology.
#[must_use]
pub fn latency_fact(ms: Option<u64>, title: String) -> AnyView {
    view! { <span title=title>{latency(ms)}</span> }.into_any()
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

#[cfg(test)]
mod tests {
    use super::param_of;

    #[test]
    fn a_route_parameter_is_read_from_the_address_and_decoded() {
        assert_eq!(
            "org-a",
            param_of("/organisations/org-a/node-a/routes", "org")
        );
        assert_eq!(
            "node-a",
            param_of("/organisations/org-a/node-a/routes", "node")
        );
        assert_eq!(
            "routes",
            param_of("/organisations/org-a/node-a/routes", "tab")
        );
        assert_eq!("", param_of("/organisations/org-a", "tab"));
        assert_eq!(
            "access-log",
            param_of("/configuration/access-log/edit", "tab")
        );
        assert_eq!("edit", param_of("/configuration/access-log/edit", "item"));
        assert_eq!(
            "org.example::weights+v",
            param_of(
                "/stored-queries/org.example%3A%3Aweights%2Bv/versions",
                "name"
            )
        );
        assert_eq!("versions", param_of("/stored-queries/x/versions", "tab"));
        assert_eq!("stored", param_of("/query/stored", "tab"));
        assert_eq!("language", param_of("/preferences/language", "section"));
        assert_eq!("resolver", param_of("/services/resolver", "service"));
        assert_eq!("", param_of("/query", "org"));
    }
}
