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
use http::StatusCode;
use leptos::prelude::*;
use openehr_its::rest::generated::definition::StoredQuery;

use crate::components::{badge, empty_state, icon};
use crate::config::settings::ReplicaSettings;
use crate::gateway::{AccessToken, Gateway, GatewayError};
use crate::highlight;
use crate::styles::{self, Tone};
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
        answered_by: answered_by(&[("stored queries", report.replica.as_deref())]),
    }
}

/// The empty state of a gateway that holds no stored query.
fn none_held() -> String {
    let body = view! { <p class=styles::EMPTY_BODY>"The gateway holds no stored query."</p> };
    view! {
        <div class=styles::CARD>
            {empty_state(icondata_lu::LuDatabase, "No stored queries", body, ())}
        </div>
    }
    .to_html()
}

/// One card per namespace of `items`, each with its versions as chips.
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
        <section class="grid grid-cols-1 gap-4" aria-label="Namespaces">
            {cards}
        </section>
    }
    .to_html()
}

/// The card of `namespace`, the `index`th of the page, with the chip of
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
        1 => String::from("1 version"),
        n => format!("{n} versions"),
    };
    let read_only = entries.iter().all(|entry| entry.read_only).then(|| {
        view! {
            <span class=Tone::Neutral
                .badge()>
                {icon(icondata_lu::LuLock, "12")} <span class="ml-1">"read-only"</span>
            </span>
        }
    });
    let title = namespace.map_or_else(|| String::from("No namespace"), str::to_owned);
    let chips = entries
        .iter()
        .map(|entry| {
            let entry = &entry.definition;
            let (_, name) = split(&entry.name);
            let name = name.to_owned();
            let version = format!("@{}", entry.version);
            let href = detail_href(&entry.name, &entry.version);
            view! {
                <li>
                    <a href=href class=CHIP>
                        <span class="min-w-0">{name}</span>
                        <span class="whitespace-nowrap text-muted">{version}</span>
                    </a>
                </li>
            }
        })
        .collect_view();
    view! {
        <section class=styles::CARD_PAD aria-labelledby=labelled>
            <header class="mb-3 flex flex-wrap items-center gap-2">
                <h2 id=heading class="m-0 font-mono text-sm font-semibold break-all text-ink">
                    {title}
                </h2>
                <span class=Tone::Accent.badge()>{count}</span>
                {read_only}
            </header>
            <ul class="m-0 flex list-none flex-wrap gap-2 p-0" role="list">
                {chips}
            </ul>
        </section>
    }
}

