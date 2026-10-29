// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of each form, as a replica reports it, or the refusal that
//! kept it from any replica, with the field it names.

use ferrofed_registry::revision::{Applied, Imported};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::notice::{Announce, notice};
use leptos::prelude::*;

use crate::act::{Acted, ReplicaAnswer};
use crate::configure::plan::Refused;
use crate::configure::render::parts::diff_table;
use crate::configure::render::{SavedLine, StoredLine, mark_said};
use crate::configure::said::replica_said;
use crate::configure::said::{ActionAnswer, Answer, FieldNote};
use crate::locale::{Msg, tr, tr_with};
use crate::read::Refusal;

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
        answer: Answer::Refused(refused.refusal.clone()),
        field,
    }
}

/// The answer of a revision saved at `answer`'s replica, with its preview.
#[must_use]
pub fn saved(answer: &ReplicaAnswer<SavedLine>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        answer: Answer::Saved(answer.clone()),
        field: None,
    }
}

/// The answer of `answer` drawn.
#[must_use]
pub fn saved_view(answer: &ReplicaAnswer<SavedLine>) -> AnyView {
    replica_said(answer, "save", saved_line).into_any()
}

/// What a replica reports of a saved revision.
fn saved_line(replica: &str, line: &SavedLine) -> AnyView {
    let id = line.revision.id;
    let preview = match (&line.changes, line.running) {
        (Some(changes), Some(running)) => view! {
            <p>
                {tr_with(
                    Msg::WhatChangesAgainstRunningRevision,
                    &[("running", (running).to_string().into())],
                )}
            </p>
            {diff_table(changes, (Some(running), id))}
        }
        .into_any(),
        _ => notice(
            Tone::Warn,
            Announce::Alert,
            None,
            tr(Msg::PreviewChangesCouldNotReadReload),
        )
        .into_any(),
    };
    view! {
        {said(
            view! {
                <strong>
                    {tr_with(
                        Msg::ReplicaSavedRevision,
                        &[
                            ("replica", (replica).to_string().into()),
                            ("id", (id).to_string().into()),
                        ],
                    )}
                </strong>
                {tr(Msg::NotAppliedApplyFromCardTop)}
            },
        )}
        {preview}
    }
    .into_any()
}

/// The answer of an apply or a rollback at `answer`'s replica.
#[must_use]
pub fn applied(answer: &ReplicaAnswer<Applied>, what: &str) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        answer: Answer::Applied(answer.clone(), what.to_owned()),
        field: None,
    }
}

/// The answer of `answer` drawn.
#[must_use]
pub fn applied_view(answer: &ReplicaAnswer<Applied>, what: &str) -> AnyView {
    replica_said(answer, what, applied_line).into_any()
}

/// What a replica reports of an apply or a rollback.
fn applied_line(replica: &str, line: &Applied) -> AnyView {
    let restored = line
        .revision
        .restores
        .map(|restored| {
            tr_with(
                Msg::RestoringContentRevision,
                &[("restored", (restored).to_string().into())],
            )
        })
        .unwrap_or_default();
    let awaits = line
        .current
        .awaiting
        .as_ref()
        .map(|_| tr(Msg::AwaitsYourConfirmationConfirmCardTop));
    said(view! {
        <strong>
            {tr_with(
                Msg::ReplicaAppliedRevision,
                &[
                    ("replica", (replica).to_string().into()),
                    ("id", (line.revision.id).to_string().into()),
                    ("restored", (restored).clone().into()),
                ],
            )}
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
        answer: Answer::Stored(answer.clone()),
        field: None,
    }
}

/// The answer of `answer` drawn.
#[must_use]
pub fn stored_view(answer: &ReplicaAnswer<StoredLine>) -> AnyView {
    replica_said(answer, "store", stored_line).into_any()
}

/// What a replica reports of a stored-query version stored.
fn stored_line(replica: &str, line: &StoredLine) -> AnyView {
    said(view! {
        <strong>
            {tr_with(
                Msg::ReplicaStoredVersion,
                &[
                    ("replica", (replica).to_string().into()),
                    ("name", (line.name).clone().into()),
                    ("version", (line.version).clone().into()),
                ],
            )}
        </strong>
    })
}

/// The answer of an import at `answer`'s replica, with its changes.
#[must_use]
pub fn imported(answer: &ReplicaAnswer<Imported>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        answer: Answer::Imported(answer.clone()),
        field: None,
    }
}

/// The answer of `answer` drawn.
#[must_use]
pub fn imported_view(answer: &ReplicaAnswer<Imported>) -> AnyView {
    replica_said(answer, "import", imported_line).into_any()
}

/// What a replica reports of an import.
fn imported_line(replica: &str, line: &Imported) -> AnyView {
    let id = line.revision.id;
    view! {
        {said(
            view! {
                <strong>
                    {tr_with(
                        Msg::ReplicaImportedRevisionDryRun,
                        &[
                            ("replica", (replica).to_string().into()),
                            ("id", (id).to_string().into()),
                        ],
                    )}
                </strong>
                {tr(Msg::NotAppliedApplyFromCardTop)}
            },
        )}
        <p>
            {tr_with(
                Msg::WhatChangesAgainstRunningRevision2,
                &[("from", (line.diff.from).to_string().into())],
            )}
        </p>
        {diff_table(&line.diff.changes, (Some(line.diff.from), id))}
    }
    .into_any()
}

/// The notice of what a replica did, `body` in words.
pub(crate) fn said(body: impl IntoView + 'static) -> AnyView {
    notice(Tone::Ok, Announce::Status, None, body).into_any()
}
