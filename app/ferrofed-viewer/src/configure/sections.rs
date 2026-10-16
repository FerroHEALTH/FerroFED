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
    view! {
        <p role="status" class="mt-0">
            <strong>
                {format!(
                    "Replica {replica} recorded the confirmation of revision {}.",
                    mark.revision,
                )}
            </strong>
            " It stays running, and the gateway no longer rolls it back."
        </p>
    }
    .into_any()
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
        " It may now be applied."
    } else {
        " It still takes an approval before it applies."
    };
    view! {
        <p role="status" class="mt-0">
            <strong>
                {format!(
                    "Replica {replica} recorded the approval of revision {}.",
                    review.revision,
                )}
            </strong>
            {next}
        </p>
    }
    .into_any()
}
