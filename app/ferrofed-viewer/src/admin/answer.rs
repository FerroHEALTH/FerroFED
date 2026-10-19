// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of an admin action's answer, which the page shows
//! as it stands.
//!
//! Leptos escapes every text and attribute value it renders, so a code, a
//! claimant or a node's error text arrives as text, never as markup.

use ferrofed_registry::operator::HeldBefore;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::notice::{Announce, notice};
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ClearLine, RepairLine, ReplicaAnswer};
use crate::admin::model::ActionAnswer;
use crate::locale::{Msg, tr, tr_with};
use crate::views::model::{Refusal, ViewError};

/// The answer of an action `refused` kept from reaching any replica.
#[must_use]
pub fn refused(refused: &Refusal) -> ActionAnswer {
    ActionAnswer {
        changed: false,
        field: None,
        html: crate::views::refusal(refused).to_html(),
    }
}

/// The answer of a clear at each replica of `answers`.
#[must_use]
pub fn cleared(answers: &[ReplicaAnswer<ClearLine>]) -> ActionAnswer {
    let items = answers
        .iter()
        .map(|answer| {
            let said = replica_said(answer, "clear", clear_line);
            view! { <li>{said}</li> }
        })
        .collect_view();
    ActionAnswer {
        changed: false,
        field: None,
        html: view! { <ul class="fui-divided">{items}</ul> }.to_html(),
    }
}

/// The answer of a drift repair at its one replica.
#[must_use]
pub fn repaired(answer: &ReplicaAnswer<RepairLine>) -> ActionAnswer {
    ActionAnswer {
        changed: false,
        field: None,
        html: replica_said(answer, &tr(Msg::DriftRepair), repair_line).to_html(),
    }
}

/// What `answer`'s replica did with the action `what`, its report drawn by
/// `done`.
pub(crate) fn replica_said<T>(
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
                    &[("said", (said.trim_end_matches('.')).to_string().into())],
                ),
                None => String::new(),
            };
            refused_by(tr_with(
                Msg::ReplicaRefusedThe,
                &[
                    ("replica", (replica).to_string().into()),
                    ("what", (what).to_string().into()),
                    ("status", (status).to_string().into()),
                    ("code", (code).clone().into()),
                    ("hint", (hint).clone().into()),
                ],
            ))
        }
        Acted::Refused(Refusal::NotAuthenticated { code }) => {
            let code = code.clone().unwrap_or_else(|| tr(Msg::NoCode));
            refused_by(view! {
                {tr_with(Msg::ReplicaDidNotAcceptYourSign, &[("replica", (replica).to_string().into()), ("what", (what).to_string().into()), ("code", (code).clone().into())])}
                <a href=crate::app::SIGN_IN rel="external">
                    {tr(Msg::SignInAgain)}
                </a>
            })
        }
        Acted::Refused(Refusal::Invalid { reason }) => refused_by(tr_with(
            Msg::Replica2,
            &[
                ("replica", (replica).to_string().into()),
                ("reason", (reason).clone().into()),
            ],
        )),
        Acted::Fault(ViewError::Unreachable) => refused_by(tr_with(
            Msg::ReplicaAdminListenerGaveNoAnswer,
            &[
                ("replica", (replica).to_string().into()),
                ("what", (what).to_string().into()),
            ],
        )),
        Acted::Fault(fault) => refused_by(tr_with(
            Msg::Replica3,
            &[
                ("replica", (replica).to_string().into()),
                ("fault", (fault).to_string().into()),
            ],
        )),
    }
}

/// The alert `body` says, for a replica that refused or did not answer.
fn refused_by(body: impl IntoView + 'static) -> AnyView {
    notice(Tone::Bad, Announce::Alert, None, body).into_any()
}

/// What a replica reports of a clear.
fn clear_line(replica: &str, line: &ClearLine) -> AnyView {
    let ehr_id = line.ehr_id.clone().unwrap_or_else(|| tr(Msg::TheEhrId));
    let held = match line.held {
        HeldBefore::Collision => tr_with(
            Msg::HeldCollisionBetween,
            &[("line", (line.claimants.join(", ")).clone().into())],
        ),
        HeldBefore::Owner => tr_with(
            Msg::ItHeldOneOwner,
            &[("line", (line.claimants.join(", ")).clone().into())],
        ),
        HeldBefore::Nothing => tr(Msg::HeldNothingWhichAnotherReplicaMay),
    };
    let index = if line.indexed {
        tr(Msg::EhrIdIndexEntry)
    } else {
        tr(Msg::NoEhrIdIndexEntry)
    };
    notice(
        Tone::Ok,
        Announce::Status,
        None,
        view! {
            <strong>{tr_with(Msg::ReplicaCleared, &[("replica", (replica).to_string().into()), ("ehr_id", (ehr_id).clone().into())])}</strong>
            {tr_with(Msg::ForgotBindingsSessions, &[("held", (held).clone().into()), ("index", (index).clone().into()), ("sessions", (line.sessions).to_string().into())])}
        },
    )
    .into_any()
}

/// What a replica reports of a drift repair: the status and whether every
/// member took the version in words (§11.4), and each member.
fn repair_line(replica: &str, line: &RepairLine) -> AnyView {
    let registry = line
        .registry
        .clone()
        .map(|registry| {
            tr_with(
                Msg::TheRegistrySCopy,
                &[("registry", (registry).clone().into())],
            )
        })
        .unwrap_or_default();
    let completeness = if line.complete {
        notice(
            Tone::Ok,
            Announce::Status,
            None,
            view! {
                <strong>{tr(Msg::Complete)}</strong>
                {tr(Msg::EveryMemberNamedTookVersion)}
            },
        )
        .into_any()
    } else {
        notice(
            Tone::Warn,
            Announce::Alert,
            None,
            view! {
                <strong>{tr(Msg::Incomplete)}</strong>
                {tr(Msg::NotEveryMemberNamedTookVersion)}
            },
        )
        .into_any()
    };
    let endpoints = crate::query::answer::endpoints_table(&line.endpoints);
    view! {
        <p>
            {tr_with(Msg::ReplicaSentVersionAgain, &[("replica", (replica).to_string().into()), ("name", (line.name).clone().into()), ("version", (line.version).clone().into()), ("status", (line.status).to_string().into()), ("registry", (registry).clone().into())])}
        </p>
        {completeness}
        {endpoints}
    }
    .into_any()
}
