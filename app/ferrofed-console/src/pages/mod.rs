// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages, each drawn whole by the server from what it reads
//! of the gateway, with FerroUI's templates and components.
//!
//! A page's handler reads the gateway first and then draws, so the
//! document arrives whole and reads with the scripts refused. A read the
//! gateway refused draws its status and stable code in words, and a fault
//! of the console says what failed with the way to try again; neither
//! draws an empty view, as FerroUI asks. Each page writes its own markup
//! from escaped parts ([`h`], [`t`]) beside the library's components.

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

use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::button::{Variant, link_button};
use ferrohealth_ui::view::refusal::{Issue, Refusal as Refused, Severity, fault, refusal};
use ferrohealth_ui::view::surface::disclosure;

use crate::locale::{Msg, current, tr, tr_with};
use crate::read::{Fault, Refusal, Stop};

/// The markup `markup`, made by the page from escaped parts.
#[must_use]
pub const fn h(markup: String) -> Html {
    Html::raw(markup)
}

/// `text` escaped as a text node or an attribute value.
#[must_use]
pub fn t(text: impl AsRef<str>) -> Html {
    Html::text(text)
}

/// `part`, or no markup at all.
#[must_use]
pub fn opt(part: Option<Html>) -> Html {
    part.unwrap_or_default()
}

/// The words of a read that stopped short of its view: the gateway's
/// refusal with its status and stable code, or the console's fault with
/// the way to try again.
#[must_use]
pub fn stopped(stop: &Stop) -> Html {
    match stop {
        Stop::Refused(refused) => refused_view(refused),
        Stop::Fault(failed) => fault_view(failed),
    }
}

/// The gateway's refusal `refused`, or the console's of input it cannot
/// send.
#[must_use]
pub fn refused_view(refused: &Refusal) -> Html {
    if let Refusal::Invalid { reason } = refused {
        return ferrohealth_ui::view::notice::notice_text(
            ferrohealth_ui::tone::Tone::Bad,
            ferrohealth_ui::view::notice::Announce::Alert,
            reason,
        );
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
        .map(|code| vec![Issue::new(Severity::Error, code)])
        .unwrap_or_default();
    let sign_in = matches!(refused, Refusal::NotAuthenticated { .. }).then(|| {
        h(format!(
            "<p><a href=\"{}\" rel=\"external\">{}</a></p>",
            crate::app::SIGN_IN,
            t(tr(Msg::SignInAgain))
        ))
    });
    refusal(
        &current(),
        &Refused {
            status,
            summary,
            issues,
            request: None,
            evidence: None,
        },
    )
    .then(opt(sign_in))
}

/// The console's own fault `failed`, with the way to read the page again.
#[must_use]
pub fn fault_view(failed: &Fault) -> Html {
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
    Html::join([
        fault(words),
        h(format!(
            "<p class=\"fui-actions\">{}</p>",
            link_button(
                here(),
                Variant::Secondary,
                Some(Glyph::REFRESH_CW),
                tr(Msg::TryAgain)
            )
        )),
        disclosure(
            tr(Msg::TechnicalDetail),
            h(format!("<p class=\"fui-small\">{}</p>", t(detail))),
        ),
    ])
}

/// The route's parameter `name`, empty when it has none.
#[must_use]
pub fn param(name: &str) -> String {
    crate::visit::with(|visit| param_of(&visit.path, name))
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
            percent_encoding::percent_decode_str(segment)
                .decode_utf8_lossy()
                .into_owned()
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
    crate::visit::with(|visit| {
        url::form_urlencoded::parse(visit.query.as_bytes())
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
            .collect()
    })
}

/// The path of the page being drawn.
#[must_use]
pub fn here() -> String {
    crate::visit::with(|visit| visit.path.clone())
}

/// Sets the answer's status to `status`, for a page that is a refusal or
/// a fault of its own, unless an earlier part set a server error.
pub fn set_status(status: http::StatusCode) {
    crate::visit::with(|visit| visit.status(status));
}

/// A time as the gateway writes it (RFC 3339), relative to now in a list
/// ("13 s ago"), with the full timestamp as its tooltip (family rule 11).
#[must_use]
pub fn relative(at: &str) -> Html {
    let Ok(then) = at.parse::<jiff::Timestamp>() else {
        return h(format!("<span>{}</span>", t(at)));
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
    h(format!(
        "<time datetime=\"{at}\" title=\"{at}\">{}</time>",
        t(words),
        at = t(at)
    ))
}

/// The first character of a slot's marker in a message's text: a private
/// use character no message holds.
const SLOT_OPEN: char = '\u{E000}';

/// The last character of a slot's marker.
const SLOT_CLOSE: char = '\u{E001}';

/// The message `msg` with each placeable of `slots` drawn as its markup.
///
/// The placeables `args` are filled in as text, and each of `slots`, such
/// as a link or a code span, is drawn where the message puts it, for
/// phrasing content such as a page subtitle (`Header::subtitle_view`).
#[must_use]
pub fn marked(
    msg: Msg,
    args: &[(&'static str, ferrohealth_ui::i18n::Arg)],
    slots: Vec<(&'static str, Html)>,
) -> Html {
    let mut all = args.to_vec();
    let mut parts_of: Vec<Option<Html>> = Vec::with_capacity(slots.len());
    for (index, (name, slot)) in slots.into_iter().enumerate() {
        all.push((name, format!("{SLOT_OPEN}{index}{SLOT_CLOSE}").into()));
        parts_of.push(Some(slot));
    }
    let text = tr_with(msg, &all);
    let mut parts: Vec<Html> = Vec::new();
    let mut pieces = text.split(SLOT_OPEN);
    if let Some(lead) = pieces.next() {
        parts.push(t(lead));
    }
    for piece in pieces {
        let (index, rest) = piece.split_once(SLOT_CLOSE).unwrap_or(("", piece));
        // A marker the catalog never wrote is no slot: its text stays as written.
        match index
            .parse::<usize>()
            .ok()
            .and_then(|index| parts_of.get_mut(index))
            .and_then(Option::take)
        {
            Some(slot) => parts.push(slot),
            None => parts.push(t(index)),
        }
        parts.push(t(rest));
    }
    Html::join(parts)
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
pub fn latency(ms: Option<u64>) -> Html {
    match ms {
        Some(ms) => t(tr_with(Msg::Milliseconds, &[("count", ms.into())])),
        None => h(format!(
            "<span class=\"fui-muted\" title=\"{}\">\u{2013}</span>",
            t(tr(Msg::NoMeasure))
        )),
    }
}

/// A latency fact: [`latency`] with `title` naming the statistic and its
/// window, shown on hover and read by assistive technology.
#[must_use]
pub fn latency_fact(ms: Option<u64>, title: String) -> Html {
    h(format!(
        "<span title=\"{}\">{}</span>",
        t(title),
        latency(ms)
    ))
}

/// What owns a member entry, as a provenance tag (F4) where the console
/// cannot change it.
#[must_use]
pub fn owner_tag(owner: ferrofed_registry::console::Owner) -> Html {
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
pub fn table(caption: &str, columns: &[ferrohealth_ui::table::view::Column], rows: Html) -> Html {
    ferrohealth_ui::table::view::data_table(ferrohealth_ui::table::view::DataTable {
        caption,
        columns,
        sorting: ferrohealth_ui::table::view::Sorting::default(),
        body: ferrohealth_ui::table::view::Body::Rows(rows),
    })
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
