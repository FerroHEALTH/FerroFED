// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of the onward credentials and binding sections,
//! of the confirmation a running revision awaits, and of the approval the
//! newest revision takes, with their forms and answers.
//!
//! A secret is shown as set, with the file it is read from, or not set;
//! its value is never on the page, and its field is a password field the
//! page never fills. Leptos escapes every text and attribute value, so a
//! resource's settings, filled into a one-line field as its value
//! attribute, arrive as text. No specification governs this: our own
//! design.

use ferrofed_registry::console::{ConsoleState, Owner, Resources};
use ferrofed_registry::resource::{BindingSection, Resource, SecretSlot, SectionResource};
use ferrofed_registry::revision::{Current, Review, RevisionId, Signed, Standing};
use leptos::prelude::*;

use crate::admin::act::server::{Acted, ReplicaAnswer};
use crate::admin::answer::replica_said;
use crate::admin::model::ActionAnswer;
use crate::configure::render::{FormView, author, base, confirm, field, hidden};

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

/// The secrets `slots` name, in words: each set and read from its file.
fn secrets_said(slots: &[SecretSlot]) -> String {
    if slots.is_empty() {
        return String::from("none set");
    }
    slots
        .iter()
        .map(|slot| format!("{}: set, read from {}", slot.key, slot.reference))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The credentials and binding sections, each with its owner, where it
/// stands, and its secrets by reference.
#[must_use]
pub fn section_table(resources: &Resources) -> AnyView {
    let rows = resources
        .sections
        .iter()
        .map(|section| {
            let state = if section.owner == Owner::Console {
                state(section.state)
            } else {
                "running"
            };
            view! {
                <tr>
                    <th scope="row">
                        <code>{section.resource.to_string()}</code>
                    </th>
                    <td>{owner(section.owner)}</td>
                    <td>{state}</td>
                    <td>{secrets_said(&section.secrets)}</td>
                    <td>
                        <code>{section.settings.clone().unwrap_or_default()}</code>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section aria-labelledby="config-sections">
            <h2 id="config-sections">"Credentials and bindings"</h2>
            <p>
                "An onward credential or a binding section the configuration file provides is "
                "read-only here. One the console holds is changed through a revision; once "
                "applied, it awaits an operator's confirmation, and the gateway rolls it back "
                "without one. A secret is shown as set, with the file it is read from, and "
                "never by its value."
            </p>
            <table>
                <caption>"Credentials and binding sections and their owners"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Resource"</th>
                        <th scope="col">"Owner"</th>
                        <th scope="col">"State"</th>
                        <th scope="col">"Secrets"</th>
                        <th scope="col">"Settings"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </section>
    }
    .into_any()
}

/// Who signed `signed`, and when, in words.
fn signed_said(signed: &Signed) -> String {
    format!("{} at {}", author(&signed.by), signed.at)
}

/// What the running revision awaits, and what the newest one takes before
/// and after it applies.
#[must_use]
pub fn notices(standing: &Standing) -> AnyView {
    let awaiting = standing.current.as_ref().and_then(|mark| {
        mark.awaiting.as_ref().map(|awaiting| {
            format!(
                "Revision {} runs awaiting its confirmation until {}: it changed {}. Unless an operator confirms it by then, the gateway rolls it back.",
                mark.revision,
                awaiting.deadline,
                awaiting.resources.join(", ")
            )
        })
    });
    let confirmed = standing.current.as_ref().and_then(|mark| {
        mark.confirmed.as_ref().map(|signed| {
            format!(
                "Revision {} was confirmed by {}.",
                mark.revision,
                signed_said(signed)
            )
        })
    });
    let review = standing.review.as_ref().map(review_said);
    view! {
        {awaiting.map(|said| view! { <p role="alert">{said}</p> })}
        {confirmed.map(|said| view! { <p>{said}</p> })}
        {review}
    }
    .into_any()
}

/// What `review` says the newest revision takes.
fn review_said(review: &Review) -> AnyView {
    let confirms = (!review.confirms.is_empty()).then(|| {
        format!(
            "Once applied, revision {} awaits a confirmation: it changes {}.",
            review.revision,
            review.confirms.join(", ")
        )
    });
    let approval = (!review.approval.is_empty()).then(|| {
        let approvals = if review.approvals.is_empty() {
            String::from("none yet")
        } else {
            review
                .approvals
                .iter()
                .map(signed_said)
                .collect::<Vec<_>>()
                .join("; ")
        };
        let standing = if review.approved {
            "It may apply."
        } else {
            "It does not apply until then."
        };
        format!(
            "Revision {} changes {}, which take the approval of an operator other than its author, {}. Approved by: {approvals}. {standing}",
            review.revision,
            review.approval.join(", "),
            author(&review.author)
        )
    });
    view! {
        {confirms.map(|said| view! { <p role="status">{said}</p> })}
        {approval.map(|said| view! { <p role="status">{said}</p> })}
    }
    .into_any()
}

/// The confirmation the running revision awaits and the approval the
/// newest one takes, each a form, first on the page.
#[must_use]
pub(super) fn decisions(standing: &Standing) -> Vec<FormView> {
    let mut forms = Vec::new();
    if let Some(mark) = &standing.current
        && mark.awaiting.is_some()
    {
        forms.push(confirm_form(mark));
    }
    if let Some(review) = &standing.review
        && !review.approval.is_empty()
        && !review.approved
    {
        forms.push(approve_form(review));
    }
    forms
}

/// The confirmation of the running revision `mark`.
fn confirm_form(mark: &Current) -> FormView {
    let said = mark.awaiting.as_ref().map(|awaiting| {
        format!(
            "Confirm it before {} once you have checked the gateway works with it; otherwise the gateway rolls it back.",
            awaiting.deadline
        )
    });
    let fields = view! {
        {hidden("action", String::from("confirm"))}
        {hidden("revision", mark.revision.to_string())}
        <p>{said}</p>
        {confirm("confirm-revision", "I confirm the running revision works as intended")}
    };
    FormView {
        id: String::from("confirm-revision"),
        heading: format!("Confirm revision {}", mark.revision),
        label: String::from("Confirm the revision"),
        fields: fields.to_html(),
    }
}

/// The approval of the revision `review` reviews.
fn approve_form(review: &Review) -> FormView {
    let said = format!(
        "Its author, {}, cannot approve it: another operator does, and each approval names its operator.",
        author(&review.author)
    );
    let fields = view! {
        {hidden("action", String::from("approve"))}
        {hidden("revision", review.revision.to_string())}
        <p>{said}</p>
        {confirm(
            "approve-revision",
            "I approve this revision as an operator other than its author",
        )}
    };
    FormView {
        id: String::from("approve-revision"),
        heading: format!("Approve revision {}", review.revision),
        label: String::from("Approve the revision"),
        fields: fields.to_html(),
    }
}

/// The forms of the credentials and binding sections: an addition of each,
/// an edit of each the console holds, and a removal.
#[must_use]
pub(super) fn forms(standing: &Standing, resources: &Resources) -> Vec<FormView> {
    let head = standing.head;
    let mut forms = vec![credentials_form(head, None)];
    let owned: Vec<&SectionResource> = resources
        .sections
        .iter()
        .filter(|section| {
            section.owner == Owner::Console
                && matches!(
                    section.state,
                    Some(ConsoleState::Running | ConsoleState::Pending)
                )
        })
        .collect();
    let present: Vec<&Resource> = resources
        .sections
        .iter()
        .map(|section| &section.resource)
        .collect();
    let addable: Vec<BindingSection> = resources
        .bindings
        .iter()
        .copied()
        .filter(|section| !present.contains(&&Resource::Binding(*section)))
        .collect();
    if !addable.is_empty() {
        forms.push(binding_form(head, Section::Add(&addable)));
    }
    for section in &owned {
        match &section.resource {
            Resource::Credentials(_) => forms.push(credentials_form(head, Some(section))),
            Resource::Binding(binding) => {
                forms.push(binding_form(head, Section::Edit(*binding, section)));
            }
            _ => {}
        }
    }
    if !owned.is_empty() {
        forms.push(remove_form(head, &owned));
    }
    forms
}

/// The secret fields of the form `form`: the key, a value the page never
/// fills, a file, or a clear.
fn secret_fields(form: &str, owned: &[SecretSlot]) -> impl IntoView {
    let value = format!("{form}-secret_value");
    let clear = format!("{form}-secret_clear");
    let said = format!(
        "Secrets set now: {}. Set one by its value, which the gateway writes to a file of its own when it is configured with a secrets directory, or by a file the gateway reads; or clear it.",
        secrets_said(owned)
    );
    view! {
        <p>{said}</p>
        {field(
            form,
            (
                "secret_key",
                "Secret key inside the resource, such as bearer_token or oauth2.client_secret",
            ),
            String::new(),
            (false, false),
        )}
        <p>
            <label for=value.clone()>"Secret value, never shown again"</label>
            " "
            <input type="password" id=value name="form[secret_value]" autocomplete="new-password" />
        </p>
        {field(
            form,
            ("secret_file", "Or the absolute path of the file it is read from"),
            String::new(),
            (false, false),
        )}
        <p>
            <input type="checkbox" id=clear.clone() name="form[secret_clear]" value="yes" />
            <label for=clear>"Or clear the secret"</label>
        </p>
    }
}

/// The save of the onward credentials of one endpoint, `owned` filled in
/// for an edit.
fn credentials_form(head: Option<RevisionId>, owned: Option<&SectionResource>) -> FormView {
    let (id, heading, endpoint) = match owned.map(|section| &section.resource) {
        Some(Resource::Credentials(endpoint)) => (
            format!("edit-credentials-{endpoint}"),
            format!("Edit the onward credentials of {endpoint}"),
            endpoint.clone(),
        ),
        _ => (
            String::from("add-credentials"),
            String::from("Add onward credentials"),
            String::new(),
        ),
    };
    let settings = owned
        .and_then(|section| section.settings.clone())
        .unwrap_or_default();
    let slots = owned
        .map(|section| section.secrets.clone())
        .unwrap_or_default();
    let fields = view! {
        {hidden("action", String::from("save-credentials"))}
        {hidden("base", base(head))}
        {field(&id, ("endpoint", "endpoint_id"), endpoint, (true, owned.is_some()))}
        {field(
            &id,
            ("settings", "Settings without secrets, as one TOML inline table"),
            settings,
            (false, false),
        )}
        {secret_fields(&id, &slots)}
        {field(&id, ("reason", "Reason"), String::new(), (true, false))}
        {confirm(
            &id,
            "I confirm saving these credentials as a new revision, which I apply and then confirm",
        )}
    };
    let fields = fields.to_html();
    FormView {
        id,
        heading,
        label: String::from("Save as a new revision"),
        fields,
    }
}

/// A binding section's form: an addition of one of those not set, or an
/// edit of one the console holds.
#[derive(Clone, Copy)]
enum Section<'a> {
    /// Add one of these sections.
    Add(&'a [BindingSection]),
    /// Edit this section the console holds.
    Edit(BindingSection, &'a SectionResource),
}

/// The save of a binding section.
fn binding_form(head: Option<RevisionId>, section: Section<'_>) -> FormView {
    let (id, heading, owned) = match &section {
        Section::Add(_) => (
            String::from("add-binding"),
            String::from("Add a binding section"),
            None,
        ),
        Section::Edit(binding, owned) => (
            format!("edit-binding-{}", binding.as_str()),
            format!("Edit {}", owned.resource),
            Some(*owned),
        ),
    };
    let chosen = match section {
        Section::Add(sections) => {
            let select = format!("{id}-section");
            let choices = sections
                .iter()
                .map(|section| {
                    let shown = Resource::Binding(*section).to_string();
                    view! { <option value=section.as_str()>{shown}</option> }
                })
                .collect_view();
            view! {
                <p>
                    <label for=select.clone()>"Binding section"</label>
                    " "
                    <select id=select name="form[section]">
                        {choices}
                    </select>
                </p>
            }
            .into_any()
        }
        Section::Edit(binding, _) => hidden("section", binding.as_str().to_owned()).into_any(),
    };
    let settings = owned
        .and_then(|owned| owned.settings.clone())
        .unwrap_or_default();
    let slots = owned.map(|owned| owned.secrets.clone()).unwrap_or_default();
    let fields = view! {
        {hidden("action", String::from("save-binding"))}
        {hidden("base", base(head))}
        {chosen}
        {field(
            &id,
            ("settings", "Settings without secrets, as one TOML inline table"),
            settings,
            (false, false),
        )}
        {secret_fields(&id, &slots)}
        {field(&id, ("reason", "Reason"), String::new(), (true, false))}
        {confirm(
            &id,
            "I confirm saving this section as a new revision, which I apply and then confirm",
        )}
    };
    let fields = fields.to_html();
    FormView {
        id,
        heading,
        label: String::from("Save as a new revision"),
        fields,
    }
}

/// The removal of one of the credentials and binding sections `owned`.
fn remove_form(head: Option<RevisionId>, owned: &[&SectionResource]) -> FormView {
    let choices = owned
        .iter()
        .map(|section| {
            view! { <option value=choice(&section.resource)>{section.resource.to_string()}</option> }
        })
        .collect_view();
    let fields = view! {
        {hidden("action", String::from("remove-section"))}
        {hidden("base", base(head))}
        <p>
            <label for="remove-section-resource">"Credential or binding section"</label>
            " "
            <select id="remove-section-resource" name="form[resource]">
                {choices}
            </select>
        </p>
        {field("remove-section", ("reason", "Reason"), String::new(), (true, false))}
        {confirm(
            "remove-section",
            "I confirm saving this removal as a new revision, which I apply and then confirm",
        )}
    };
    FormView {
        id: String::from("remove-section"),
        heading: String::from("Remove a credential or binding section"),
        label: String::from("Save the removal as a new revision"),
        fields: fields.to_html(),
    }
}

/// The answer of a confirmation at `answer`'s replica.
#[must_use]
pub fn confirmed(answer: &ReplicaAnswer<Current>) -> ActionAnswer {
    ActionAnswer {
        changed: matches!(answer.acted, Acted::Done(_)),
        html: replica_said(answer, "confirmation", confirmed_line).to_html(),
    }
}

/// What a replica reports of a confirmation.
fn confirmed_line(replica: &str, mark: &Current) -> AnyView {
    view! {
        <p role="status">
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
        <p role="status">
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

/// Who owns a resource, in words that say whether the console changes it.
pub(super) fn owner(owner: Owner) -> &'static str {
    match owner {
        Owner::File => "file (read-only)",
        Owner::Directory => "care services directory (read-only)",
        Owner::Console => "console",
        Owner::Gateway => "gateway (read-only)",
        Owner::Store => "stored-query store (versions are immutable)",
        _ => "another source (read-only)",
    }
}

/// Where a console member stands, in words.
pub(super) fn state(state: Option<ConsoleState>) -> &'static str {
    match state {
        None | Some(ConsoleState::Running) => "running",
        Some(ConsoleState::Pending) => "saved, not applied",
        Some(ConsoleState::Removing) => "removed in the newest revision, not applied",
        Some(ConsoleState::Shadowed) => "shadowed: the registry document provides it",
        Some(_) => "unknown",
    }
}
