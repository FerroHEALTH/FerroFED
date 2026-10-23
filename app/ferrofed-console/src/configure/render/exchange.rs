// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The History tab, a timeline of the revisions with each one's download
//! and its comparison with the running revision, and the Import and export
//! tab: the running revision's download and the import of TOML as a dry
//! run.

use ferrofed_registry::revision::{Origin, Revision, RevisionId};
use leptos::prelude::*;

use crate::configure::render::parts::{diff_table, error_line, form, head, hidden, reason};
use crate::configure::render::{Loaded, author};
use crate::locale::{Msg, tr, tr_with};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::filter::{FilterChoice, FilterMenu, MenuForm, filter_menu};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{Field, TEXTAREA};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::time::timestamp;

/// The origins the history filters by, each its value in the address, its
/// word and the tone of its badge.
const ORIGINS: [(&str, Msg, Tone); 5] = [
    ("operator", Msg::OriginOperator, Tone::Neutral),
    ("file", Msg::OriginFile, Tone::Info),
    ("rollback", Msg::OriginRollback, Tone::Warn),
    ("import", Msg::OriginImport, Tone::Accent),
    ("other", Msg::OriginOther, Tone::Neutral),
];

/// The value in the address of the origin of `revision`.
const fn origin_of(revision: &Revision) -> &'static str {
    match revision.origin {
        Origin::Operator => "operator",
        Origin::File => "file",
        Origin::Rollback => "rollback",
        Origin::Import => "import",
        _ => "other",
    }
}

/// Where `revision` came from, as a badge; a rollback names the revision
/// it restores.
fn origin(revision: &Revision) -> AnyView {
    let key = origin_of(revision);
    let (word, tone) = ORIGINS
        .iter()
        .find(|(value, _, _)| *value == key)
        .map_or((tr(Msg::OriginOther), Tone::Neutral), |(_, word, tone)| {
            (tr(*word), *tone)
        });
    let word = revision.restores.map_or(word, |restored| {
        tr_with(
            Msg::RollbackTo,
            &[("restored", (restored).to_string().into())],
        )
    });
    badge(tone, &word).into_any()
}

/// The origin menu over `revisions` (T1's filter menu): a choice for each
/// origin they hold, with how many hold it, those the address `asked`
/// names checked, or every one when it names none.
fn origin_menu(revisions: &[Revision], asked: &[String]) -> AnyView {
    let choices = ORIGINS
        .iter()
        .filter_map(|(value, word, _)| {
            let count = revisions
                .iter()
                .filter(|revision| origin_of(revision) == *value)
                .count();
            (count > 0).then(|| {
                FilterChoice::new(
                    *value,
                    tr(*word),
                    u64::try_from(count).unwrap_or(u64::MAX),
                    asked.is_empty() || asked.iter().any(|origin| origin == value),
                )
            })
        })
        .collect();
    filter_menu(FilterMenu {
        id: "history-origin",
        name: "origin",
        glyph: Glyph::LIST_FILTER,
        label: tr(Msg::HistoryOrigin).into(),
        title: tr(Msg::HistoryOriginTitle),
        choices,
        note: None,
        form: MenuForm::Own {
            action: Some("/configuration/history"),
            fields: &[],
        },
    })
}

/// Where `revision` stands against the running one `running` and the one
/// before it `previous`, as a pill that says so in words.
///
/// The gateway records which revision runs and which ran before it; any
/// other revision older than the running one is shown as not running.
fn standing(
    revision: &Revision,
    (running, previous): (Option<RevisionId>, Option<RevisionId>),
) -> AnyView {
    let (word, tone) = if Some(revision.id) == running {
        (tr(Msg::StateRunning), Tone::Ok)
    } else if running.is_some_and(|running| revision.id > running) {
        (tr(Msg::StatePendingWord), Tone::Warn)
    } else if Some(revision.id) == previous
        || matches!(revision.origin, Origin::File | Origin::Rollback)
    {
        (tr(Msg::AppliedBefore), Tone::Neutral)
    } else {
        (tr(Msg::NotRunning), Tone::Neutral)
    };
    pill(tone, &word).into_any()
}

/// One revision of the timeline.
fn entry(loaded: &Loaded<'_>, revision: &Revision) -> AnyView {
    let running = loaded.running();
    let mark = loaded.standing.current.as_ref();
    let previous = mark.and_then(|mark| mark.previous);
    let id = revision.id;
    let compare = (Some(id) != running && running.is_some()).then(|| {
        view! {
            <a href=format!("/configuration/history?compare={id}") class=Variant::Ghost.class()>
                {tr(Msg::CompareWithRunning)}
                <span class="fui-sr-only">
                    {tr_with(Msg::Revision, &[("id", (id).to_string().into())])}
                </span>
            </a>
        }
    });
    let rollback = mark
        .filter(|mark| mark.revision == id && mark.awaiting.is_none())
        .map(|mark| crate::configure::render::top::rollback(mark, "main"));
    view! {
        <li class="fui-stack">
            <div class="fui-cluster">
                <h3 class="fui-card-title">
                    {tr_with(Msg::Revision2, &[("id", (id).to_string().into())])}
                </h3>
                {origin(revision)}
                {standing(revision, (running, previous))}
            </div>
            <p class="fui-muted">
                {author(&revision.author)} " · " {timestamp(&revision.at.to_string())}
            </p>
            <p class="fui-small">{revision.reason.clone()}</p>
            <div class="fui-cluster">
                <a
                    href=crate::configure::export::href(id)
                    rel="external"
                    download=""
                    class=Variant::Ghost.class()
                >
                    {icon(Glyph::DOWNLOAD, "16")}
                    {tr_with(Msg::ExportRevisionAsToml, &[("id", (id).to_string().into())])}
                </a>
                {compare}
                {rollback}
            </div>
        </li>
    }
    .into_any()
}

