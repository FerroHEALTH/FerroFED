// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of the stored-query list and of one stored
//! query's detail.
//!
//! Every value reaches the markup as `view!` text or an attribute value,
//! which Leptos escapes, and never as a child of `textarea`, `script`,
//! `style` or `noscript`, so a definition's name or AQL holding markup reads
//! as text (the Leptos book, `view/01_basic_component`).

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ferrofed_registry::operator::{MAX_PAGE, Page, PageRequest, StoredVersion};
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::{Variant, label};
use ferrohealth_ui::view::code::code_panel;
use ferrohealth_ui::view::empty::empty_state;
use ferrohealth_ui::view::layout::{main_aside, tile_list};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::{
    Crumb, Header, card, definitions, page_heading, page_meta, stack,
};
use http::StatusCode;
use leptos::prelude::*;
use openehr_its::rest::generated::definition::StoredQuery;

use crate::config::settings::ReplicaSettings;
use crate::gateway::{AccessToken, Gateway, GatewayError};
use crate::locale::{Msg, tr, tr_with};
use crate::views::STORED_QUERIES;
use crate::views::load::server::answered_by;
use crate::views::stored::{StoredDetail, StoredView, console_href, detail_href};

/// One stored-query version as the operator surface lists it, with whether
/// the gateway takes a write under its name.
type Listed = StoredVersion<StoredQuery>;

/// The separator of a qualified query name's namespace and its name
/// (ITS-REST Query API, "Qualified query name").
const SEPARATOR: &str = "::";

/// The namespace and the name of the qualified query name `qualified`, the
/// namespace `None` for a name that has none.
#[must_use]
pub fn split(qualified: &str) -> (Option<&str>, &str) {
    match qualified.split_once(SEPARATOR) {
        Some((namespace, name)) => (Some(namespace), name),
        None => (None, qualified),
    }
}

/// The stored-query list of `report`, offering the drift repair at the
/// first of `replicas`.
#[must_use]
pub fn list(report: &Page<Listed>, replicas: &[ReplicaSettings]) -> StoredView {
    let mut names: Vec<String> = report
        .items
        .iter()
        .map(|entry| entry.definition.name.clone())
        .collect();
    names.dedup();
    let html = if report.total == 0 {
        none_held()
    } else {
        namespaces(&report.items)
    };
    StoredView {
        html,
        offset: report.offset,
        shown: u32::try_from(report.items.len()).unwrap_or(u32::MAX),
        total: report.total,
        repair_form: replicas
            .first()
            .map(|replica| crate::admin::fields::repair(replica, &names)),
        answered_by: answered_by(&[(&tr(Msg::StoredQueries2), report.replica.as_deref())]),
    }
}

/// The empty state of a gateway that holds no stored query.
fn none_held() -> String {
    view! {
        <div class="fui-card">
            {empty_state(
                Glyph::DATABASE,
                tr(Msg::NoStoredQueries),
                tr(Msg::GatewayHoldsNoStoredQuery),
                (),
            )}
        </div>
    }
    .to_html()
}

/// One card per namespace of `items`, each with its versions as tiles.
fn namespaces(items: &[Listed]) -> String {
    let mut grouped: BTreeMap<Option<&str>, Vec<&Listed>> = BTreeMap::new();
    for entry in items {
        grouped
            .entry(split(&entry.definition.name).0)
            .or_default()
            .push(entry);
    }
    let cards = grouped
        .into_iter()
        .enumerate()
        .map(|(index, (namespace, entries))| namespace_card(index, namespace, &entries))
        .collect_view();
    view! {
        <section class="fui-stack" aria-label=tr(Msg::Namespaces)>
            {cards}
        </section>
    }
    .to_html()
}

