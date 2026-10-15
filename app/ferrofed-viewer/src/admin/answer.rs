// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of an admin action's answer, which the page shows
//! as it stands.
//!
//! Leptos escapes every text and attribute value it renders, so a code, a
//! claimant or a node's error text arrives as text, never as markup.

use ferrofed_registry::operator::HeldBefore;
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ClearLine, RepairLine, ReplicaAnswer};
use crate::admin::model::ActionAnswer;
use crate::views::model::{Refusal, ViewError};

/// The answer of an action `refused` kept from reaching any replica.
#[must_use]
pub fn refused(refused: &Refusal) -> ActionAnswer {
    ActionAnswer {
        changed: false,
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
        html: view! { <ul>{items}</ul> }.to_html(),
    }
}

/// The answer of a drift repair at its one replica.
#[must_use]
pub fn repaired(answer: &ReplicaAnswer<RepairLine>) -> ActionAnswer {
    ActionAnswer {
        changed: false,
        html: replica_said(answer, "drift repair", repair_line).to_html(),
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
            let code = code.clone().unwrap_or_else(|| String::from("no code"));
            let hint = if code == "scope-insufficient" {
                " Your access token may carry no operator scope."
            } else {
                ""
            };
            view! {
                <p role="alert">
                    {format!("Replica {replica} refused the {what}: {status} ({code}).{hint}")}
                </p>
            }
            .into_any()
        }
        Acted::Refused(Refusal::NotAuthenticated { code }) => {
            let code = code.clone().unwrap_or_else(|| String::from("no code"));
            view! {
                <p role="alert">
                    {format!(
                        "Replica {replica} did not accept your sign-in for the {what}: 401 ({code}). ",
                    )} <a href=crate::app::SIGN_IN rel="external">
                        "Sign in again"
                    </a>
                </p>
            }
            .into_any()
        }
        Acted::Refused(Refusal::Invalid { reason }) => {
            view! { <p role="alert">{format!("Replica {replica}: {reason}")}</p> }.into_any()
        }
        Acted::Fault(ViewError::Unreachable) => view! {
            <p role="alert">
                {format!(
                    "Replica {replica}: its admin listener gave no answer, so whether the {what} ran there is unknown. Reload the page before running it again; a configuration change shows in the revision history.",
                )}
            </p>
        }
        .into_any(),
        Acted::Fault(fault) => {
            view! { <p role="alert">{format!("Replica {replica}: {fault}.")}</p> }.into_any()
        }
    }
}

/// What a replica reports of a clear.
fn clear_line(replica: &str, line: &ClearLine) -> AnyView {
    let ehr_id = line
        .ehr_id
        .clone()
        .unwrap_or_else(|| String::from("the ehr_id"));
    let held = match line.held {
        HeldBefore::Collision => {
            format!("It held a collision between {}.", line.claimants.join(", "))
        }
        HeldBefore::Owner => format!("It held one owner, {}.", line.claimants.join(", ")),
        HeldBefore::Nothing => {
            String::from("It held nothing for it, which another replica may still hold.")
        }
    };
    let index = if line.indexed {
        "an ehr_id index entry"
    } else {
        "no ehr_id index entry"
    };
    view! {
        <p role="status">
            <strong>{format!("Replica {replica} cleared {ehr_id}.")}</strong>
            {format!(" {held} It forgot {index} and the bindings of {} sessions.", line.sessions)}
        </p>
    }
    .into_any()
}

/// What a replica reports of a drift repair: the status and whether every
/// member took the version in words (§11.4), and each member.
fn repair_line(replica: &str, line: &RepairLine) -> AnyView {
    let registry = line
        .registry
        .clone()
        .map(|registry| format!(" The registry's copy: {registry}."))
        .unwrap_or_default();
    let completeness = if line.complete {
        view! {
            <p role="status" class="complete">
                <strong>"Complete."</strong>
                " Every member named took the version."
            </p>
        }
        .into_any()
    } else {
        view! {
            <p role="alert" class="incomplete">
                <strong>"Incomplete."</strong>
                " Not every member named took the version; the members below say which."
            </p>
        }
        .into_any()
    };
    let endpoints = crate::query::answer::endpoints_table(&line.endpoints);
    view! {
        <p>
            {format!(
                "Replica {replica} sent {} version {} again: {}.{registry}",
                line.name,
                line.version,
                line.status,
            )}
        </p>
        {completeness}
        {endpoints}
    }
    .into_any()
}
