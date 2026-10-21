// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Members tab: the members with their owners and states, the detail of
//! a read-only member, and the form that adds or edits a console member,
//! each endpoint a row of its own fields.

use ferrofed_registry::console::{
    ConsoleEndpoint, ConsoleMember, ConsoleState, MemberResource, Owner,
};
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::{ACTIONS, Column, NUM};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{Field, INPUT, SWITCH};
use ferrohealth_ui::view::layout::{FieldRow, keyed_field_row};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::definitions;
use leptos::prelude::*;

use crate::configure::member::{ADD_FHIR, ADD_QUERY, fhir_field, field, query_field};
use crate::configure::plan::removal_reason;
use crate::configure::render::Loaded;
use crate::configure::render::parts::{
    Text, add_button, confirmed, error_line, form, head, hidden, reason, remove_button, save_bar,
    template, text, trail,
};
use crate::locale::{Msg, tr, tr_with};
use crate::views::model::Refusal;

/// The path of the member `node`'s form or detail.
fn href(node: &str) -> String {
    format!("/configuration/members/{node}")
}

/// The owner of a member or a resource as a badge that says whether the
/// console changes it.
pub(super) fn owner(owner: Owner) -> AnyView {
    let (word, tone) = match owner {
        Owner::Console => (tr(Msg::Console), Tone::Accent),
        Owner::File => (tr(Msg::FileReadOnly), Tone::Neutral),
        Owner::Directory => (tr(Msg::DirectoryReadOnly), Tone::Neutral),
        Owner::Gateway => (tr(Msg::GatewayReadOnly), Tone::Neutral),
        Owner::Store => (tr(Msg::StoreImmutable), Tone::Neutral),
        _ => (tr(Msg::ElsewhereReadOnly), Tone::Neutral),
    };
    badge(tone, &word).into_any()
}

/// Where a console resource stands, as a pill that says so in words; one
/// the console does not hold runs as its source provides it.
pub(super) fn state(state: Option<ConsoleState>) -> AnyView {
    let (word, tone) = match state {
        None | Some(ConsoleState::Running) => (tr(Msg::StateRunning), Tone::Ok),
        Some(ConsoleState::Pending) => (tr(Msg::SavedNotApplied), Tone::Warn),
        Some(ConsoleState::Removing) => (tr(Msg::RemovedNotApplied), Tone::Warn),
        Some(ConsoleState::Shadowed) => (tr(Msg::ShadowedByTheFile), Tone::Neutral),
        Some(_) => (tr(Msg::StateUnknown), Tone::Neutral),
    };
    pill(tone, &word).into_any()
}