/// The card of `namespace`, the `index`th of the page, with the tile of
/// each of its `entries`, marked read-only when the gateway takes no write
/// under any of them.
fn namespace_card(
    index: usize,
    namespace: Option<&str>,
    entries: &[&Listed],
) -> impl IntoView + use<> {
    let heading = format!("namespace-{index}");
    let labelled = heading.clone();
    let count = match entries.len() {
        1 => tr(Msg::N1Version),
        n => tr_with(Msg::Versions2, &[("n", (n).to_string().into())]),
    };
    let read_only = entries
        .iter()
        .all(|entry| entry.read_only)
        .then(|| badge(Tone::Neutral, &tr(Msg::ReadOnlyBadge)));
    let title = namespace.map_or_else(|| tr(Msg::NoNamespace), str::to_owned);
    let tiles = entries
        .iter()
        .map(|entry| {
            let entry = &entry.definition;
            let (_, name) = split(&entry.name);
            let name = name.to_owned();
            let version = format!("@{}", entry.version);
            let href = detail_href(&entry.name, &entry.version);
            view! {
                <a href=href class="fui-mono">
                    {name}
                    <span class="fui-muted">{version}</span>
                </a>
            }
            .into_any()
        })
        .collect();
    view! {
        <section class="fui-card" aria-labelledby=labelled>
            <div class="fui-cluster">
                <h2 id=heading class="fui-card-title fui-mono fui-break">
                    {title}
                </h2>
                {badge(Tone::Accent, &count)}
                {read_only}
            </div>
            {tile_list(tr(Msg::Versions), tiles)}
        </section>
    }
}
/// Every held version of the stored query `name`, newest first, and the
/// replica that answered.
///
/// The versions are read page by page from the operator surface's versions
/// of that one name, and are none when the gateway holds no version under
/// it.
///
/// # Errors
/// Returns the [`GatewayError`] of the first page the gateway did not
/// answer, other than its `404` of a name it holds nothing under.
pub async fn versions_of(
    gateway: &Gateway,
    token: &AccessToken,
    name: &str,
) -> Result<(Vec<Listed>, Option<String>), GatewayError> {
    let mut held = Vec::new();
    let mut replica = None;
    let mut offset = 0_u64;
    loop {
        let asked = PageRequest {
            offset,
            limit: MAX_PAGE,
        };
        let page = match gateway.stored_query(token, name, asked).await {
            Ok(page) => page,
            Err(error) if none_held_under(&error) => break,
            Err(error) => return Err(error),
        };
        replica = replica.or(page.replica);
        let read = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
        held.extend(page.items);
        offset = offset.saturating_add(read);
        // NOTE: no specification governs this: our own design; an empty page
        // ends the walk, so a listing that shrinks while it is read cannot loop.
        if read == 0 || offset >= page.total {
            break;
        }
    }
    held.sort_by(|left, right| newest_first(&left.definition.version, &right.definition.version));
    Ok((held, replica))
}

/// The stable error code of the gateway's `404` for a stored-query name it
/// holds no version under.
const STORED_QUERY_UNKNOWN: &str = "stored-query-unknown";

/// Whether `error` is the gateway's answer that it holds no version under
/// the name asked: a `404` carrying [`STORED_QUERY_UNKNOWN`]. Any other
/// `404` is a refusal the page shows with its status and code.
fn none_held_under(error: &GatewayError) -> bool {
    error.refusal().is_some_and(|(status, code)| {
        status == StatusCode::NOT_FOUND && code.as_deref() == Some(STORED_QUERY_UNKNOWN)
    })
}

/// The order of two versions, the newer first: by their numbers where both
/// are dotted numbers (ITS-REST's semver segment), and as text otherwise.
fn newest_first(left: &str, right: &str) -> Ordering {
    let numbers = |version: &str| -> Option<Vec<u64>> {
        version.split('.').map(|part| part.parse().ok()).collect()
    };
    match (numbers(left), numbers(right)) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => right.cmp(left),
    }
}

