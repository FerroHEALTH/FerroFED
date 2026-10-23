// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored queries the gateway holds: their home, grouped by namespace,
//! and one stored query's page.
//!
//! Each namespace is a section with its provenance (the gateway's own
//! namespace is locked, F4), each query with its versions newest first. A
//! query's page holds its AQL highlighted with line numbers (B1) and its
//! versions on binder tabs, the link that runs it in the query console,
//! and the drift repair, which sends one version again to the members
//! (§12.7). A qualified name is the definition's name (ITS-REST Query API,
//! "Qualified query name"), never a patient identifier, so it sits in the
//! path (§5.4.1, N33).

use std::collections::BTreeMap;

use ferrofed_registry::operator::StoredVersion;
use ferrohealth_ui::highlight::Language;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::code::{Code, CodeHead, code_section};
use ferrohealth_ui::view::form::{Field, INPUT};
use ferrohealth_ui::view::load_report::{Arrival, LoadReport, Outcome, load_report};
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::popover::{PopoverForm, popover_form};
use ferrohealth_ui::view::surface::{
    Crumb, Header, SectionHead, page_heading, panel_section, panel_sections_card, stack,
};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::tag::Tag;
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;
use openehr_its::rest::generated::definition::StoredQuery;

use crate::act::Acted;
use crate::act::repair::RepairOutcome;
use crate::app::STORED_QUERIES;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::{refused_view, stopped};
use crate::read::Stop;

/// The most pages of stored versions a page reads.
const PAGES: u64 = 20;

/// The namespace the gateway reserves for its own stored queries.
const GATEWAY_NAMESPACE: &str = "eu.ferrofed.eehrxf";

/// The id of the drift repair's dialog.
const REPAIR_DIALOG: &str = "repair-drift";

/// One stored version.
type Held = StoredVersion<StoredQuery>;

/// `text` percent-encoded for a path segment or a query value: every byte
/// but an ASCII letter, digit, `-`, `.`, `_`, `~` or `:` (RFC 3986 §2.1,
/// §3.3).
#[must_use]
pub fn encoded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~:".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            // NOTE: no specification governs this: our own design; a nibble is always a
            // hexadecimal digit, so the filter keeps both.
            out.extend(
                [byte >> 4, byte & 0x0F]
                    .into_iter()
                    .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
                    .map(|digit| digit.to_ascii_uppercase()),
            );
        }
    }
    out
}

/// The path of the stored query `name`.
#[must_use]
pub fn href(name: &str) -> String {
    format!("{STORED_QUERIES}/{}", encoded(name))
}

/// The path of the query console with version `version` of the stored
/// query `name` chosen to run: the name and the version, never its AQL or
/// a parameter.
#[must_use]
pub fn console_href(name: &str, version: &str) -> String {
    format!(
        "{}?stored={}&version={}",
        crate::pages::query::STORED_TAB,
        encoded(name),
        encoded(version)
    )
}

/// Every stored version the gateway holds, read page by page.
async fn held() -> Result<Vec<Held>, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let mut found = Vec::new();
    let mut offset = 0;
    for _ in 0..PAGES {
        let page = state
            .gateway()
            .stored_queries(
                &token,
                crate::read::stored_page(offset, crate::read::PAGE_SIZE),
            )
            .await?;
        let read = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
        found.extend(page.items);
        offset = offset.saturating_add(read);
        if read == 0 || offset >= page.total {
            break;
        }
    }
    Ok(found)
}

/// Every version of the stored query `name`, read page by page.
async fn versions_of(name: &str) -> Result<Vec<Held>, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let mut found = Vec::new();
    let mut offset = 0;
    for _ in 0..PAGES {
        let page = state
            .gateway()
            .stored_query(
                &token,
                name,
                crate::read::stored_page(offset, crate::read::PAGE_SIZE),
            )
            .await?;
        let read = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
        found.extend(page.items);
        offset = offset.saturating_add(read);
        // NOTE: no specification governs this: our own design; an empty page ends the
        // walk, so a listing that shrinks while it is read cannot loop.
        if read == 0 || offset >= page.total {
            break;
        }
    }
    Ok(found)
}

/// Every stored query's name with how many versions the gateway holds.
///
/// # Errors
/// Returns the [`Stop`] of a read that failed.
pub async fn names() -> Result<Vec<(String, u32)>, Stop> {
    let mut counted: BTreeMap<String, u32> = BTreeMap::new();
    for version in held().await? {
        let entry = counted.entry(version.definition.name.clone()).or_default();
        *entry = entry.saturating_add(1);
    }
    Ok(counted.into_iter().collect())
}

