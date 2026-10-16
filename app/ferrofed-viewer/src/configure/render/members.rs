// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Members tab: the members with their owners and states, the detail of
//! a read-only member, and the form that adds or edits a console member,
//! each endpoint a row of its own fields.

use ferrofed_registry::console::{
    ConsoleEndpoint, ConsoleMember, ConsoleState, MemberResource, Owner,
};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use leptos::prelude::*;

use crate::configure::member::{ADD_FHIR, ADD_QUERY, fhir_field, field, query_field};
use crate::configure::plan::removal_reason;
use crate::configure::render::Loaded;
use crate::configure::render::parts::{
    Text, add_button, confirmed, error_line, form, head, hidden, reason, remove_button, save_bar,
    template, text,
};
use crate::styles::{self, Button};
use crate::views::model::Refusal;

/// The path of the member `node`'s form or detail.
fn href(node: &str) -> String {
    format!("/configuration/members/{node}")
}

/// The owner of a member or a resource as a badge that says whether the
/// console changes it.
pub(super) fn owner(owner: Owner) -> AnyView {
    let (word, tone) = match owner {
        Owner::Console => ("Console", "badge tone-accent"),
        Owner::File => ("File, read-only", "badge tone-neutral"),
        Owner::Directory => ("Directory, read-only", "badge tone-neutral"),
        Owner::Gateway => ("Gateway, read-only", "badge tone-neutral"),
        Owner::Store => ("Store, immutable", "badge tone-neutral"),
        _ => ("Elsewhere, read-only", "badge tone-neutral"),
    };
    view! { <span class=tone>{word}</span> }.into_any()
}

/// Where a console resource stands, as a pill that says so in words; one
/// the console does not hold runs as its source provides it.
pub(super) fn state(state: Option<ConsoleState>) -> AnyView {
    let (word, tone) = match state {
        None | Some(ConsoleState::Running) => ("running", "pill tone-ok"),
        Some(ConsoleState::Pending) => ("saved, not applied", "pill tone-warn"),
        Some(ConsoleState::Removing) => ("removed, not applied", "pill tone-warn"),
        Some(ConsoleState::Shadowed) => ("shadowed by the file", "pill tone-neutral"),
        Some(_) => ("unknown", "pill tone-neutral"),
    };
    view! { <span class=tone>{word}</span> }.into_any()
}

/// The removal of the console member `node`, a confirmation step asking
/// for the reason.
fn remove(loaded: &Loaded<'_>, node: &str) -> AnyView {
    let reason_id = removal_reason(node);
    form(
        "main",
        "inline",
        view! {
            {hidden("action", String::from("remove-member"))}
            {hidden("base", loaded.base())}
            {hidden("node", node.to_owned())}
            {confirmed(
                format!("remove-{node}"),
                (
                    view! {
                        "Remove"
                        <span class="sr-only">{format!(" {node}")}</span>
                    },
                    Button::Ghost,
                ),
                (
                    format!(
                        "Remove member {node}? It is saved as {} without it, which you apply separately.",
                        loaded.next(),
                    ),
                    "Save the removal",
                ),
                text(
                    (&reason_id, "form[reason]"),
                    String::from("Reason"),
                    String::new(),
                    Text {
                        required: true,
                        ..Text::default()
                    },
                ),
            )}
        },
    )
    .into_any()
}