/// The detail of the stored query `name`, whose versions the gateway holds
/// are `held`, newest first, showing version `asked` or the newest.
///
/// The detail is marked read-only when the gateway takes no write under the
/// version shown.
#[must_use]
pub fn detail(
    name: &str,
    held: &[Listed],
    asked: Option<&str>,
    replica: Option<&str>,
) -> StoredDetail {
    let Some(newest) = held.first() else {
        return StoredDetail {
            html: not_held(name),
        };
    };
    let chosen =
        asked.and_then(|asked| held.iter().find(|entry| entry.definition.version == asked));
    let missing = asked.filter(|_| chosen.is_none()).map(|asked| {
        notice(
            Tone::Warn,
            Announce::Alert,
            None,
            tr_with(
                Msg::GatewayHoldsNoVersionQueryNewest,
                &[("asked", (asked).to_string().into())],
            ),
        )
    });
    let shown = chosen.unwrap_or(newest);
    let read_only = shown
        .read_only
        .then(|| badge(Tone::Neutral, &tr(Msg::ReadOnlyBadge)));
    let shown = &shown.definition;
    let (namespace, short) = split(name);
    let open = view! {
        {read_only}
        <a href=console_href(name, &shown.version) rel="external" class=Variant::Primary.class()>
            {label(Some(Glyph::SQUARE_TERMINAL), tr(Msg::OpenInQueryConsole))}
        </a>
    };
    let header = page_heading(
        Header {
            title: name.to_owned(),
            subtitle: Some(tr_with(
                Msg::VersionStored,
                &[
                    ("version", (shown.version).clone().into()),
                    ("saved", (shown.saved).clone().into()),
                ],
            )),
            crumbs: vec![Crumb::new(tr(Msg::StoredQueries), STORED_QUERIES)],
            mono: true,
        },
        open,
    );
    let versions = versions_nav(name, held, &shown.version);
    let mono = |text: String| view! { <span class="fui-mono fui-break">{text}</span> }.into_any();
    let facts = definitions(vec![
        (
            tr(Msg::Namespace),
            mono(namespace.unwrap_or("none").to_owned()),
        ),
        (tr(Msg::Name), mono(short.to_owned())),
        (tr(Msg::Version), mono(shown.version.clone())),
        (tr(Msg::Stored), shown.saved.clone().into_any()),
    ]);
    let aql = code_panel(
        &tr_with(
            Msg::TheAqlOfVersion,
            &[("version", (shown.version).clone().into())],
        ),
        Language::Aql,
        &shown.q,
    );
    let replica = replica_line(replica);
    let html = view! {
        {header}
        <div class="fui-stack">
            {missing}
            {main_aside(
                card(tr(Msg::Aql), aql),
                stack(
                    view! {
                        {card(tr(Msg::Versions), versions)}
                        {card(tr(Msg::Details), facts)}
                    },
                ),
            )} {replica}
        </div>
    }
    .to_html();
    StoredDetail { html }
}

/// The version switch: a link to each of `held`, the one `shown` marked as
/// the page, in words as well as in its weight and tint.
fn versions_nav(name: &str, held: &[Listed], shown: &str) -> impl IntoView + use<> {
    let links = held
        .iter()
        .map(|entry| {
            let entry = &entry.definition;
            let here = entry.version == shown;
            let current = here.then_some("page");
            let mark = here.then(|| view! { <span class="fui-small">{tr(Msg::Shown)}</span> });
            view! {
                <li>
                    <a
                        href=detail_href(name, &entry.version)
                        class="fui-nav-entry"
                        aria-current=current
                    >
                        <span class="fui-mono">{entry.version.clone()}</span>
                        <span class="fui-small fui-muted">{entry.saved.clone()}</span>
                        {mark}
                    </a>
                </li>
            }
        })
        .collect_view();
    view! {
        <nav aria-label=tr(Msg::Versions)>
            <ul class="fui-nav-list">{links}</ul>
        </nav>
    }
}

/// The line naming the replica that answered, or saying none was named;
/// nothing when the replica has no name.
fn replica_line(replica: Option<&str>) -> impl IntoView + use<> {
    page_meta(answered_by(&[(&tr(Msg::StoredQueries2), replica)]))
}

/// The empty state of a stored query the gateway does not hold.
fn not_held(name: &str) -> String {
    let back = view! {
        <a href=STORED_QUERIES class=Variant::Secondary.class()>
            {label(Some(Glyph::ARROW_LEFT), tr(Msg::BackStoredQueries))}
        </a>
    };
    view! {
        <div class="fui-card">
            {empty_state(
                Glyph::SEARCH_X,
                tr(Msg::NoSuchStoredQuery),
                tr_with(
                    Msg::GatewayHoldsNoStoredQueryNamed,
                    &[("name", (name).to_string().into())],
                ),
                back,
            )}
        </div>
    }
    .to_html()
}
