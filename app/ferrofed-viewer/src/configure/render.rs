// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server's rendering of the configuration page and of each form's
//! answer, which the page shows as it stands.
//!
//! Leptos escapes every text and attribute value it renders, so a member's
//! name, a revision's reason, a diff's value or a stored query's name
//! arrives as text, never as markup. No data is placed as a child of a
//! `textarea`: a field the page fills in carries its value as an attribute.

use ferrofed_registry::console::{
    ConsoleMember, ConsoleState, MemberResource, Owner, QueryRegistry, QueryResource, Resources,
};
use ferrofed_registry::operator::Page;
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use leptos::prelude::*;
use leptos::server_fn::ServerFn as _;

use crate::admin::act::server::{Acted, ReplicaAnswer};
use crate::admin::answer::replica_said;
use crate::admin::model::ActionAnswer;
use crate::config::settings::ReplicaSettings;
use crate::configure::act::Configure;
use crate::configure::model::ConfigView;
use ferrofed_registry::revision::{
    Applied, Author, Current, Difference, Origin, Outcome, Revision, RevisionId, Standing,
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
    let revisions = revisions_section(replica, standing, pending);
    let members = members_section(&resources.members);
    let queries = queries_section(resources.stored_query_registry, &resources.stored_queries);
    let history = history_section(history);
    ConfigView {
        html: view! {
            {revisions}
            {members}
            {queries}
            {history}
        }
        .to_html(),
        forms: forms(standing, resources).iter().map(form_html).collect(),
    }
}

/// Who `author` is, in words.
fn author(author: &Author) -> String {
    match author {
        Author::File => String::from("the configuration file"),
        Author::Operator { subject, .. } => format!("operator {subject}"),
        Author::DevelopmentLoopback => {
            String::from("a loopback peer under the development profile")
        }
        _ => String::from("an author this console does not know"),
    }
}

/// Where a revision came from, in words.
fn origin(origin: Origin) -> &'static str {
    match origin {
        Origin::File => "file",
        Origin::Operator => "operator",
        Origin::Rollback => "rollback",
        _ => "another origin",
    }
}