/// The History tab: the newest revisions, newest first, those of the
/// origins the address names when it names some.
pub(super) fn history(loaded: &Loaded<'_>) -> AnyView {
    let Some(history) = loaded.history else {
        return ().into_any();
    };
    let asked: Vec<String> = crate::pages::query_values("origin")
        .into_iter()
        .filter(|origin| ORIGINS.iter().any(|(value, _, _)| value == origin))
        .collect();
    let shown = tr_with(
        Msg::NewestRevisionsNewestFirst,
        &[
            ("history", (history.items.len()).to_string().into()),
            ("total", (history.total).to_string().into()),
        ],
    );
    let kept: Vec<&Revision> = history
        .items
        .iter()
        .filter(|revision| {
            asked.is_empty() || asked.iter().any(|origin| origin == origin_of(revision))
        })
        .collect();
    let entries = if kept.is_empty() {
        view! { <p class="fui-muted">{tr(Msg::HistoryNoneOfOrigin)}</p> }.into_any()
    } else {
        let items = kept
            .into_iter()
            .map(|revision| entry(loaded, revision))
            .collect_view();
        view! {
            <ol class="fui-divided" aria-labelledby="history-title">
                {items}
            </ol>
        }
        .into_any()
    };
    view! {
        {head(
            ("history-title", tr(Msg::History)),
            (tr(Msg::EveryRevisionWhoMadeWhyEach), "export-and-import"),
            (),
        )}
        <div class="fui-split">
            <p class="fui-help">{shown}</p>
            {origin_menu(&history.items, &asked)}
        </div>
        {entries}
    }
    .into_any()
}

/// The comparison the history shows above its timeline, when one revision
/// is compared with the running one.
pub(super) fn compared(loaded: &Loaded<'_>) -> AnyView {
    let Some((other, changes)) = loaded.compared else {
        return ().into_any();
    };
    let running = loaded.running();
    let title = running.map_or_else(
        || tr_with(Msg::Revision3, &[("other", (other).to_string().into())]),
        |running| {
            tr_with(
                Msg::RevisionAgainstRunningRevision,
                &[
                    ("other", (other).to_string().into()),
                    ("running", (running).to_string().into()),
                ],
            )
        },
    );
    view! {
        <section class="fui-card" aria-labelledby="compare-title">
            <div class="fui-split">
                <h2 id="compare-title" class="fui-card-title">
                    {title}
                </h2>
                <a href="/configuration/history" class=Variant::Ghost.class()>
                    {tr(Msg::CloseTheComparison)}
                </a>
            </div>
            {diff_table(changes, (running, other))}
        </section>
    }
    .into_any()
}

/// The download of the running revision, above the import.
pub(super) fn export(loaded: &Loaded<'_>) -> AnyView {
    let download = match loaded.running() {
        Some(running) => view! {
            <a
                href=crate::configure::export::href(running)
                rel="external"
                download=""
                class=Variant::Secondary.class()
            >
                {icon(Glyph::DOWNLOAD, "16")}
                {tr(Msg::DownloadAsToml)}
            </a>
        }
        .into_any(),
        None => {
            view! { <p class="fui-muted">{tr(Msg::NoRevisionRunsSoNoneDownloads)}</p> }.into_any()
        }
    };
    view! {
        <section class="fui-card" aria-labelledby="export-title">
            {head(
                ("export-title", tr(Msg::Export)),
                (tr(Msg::ConfigurationLessBootstrapCoreEachSecret), "export-and-import"),
                (),
            )}
            {download}
        </section>
    }
    .into_any()
}

/// The import of TOML as a dry-run revision on the newest one.
pub(super) fn import(loaded: &Loaded<'_>) -> AnyView {
    let posted = loaded
        .drafted(&["import"])
        .map(|form| form.toml.clone())
        .unwrap_or_default();
    let body = view! {
        {hidden("action", String::from("import"))}
        {hidden("base", loaded.base())}
        {Field::new("cfg-toml", tr(Msg::Toml))
            .after(error_line("cfg-toml"))
            .view(
                view! {
                    <textarea
                        id="cfg-toml"
                        name="form[toml]"
                        rows="12"
                        spellcheck="false"
                        required
                        class=format!("{TEXTAREA} fui-mono")
                        aria-describedby="cfg-toml-error"
                    >
                        {posted}
                    </textarea>
                },
            )}
        {reason()}
        <div class="fui-actions">
            <button type="submit" class=Variant::Primary.class() aria-describedby="cfg-import-hint">
                {tr(Msg::ImportDryRunRevision)}
            </button>
            <p id="cfg-import-hint" class="fui-help">
                {tr_with(
                    Msg::SavedNotAppliedOutcomeShowsWhat,
                    &[("loaded", (loaded.next()).clone().into())],
                )}
            </p>
        </div>
    };
    view! {
        {head(
            ("import-title", tr(Msg::Import)),
            (tr(Msg::PasteTomlExportWroteOneKept), "export-and-import"),
            (),
        )}
        {form("fui-stack", body)}
    }
    .into_any()
}
