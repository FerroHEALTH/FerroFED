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
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::TEXTAREA;
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::time::timestamp;

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
    badge(Tone::Neutral, &word).into_any()
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
        ("running", Tone::Ok)
    } else if running.is_some_and(|running| revision.id > running) {
        ("saved, not applied", Tone::Warn)
    } else if Some(revision.id) == previous
        || matches!(revision.origin, Origin::File | Origin::Rollback)
    {
        ("applied before", Tone::Neutral)
    } else {
        ("not running", Tone::Neutral)
    };
    pill(tone, word).into_any()
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
                "Compare with running"
                <span class="fui-sr-only">{format!(" (revision {id})")}</span>
            </a>
        }
    });
    let rollback = mark
        .filter(|mark| mark.revision == id && mark.awaiting.is_none())
        .map(|mark| crate::configure::render::top::rollback(mark, "main"));
    view! {
        <li class="fui-stack">
            <div class="fui-cluster">
                <h3 class="fui-card-title">{format!("Revision {id}")}</h3>
                {origin(revision)}
                {standing(revision, (running, previous))}
            </div>
            <p class="fui-muted">
                {author(&revision.author)}
                " · "
                {timestamp(&revision.at.to_string())}
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
        <p class="fui-help">{shown}</p>
        <ol class="fui-divided" aria-labelledby="history-title">
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
        <section class="fui-card" aria-labelledby="compare-title">
            <div class="fui-split">
                <h2 id="compare-title" class="fui-card-title">
                    {title}
                </h2>
                <a href="/configuration/history" class=Variant::Ghost.class()>
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
                class=Variant::Secondary.class()
            >
                {icon(Glyph::DOWNLOAD, "16")}
                "Download as TOML"
            </a>
        }
        .into_any(),
        None => {
            view! { <p class="fui-muted">"No revision runs, so none downloads."</p> }.into_any()
        }
    };
    view! {
        <section class="fui-card" aria-labelledby="export-title">
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
        <div class="fui-field">
            <label for="cfg-toml" class="fui-label">"TOML"</label>
            <textarea
                id="cfg-toml"
                name="form[toml]"
                rows="12"
                spellcheck="false"
                required
                class=format!("{TEXTAREA} fui-mono")
                data-lines="cfg-toml-lines"
                aria-describedby="cfg-toml-lines cfg-toml-error"
            ></textarea>
            <p id="cfg-toml-lines" class="fui-help">
                "0 lines"
            </p>
            {error_line("cfg-toml")}
        </div>
        {reason()}
        <div class="fui-action-bar">
            <button type="submit" class=Variant::Primary.class() aria-describedby="cfg-import-hint">
                "Import as a dry-run revision"
            </button>
            <p id="cfg-import-hint" class="fui-help">
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
        {form("main", "fui-stack", body)}
    }
    .into_any()
}