/// The running revision's mark, in words.
fn mark_said(mark: &Current) -> String {
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

/// The revisions: which runs, which is newest, and what the newest changes
/// when it does not run.
fn revisions_section(
    replica: &ReplicaSettings,
    standing: &Standing,
    pending: Option<&[Difference]>,
) -> AnyView {
    let running = standing
        .current
        .as_ref()
        .map_or_else(|| String::from("No revision is marked running."), mark_said);
    let newest = standing.head.map_or_else(
        || String::from("The store holds no revision."),
        |head| format!("The newest revision is {head}."),
    );
    let preview = match (standing.head, pending) {
        (Some(head), Some(changes)) => {
            let running = standing
                .current
                .as_ref()
                .map(|mark| mark.revision.to_string())
                .unwrap_or_default();
            view! {
                <p role="status">
                    {format!(
                        "Revision {head} is saved and not applied. What it changes against revision {running}:",
                    )}
                </p>
                {diff_table(changes, head)}
            }
            .into_any()
        }
        _ => ().into_any(),
    };
    let goes = format!(
        "Changes go to replica {}, the first one configured: the replicas share one revision store, and each applies the revision marked running.",
        replica.name
    );
    view! {
        <section aria-labelledby="config-revisions">
            <h2 id="config-revisions">"Revisions"</h2>
            <p>{running}</p>
            <p>{newest}</p>
            {preview}
            <p>{goes}</p>
        </section>
    }
    .into_any()
}

/// The keys `changes` names, against revision `to`.
fn diff_table(changes: &[Difference], to: RevisionId) -> AnyView {
    if changes.is_empty() {
        return view! { <p>"It changes no setting."</p> }.into_any();
    }
    let rows = changes
        .iter()
        .map(|change| {
            view! {
                <tr>
                    <th scope="row">
                        <code>{change.key.clone()}</code>
                    </th>
                    <td>{change.change.as_str()}</td>
                    <td>
                        <code>{change.from.clone().unwrap_or_default()}</code>
                    </td>
                    <td>
                        <code>{change.to.clone().unwrap_or_default()}</code>
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <table>
            <caption>{format!("What revision {to} changes, by key")}</caption>
            <thead>
                <tr>
                    <th scope="col">"Key"</th>
                    <th scope="col">"Change"</th>
                    <th scope="col">"Running value"</th>
                    <th scope="col">{format!("Value in revision {to}")}</th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    }
    .into_any()
}

/// Who owns a resource, in words that say whether the console changes it.
fn owner(owner: Owner) -> &'static str {
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
fn state(state: Option<ConsoleState>) -> &'static str {
    match state {
        None | Some(ConsoleState::Running) => "running",
        Some(ConsoleState::Pending) => "saved, not applied",
        Some(ConsoleState::Removing) => "removed in the newest revision, not applied",
        Some(ConsoleState::Shadowed) => "shadowed: the registry document provides it",
        Some(_) => "unknown",
    }
}

/// The members, each with its owner.
fn members_section(members: &[MemberResource]) -> AnyView {
    let rows = members
        .iter()
        .map(|member| {
            let endpoints = member
                .endpoints
                .iter()
                .map(ferrofed_registry::id::EndpointId::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            view! {
                <tr>
                    <th scope="row">{member.node.as_str().to_owned()}</th>
                    <td>{member.organisation.as_str().to_owned()}</td>
                    <td>{member.system_id.as_str().to_owned()}</td>
                    <td>{endpoints}</td>
                    <td>{owner(member.owner)}</td>
                    <td>{state(member.state)}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <section aria-labelledby="config-members">
            <h2 id="config-members">"Members"</h2>
            <p>
                "A member the file's registry document or a care services directory provides "
                "is read-only here: change it in its source. A console member is changed "
                "through a revision you preview and then apply."
            </p>
            <table>
                <caption>"Members and their owners"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Node"</th>
                        <th scope="col">"Organisation"</th>
                        <th scope="col">"system_id"</th>
                        <th scope="col">"Endpoints"</th>
                        <th scope="col">"Owner"</th>
                        <th scope="col">"State"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </section>
    }
    .into_any()
}

/// The stored-query versions, each with its owner.
fn queries_section(registry: QueryRegistry, queries: &[QueryResource]) -> AnyView {
    if registry == QueryRegistry::Absent {
        return view! {
            <section aria-labelledby="config-queries">
                <h2 id="config-queries">"Stored queries"</h2>
                <p>"The gateway offers no stored-query registry."</p>
            </section>
        }
        .into_any();
    }
    let rows = queries
        .iter()
        .map(|query| {
            view! {
                <tr>
                    <th scope="row">{query.name.clone()}</th>
                    <td>{query.version.clone()}</td>
                    <td>{owner(query.owner)}</td>
                </tr>
            }
        })
        .collect_view();
    let read_only = (registry == QueryRegistry::ReadOnly).then(|| {
        view! {
            <p>
                "The stored queries are read from the read-only definition files, so the "
                "console stores no version: change the files."
            </p>
        }
    });
    view! {
        <section aria-labelledby="config-queries">
            <h2 id="config-queries">"Stored queries"</h2>
            <p>
                "A stored version is immutable (§12.7): a change is a new version, and "
                "none is edited or removed."
            </p>
            {read_only}
            <table>
                <caption>"Stored-query versions and their owners"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Name"</th>
                        <th scope="col">"Version"</th>
                        <th scope="col">"Owner"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </section>
    }
    .into_any()
}

/// The newest revisions, newest first.
fn history_section(history: &Page<Revision>) -> AnyView {
    let rows = history
        .items
        .iter()
        .map(|revision| {
            let origin = match revision.restores {
                Some(restored) => format!("rollback to revision {restored}"),
                None => origin(revision.origin).to_owned(),
            };
            view! {
                <tr>
                    <th scope="row">{revision.id.to_string()}</th>
                    <td>{origin}</td>
                    <td>{author(&revision.author)}</td>
                    <td>{revision.at.to_string()}</td>
                    <td>{revision.reason.clone()}</td>
                </tr>
            }
        })
        .collect_view();
    let shown = format!(
        "The {} newest of {} revisions, newest first",
        history.items.len(),
        history.total
    );
    view! {
        <section aria-labelledby="config-history">
            <h2 id="config-history">"History"</h2>
            <table>
                <caption>{shown}</caption>
                <thead>
                    <tr>
                        <th scope="col">"Revision"</th>
                        <th scope="col">"Origin"</th>
                        <th scope="col">"Made by"</th>
                        <th scope="col">"At"</th>
                        <th scope="col">"Reason"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </section>
    }
    .into_any()
}

/// One form of the page, before it is rendered.
struct FormView {
    /// The id of the form's heading, unique on the page.
    id: String,
    /// The form's heading.
    heading: String,
    /// Its submit button's label.
    label: String,
    /// Its fields, rendered as HTML.
    fields: String,
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

/// The forms the page offers for `standing` and `resources`.
fn forms(standing: &Standing, resources: &Resources) -> Vec<FormView> {
    let mut forms = Vec::new();
    let running = standing.current.as_ref().map(|mark| mark.revision);
    if let Some(head) = standing.head.filter(|head| Some(*head) != running) {
        forms.push(apply_form(head));
    }
    if let Some(mark) = &standing.current
        && let Some(previous) = mark.previous
    {
        forms.push(rollback_form(mark.revision, previous));
    }
    let editable: Vec<&ConsoleMember> = resources
        .members
        .iter()
        .filter(|member| {
            matches!(
                member.state,
                Some(ConsoleState::Running | ConsoleState::Pending)
            )
        })
        .filter_map(|member| member.member.as_ref())
        .collect();
    forms.push(member_form(
        "add-member",
        "Add a console member",
        standing.head,
        None,
    ));
    for member in &editable {
        let id = format!("edit-{}", member.node.id.as_str());
        let heading = format!("Edit console member {}", member.node.id.as_str());
        forms.push(member_form(&id, &heading, standing.head, Some(member)));
    }
    let removable: Vec<&str> = resources
        .members
        .iter()
        .filter(|member| {
            member.owner == Owner::Console && member.state != Some(ConsoleState::Removing)
        })
        .map(|member| member.node.as_str())
        .collect();
    if !removable.is_empty() {
        forms.push(remove_form(standing.head, &removable));
    }
    if resources.stored_query_registry == QueryRegistry::Writable {
        forms.push(query_form());
    }
    forms
}

/// A hidden field `name` of the form, holding `value`.
fn hidden(name: &'static str, value: String) -> impl IntoView {
    view! { <input type="hidden" name=format!("form[{name}]") value=value /> }
}

/// A labelled one-line field `name` of the form `form`, holding `value`.
fn field(
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
fn confirm(form: &str, label: &'static str) -> impl IntoView {
    let id = format!("{form}-confirm");
    view! {
        <p>
            <input type="checkbox" id=id.clone() name="form[confirm]" value="yes" required />
            <label for=id>{label}</label>
        </p>
    }
}

/// The base a change is saved on, `""` for none.
fn base(head: Option<RevisionId>) -> String {
    head.map(|head| head.to_string()).unwrap_or_default()
}

/// The apply of revision `head`.
fn apply_form(head: RevisionId) -> FormView {
    let said = format!(
        "Applies revision {head} through the reload path; every replica sharing the store follows it. Its changes are listed under Revisions."
    );
    let fields = view! {
        {hidden("action", String::from("apply"))}
        {hidden("revision", head.to_string())}
        <p>{said}</p>
        {confirm("apply", "I confirm applying this revision to the running gateway")}
    };
    FormView {
        id: String::from("apply"),
        heading: format!("Apply revision {head}"),
        label: String::from("Apply the revision"),
        fields: fields.to_html(),
    }
}

/// The rollback of the running revision `running` to `previous`'s content.
fn rollback_form(running: RevisionId, previous: RevisionId) -> FormView {
    let said = format!(
        "Saves the content of revision {previous} as a new revision and applies it, in one step."
    );
    let fields = view! {
        {hidden("action", String::from("rollback"))}
        {hidden("current", running.to_string())}
        <p>{said}</p>
        {field("rollback", ("reason", "Reason"), String::new(), (true, false))}
        {confirm("rollback", "I confirm rolling the running configuration back")}
    };
    FormView {
        id: String::from("rollback"),
        heading: format!("Roll back revision {running}"),
        label: String::from("Roll back"),
        fields: fields.to_html(),
    }
}

/// The Query API endpoints of `member`, as the endpoints field writes them.
fn endpoints_text(member: &ConsoleMember) -> String {
    member
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.connection_type == ConnectionType::OpenehrRestQuery)
        .map(|endpoint| {
            let status = if endpoint.status == EndpointStatus::Suspended {
                " suspended"
            } else {
                ""
            };
            format!("{} {}{status}", endpoint.id.as_str(), endpoint.url)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The save of a console member, `member` filled in for an edit.
fn member_form(
    id: &str,
    heading: &str,
    head: Option<RevisionId>,
    member: Option<&ConsoleMember>,
) -> FormView {
    let value = |read: fn(&ConsoleMember) -> String| member.map(read).unwrap_or_default();
    let editing = member.is_some();
    let fields = view! {
        {hidden("action", String::from("save-member"))}
        {hidden("base", base(head))}
        {field(
            id,
            ("organisation", "Organisation id"),
            value(|m| m.organisation.id.as_str().to_owned()),
            (true, false),
        )}
        {field(
            id,
            ("organisation_name", "Organisation name"),
            value(|m| m.organisation.name.clone().unwrap_or_default()),
            (false, false),
        )}
        {field(id, ("node", "node_id"), value(|m| m.node.id.as_str().to_owned()), (true, editing))}
        {field(
            id,
            ("system_id", "system_id"),
            value(|m| m.node.system_id.as_str().to_owned()),
            (true, false),
        )}
        {field(
            id,
            ("product", "CDR product"),
            value(|m| m.node.product.clone().unwrap_or_default()),
            (false, false),
        )}
        {field(
            id,
            ("product_version", "CDR product version"),
            value(|m| m.node.version.clone().unwrap_or_default()),
            (false, false),
        )}
        {field(
            id,
            (
                "endpoints",
                "Query API endpoints: endpoint_id and base URL, then suspended if it is out of service; separate endpoints with a semicolon",
            ),
            value(endpoints_text),
            (true, false),
        )}
        {field(id, ("reason", "Reason"), String::new(), (true, false))}
        {confirm(id, "I confirm saving this member as a new revision, which I apply separately")}
    };
    FormView {
        id: id.to_owned(),
        heading: heading.to_owned(),
        label: String::from("Save as a new revision"),
        fields: fields.to_html(),
    }
}

/// The removal of one of the console members `nodes`.
fn remove_form(head: Option<RevisionId>, nodes: &[&str]) -> FormView {
    let choices = nodes
        .iter()
        .map(|node| view! { <option value=(*node).to_owned()>{(*node).to_owned()}</option> })
        .collect_view();
    let fields = view! {
        {hidden("action", String::from("remove-member"))}
        {hidden("base", base(head))}
        <p>
            <label for="remove-member-node">"Console member"</label>
            " "
            <select id="remove-member-node" name="form[node]">
                {choices}
            </select>
        </p>
        {field("remove-member", ("reason", "Reason"), String::new(), (true, false))}
        {confirm(
            "remove-member",
            "I confirm saving this removal as a new revision, which I apply separately",
        )}
    };
    FormView {
        id: String::from("remove-member"),
        heading: String::from("Remove a console member"),
        label: String::from("Save the removal as a new revision"),
        fields: fields.to_html(),
    }
}

/// The store of a new stored-query version.
fn query_form() -> FormView {
    let fields = view! {
        {hidden("action", String::from("store-query"))}
        <p>
            "Stores a new version in the gateway's stored-query store at once. A stored "
            "version is immutable: it is never edited or removed."
        </p>
        {field("store-query", ("name", "Qualified name"), String::new(), (true, false))}
        {field(
            "store-query",
            ("version", "Version, major.minor.patch"),
            String::new(),
            (true, false),
        )}
        <p>
            <label for="store-query-aql">"AQL"</label>
            <textarea
                id="store-query-aql"
                name="form[aql]"
                rows="6"
                spellcheck="false"
                required
            ></textarea>
        </p>
        {confirm(
            "store-query",
            "I confirm storing this version, which cannot be changed afterwards",
        )}
    };
    FormView {
        id: String::from("store-query"),
        heading: String::from("Store a stored-query version"),
        label: String::from("Store the version"),
        fields: fields.to_html(),
    }
}

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