/// One member's row.
fn row(loaded: &Loaded<'_>, member: &MemberResource) -> AnyView {
    let node = member.node.as_str().to_owned();
    let ids = member
        .endpoints
        .iter()
        .map(ferrofed_registry::id::EndpointId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let count = member.endpoints.len().to_string();
    let editable = member.owner == Owner::Console
        && matches!(
            member.state,
            Some(ConsoleState::Running | ConsoleState::Pending)
        );
    let actions = if editable {
        view! {
            <a href=href(&node) class=styles::BUTTON_GHOST>
                "Edit"
                <span class="sr-only">{format!(" {node}")}</span>
            </a>
            {remove(loaded, &node)}
        }
        .into_any()
    } else {
        view! {
            <a href=href(&node) class=styles::BUTTON_GHOST>
                "View"
                <span class="sr-only">{format!(" {node}")}</span>
            </a>
        }
        .into_any()
    };
    view! {
        <tr>
            <th scope="row">
                <code>{node.clone()}</code>
            </th>
            <td>{member.organisation.as_str().to_owned()}</td>
            <td>
                <code>{member.system_id.as_str().to_owned()}</code>
            </td>
            <td class=styles::NUM title=ids>
                {count}
            </td>
            <td>{owner(member.owner)}</td>
            <td>{state(member.state)}</td>
            <td>
                <div class="flex flex-wrap items-center justify-end gap-1">{actions}</div>
            </td>
        </tr>
    }
    .into_any()
}

/// The Members tab: every member with its owner and state.
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let members = &loaded.resources.members;
    let rows = if members.is_empty() {
        view! {
            <tr>
                <td colspan="7" class="text-muted">
                    "No member is configured yet."
                </td>
            </tr>
        }
        .into_any()
    } else {
        members
            .iter()
            .map(|member| row(loaded, member))
            .collect_view()
            .into_any()
    };
    view! {
        {head(
            ("members-title", String::from("Members")),
            (
                "A member the registry document or a care services directory provides is read-only here.",
                "console-managed-resources",
            ),
            view! {
                <a href="/configuration/members/new" class=styles::BUTTON_PRIMARY>
                    {crate::components::icon(icondata_lu::LuPlus, "16")}
                    "Add member"
                </a>
            },
        )}
        <div
            class=format!("{} relative", styles::TABLE_SCROLL)
            tabindex="0"
            role="region"
            aria-labelledby="members-title"
        >
            <table>
                <caption class="sr-only">"Members and their owners"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Node"</th>
                        <th scope="col">"Organisation"</th>
                        <th scope="col">"system_id"</th>
                        <th scope="col" class=styles::NUM>
                            "Endpoints"
                        </th>
                        <th scope="col">"Owner"</th>
                        <th scope="col">"State"</th>
                        <th scope="col">
                            <span class="sr-only">"Actions"</span>
                        </th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
    .into_any()
}

/// The page of the member `node`: the form of a console member, or the
/// detail of one its source provides.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for a `node_id` no member has.
pub(super) fn member(loaded: &Loaded<'_>, node: &str) -> Result<AnyView, Refusal> {
    let found = loaded
        .resources
        .members
        .iter()
        .find(|member| member.node.as_str() == node)
        .ok_or_else(|| Refusal::Invalid {
            reason: format!("No member has the node_id {node}."),
        })?;
    let editable = matches!(
        found.state,
        Some(ConsoleState::Running | ConsoleState::Pending)
    );
    match (&found.member, editable) {
        (Some(held), true) => Ok(member_form(loaded, Some(held)).into_any()),
        _ => Ok(detail(found).into_any()),
    }
}

/// The detail of a member the console does not change, with the source
/// that provides it.
fn detail(member: &MemberResource) -> AnyView {
    let source = match (member.owner, member.state) {
        (Owner::File, _) => {
            "The configuration file's registry document provides this member: change it there."
        }
        (Owner::Directory, _) => {
            "The care services directory provides this member: change it there."
        }
        (_, Some(ConsoleState::Removing)) => {
            "The newest revision removes this member; apply it from the card above."
        }
        (_, Some(ConsoleState::Shadowed)) => {
            "The registry document provides a member with the same identity, which runs in its place."
        }
        _ => "The console does not change this member.",
    };
    let endpoints = member
        .endpoints
        .iter()
        .map(|endpoint| {
            view! {
                <li>
                    <code>{endpoint.as_str().to_owned()}</code>
                </li>
            }
        })
        .collect_view();
    view! {
        <p class="mt-0 mb-2 text-sm">
            <a href="/configuration/members">"Members"</a>
        </p>
        <h2 id="member-title" class="mt-0 mb-1 text-base font-semibold text-ink">
            <code>{member.node.as_str().to_owned()}</code>
        </h2>
        <p class="mt-0 mb-4 text-sm text-muted" role="note">
            {source}
        </p>
        <dl class="m-0 grid grid-cols-1 gap-x-6 gap-y-2 sm:grid-cols-[12rem_1fr]">
            <dt class="text-muted font-medium">"Organisation"</dt>
            <dd class="m-0">{member.organisation.as_str().to_owned()}</dd>
            <dt class="text-muted font-medium">"system_id"</dt>
            <dd class="m-0">
                <code>{member.system_id.as_str().to_owned()}</code>
            </dd>
            <dt class="text-muted font-medium">"Owner"</dt>
            <dd class="m-0">{owner(member.owner)}</dd>
            <dt class="text-muted font-medium">"State"</dt>
            <dd class="m-0">{state(member.state)}</dd>
            <dt class="text-muted font-medium">"Endpoints"</dt>
            <dd class="m-0">
                <ul class="m-0 list-none p-0">{endpoints}</ul>
            </dd>
        </dl>
    }
    .into_any()
}

/// The classes of one group of a form: a heading over its fields, set off
/// by a hairline.
pub(super) const GROUP: &str = "m-0 mb-6 min-w-0 rounded-none border-0 border-t border-line \
                                bg-transparent p-0 pt-4 shadow-none";

/// The classes of a group's heading.
pub(super) const GROUP_TITLE: &str = "mb-3 text-sm font-semibold text-ink";

/// The classes of one repeatable row: a small card of its own fields.
const ROW: &str = "m-0 min-w-0 rounded-card border border-line bg-surface p-4 shadow-none";

/// The classes of a row's heading.
const ROW_TITLE: &str = "mb-2 text-xs font-semibold tracking-wide text-muted uppercase";

/// The "In service" switch of the endpoint field `id`, named `name`.
fn in_service(id: String, name: String, on: bool) -> AnyView {
    let label_for = id.clone();
    view! {
        <label for=label_for class="inline-flex items-center gap-2 text-sm font-medium">
            <input type="checkbox" role="switch" id=id name=name value="on" checked=on />
            "In service"
        </label>
    }
    .into_any()
}

/// A Query API endpoint row `key`, holding `endpoint` when the member has
/// it.
fn query_row(key: &str, endpoint: Option<&ConsoleEndpoint>) -> AnyView {
    let row = format!("cfg-q-{key}");
    let named = endpoint.map_or_else(
        || String::from("New Query API endpoint"),
        |endpoint| format!("Query API endpoint {}", endpoint.id.as_str()),
    );
    let name = |part: &str| format!("form[query][{key}][{part}]");
    view! {
        <fieldset id=row.clone() class=ROW>
            <legend class=ROW_TITLE>{named.clone()}</legend>
            <input type="hidden" name="form[query_rows]" value=key.to_owned() />
            <div class="clear-both grid grid-cols-1 gap-x-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
                {text(
                    (&query_field(key, "id"), &name("id")),
                    String::from("Endpoint id"),
                    endpoint.map(|endpoint| endpoint.id.as_str().to_owned()).unwrap_or_default(),
                    Text {
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
                {text(
                    (&query_field(key, "url"), &name("url")),
                    String::from("Base URL"),
                    endpoint.map(|endpoint| endpoint.url.clone()).unwrap_or_default(),
                    Text {
                        kind: "url",
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
            </div>
            <div class="flex flex-wrap items-center justify-between gap-2">
                {in_service(
                    query_field(key, "service"),
                    name("in_service"),
                    endpoint.is_none_or(|endpoint| endpoint.status != EndpointStatus::Suspended),
                )} {remove_button(&row, format!("Remove {named}"))}
            </div>
        </fieldset>
    }
    .into_any()
}

// NOTE: Regulation (EU) 2025/327 Art 14(1), points (a) to (f): the priority
// categories by the codes the exchange format names them.
/// The categories the console offers for a FHIR endpoint, each its code and
/// its name.
const CATEGORIES: [(&str, &str); 6] = [
    ("Patient-Summaries", "Patient summaries"),
    ("Electronic-Prescriptions", "Electronic prescriptions"),
    ("Electronic-Dispensations", "Electronic dispensations"),
    ("Medical-Imaging", "Medical imaging"),
    ("Laboratory-Reports", "Laboratory reports"),
    ("Discharge-Reports", "Discharge reports"),
];

/// A profile item `item` of the FHIR endpoint row `key`, holding `value`.
fn profile(key: &str, item: &str, value: String) -> AnyView {
    let id = format!("cfg-f-{key}-p-{item}");
    view! {
        <div id=format!("{id}-row") class="flex items-end gap-2">
            <div class="min-w-0 flex-1">
                <label for=id.clone() class="sr-only">
                    "Profile"
                </label>
                <input
                    type="url"
                    id=id.clone()
                    name=format!("form[fhir][{key}][profiles]")
                    value=value
                    class=styles::MONO
                    autocomplete="off"
                    spellcheck="false"
                />
            </div>
            {remove_button(&format!("{id}-row"), String::from("Remove this profile"))}
        </div>
    }
    .into_any()
}

/// The categories of the FHIR endpoint row `key`, those it serves
/// `served` ticked; a category the console does not know is offered too.
fn categories(key: &str, served: &[String]) -> AnyView {
    let group = fhir_field(key, "categories");
    let mut offered: Vec<(String, String)> = CATEGORIES
        .iter()
        .map(|(code, name)| ((*code).to_owned(), (*name).to_owned()))
        .collect();
    for code in served {
        if !offered.iter().any(|(known, _)| known == code) {
            offered.push((code.clone(), code.clone()));
        }
    }
    let boxes = offered
        .into_iter()
        .map(|(code, name)| {
            let id = format!("{group}-{code}");
            let label_for = id.clone();
            let on = served.contains(&code);
            view! {
                <label for=label_for class="inline-flex items-center gap-2 text-sm font-normal">
                    <input
                        type="checkbox"
                        id=id
                        name=format!("form[fhir][{key}][categories]")
                        value=code
                        checked=on
                    />
                    {name}
                </label>
            }
        })
        .collect_view();
    view! {
        <fieldset
            id=group.clone()
            class="m-0 mb-4 min-w-0 rounded-none border-0 bg-transparent p-0 shadow-none"
            aria-describedby=format!("{group}-error")
        >
            <legend class="mb-1 text-sm font-medium">"Categories"</legend>
            <div class="grid grid-cols-1 gap-1 sm:grid-cols-2 lg:grid-cols-3">{boxes}</div>
            {error_line(&group)}
        </fieldset>
    }
    .into_any()
}

/// A FHIR endpoint row `key`, holding `endpoint` when the member has it.
fn fhir_row(key: &str, endpoint: Option<&ConsoleEndpoint>) -> AnyView {
    let row = format!("cfg-f-{key}");
    let named = endpoint.map_or_else(
        || String::from("New FHIR endpoint"),
        |endpoint| format!("FHIR endpoint {}", endpoint.id.as_str()),
    );
    let name = |part: &str| format!("form[fhir][{key}][{part}]");
    let profiles = endpoint
        .map(|endpoint| endpoint.profiles.clone())
        .unwrap_or_default()
        .into_iter()
        .zip(0_u32..)
        .map(|(value, item)| profile(key, &item.to_string(), value))
        .collect_view();
    let served = endpoint
        .map(|endpoint| endpoint.categories.clone())
        .unwrap_or_default();
    let list = format!("cfg-f-{key}-profiles");
    let profile_template = format!("tpl-p-{key}");
    view! {
        <fieldset id=row.clone() class=ROW>
            <legend class=ROW_TITLE>{named.clone()}</legend>
            <input type="hidden" name="form[fhir_rows]" value=key.to_owned() />
            <div class="clear-both grid grid-cols-1 gap-x-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,2fr)]">
                {text(
                    (&fhir_field(key, "id"), &name("id")),
                    String::from("Endpoint id"),
                    endpoint.map(|endpoint| endpoint.id.as_str().to_owned()).unwrap_or_default(),
                    Text {
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
                {text(
                    (&fhir_field(key, "url"), &name("url")),
                    String::from("Base URL"),
                    endpoint.map(|endpoint| endpoint.url.clone()).unwrap_or_default(),
                    Text {
                        kind: "url",
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
            </div>
            {text(
                (&fhir_field(key, "system"), &name("system")),
                String::from("Identifier system of the ehr_id"),
                endpoint.and_then(|endpoint| endpoint.ehr_id_system.clone()).unwrap_or_default(),
                Text {
                    required: true,
                    mono: true,
                    hint: Some(
                        "The system the endpoint is told the ehr_id in, such as urn:oid:2.999.1.",
                    ),
                    ..Text::default()
                },
            )}
            {categories(key, &served)}
            <fieldset class="m-0 mb-4 min-w-0 rounded-none border-0 bg-transparent p-0 shadow-none">
                <legend class="mb-1 text-sm font-medium">"Profiles"</legend>
                <div id=list.clone() class="mb-2 flex flex-col gap-2">
                    {profiles}
                </div>
                {add_button(
                    &fhir_field(key, "add-profile"),
                    (&profile_template, &list),
                    ("Add profile", "__ITEM__"),
                )}
                {template(profile_template.clone(), profile(key, "__ITEM__", String::new()))}
            </fieldset>
            <div class="flex flex-wrap items-center justify-between gap-2">
                {in_service(
                    fhir_field(key, "service"),
                    name("in_service"),
                    endpoint.is_none_or(|endpoint| endpoint.status != EndpointStatus::Suspended),
                )} {remove_button(&row, format!("Remove {named}"))}
            </div>
        </fieldset>
    }
    .into_any()
}

/// The endpoints of `member` of `kind`, each with its row key.
fn endpoints(
    member: Option<&ConsoleMember>,
    kind: ConnectionType,
) -> Vec<(String, &ConsoleEndpoint)> {
    member
        .map(|member| {
            member
                .endpoints
                .iter()
                .filter(|endpoint| endpoint.connection_type == kind)
                .zip(0_u32..)
                .map(|(endpoint, key)| (key.to_string(), endpoint))
                .collect()
        })
        .unwrap_or_default()
}

/// The organisation and the node of the member form, filled in from
/// `member` for an edit, whose `node_id` stays as it is.
fn identity(member: Option<&ConsoleMember>) -> AnyView {
    let editing = member.is_some();
    let value = |read: fn(&ConsoleMember) -> String| member.map(read).unwrap_or_default();
    let required = |hint| Text {
        required: true,
        mono: true,
        hint,
        ..Text::default()
    };
    view! {
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Organisation"</legend>
            <div class="grid grid-cols-1 gap-x-4 sm:grid-cols-2">
                {text(
                    (&field("organisation"), "form[organisation]"),
                    String::from("Organisation id"),
                    value(|m| m.organisation.id.as_str().to_owned()),
                    required(None),
                )}
                {text(
                    (&field("organisation_name"), "form[organisation_name]"),
                    String::from("Organisation name"),
                    value(|m| m.organisation.name.clone().unwrap_or_default()),
                    Text::default(),
                )}
            </div>
        </fieldset>
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Node"</legend>
            <div class="grid grid-cols-1 gap-x-4 sm:grid-cols-2">
                {text(
                    (&field("node"), "form[node]"),
                    String::from("node_id"),
                    value(|m| m.node.id.as_str().to_owned()),
                    Text {
                        readonly: editing,
                        ..required(
                            editing
                                .then_some(
                                    "A member's node_id is its identity: add a new member to change it.",
                                ),
                        )
                    },
                )}
                {text(
                    (&field("system_id"), "form[system_id]"),
                    String::from("system_id"),
                    value(|m| m.node.system_id.as_str().to_owned()),
                    required(None),
                )}
                {text(
                    (&field("product"), "form[product]"),
                    String::from("CDR product"),
                    value(|m| m.node.product.clone().unwrap_or_default()),
                    Text::default(),
                )}
                {text(
                    (&field("product_version"), "form[product_version]"),
                    String::from("CDR product version"),
                    value(|m| m.node.version.clone().unwrap_or_default()),
                    Text::default(),
                )}
            </div>
        </fieldset>
    }
    .into_any()
}

/// The Query API and FHIR endpoints of the member form, a row each, with
/// one empty Query API row for a new member.
fn endpoint_groups(member: Option<&ConsoleMember>) -> AnyView {
    let query_rows = endpoints(member, ConnectionType::OpenehrRestQuery)
        .into_iter()
        .map(|(key, endpoint)| query_row(&key, Some(endpoint)))
        .collect_view();
    let fhir_rows = endpoints(member, ConnectionType::Hl7FhirRest)
        .into_iter()
        .map(|(key, endpoint)| fhir_row(&key, Some(endpoint)))
        .collect_view();
    let fresh_query = member.is_none().then(|| query_row("0", None));
    view! {
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Query API endpoints"</legend>
            <p class="mt-0 mb-3 text-sm text-muted">
                "The ITS-REST endpoints the gateway sends each member's AQL to."
            </p>
            <div id="cfg-query-rows" class="mb-3 flex flex-col gap-3">
                {query_rows}
                {fresh_query}
            </div>
            {add_button(ADD_QUERY, ("tpl-query", "cfg-query-rows"), ("Add endpoint", "__KEY__"))}
            {template(String::from("tpl-query"), query_row("__KEY__", None))}
        </fieldset>
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"FHIR endpoints"</legend>
            <p class="mt-0 mb-3 text-sm text-muted">
                "The exchange-format endpoints that serve the categories each one names."
            </p>
            <div id="cfg-fhir-rows" class="mb-3 flex flex-col gap-3">
                {fhir_rows}
            </div>
            {add_button(ADD_FHIR, ("tpl-fhir", "cfg-fhir-rows"), ("Add FHIR endpoint", "__KEY__"))}
            {template(String::from("tpl-fhir"), fhir_row("__KEY__", None))}
        </fieldset>
    }
    .into_any()
}

/// The form that adds a member, or edits the console member `member`, in
/// groups: the organisation, the node, its Query API and FHIR endpoints,
/// and the reason.
pub(super) fn member_form(loaded: &Loaded<'_>, member: Option<&ConsoleMember>) -> AnyView {
    let editing = member.is_some();
    let title = member.map_or_else(
        || String::from("Add a member"),
        |member| format!("Edit member {}", member.node.id.as_str()),
    );
    let body = view! {
        {hidden("action", String::from(if editing { "save-member" } else { "add-member" }))}
        {hidden("base", loaded.base())}
        {hidden("fhir_shown", String::from("yes"))}
        {identity(member)}
        {endpoint_groups(member)}
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Reason"</legend>
            {reason()}
        </fieldset>
        {save_bar(loaded, "/configuration/members")}
    };
    view! {
        <p class="mt-0 mb-2 text-sm">
            <a href="/configuration/members">"Members"</a>
        </p>
        {head(
            ("member-title", title),
            (
                "A member is saved as a new revision, which you apply separately.",
                "console-managed-resources",
            ),
            (),
        )}
        {form("main", "", body)}
    }
    .into_any()
}
