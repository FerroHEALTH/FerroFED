// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of the configuration page and of each form's
//! answer, which the page shows as it stands.
//!
//! Leptos escapes every text and attribute value it renders, so a member's
//! name, a revision's reason, a diff's value or a stored query's name
//! arrives as text, never as markup. No data is placed as a child of a
//! `textarea`: a field the page fills in carries its value as an attribute.

pub mod answers;
mod forms;
mod tables;

use ferrofed_registry::console::Resources;
use ferrofed_registry::operator::Page;
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::config::settings::ReplicaSettings;
use crate::configure::act::Configure;
use crate::configure::model::ConfigView;
use ferrofed_registry::revision::{
    Author, Current, Difference, Outcome, Revision, RevisionId, Standing,
};

/// What a replica reports of a saved revision, with the preview of what it
/// changes against the running revision when the console could read it.
#[derive(Debug, Clone)]
pub struct SavedLine {
    /// The revision saved.
    pub revision: Revision,
    /// The revision that runs, which the preview compares with.
    pub running: Option<RevisionId>,
    /// The keys that differ, `None` when the preview could not be read.
    pub changes: Option<Vec<Difference>>,
}

/// What a replica reports of a stored-query version stored.
#[derive(Debug, Clone)]
pub struct StoredLine {
    /// The qualified name.
    pub name: String,
    /// The version.
    pub version: String,
}

/// The page of a gateway whose configuration is not managed: no form.
#[must_use]
pub fn unmanaged(replica: &ReplicaSettings) -> ConfigView {
    let said = format!(
        "Replica {} serves no configuration revisions: its configuration is not managed (config.ui = \"off\"), so the file alone configures it and the console changes nothing.",
        replica.name
    );
    ConfigView {
        html: view! { <p role="status">{said}</p> }.to_html(),
        forms: String::new(),
    }
}

/// The configuration page: the revisions with the preview of the newest
/// one when it does not run, the members and stored queries with their
/// owners, the history, and the forms `replica` takes.
#[must_use]
pub fn page(
    replica: &ReplicaSettings,
    standing: &Standing,
    resources: &Resources,
    (history, pending): (&Page<Revision>, Option<&[Difference]>),
) -> ConfigView {
    let revisions = tables::revisions_section(replica, standing, pending);
    let members = tables::members_section(&resources.members);
    let sections = crate::configure::sections::section_table(resources);
    let queries =
        tables::queries_section(resources.stored_query_registry, &resources.stored_queries);
    let history = tables::history_section(history);
    ConfigView {
        html: view! {
            {revisions}
            {members}
            {sections}
            {queries}
            {history}
        }
        .to_html(),
        forms: forms::forms(standing, resources)
            .iter()
            .map(form_html)
            .collect(),
    }
}

/// Who `author` is, in words.
pub(super) fn author(author: &Author) -> String {
    match author {
        Author::File => String::from("the configuration file"),
        Author::Operator { subject, .. } => format!("operator {subject}"),
        Author::DevelopmentLoopback => {
            String::from("a loopback peer under the development profile")
        }
        Author::Gateway => String::from("the gateway, as no operator confirmed in time"),
        _ => String::from("an author this console does not know"),
    }
}

/// The running revision's mark, in words.
pub(super) fn mark_said(mark: &Current) -> String {
    let outcome = if mark.outcome == Outcome::PendingRestart {
        format!(
            " Some of its settings take effect only at a restart: {}.",
            mark.needs_restart.join(", ")
        )
    } else {
        String::new()
    };
    format!(
        "Revision {} runs, applied by {} at {}.{outcome}",
        mark.revision,
        author(&mark.applied_by),
        mark.applied_at
    )
}

/// One form of the page, before it is rendered.
pub(super) struct FormView {
    /// The id of the form's heading, unique on the page.
    pub(super) id: String,
    /// The form's heading.
    pub(super) heading: String,
    /// Its submit button's label.
    pub(super) label: String,
    /// Its fields, rendered as HTML.
    pub(super) fields: String,
}

/// `form` rendered whole: its heading, and a form posting its fields to the
/// configuration's server function.
fn form_html(form: &FormView) -> String {
    view! {
        <section aria-labelledby=form.id.clone()>
            <h2 id=form.id.clone()>{form.heading.clone()}</h2>
            <form method="post" action=Configure::url()>
                <div inner_html=form.fields.clone()></div>
                <button type="submit">{form.label.clone()}</button>
            </form>
        </section>
    }
    .to_html()
}

/// A hidden field `name` of the form, holding `value`.
pub(super) fn hidden(name: &'static str, value: String) -> impl IntoView {
    view! { <input type="hidden" name=format!("form[{name}]") value=value /> }
}

/// A labelled one-line field `name` of the form `form`, holding `value`.
pub(super) fn field(
    form: &str,
    (name, label): (&'static str, &'static str),
    value: String,
    (required, readonly): (bool, bool),
) -> impl IntoView {
    let id = format!("{form}-{name}");
    view! {
        <p>
            <label for=id.clone()>{label}</label>
            " "
            <input
                type="text"
                id=id
                name=format!("form[{name}]")
                value=value
                autocomplete="off"
                spellcheck="false"
                required=required
                readonly=readonly
            />
        </p>
    }
}

/// The confirmation checkbox of the form `form`, labelled `label`.
pub(super) fn confirm(form: &str, label: &'static str) -> impl IntoView {
    let id = format!("{form}-confirm");
    view! {
        <p>
            <input type="checkbox" id=id.clone() name="form[confirm]" value="yes" required />
            <label for=id>{label}</label>
        </p>
    }
}

/// The base a change is saved on, `""` for none.
pub(super) fn base(head: Option<RevisionId>) -> String {
    head.map(|head| head.to_string()).unwrap_or_default()
}
