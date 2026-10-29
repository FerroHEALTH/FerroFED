// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answer of a configuration form, and what a replica said.
//!
//! A form's answer is drawn at the top of the page: what the replica did,
//! or why no replica was asked, with the field a refusal is about. The
//! answer crosses from the server function to the page as data
//! ([`Answer`]), and the page draws it in both halves.

use ferrofed_registry::revision::{Applied, Current, Imported, Review};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::notice::{Announce, notice};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::act::{Acted, ReplicaAnswer};
use crate::configure::render::{SavedLine, StoredLine};
use crate::locale::{Msg, tr, tr_with};
use crate::read::{Fault, Refusal};

/// The refusal of one field of a form: the field's id on the page and what
/// is wrong with it, never what it holds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldNote {
    /// The id of the field on the page.
    pub id: String,
    /// What is wrong with it.
    pub message: String,
}

/// What a configuration form's run gave, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    /// No replica was asked, for this reason.
    Refused(Refusal),
    /// A revision saved, with its preview.
    Saved(ReplicaAnswer<SavedLine>),
    /// An apply or a rollback, as the action names it.
    Applied(ReplicaAnswer<Applied>, String),
    /// A stored-query version stored.
    Stored(ReplicaAnswer<StoredLine>),
    /// An import, with what it changes.
    Imported(ReplicaAnswer<Imported>),
    /// A confirmation of the running revision.
    Confirmed(ReplicaAnswer<Current>),
    /// An approval of a revision.
    Approved(ReplicaAnswer<Review>),
    /// The console could not run the form.
    Failed(Fault),
}

/// A configuration form's answer, as the page draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionAnswer {
    /// The answer, naming the replica it went to.
    pub answer: Answer,
    /// Whether the action changed what the page shows.
    pub changed: bool,
    /// The field a refusal names, which the page marks where it stands.
    pub field: Option<FieldNote>,
}

impl ActionAnswer {
    /// The answer drawn.
    #[must_use]
    pub fn view(&self) -> AnyView {
        use crate::configure::render::answers;
        match &self.answer {
            Answer::Refused(refused) => crate::pages::refused_view(refused),
            Answer::Saved(answer) => answers::saved_view(answer),
            Answer::Applied(answer, what) => answers::applied_view(answer, what),
            Answer::Stored(answer) => answers::stored_view(answer),
            Answer::Imported(answer) => answers::imported_view(answer),
            Answer::Confirmed(answer) => crate::configure::sections::confirmed_view(answer),
            Answer::Approved(answer) => crate::configure::sections::approved_view(answer),
            Answer::Failed(fault) => crate::pages::fault_view(fault),
        }
    }
}

/// The answer of an action `refused` kept from reaching any replica.
#[must_use]
pub fn refused(refused: &Refusal) -> ActionAnswer {
    ActionAnswer {
        changed: false,
        field: None,
        answer: Answer::Refused(refused.clone()),
    }
}

/// What `answer`'s replica did with the action `what`, its report drawn by
/// `done`.
pub fn replica_said<T>(
    answer: &ReplicaAnswer<T>,
    what: &str,
    done: fn(&str, &T) -> AnyView,
) -> AnyView {
    let replica = answer.replica.as_str();
    match &answer.acted {
        Acted::Done(report) => done(replica, report),
        Acted::Refused(Refusal::Gateway { status, code }) => {
            let code = code.clone().unwrap_or_else(|| tr(Msg::NoCode));
            let hint = match &answer.said {
                _ if code == "scope-insufficient" => tr(Msg::YourAccessTokenMayCarryNo),
                Some(said) => tr_with(
                    Msg::TheGatewaySays,
                    &[("said", said.trim_end_matches('.').to_owned().into())],
                ),
                None => String::new(),
            };
            refused_by(tr_with(
                Msg::ReplicaRefusedThe,
                &[
                    ("replica", replica.to_owned().into()),
                    ("what", what.to_owned().into()),
                    ("status", status.to_string().into()),
                    ("code", code.into()),
                    ("hint", hint.into()),
                ],
            ))
        }
        Acted::Refused(Refusal::NotAuthenticated { code }) => {
            let code = code.clone().unwrap_or_else(|| tr(Msg::NoCode));
            refused_by(view! {
                {tr_with(
                    Msg::ReplicaDidNotAcceptYourSign,
                    &[
                        ("replica", replica.to_owned().into()),
                        ("what", what.to_owned().into()),
                        ("code", code.into()),
                    ],
                )}
                <a href=crate::app::SIGN_IN rel="external">
                    {tr(Msg::SignInAgain)}
                </a>
            })
        }
        Acted::Refused(Refusal::Invalid { reason }) => refused_by(tr_with(
            Msg::Replica2,
            &[
                ("replica", replica.to_owned().into()),
                ("reason", reason.clone().into()),
            ],
        )),
        Acted::Fault(Fault::Unreachable) => refused_by(tr_with(
            Msg::ReplicaAdminListenerGaveNoAnswer,
            &[
                ("replica", replica.to_owned().into()),
                ("what", what.to_owned().into()),
            ],
        )),
        Acted::Fault(fault) => refused_by(tr_with(
            Msg::Replica3,
            &[
                ("replica", replica.to_owned().into()),
                ("fault", fault.to_string().into()),
            ],
        )),
    }
}

/// The alert `body` says, for a replica that refused or did not answer.
fn refused_by(body: impl IntoView + 'static) -> AnyView {
    notice(Tone::Bad, Announce::Alert, None, body).into_any()
}
