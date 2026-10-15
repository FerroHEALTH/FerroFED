// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of each form, as a replica reports it.

use ferrofed_registry::revision::{Applied, Imported};
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ReplicaAnswer};
use crate::admin::answer::replica_said;
use crate::admin::model::ActionAnswer;
use crate::configure::render::tables::diff_table;
use crate::configure::render::{SavedLine, StoredLine, mark_said};

/// The answer of a revision saved at `answer`'s replica, with its preview.
#[must_use]
pub fn saved(answer: &ReplicaAnswer<SavedLine>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "save", saved_line).to_html(),
    }
}

/// What a replica reports of a saved revision.
fn saved_line(replica: &str, line: &SavedLine) -> AnyView {
    let id = line.revision.id;
    let preview = match (&line.changes, line.running) {
        (Some(changes), Some(running)) => view! {
            <p>{format!("What it changes against the running revision {running}:")}</p>
            {diff_table(changes, id)}
        }
        .into_any(),
        _ => view! {
            <p role="alert">
                "The preview of its changes could not be read; reload the page to see it under Revisions."
            </p>
        }
        .into_any(),
    };
    view! {
        <p role="status">
            <strong>{format!("Replica {replica} saved revision {id}.")}</strong>
            " It is not applied: apply it with the form Apply revision once you have read its changes."
        </p>
        {preview}
    }
    .into_any()
}

/// The answer of an apply or a rollback at `answer`'s replica.
#[must_use]
pub fn applied(answer: &ReplicaAnswer<Applied>, what: &str) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, what, applied_line).to_html(),
    }
}

/// What a replica reports of an apply or a rollback.
fn applied_line(replica: &str, line: &Applied) -> AnyView {
    let restored = line
        .revision
        .restores
        .map(|restored| format!(", restoring the content of revision {restored}"))
        .unwrap_or_default();
    view! {
        <p role="status">
            <strong>
                {format!("Replica {replica} applied revision {}{restored}.", line.revision.id)}
            </strong>
            " "
            {mark_said(&line.current)}
        </p>
    }
    .into_any()
}

/// The answer of a stored-query version stored at `answer`'s replica.
#[must_use]
pub fn stored(answer: &ReplicaAnswer<StoredLine>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "store", stored_line).to_html(),
    }
}

/// What a replica reports of a stored-query version stored.
fn stored_line(replica: &str, line: &StoredLine) -> AnyView {
    view! {
        <p role="status">
            <strong>
                {format!("Replica {replica} stored {} version {}.", line.name, line.version)}
            </strong>
        </p>
    }
    .into_any()
}

/// The answer of an import at `answer`'s replica, with its changes.
#[must_use]
pub fn imported(answer: &ReplicaAnswer<Imported>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "import", imported_line).to_html(),
    }
}

/// What a replica reports of an import.
fn imported_line(replica: &str, line: &Imported) -> AnyView {
    let id = line.revision.id;
    view! {
        <p role="status">
            <strong>{format!("Replica {replica} imported revision {id} as a dry run.")}</strong>
            " It is not applied: apply it with the form Apply revision once you have read its changes."
        </p>
        <p>{format!("What it changes against the running revision {}:", line.diff.from)}</p>
        {diff_table(&line.diff.changes, id)}
    }
    .into_any()
}