/// The removal of the console member `node`, a confirmation step asking
/// for the reason.
fn remove(loaded: &Loaded<'_>, node: &str) -> AnyView {
    let reason_id = removal_reason(node);
    form(
        "main",
        "",
        view! {
            {hidden("action", String::from("remove-member"))}
            {hidden("base", loaded.base())}
            {hidden("node", node.to_owned())}
            {confirmed(
                format!("remove-{node}"),
                (
                    view! {
                        {tr(Msg::Remove)}
                        <span class="fui-sr-only">{format!(" {node}")}</span>
                    },
                    Variant::Ghost,
                ),
                (
                    tr_with(
                        Msg::RemoveMemberSavedWithoutWhichYou,
                        &[
                            ("node", (node).to_string().into()),
                            ("loaded", (loaded.next()).clone().into()),
                        ],
                    ),
                    tr(Msg::SaveTheRemoval),
                ),
                text(
                    (&reason_id, "form[reason]"),
                    tr(Msg::Reason),
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
        let edit = view! {
            <a href=href(&node) class=Variant::Ghost.class()>
                {tr(Msg::Edit)}
                <span class="fui-sr-only">{format!(" {node}")}</span>
            </a>
        }
        .into_any();
        view! {
            {edit}
            {remove(loaded, &node)}
        }
        .into_any()
    } else {
        view! {
            <a href=href(&node) class=Variant::Ghost.class()>
                {tr(Msg::View)}
                <span class="fui-sr-only">{format!(" {node}")}</span>
            </a>
        }
        .into_any()
    };
    view! {
        <tr>
            <th scope="row">
                <code>{node.clone()}</code>
            </th>
            <td data-label=tr(Msg::Organisation)>{member.organisation.as_str().to_owned()}</td>
            <td data-label=tr(Msg::SystemId)>
                <code>{member.system_id.as_str().to_owned()}</code>
            </td>
            <td data-label=tr(Msg::Endpoints) class=NUM title=ids>
                {count}
            </td>
            <td data-label=tr(Msg::Owner)>{owner(member.owner)}</td>
            <td data-label=tr(Msg::State)>{state(member.state)}</td>
            <td data-label=tr(Msg::Actions) class=ACTIONS>
                {actions}
            </td>
        </tr>
    }
    .into_any()
}

/// The columns of the Members tab.
fn member_columns() -> [Column; 7] {
    [
        Column::text("node", tr(Msg::Node)),
        Column::text("organisation", tr(Msg::Organisation)),
        Column::text("system_id", tr(Msg::SystemId)),
        Column::text("endpoints", tr(Msg::Endpoints)).numeric(),
        Column::text("owner", tr(Msg::Owner)),
        Column::text("state", tr(Msg::State)),
        Column::text("actions", tr(Msg::Actions)),
    ]
}

/// The Members tab: every member with its owner and state.
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let members = &loaded.resources.members;
    let rows = if members.is_empty() {
        view! {
            <tr>
                <td colspan="7" class="fui-muted">
                    {tr(Msg::NoMemberConfiguredYet)}
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
            ("members-title", tr(Msg::Members)),
            (tr(Msg::MemberRegistryDocumentCareServicesDirectory), "console-managed-resources"),
            view! {
                <a href="/configuration/members/new" class=Variant::Primary.class()>
                    {icon(Glyph::PLUS, "16")}
                    {tr(Msg::AddMember)}
                </a>
            },
        )}
        {crate::views::table(&tr(Msg::MembersAndTheirOwners), &member_columns(), rows)}
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
            reason: tr_with(
                Msg::NoMemberHasNodeId,
                &[("node", (node).to_string().into())],
            ),
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
        (Owner::File, _) => tr(Msg::ConfigurationFileSRegistryDocumentProvides),
        (Owner::Directory, _) => tr(Msg::CareServicesDirectoryProvidesMemberChange),
        (_, Some(ConsoleState::Removing)) => tr(Msg::NewestRevisionRemovesMemberApplyFrom),
        (_, Some(ConsoleState::Shadowed)) => tr(Msg::RegistryDocumentProvidesMemberSameIdentity),
        _ => tr(Msg::ConsoleDoesNotChangeMember),
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
        {trail((tr(Msg::Members), "/configuration/members"), member.node.as_str().to_owned())}
        <h2 id="member-title" class="fui-card-title">
            <code>{member.node.as_str().to_owned()}</code>
        </h2>
        <p class="fui-muted" role="note">
            {source}
        </p>
        {definitions(
            vec![
                (tr(Msg::Organisation), member.organisation.as_str().to_owned().into_any()),
                (
                    tr(Msg::SystemId),
                    view! { <code>{member.system_id.as_str().to_owned()}</code> }.into_any(),
                ),
                (tr(Msg::Owner), owner(member.owner)),
                (tr(Msg::State), state(member.state)),
                (tr(Msg::Endpoints), view! { <ul class="fui-bullets">{endpoints}</ul> }.into_any()),
            ],
        )}
    }
    .into_any()
}

/// The classes of one group of a form: a legend over its fields.
pub(super) const GROUP: &str = "fui-fieldset";

/// The classes of one repeatable row: a sunken box of its own fields.
const ROW: &str = "fui-fieldset fui-well";

/// The classes of a row's legend.
const ROW_TITLE: &str = "fui-label";

/// The "In service" switch of the endpoint field `id`, named `name`.
fn in_service(id: String, name: String, on: bool) -> AnyView {
    let label_for = id.clone();
    view! {
        <div class="fui-check">
            <input
                type="checkbox"
                role="switch"
                class=SWITCH
                id=id
                name=name
                value="on"
                checked=on
            />
            <label for=label_for>{tr(Msg::InService)}</label>
        </div>
    }
    .into_any()
}

/// A Query API endpoint row `key`, holding `endpoint` when the member has
/// it.
fn query_row(key: &str, endpoint: Option<&ConsoleEndpoint>) -> AnyView {
    let row = format!("cfg-q-{key}");
    let named = endpoint.map_or_else(
        || tr(Msg::NewQueryApiEndpoint),
        |endpoint| {
            tr_with(
                Msg::QueryApiEndpoint,
                &[("endpoint", (endpoint.id.as_str()).to_string().into())],
            )
        },
    );
    let name = |part: &str| format!("form[query][{key}][{part}]");
    view! {
        <fieldset id=row.clone() class=ROW>
            <legend class=ROW_TITLE>{named.clone()}</legend>
            <input type="hidden" name="form[query_rows]" value=key.to_owned() />
            <div class="fui-field-row fui-field-row-narrow-wide">
                {text(
                    (&query_field(key, "id"), &name("id")),
                    tr(Msg::EndpointId),
                    endpoint.map(|endpoint| endpoint.id.as_str().to_owned()).unwrap_or_default(),
                    Text {
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
                {text(
                    (&query_field(key, "url"), &name("url")),
                    tr(Msg::BaseUrl),
                    endpoint.map(|endpoint| endpoint.url.clone()).unwrap_or_default(),
                    Text {
                        kind: "url",
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
            </div>
            <div class="fui-split">
                {in_service(
                    query_field(key, "service"),
                    name("in_service"),
                    endpoint.is_none_or(|endpoint| endpoint.status != EndpointStatus::Suspended),
                )}
                {remove_button(&row, tr_with(Msg::Remove3, &[("named", (named).clone().into())]))}
            </div>
        </fieldset>
    }
    .into_any()
}

// NOTE: Regulation (EU) 2025/327 Art 14(1), points (a) to (f): the priority
// categories by the codes the exchange format names them.
/// The categories the console offers for a FHIR endpoint, each its code and
/// its name.
const CATEGORIES: [(&str, Msg); 6] = [
    ("Patient-Summaries", Msg::PatientSummaries2),
    ("Electronic-Prescriptions", Msg::ElectronicPrescriptions2),
    ("Electronic-Dispensations", Msg::ElectronicDispensations2),
    ("Medical-Imaging", Msg::MedicalImaging2),
    ("Laboratory-Reports", Msg::LaboratoryReports2),
    ("Discharge-Reports", Msg::DischargeReports2),
];

/// A profile item `item` of the FHIR endpoint row `key`, holding `value`.
fn profile(key: &str, item: &str, value: String) -> AnyView {
    let id = format!("cfg-f-{key}-p-{item}");
    let body = view! {
        {Field::new(id.clone(), tr(Msg::Profile))
            .label_hidden()
            .view(
                view! {
                    <input
                        type="url"
                        id=id.clone()
                        name=format!("form[fhir][{key}][profiles]")
                        value=value
                        class=format!("{INPUT} fui-mono")
                        autocomplete="off"
                        spellcheck="false"
                    />
                },
            )}
        {remove_button(&format!("{id}-row"), tr(Msg::RemoveThisProfile))}
    };
    keyed_field_row(format!("{id}-row"), FieldRow::WithActions, body)
}

/// The categories of the FHIR endpoint row `key`, those it serves
/// `served` ticked; a category the console does not know is offered too.
fn categories(key: &str, served: &[String]) -> AnyView {
    let group = fhir_field(key, "categories");
    let mut offered: Vec<(String, String)> = CATEGORIES
        .iter()
        .map(|(code, name)| ((*code).to_owned(), tr(*name)))
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
                <label for=label_for class="fui-chip-check">
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
            class="fui-fieldset fui-choice"
            aria-describedby=format!("{group}-error")
        >
            <legend class="fui-label">{tr(Msg::Categories)}</legend>
            <div class="fui-chip-checks">{boxes}</div>
            {error_line(&group)}
        </fieldset>
    }
    .into_any()
}

/// A FHIR endpoint row `key`, holding `endpoint` when the member has it.
fn fhir_row(key: &str, endpoint: Option<&ConsoleEndpoint>) -> AnyView {
    let row = format!("cfg-f-{key}");
    let named = endpoint.map_or_else(
        || tr(Msg::NewFhirEndpoint),
        |endpoint| {
            tr_with(
                Msg::FhirEndpoint,
                &[("endpoint", (endpoint.id.as_str()).to_string().into())],
            )
        },
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
            <div class="fui-field-row fui-field-row-narrow-wide">
                {text(
                    (&fhir_field(key, "id"), &name("id")),
                    tr(Msg::EndpointId),
                    endpoint.map(|endpoint| endpoint.id.as_str().to_owned()).unwrap_or_default(),
                    Text {
                        required: true,
                        mono: true,
                        ..Text::default()
                    },
                )}
                {text(
                    (&fhir_field(key, "url"), &name("url")),
                    tr(Msg::BaseUrl),
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
                tr(Msg::IdentifierSystemEhrId),
                endpoint.and_then(|endpoint| endpoint.ehr_id_system.clone()).unwrap_or_default(),
                Text {
                    required: true,
                    mono: true,
                    hint: Some(tr(Msg::SystemEndpointToldEhrIdSuch)),
                    ..Text::default()
                },
            )}
            {categories(key, &served)}
            <fieldset class="fui-fieldset fui-choice">
                <legend class="fui-label">{tr(Msg::Profiles)}</legend>
                <div id=list.clone() class="fui-stack">
                    {profiles}
                </div>
                {add_button(
                    &fhir_field(key, "add-profile"),
                    (&profile_template, &list),
                    (tr(Msg::AddProfile), "__ITEM__"),
                )}
                {template(profile_template.clone(), profile(key, "__ITEM__", String::new()))}
            </fieldset>
            <div class="fui-split">
                {in_service(
                    fhir_field(key, "service"),
                    name("in_service"),
                    endpoint.is_none_or(|endpoint| endpoint.status != EndpointStatus::Suspended),
                )}
                {remove_button(&row, tr_with(Msg::Remove4, &[("named", (named).clone().into())]))}
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
            <legend>{tr(Msg::Organisation)}</legend>
            <div class="fui-field-row fui-field-row-halves">
                {text(
                    (&field("organisation"), "form[organisation]"),
                    tr(Msg::OrganisationId),
                    value(|m| m.organisation.id.as_str().to_owned()),
                    required(None),
                )}
                {text(
                    (&field("organisation_name"), "form[organisation_name]"),
                    tr(Msg::OrganisationName),
                    value(|m| m.organisation.name.clone().unwrap_or_default()),
                    Text::default(),
                )}
            </div>
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::Node)}</legend>
            <div class="fui-field-row fui-field-row-halves">
                {text(
                    (&field("node"), "form[node]"),
                    tr(Msg::NodeIdLabel),
                    value(|m| m.node.id.as_str().to_owned()),
                    Text {
                        readonly: editing,
                        ..required(
                            Some(
                                if editing {
                                    tr(Msg::MemberSNodeIdIdentityAdd)
                                } else {
                                    tr_with(Msg::ConfigurationKey, &[("key", "node_id".into())])
                                },
                            ),
                        )
                    },
                )}
                {text(
                    (&field("system_id"), "form[system_id]"),
                    tr(Msg::SystemId),
                    value(|m| m.node.system_id.as_str().to_owned()),
                    required(Some(tr_with(Msg::ConfigurationKey, &[("key", "system_id".into())]))),
                )}
                {text(
                    (&field("product"), "form[product]"),
                    tr(Msg::CdrProduct),
                    value(|m| m.node.product.clone().unwrap_or_default()),
                    Text::default(),
                )}
                {text(
                    (&field("product_version"), "form[product_version]"),
                    tr(Msg::CdrProductVersion),
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
            <legend>{tr(Msg::QueryApiEndpoints)}</legend>
            <p class="fui-muted">{tr(Msg::RestEndpointsGatewaySendsEachMember)}</p>
            <div id="cfg-query-rows" class="fui-stack">
                {query_rows}
                {fresh_query}
            </div>
            {add_button(
                ADD_QUERY,
                ("tpl-query", "cfg-query-rows"),
                (tr(Msg::AddEndpoint), "__KEY__"),
            )}
            {template(String::from("tpl-query"), query_row("__KEY__", None))}
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::FhirEndpoints)}</legend>
            <p class="fui-muted">{tr(Msg::ExchangeFormatEndpointsServeCategoriesEach)}</p>
            <div id="cfg-fhir-rows" class="fui-stack">
                {fhir_rows}
            </div>
            {add_button(
                ADD_FHIR,
                ("tpl-fhir", "cfg-fhir-rows"),
                (tr(Msg::AddFhirEndpoint), "__KEY__"),
            )}
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
        || tr(Msg::AddAMember),
        |member| {
            tr_with(
                Msg::EditMember,
                &[("member", (member.node.id.as_str()).to_string().into())],
            )
        },
    );
    let body = view! {
        {hidden("action", String::from(if editing { "save-member" } else { "add-member" }))}
        {hidden("base", loaded.base())}
        {hidden("fhir_shown", String::from("yes"))}
        {identity(member)}
        {endpoint_groups(member)}
        {reason()}
        {save_bar(loaded, "/configuration/members")}
    };
    view! {
        {trail((tr(Msg::Members), "/configuration/members"), title.clone())}
        {head(
            ("member-title", title),
            (tr(Msg::MemberSavedNewRevisionWhichYou), "console-managed-resources"),
            (),
        )}
        {form("main", "fui-stack", body)}
    }
    .into_any()
}