/// A chip that leads to one version of a stored query.
const CHIP: &str = "state-change inline-flex items-center rounded-full border border-line-strong \
                    bg-raised px-2.5 py-1 font-mono text-xs break-all text-ink no-underline \
                    hover:border-accent hover:text-accent";

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
        view! {
            <p role="alert">
                {format!(
                    "The gateway holds no version {asked} of this query. The newest is shown.",
                )}
            </p>
        }
    });
    let shown = chosen.unwrap_or(newest);
    let read_only = shown.read_only.then(|| badge("read-only", Tone::Neutral));
    let shown = &shown.definition;
    let (namespace, short) = split(name);
    let header = view! {
        <a href=STORED_QUERIES class="mb-3 inline-flex items-center gap-1 text-sm">
            {icon(icondata_lu::LuArrowLeft, "14")}
            "Stored queries"
        </a>
        <header class=styles::PAGE_HEADER>
            <div class="min-w-0">
                <h1 class="m-0 font-mono text-xl font-semibold break-all text-ink">
                    {name.to_owned()}
                </h1>
                <p class=format!("{} flex flex-wrap items-center gap-2", styles::PAGE_SUBTITLE)>
                    <span>{format!("Version {}, stored {}.", shown.version, shown.saved)}</span>
                    {read_only}
                </p>
            </div>
            <a href=console_href(name, &shown.version) rel="external" class=styles::BUTTON_PRIMARY>
                {icon(icondata_lu::LuSquareTerminal, "16")}
                "Open in query console"
            </a>
        </header>
    };
    let versions = versions_nav(name, held, &shown.version);
    let facts = view! {
        <dl class="m-0 grid grid-cols-[max-content_minmax(0,1fr)] gap-x-6 gap-y-2 text-sm">
            <dt class="text-muted">"Namespace"</dt>
            <dd class="m-0 font-mono break-all">{namespace.unwrap_or("none").to_owned()}</dd>
            <dt class="text-muted">"Name"</dt>
            <dd class="m-0 font-mono break-all">{short.to_owned()}</dd>
            <dt class="text-muted">"Version"</dt>
            <dd class="m-0 font-mono">{shown.version.clone()}</dd>
            <dt class="text-muted">"Stored"</dt>
            <dd class="m-0 break-all">{shown.saved.clone()}</dd>
        </dl>
    };
    let aql = highlight::code_block(
        format!("The AQL of version {}", shown.version),
        highlight::aql(&shown.q),
    );
    let replica = replica_line(replica);
    let html = view! {
        {header}
        {missing}
        <div class="grid grid-cols-1 items-start gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
            <section class=styles::CARD_PAD aria-labelledby="stored-aql">
                <h2 id="stored-aql" class=styles::CARD_TITLE>
                    "AQL"
                </h2>
                {aql}
            </section>
            <div class="flex flex-col gap-4">
                <section class=styles::CARD_PAD aria-labelledby="stored-versions">
                    <h2 id="stored-versions" class=styles::CARD_TITLE>
                        "Versions"
                    </h2>
                    {versions}
                </section>
                <section class=styles::CARD_PAD aria-labelledby="stored-facts">
                    <h2 id="stored-facts" class=styles::CARD_TITLE>
                        "Details"
                    </h2>
                    {facts}
                </section>
            </div>
        </div>
        {replica}
    }
    .to_html();
    StoredDetail { html }
}

/// The version switch: a link to each of `held`, the one `shown` marked as
/// the page, in words as well as in its weight and tint.
fn versions_nav(name: &str, held: &[Listed], shown: &str) -> impl IntoView {
    let links = held
        .iter()
        .map(|entry| {
            let entry = &entry.definition;
            let here = entry.version == shown;
            let current = here.then_some("page");
            let mark = here.then(|| view! { <span class="text-xs font-normal">"shown"</span> });
            view! {
                <li>
                    <a href=detail_href(name, &entry.version) class=VERSION aria-current=current>
                        <span class="font-mono">{entry.version.clone()}</span>
                        <span class="text-xs text-muted group-aria-[current=page]:text-accent">
                            {entry.saved.clone()}
                        </span>
                        {mark}
                    </a>
                </li>
            }
        })
        .collect_view();
    view! {
        <nav aria-label="Versions">
            <ul class="m-0 flex list-none flex-col gap-1 p-0" role="list">
                {links}
            </ul>
        </nav>
    }
}

/// A link to one version: while it is the one shown, tinted, set in bold
/// and marked by a bar at its start, so colour is never the only mark.
const VERSION: &str = "group state-change flex flex-wrap items-baseline gap-x-2 rounded-control px-3 \
                       py-1.5 text-ink no-underline hover:bg-inset hover:text-ink \
                       aria-[current=page]:bg-accent-tint aria-[current=page]:font-semibold \
                       aria-[current=page]:text-accent \
                       aria-[current=page]:shadow-[inset_3px_0_0_var(--accent)]";

/// The line naming the replica that answered, or saying none was named;
/// nothing when the replica has no name.
fn replica_line(replica: Option<&str>) -> impl IntoView {
    answered_by(&[("stored queries", replica)])
        .map(|line| view! { <p class="mt-4 text-xs text-muted">{line}</p> })
}

/// The empty state of a stored query the gateway does not hold.
fn not_held(name: &str) -> String {
    let body = view! {
        <p class=styles::EMPTY_BODY>{format!("The gateway holds no stored query named {name}.")}</p>
    };
    let back = view! {
        <a href=STORED_QUERIES class=styles::BUTTON_SECONDARY>
            {icon(icondata_lu::LuArrowLeft, "16")}
            "Back to the stored queries"
        </a>
    };
    view! {
        <div class=styles::CARD>
            {empty_state(icondata_lu::LuSearchX, "No such stored query", body, back)}
        </div>
    }
    .to_html()
}
