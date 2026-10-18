// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of each form, as a replica reports it, or the refusal that
//! kept it from any replica, with the field it names.

use ferrofed_registry::revision::{Applied, Imported};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::notice::{Announce, notice};
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ReplicaAnswer};
use crate::admin::answer::replica_said;
use crate::admin::model::{ActionAnswer, FieldNote};
use crate::configure::plan::Refused;
use crate::configure::render::parts::diff_table;
use crate::configure::render::{SavedLine, StoredLine, mark_said};
use crate::views::model::Refusal;

/// The answer of a form `refused` kept from reaching any replica, marking
/// the field it names.
#[must_use]
pub fn refused(refused: &Refused) -> ActionAnswer {
    let field = refused
        .field
        .as_ref()
        .and_then(|id| match &refused.refusal {
            Refusal::Invalid { reason } => Some(FieldNote {
                id: id.clone(),
                message: reason.clone(),
            }),
            _ => None,
        });
    ActionAnswer {
        changed: false,
        html: crate::views::refusal(&refused.refusal).to_html(),
        field,
    }
}

/// The answer of a revision saved at `answer`'s replica, with its preview.
#[must_use]
pub fn saved(answer: &ReplicaAnswer<SavedLine>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "save", saved_line).to_html(),
        field: None,
    }
}

/// What a replica reports of a saved revision.
fn saved_line(replica: &str, line: &SavedLine) -> AnyView {
    let id = line.revision.id;
    let preview = match (&line.changes, line.running) {
        (Some(changes), Some(running)) => view! {
            <p>{format!("What it changes against the running revision {running}:")}</p>
            {diff_table(changes, (Some(running), id))}
        }
        .into_any(),
        _ => notice(
            Tone::Warn,
            Announce::Alert,
            None,
            "The preview of its changes could not be read; reload the page to see it in the card at the top.",
        )
        .into_any(),
    };
    view! {
        {said(view! {
            <strong>{format!("Replica {replica} saved revision {id}.")}</strong>
            " It is not applied: apply it from the card at the top once you have read its changes."
        })}
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
        field: None,
    }
}

/// What a replica reports of an apply or a rollback.
fn applied_line(replica: &str, line: &Applied) -> AnyView {
    let restored = line
        .revision
        .restores
        .map(|restored| format!(", restoring the content of revision {restored}"))
        .unwrap_or_default();
    let awaits = line.current.awaiting.as_ref().map(
        |_| " It awaits your confirmation: confirm it in the card at the top, or roll it back.",
    );
    said(view! {
        <strong>
            {format!("Replica {replica} applied revision {}{restored}.", line.revision.id)}
        </strong>
        " "
        {mark_said(&line.current)}
        {awaits}
    })
}

/// The answer of a stored-query version stored at `answer`'s replica.
#[must_use]
pub fn stored(answer: &ReplicaAnswer<StoredLine>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "store", stored_line).to_html(),
        field: None,
    }
}

/// What a replica reports of a stored-query version stored.
fn stored_line(replica: &str, line: &StoredLine) -> AnyView {
    said(view! {
        <strong>
            {format!("Replica {replica} stored {} version {}.", line.name, line.version)}
        </strong>
    })
}

/// The answer of an import at `answer`'s replica, with its changes.
#[must_use]
pub fn imported(answer: &ReplicaAnswer<Imported>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "import", imported_line).to_html(),
        field: None,
    }
}

/// What a replica reports of an import.
fn imported_line(replica: &str, line: &Imported) -> AnyView {
    let id = line.revision.id;
    view! {
        {said(view! {
            <strong>{format!("Replica {replica} imported revision {id} as a dry run.")}</strong>
            " It is not applied: apply it from the card at the top once you have read its changes."
        })}
        <p>{format!("What it changes against the running revision {}:", line.diff.from)}</p>
        {diff_table(&line.diff.changes, (Some(line.diff.from), id))}
    }
    .into_any()
}

/// The notice of what a replica did, `body` in words.
pub(crate) fn said(body: impl IntoView + 'static) -> AnyView {
    notice(Tone::Ok, Announce::Status, None, body).into_any()
}
