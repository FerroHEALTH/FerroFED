// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The History tab, a timeline of the revisions with each one's download
//! and its comparison with the running revision, and the Import and export
//! tab: the running revision's download and the import of TOML as a dry
//! run.

use ferrofed_registry::revision::{Origin, Revision, RevisionId};
use leptos::prelude::*;

use crate::configure::render::parts::{diff_table, error_line, form, head, hidden, reason};
use crate::configure::render::{Loaded, author, when};
use crate::styles;

/// Where `revision` came from, as a badge.
fn origin(revision: &Revision) -> AnyView {
    let word = match (revision.restores, revision.origin) {
        (Some(restored), _) => format!("rollback to {restored}"),
        (None, Origin::File) => String::from("file"),
        (None, Origin::Operator) => String::from("operator"),
        (None, Origin::Rollback) => String::from("rollback"),
        (None, Origin::Import) => String::from("import"),
        (None, _) => String::from("other"),
    };
    view! { <span class="badge tone-neutral">{word}</span> }.into_any()
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
        ("running", "pill tone-ok")
    } else if running.is_some_and(|running| revision.id > running) {
        ("saved, not applied", "pill tone-warn")
    } else if Some(revision.id) == previous
        || matches!(revision.origin, Origin::File | Origin::Rollback)
    {
        ("applied before", "pill tone-neutral")
    } else {
        ("not running", "pill tone-neutral")
    };
    view! { <span class=tone>{word}</span> }.into_any()
}

/// One revision of the timeline.
fn entry(loaded: &Loaded<'_>, revision: &Revision) -> AnyView {
    let running = loaded.running();
    let mark = loaded.standing.current.as_ref();
    let previous = mark.and_then(|mark| mark.previous);
    let id = revision.id;
    let compare = (Some(id) != running && running.is_some()).then(|| {
        view! {
            <a href=format!("/configuration/history?compare={id}") class=styles::BUTTON_GHOST>
                "Compare with running"
                <span class="sr-only">{format!(" (revision {id})")}</span>
            </a>
        }
    });
    let rollback = mark
        .filter(|mark| mark.revision == id && mark.awaiting.is_none())
        .map(|mark| crate::configure::render::top::rollback(mark, "main"));
    view! {
        <li class="relative border-l-2 border-line pb-6 pl-6 last:pb-0">
            <span
                class="absolute top-1 -left-[0.4375rem] size-3 rounded-full border-2 border-raised bg-line-strong"
                aria-hidden="true"
            ></span>
            <div class="flex flex-wrap items-center gap-2">
                <h3 class="m-0 text-sm font-semibold">{format!("Revision {id}")}</h3>
                {origin(revision)}
                {standing(revision, (running, previous))}
            </div>
            <p class="mt-1 mb-1 text-sm text-muted">
                {format!("{} · {}", author(&revision.author), when(revision.at))}
            </p>
            <p class="mt-0 mb-2 text-sm">{revision.reason.clone()}</p>
            <div class="flex flex-wrap items-center gap-1">
                <a
                    href=crate::configure::export::href(id)
                    rel="external"
                    download=""
                    class=styles::BUTTON_GHOST
                >
                    {crate::components::icon(icondata_lu::LuDownload, "16")}
                    {format!("Export revision {id} as TOML")}
                </a>
                {compare}
                {rollback}
            </div>
        </li>
    }
    .into_any()
}

/// The History tab: the newest revisions, newest first.
pub(super) fn history(loaded: &Loaded<'_>) -> AnyView {
    let Some(history) = loaded.history else {
        return ().into_any();
    };
    let shown = format!(
        "The {} newest of {} revisions, newest first.",
        history.items.len(),
        history.total
    );
    let entries = history
        .items
        .iter()
        .map(|revision| entry(loaded, revision))
        .collect_view();
    view! {
        {head(
            ("history-title", String::from("History")),
            (
                "Every revision, who made it and why; each one downloads as TOML.",
                "export-and-import",
            ),
            (),
        )}
        <p class="mt-0 mb-4 text-xs text-muted">{shown}</p>
        <ol class="m-0 list-none p-0 pl-2" aria-labelledby="history-title">
            {entries}
        </ol>
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
        || format!("Revision {other}"),
        |running| format!("Revision {other} against the running revision {running}"),
    );
    view! {
        <section class=format!("{} mb-4", styles::CARD_PAD) aria-labelledby="compare-title">
            <div class="flex flex-wrap items-start justify-between gap-3">
                <h2 id="compare-title" class="mt-0 mb-3 text-base font-semibold text-ink">
                    {title}
                </h2>
                <a href="/configuration/history" class=styles::BUTTON_GHOST>
                    "Close the comparison"
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
                class=styles::BUTTON_SECONDARY
            >
                {crate::components::icon(icondata_lu::LuDownload, "16")}
                "Download the running configuration as TOML"
            </a>
        }
        .into_any(),
        None => view! { <p class="m-0 text-muted">"No revision runs, so none downloads."</p> }
            .into_any(),
    };
    view! {
        <section class=format!("{} mb-4", styles::CARD_PAD) aria-labelledby="export-title">
            {head(
                ("export-title", String::from("Export")),
                (
                    "The configuration less its bootstrap core, each secret as its file reference alone.",
                    "export-and-import",
                ),
                (),
            )}
            {download}
        </section>
    }
    .into_any()
}

/// The import of TOML as a dry-run revision on the newest one.
pub(super) fn import(loaded: &Loaded<'_>) -> AnyView {
    let body = view! {
        {hidden("action", String::from("import"))}
        {hidden("base", loaded.base())}
        <div class=styles::FIELD>
            <label for="cfg-toml">"TOML"</label>
            <textarea
                id="cfg-toml"
                name="form[toml]"
                rows="16"
                spellcheck="false"
                required
                class="min-h-72 w-full"
                data-lines="cfg-toml-lines"
                aria-describedby="cfg-toml-lines cfg-toml-error"
            ></textarea>
            <p id="cfg-toml-lines" class=styles::HELP>
                "0 lines"
            </p>
            {error_line("cfg-toml")}
        </div>
        {reason()}
        <div class="flex flex-wrap items-center gap-3 border-t border-line pt-4">
            <button type="submit" class=styles::BUTTON_PRIMARY aria-describedby="cfg-import-hint">
                "Import as a dry-run revision"
            </button>
            <p id="cfg-import-hint" class="m-0 text-sm text-muted">
                {format!(
                    "It is saved as {} and not applied; the outcome shows what it changes.",
                    loaded.next(),
                )}
            </p>
        </div>
    };
    view! {
        {head(
            ("import-title", String::from("Import")),
            (
                "Paste the TOML an export wrote, or one kept in version control.",
                "export-and-import",
            ),
            (),
        )}
        {form("main", "", body)}
    }
    .into_any()
}