/// The order of the version `version`: its numbers, a part that is no
/// number after every number.
fn version_key(version: &str) -> Vec<(u64, String)> {
    version
        .split(['.', '-', '+'])
        .map(|part| {
            part.parse::<u64>()
                .map_or((u64::MAX, part.to_owned()), |number| {
                    (number, String::new())
                })
        })
        .collect()
}

/// The namespace of the qualified name `name`: what comes before its `::`.
fn namespace(name: &str) -> &str {
    name.split_once("::")
        .map_or(name, |(namespace, _)| namespace)
}

/// The versions `held` grouped by namespace and name, each name's versions
/// newest first.
fn grouped(held: &[Held]) -> BTreeMap<String, BTreeMap<String, Vec<&Held>>> {
    let mut groups: BTreeMap<String, BTreeMap<String, Vec<&Held>>> = BTreeMap::new();
    for version in held {
        let name = version.definition.name.clone();
        groups
            .entry(namespace(&name).to_owned())
            .or_default()
            .entry(name)
            .or_default()
            .push(version);
    }
    for names in groups.values_mut() {
        for versions in names.values_mut() {
            versions
                .sort_by_key(|version| std::cmp::Reverse(version_key(&version.definition.version)));
        }
    }
    groups
}

/// The stored queries.
#[must_use]
pub fn list() -> AnyView {
    let body = Suspend::new(async move {
        let heading = page_heading(
            Header {
                title: tr(Msg::StoredTitle).into(),
                subtitle: Some(tr(Msg::StoredSubtitle).into()),
                ..Header::default()
            },
            new_version(),
        )
        .into_any();
        match held().await {
            Err(stop) => view! {
                {heading}
                {stopped(&stop)}
            }
            .into_any(),
            Ok(held) if held.is_empty() => view! {
                {heading}
                <p class="fui-muted">{tr(Msg::StoredNone)}</p>
            }
            .into_any(),
            Ok(held) => {
                let sections = grouped(&held)
                    .into_iter()
                    .map(|(namespace, names)| namespace_card(&namespace, &names))
                    .collect_view();
                view! {
                    {heading}
                    {stack(sections)}
                }
                .into_any()
            }
        }
    });
    view! {
        {crate::app::title(tr(Msg::StoredTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
}

/// The title row's New stored query version, where the console manages
/// the configuration.
fn new_version() -> ActionGroup {
    if crate::pages::configuration::console_manages() {
        ActionGroup::primary(Action::new(
            Some(Glyph::PLUS),
            tr(Msg::StoredNewVersion),
            Act::Link(String::from(crate::pages::configuration::NEW_QUERY)),
        ))
    } else {
        ActionGroup::none()
    }
}

/// One namespace's card: its provenance and its queries with their
/// versions.
fn namespace_card(namespace: &str, names: &BTreeMap<String, Vec<&Held>>) -> AnyView {
    // NOTE: no specification governs this: our own design; the read-only flag is the
    // gateway's, so a namespace it reserves or holds read-only is marked so.
    let owned = namespace == GATEWAY_NAMESPACE;
    let read_only = names.values().flatten().any(|version| version.read_only);
    let provenance = if owned {
        Tag::new(tr(Msg::StoredReadOnly))
            .locked(tr(Msg::StoredGatewayOwned))
            .view()
    } else if read_only {
        Tag::new(tr(Msg::StoredReadOnly))
            .locked(tr(Msg::StoredReadOnlyTitle))
            .view()
    } else {
        Tag::new(namespace.to_owned()).mono().view()
    };
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("name", tr(Msg::ColumnQuery))
        },
        Column::text("versions", tr(Msg::ColumnVersions)),
    ];
    let rows = names
        .iter()
        .map(|(name, versions)| {
            let shown = versions
                .iter()
                .map(|version| Tag::new(version.definition.version.clone()).mono().view())
                .collect_view();
            view! {
                <tr>
                    {title_cell(Some(&href(name)), view! { <code>{name.clone()}</code> }, None)}
                    {cell(&columns[1], view! { <span class="fui-cluster">{shown}</span> })}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    panel_sections_card(
        namespace.to_owned(),
        panel_section(
            Some(SectionHead {
                title: namespace.to_owned(),
                count: Some(u64::try_from(names.len()).unwrap_or(u64::MAX)),
                actions: Some(provenance),
            }),
            data_table(DataTable {
                caption: namespace,
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(rows),
            }),
        ),
    )
}

/// One stored query, on the tab the path names.
#[must_use]
pub fn page() -> AnyView {
    let name = crate::pages::param("name");
    let tab = crate::pages::param("tab");
    if !matches!(tab.as_str(), "" | "versions") {
        return crate::pages::errors::not_found();
    }
    let asked = crate::pages::query_param("version");
    let repaired = use_context::<RepairOutcome>();
    let body = Suspend::new(async move {
        let read = versions_of(&name).await;
        match read {
            Err(Stop::Refused(crate::read::Refusal::Gateway { status: 404, .. })) => view! {
                {repaired.map(repair_answer)}
                {crate::pages::errors::not_found()}
            }
            .into_any(),
            Err(stop) => view! {
                {repaired.map(repair_answer)}
                {stopped(&stop)}
            }
            .into_any(),
            Ok(versions) => {
                let mut versions = versions;
                versions.sort_by_key(|version| {
                    std::cmp::Reverse(version_key(&version.definition.version))
                });
                detail(
                    &name,
                    &versions,
                    tab == "versions",
                    asked.as_deref(),
                    repaired,
                )
            }
        }
    });
    view! { <Suspense fallback=|| ()>{body}</Suspense> }.into_any()
}

/// The page of the stored query `name` and its `versions`, newest first,
/// on the Versions tab when `on_versions`, showing the version `asked`
/// names or the newest.
fn detail(
    name: &str,
    versions: &[Held],
    on_versions: bool,
    asked: Option<&str>,
    repaired: Option<RepairOutcome>,
) -> AnyView {
    let shown = asked
        .and_then(|asked| {
            versions
                .iter()
                .find(|version| version.definition.version.clone() == asked)
        })
        .or_else(|| versions.first());
    let Some(shown) = shown else {
        return crate::pages::errors::not_found();
    };
    let version = shown.definition.version.clone();
    let missing = asked.filter(|asked| *asked != version).map(|asked| {
        notice(
            Tone::Warn,
            Announce::Status,
            None,
            tr_with(
                Msg::StoredVersionNotHeld,
                &[("version", asked.to_owned().into())],
            ),
        )
    });
    let base = href(name);
    let count = u64::try_from(versions.len()).unwrap_or(u64::MAX);
    let tabs = TabRow {
        label: name.to_owned(),
        tabs: vec![
            Tab::new("aql", base.clone(), tr(Msg::StoredTabAql)),
            Tab::new(
                "versions",
                format!("{base}/versions"),
                tr(Msg::StoredTabVersions),
            )
            .count(
                count,
                tr_with(Msg::StoredVersionCount, &[("count", Arg::from(count))]),
            ),
        ],
        current: String::from(if on_versions { "versions" } else { "aql" }),
    };
    let body = if on_versions {
        versions_table(name, versions)
    } else {
        aql(shown)
    };
    let repair =
        crate::pages::configuration::console_manages().then(|| repair_form(name, &version));
    let answer = repaired.map(repair_answer);
    let read_only = shown.read_only.then(|| {
        Tag::new(tr(Msg::StoredReadOnly))
            .locked(tr(Msg::StoredReadOnlyTitle))
            .view()
    });
    view! {
        {crate::app::title(name.to_owned())}
        {detail_page(DetailPage {
            header: Header {
                title: name.to_owned().into(),
                subtitle: Some(
                    tr_with(Msg::StoredSubtitleOne, &[("version", version.clone().into())]).into(),
                ),
                crumbs: vec![Crumb::new(tr(Msg::StoredTitle), STORED_QUERIES)],
                mono: true,
                badge: read_only,
            },
            actions: ActionGroup::primary(
                Action::new(
                    Some(Glyph::PLAY),
                    tr(Msg::StoredRun),
                    Act::Link(console_href(name, &version)),
                ),
            ),
            stepper: None,
            tabs: Some(tabs),
            body: view! {
                {missing}
                {answer}
                {repair}
                {body}
            }
                .into_any(),
        })}
    }
    .into_any()
}

/// The AQL of `shown`, highlighted with its line numbers.
fn aql(shown: &Held) -> AnyView {
    let text = shown.definition.q.clone();
    code_section(
        CodeHead {
            title: tr_with(
                Msg::StoredAqlOf,
                &[("version", shown.definition.version.clone().into())],
            ),
            tags: vec![String::from("AQL")],
            digest: None,
            copy: None,
            actions: None,
        },
        &Code {
            label: tr(Msg::StoredTabAql),
            language: Language::Aql,
            text,
            ids: None,
            current: None,
            diagnostics: Vec::new(),
        },
    )
}

/// The versions of the stored query `name`, newest first.
fn versions_table(name: &str, versions: &[Held]) -> AnyView {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("version", tr(Msg::ColumnVersion))
        },
        Column::text("saved", tr(Msg::ColumnSaved)),
        Column::text("owner", tr(Msg::ColumnOwner)),
    ];
    let rows = versions
        .iter()
        .map(|version| {
            let number = version.definition.version.clone();
            let link = format!("{}?version={}", href(name), encoded(&number));
            let owner = if version.read_only {
                Tag::new(tr(Msg::StoredReadOnly))
                    .locked(tr(Msg::StoredReadOnlyTitle))
                    .view()
            } else {
                Tag::new(tr(Msg::OwnerStore)).view()
            };
            view! {
                <tr>
                    {title_cell(Some(&link), view! { <code>{number.clone()}</code> }, None)}
                    {cell(&columns[1], crate::pages::relative(&version.definition.saved))}
                    {cell(&columns[2], owner)}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::StoredTabVersions),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The drift repair's form in its dialog: the version and the members.
fn repair_form(name: &str, version: &str) -> AnyView {
    let action = format!("{}?version={}", href(name), encoded(version));
    let version_field = Field::new("repair-version", tr(Msg::ColumnVersion)).view(view! {
        <input
            type="text"
            id="repair-version"
            name="version"
            class=INPUT
            value=version.to_owned()
            required
        />
    });
    let endpoints = Field::new("repair-endpoints", tr(Msg::RepairEndpoints))
        .help(tr(Msg::RepairEndpointsHelp))
        .view(view! { <input type="text" id="repair-endpoints" name="endpoint" class=INPUT value="*" required /> });
    let replica = use_context::<crate::server::ConsoleState>()
        .and_then(|state| {
            state
                .settings()
                .admin
                .first()
                .map(|replica| replica.name.clone())
        })
        .unwrap_or_default();
    let body = view! {
        <form method="post" action=action class="fui-form">
            {version_field}
            {endpoints}
            <p class="fui-small fui-muted">
                {tr_with(Msg::RepairReplica, &[("replica", replica.into())])}
            </p>
            <input type="hidden" name=crate::act::clear::CONFIRM value="yes" />
            <div class="fui-dialog-actions">
                <button
                    type="button"
                    class=Variant::Secondary.class()
                    command="close"
                    commandfor=REPAIR_DIALOG
                >
                    {tr(Msg::Cancel)}
                </button>
                <button type="submit" class=Variant::Primary.class()>
                    {tr(Msg::RepairSubmit)}
                </button>
            </div>
        </form>
    }
    .into_any();
    popover_form(PopoverForm {
        id: REPAIR_DIALOG,
        variant: Variant::Secondary,
        glyph: Glyph::REFRESH_CW,
        label: tr(Msg::RepairAction).into(),
        title: tr(Msg::RepairTitle).into(),
        body,
    })
}

/// The answer of a drift repair: what the replica did, member by member.
fn repair_answer(outcome: RepairOutcome) -> AnyView {
    let answer = match outcome {
        RepairOutcome::Refused(refused) => return refused_view(&refused),
        RepairOutcome::Answered(answer) => answer,
    };
    match &answer.acted {
        Acted::Refused(_) | Acted::Fault(_) => {
            crate::configure::said::replica_said(&answer, &tr(Msg::RepairWhat), |_, _| {
                ().into_any()
            })
        }
        Acted::Done(line) => {
            let outcome = if line.complete {
                Outcome::Ok
            } else {
                Outcome::Warn
            };
            view! {
                {load_report(LoadReport {
                    outcome,
                    arrival: Arrival::AfterAction,
                    title: tr_with(
                            if line.complete { Msg::RepairDone } else { Msg::RepairPartly },
                            &[
                                ("version", line.version.clone().into()),
                                ("replica", answer.replica.clone().into()),
                            ],
                        )
                        .into_any(),
                    counts: Vec::new(),
                    action: None,
                })}
                {crate::pages::query::outcome(&line.endpoints, line.status)}
            }
            .into_any()
        }
    }
}
