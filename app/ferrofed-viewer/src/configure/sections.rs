// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The answers of a confirmation and of an approval.
//!
//! Beside them, how a removal names a credential or a binding section. No
//! specification governs this: our own design.

use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Current, Review};
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ReplicaAnswer};
use crate::admin::answer::replica_said;
use crate::admin::model::ActionAnswer;
use crate::configure::render::answers::said;
use crate::locale::{Msg, tr, tr_with};

/// How a removal names `resource`: `credentials:<endpoint id>` or
/// `binding:<section>`.
#[must_use]
pub fn choice(resource: &Resource) -> String {
    match resource {
        Resource::Credentials(endpoint) => format!("credentials:{endpoint}"),
        Resource::Binding(section) => format!("binding:{}", section.as_str()),
        _ => String::new(),
    }
}

/// The answer of a confirmation at `answer`'s replica.
#[must_use]
pub fn confirmed(answer: &ReplicaAnswer<Current>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "confirmation", confirmed_line).to_html(),
        field: None,
    }
}

/// What a replica reports of a confirmation.
fn confirmed_line(replica: &str, mark: &Current) -> AnyView {
    said(view! {
        <strong>
            {tr_with(Msg::ReplicaRecordedConfirmationRevision, &[("replica", (replica).to_string().into()), ("revision", (mark.revision).to_string().into())])}
        </strong>
        {tr(Msg::StaysRunningGatewayNoLongerRolls)}
    })
}

/// The answer of an approval at `answer`'s replica.
#[must_use]
pub fn approved(answer: &ReplicaAnswer<Review>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "approval", approved_line).to_html(),
        field: None,
    }
}

/// What a replica reports of an approval.
fn approved_line(replica: &str, review: &Review) -> AnyView {
    let next = if review.approved {
        tr(Msg::MayNowApplied)
    } else {
        tr(Msg::StillTakesApprovalBeforeApplies)
    };
    said(view! {
        <strong>
            {tr_with(Msg::ReplicaRecordedApprovalRevision, &[("replica", (replica).to_string().into()), ("revision", (review.revision).to_string().into())])}
        </strong>
        {next}
    })
}
