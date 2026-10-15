// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page's forms, before each is rendered whole.

use ferrofed_registry::console::{ConsoleMember, ConsoleState, Owner, QueryRegistry, Resources};
use ferrofed_registry::revision::{RevisionId, Standing};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use leptos::prelude::*;

use crate::configure::render::{FormView, base, confirm, field, hidden};

/// The forms the page offers for `standing` and `resources`.
pub(super) fn forms(standing: &Standing, resources: &Resources) -> Vec<FormView> {
    let mut forms = crate::configure::sections::decisions(standing);
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
    forms.extend(crate::configure::sections::forms(standing, resources));
    forms
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

/// The FHIR endpoints of `member`, as the FHIR endpoints field writes them.
fn fhir_text(member: &ConsoleMember) -> String {
    member
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.connection_type == ConnectionType::Hl7FhirRest)
        .map(|endpoint| {
            let status = if endpoint.status == EndpointStatus::Suspended {
                " suspended"
            } else {
                ""
            };
            format!(
                "{} {} system={} categories={} profiles={}{status}",
                endpoint.id.as_str(),
                endpoint.url,
                endpoint.ehr_id_system.as_deref().unwrap_or_default(),
                endpoint.categories.join(","),
                endpoint.profiles.join(","),
            )
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
        {field(
            id,
            (
                "fhir_endpoints",
                "FHIR endpoints: endpoint_id and base URL, then system= the identifier system of the ehr_id, categories= and profiles=, each a list separated by commas, then suspended if it is out of service; separate endpoints with a semicolon",
            ),
            value(fhir_text),
            (false, false),
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
