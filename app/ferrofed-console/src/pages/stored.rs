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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
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
use ferrohealth_ui::view::template::list::HeadingCount;
use openehr_its::rest::generated::definition::StoredQuery;

use crate::act::Acted;
use crate::act::fields::Fields;
use crate::act::repair::RepairOutcome;
use crate::app::STORED_QUERIES;
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, opt, refused_view, stopped, t};
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

/// Every stored version the gateway holds, as the operator of the request
/// reads them, with no AQL, which the home does not draw.
///
/// # Errors
/// Returns the [`Stop`] of a missing session or a read the gateway refused
/// or did not answer.
pub async fn held_now() -> Result<Vec<Held>, Stop> {
    let mut held = held().await?;
    for version in &mut held {
        version.definition.q.clear();
    }
    Ok(held)
}

/// Every version of the stored query `name`, newest first, as the operator
/// of the request reads them, with the AQL of the version `version` names
/// or of the newest alone.
///
/// # Errors
/// As [`held_now`].
pub async fn versions_now(name: String, version: Option<String>) -> Result<Vec<Held>, Stop> {
    let mut versions = versions_of(&name).await?;
    versions.sort_by_key(|held| std::cmp::Reverse(version_key(&held.definition.version)));
    let shown = version
        .filter(|asked| {
            versions
                .iter()
                .any(|held| held.definition.version == *asked)
        })
        .or_else(|| versions.first().map(|held| held.definition.version.clone()));
    // NOTE: no specification governs this: our own design; the page draws one version's
    // AQL, so no other is held while it draws.
    for held in &mut versions {
        if Some(&held.definition.version) != shown.as_ref() {
            held.definition.q.clear();
        }
    }
    Ok(versions)
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
pub async fn list() -> Html {
    let held_read = held_now().await;
    let body = {
        let heading = page_heading(
            &current(),
            Header {
                title: tr(Msg::StoredTitle).into(),
                subtitle: Some(tr(Msg::StoredSubtitle).into()),
                ..Header::default()
            },
            new_version(),
        );
        match held_read {
            Err(stop) => heading.then(stopped(&stop)),
            Ok(held) if held.is_empty() => heading.then(h(format!(
                "<p class=\"fui-muted\">{}</p>",
                t(tr(Msg::StoredNone))
            ))),
            Ok(held) => {
                let sections = Html::join(
                    grouped(&held)
                        .into_iter()
                        .map(|(namespace, names)| namespace_card(&namespace, &names)),
                );
                heading.then(stack(sections))
            }
        }
    };
    crate::app::title(tr(Msg::StoredTitle)).then(body)
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
fn namespace_card(namespace: &str, names: &BTreeMap<String, Vec<&Held>>) -> Html {
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
    let rows = Html::join(names.iter().map(|(name, versions)| {
        let shown = Html::join(
            versions
                .iter()
                .map(|version| Tag::new(version.definition.version.clone()).mono().view()),
        );
        h(format!(
            "<tr>{}{}</tr>",
            title_cell(
                Some(&href(name)),
                h(format!("<code>{}</code>", t(name))),
                None
            ),
            cell(
                &columns[1],
                h(format!("<span class=\"fui-cluster\">{shown}</span>"))
            ),
        ))
    }));
    panel_sections_card(
        namespace.to_owned(),
        panel_section(
            &current(),
            Some(SectionHead {
                title: namespace.to_owned(),
                count: Some(HeadingCount::All(
                    u64::try_from(names.len()).unwrap_or(u64::MAX),
                )),
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

/// One stored query, on the tab the path names, with the answer of the
/// drift repair `posted` to it at the top.
pub async fn page(posted: Option<Fields>) -> Html {
    let name = crate::pages::param("name");
    let tab = crate::pages::param("tab");
    if !matches!(tab.as_str(), "" | "versions") {
        return crate::pages::errors::not_found();
    }
    let asked = crate::pages::query_param("version");
    // NOTE: no specification governs this: our own design; the repair's answer is drawn
    // at the top and its dialog opens with the version shown again.
    let repaired = match posted {
        Some(fields) => Some(crate::act::repair::repair_drift(&name, &fields).await),
        None => None,
    };
    match versions_now(name.clone(), asked.clone()).await {
        Err(Stop::Refused(crate::read::Refusal::Gateway { status: 404, .. })) => {
            opt(repaired.map(repair_answer)).then(crate::pages::errors::not_found())
        }
        Err(stop) => opt(repaired.map(repair_answer)).then(stopped(&stop)),
        Ok(mut versions) => {
            versions
                .sort_by_key(|version| std::cmp::Reverse(version_key(&version.definition.version)));
            detail(
                &name,
                &versions,
                tab == "versions",
                asked.as_deref(),
                repaired,
            )
        }
    }
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
) -> Html {
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
            t(tr_with(
                Msg::StoredVersionNotHeld,
                &[("version", asked.to_owned().into())],
            )),
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
    let repair = crate::pages::configuration::console_manages().then(|| repair_form(&version));
    let answer = repaired.map(repair_answer);
    let read_only = shown.read_only.then(|| {
        Tag::new(tr(Msg::StoredReadOnly))
            .locked(tr(Msg::StoredReadOnlyTitle))
            .view()
    });
    crate::app::title(name.to_owned()).then(detail_page(
        &current(),
        DetailPage {
            header: Header {
                title: name.to_owned().into(),
                subtitle: None,
                subtitle_view: Some(crate::pages::marked(
                    Msg::StoredSubtitleOne,
                    &[("version", version.clone().into())],
                    vec![("name", h(format!("<code>{}</code>", t(name))))],
                )),
                crumbs: vec![Crumb::new(tr(Msg::StoredTitle), STORED_QUERIES)],
                mono: true,
                badge: read_only,
            },
            actions: ActionGroup::primary(Action::new(
                Some(Glyph::PLAY),
                tr(Msg::StoredRun),
                Act::Link(console_href(name, &version)),
            )),
            stepper: None,
            tabs: Some(tabs),
            body: Html::join([opt(missing), opt(answer), opt(repair), body]),
        },
    ))
}

/// The AQL of `shown`, highlighted with its line numbers.
fn aql(shown: &Held) -> Html {
    let text = shown.definition.q.clone();
    code_section(
        &current(),
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
fn versions_table(name: &str, versions: &[Held]) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("version", tr(Msg::ColumnVersion))
        },
        Column::text("saved", tr(Msg::ColumnSaved)),
        Column::text("owner", tr(Msg::ColumnOwner)),
    ];
    let rows = Html::join(versions.iter().map(|version| {
        let number = version.definition.version.clone();
        let link = format!("{}?version={}", href(name), encoded(&number));
        let owner = if version.read_only {
            Tag::new(tr(Msg::StoredReadOnly))
                .locked(tr(Msg::StoredReadOnlyTitle))
                .view()
        } else {
            Tag::new(tr(Msg::OwnerStore)).view()
        };
        h(format!(
            "<tr>{}{}{}</tr>",
            title_cell(Some(&link), h(format!("<code>{}</code>", t(&number))), None),
            cell(
                &columns[1],
                crate::pages::relative(&version.definition.saved)
            ),
            cell(&columns[2], owner),
        ))
    }));
    data_table(DataTable {
        caption: &tr(Msg::StoredTabVersions),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
}

/// The drift repair's form in its dialog: the version and the members,
/// posting to the stored query's page it sits on.
fn repair_form(version: &str) -> Html {
    let version_field = Field::new("repair-version", tr(Msg::ColumnVersion)).view(h(format!(
        "<input type=\"text\" id=\"repair-version\" name=\"version\" class=\"{INPUT}\" value=\"{}\" required>",
        t(version)
    )));
    let endpoints = Field::new("repair-endpoints", tr(Msg::RepairEndpoints))
        .help(tr(Msg::RepairEndpointsHelp))
        .view(h(format!(
            "<input type=\"text\" id=\"repair-endpoints\" name=\"endpoint\" class=\"{INPUT}\" value=\"*\" required>"
        )));
    let replica = crate::visit::with(crate::visit::Visit::replicas)
        .into_iter()
        .next()
        .unwrap_or_default();
    let action = crate::pages::here_with_query();
    let body = h(format!(
        "<form method=\"post\" action=\"{}\" class=\"fui-form\">{version_field}{endpoints}\
         <p class=\"fui-small fui-muted\">{}</p>\
         <input type=\"hidden\" name=\"{}\" value=\"yes\">\
         <div class=\"fui-dialog-actions\">\
         <button type=\"button\" class=\"{}\" command=\"close\" commandfor=\"{REPAIR_DIALOG}\">{}</button>\
         <button type=\"submit\" class=\"{}\">{}</button></div></form>",
        t(action),
        t(tr_with(Msg::RepairReplica, &[("replica", replica.into())])),
        crate::act::clear::CONFIRM,
        Variant::Secondary.class(),
        t(tr(Msg::Cancel)),
        Variant::Primary.class(),
        t(tr(Msg::RepairSubmit)),
    ));
    popover_form(PopoverForm {
        id: REPAIR_DIALOG.into(),
        variant: Variant::Secondary,
        glyph: Glyph::REFRESH_CW,
        label: tr(Msg::RepairAction).into(),
        title: tr(Msg::RepairTitle).into(),
        body,
    })
}

/// The answer of a drift repair: what the replica did, member by member.
fn repair_answer(outcome: RepairOutcome) -> Html {
    let answer = match outcome {
        RepairOutcome::Refused(refused) => return refused_view(&refused),
        RepairOutcome::Failed(fault) => return crate::pages::fault_view(&fault),
        RepairOutcome::Answered(answer) => answer,
    };
    match &answer.acted {
        Acted::Refused(_) | Acted::Fault(_) => {
            crate::configure::said::replica_said(&answer, &tr(Msg::RepairWhat), |_, _| {
                Html::empty()
            })
        }
        Acted::Done(line) => {
            let outcome = if line.complete {
                Outcome::Ok
            } else {
                Outcome::Warn
            };
            load_report(LoadReport {
                outcome,
                arrival: Arrival::AfterAction,
                title: t(tr_with(
                    if line.complete {
                        Msg::RepairDone
                    } else {
                        Msg::RepairPartly
                    },
                    &[
                        ("version", line.version.clone().into()),
                        ("replica", answer.replica.clone().into()),
                    ],
                )),
                counts: Vec::new(),
                action: None,
            })
            .then(crate::pages::query::outcome(&line.endpoints, line.status))
        }
    }
}
