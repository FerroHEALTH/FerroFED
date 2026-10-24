// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Members tab: the members with their owners and states, the detail of
//! a read-only member, and the form that adds or edits a console member,
//! each endpoint a row of its own fields.

use ferrofed_registry::console::{ConsoleMember, ConsoleState, MemberResource, Owner};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::{ACTIONS, Column, NUM};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{Field, INPUT, SWITCH};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::definitions;
use ferrohealth_ui::widget::repeat::{Fields, ROW, Repeat, RepeatRow, repeat};
use leptos::prelude::*;

use crate::configure::draft::{self, Edit};
use crate::configure::member::{ADD_FHIR, ADD_QUERY, fhir_field, field, query_field};
use crate::configure::model::{ConfigForm, FhirEndpointInput, QueryEndpointInput};
use crate::configure::plan::removal_dialog;
use crate::configure::render::Loaded;
use crate::configure::render::parts::{
    Removal, Text, confirmed, error_line, form, head, hidden, reason, removal_reason_field,
    row_actions, row_add, row_remove, save_bar, text, trail,
};
use crate::locale::{Msg, tr, tr_with};
use crate::read::Refusal;

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
        Some(ConsoleState::Pending) => (tr(Msg::StatePendingWord), Tone::Warn),
        Some(ConsoleState::Removing) => (tr(Msg::StateRemovingWord), Tone::Warn),
        Some(ConsoleState::Shadowed) => (tr(Msg::ShadowedByTheFile), Tone::Neutral),
        Some(_) => (tr(Msg::StateUnknown), Tone::Neutral),
    };
    pill(tone, &word).into_any()
}

/// The fields the removal of the console member `node` posts: what it
/// removes and the reason it asks for.
fn removal_fields(loaded: &Loaded<'_>, node: &str) -> AnyView {
    view! {
        {hidden("action", String::from("remove-member"))}
        {hidden("base", loaded.base())}
        {hidden("node", node.to_owned())}
        {removal_reason_field(node)}
    }
    .into_any()
}

/// The removal of the console member `node` from its own page, a
/// confirmation step in the dialog `dialog` asking for the reason, opened
/// by a button named `label`.
fn removal(loaded: &Loaded<'_>, node: &str, (dialog, label): (String, String)) -> AnyView {
    form(
        "",
        confirmed(
            dialog,
            (Glyph::TRASH, label, Variant::Danger),
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
            removal_fields(loaded, node),
        ),
    )
    .into_any()
}

/// What the row menu's Remove… of the console member `node` asks and posts.
fn row_removal(loaded: &Loaded<'_>, node: &str) -> Removal {
    Removal {
        title: tr_with(Msg::RemoveMemberTitle, &[("node", node.to_owned().into())]),
        message: tr_with(
            Msg::RemovalSavedWithoutIt,
            &[("loaded", loaded.next().clone().into())],
        ),
        fields: removal_fields(loaded, node),
    }
}

/// One member's row; a console member's row ends in its row menu, with
/// Remove… and its dialog when the console holds it.
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
        let removes = member.member.is_some().then(|| row_removal(loaded, &node));
        row_actions(&removal_dialog(&node), &node, href(&node), removes)
    } else {
        let view = view! {
            <a href=href(&node) class=Variant::Ghost.class()>
                {tr(Msg::View)}
                <span class="fui-sr-only">{format!(" {node}")}</span>
            </a>
        };
        view.into_any()
    };
    let row = view! {
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
    };
    row.into_any()
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
        {crate::pages::table(&tr(Msg::MembersAndTheirOwners), &member_columns(), rows)}
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
        (Some(held), true) => Ok(view! {
            {member_form(loaded, Some(held))}
            <div class="fui-actions">
                {removal(loaded, node, (String::from("remove-member"), tr(Msg::Remove)))}
            </div>
        }
        .into_any()),
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

/// The heading of the Query API endpoint row `row`.
fn query_named(row: &QueryEndpointInput) -> String {
    if row.id.trim().is_empty() {
        tr(Msg::NewQueryApiEndpoint)
    } else {
        tr_with(
            Msg::QueryApiEndpoint,
            &[("endpoint", row.id.clone().into())],
        )
    }
}

/// A Query API endpoint row (F7 sub-card), holding what `row` holds.
fn query_row(row: &QueryEndpointInput) -> RepeatRow {
    let key = row.key.as_str();
    let named = query_named(row);
    let name = |part: &str| format!("form[query][{key}][{part}]");
    let fields = view! {
        <input type="hidden" name="form[query_rows]" value=key.to_owned() />
        <div class="fui-field-row fui-field-row-narrow-wide">
            {text(
                (&query_field(key, "id"), &name("id")),
                tr(Msg::EndpointId),
                row.id.clone(),
                Text {
                    required: true,
                    mono: true,
                    ..Text::default()
                },
            )}
            {text(
                (&query_field(key, "url"), &name("url")),
                tr(Msg::BaseUrl),
                row.url.clone(),
                Text {
                    kind: "url",
                    required: true,
                    mono: true,
                    ..Text::default()
                },
            )}
        </div>
        {in_service(query_field(key, "service"), name("in_service"), row.in_service)}
    }
    .into_any();
    RepeatRow {
        key: key.to_owned(),
        heading: Some(named.clone()),
        fields,
        end: row_remove(
            &Edit::RemoveQuery(key.to_owned()),
            tr_with(Msg::Remove3, &[("named", named.into())]),
        ),
        after: None,
    }
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

/// The key token of a profile's blank row, its own beside [`ROW`], since
/// the profiles are a list nested in an endpoint's card (F7).
const PROFILE: &str = "__profile__";

/// The profile at `at` of the FHIR endpoint row `key`, holding `value`,
/// a row of one field (F7).
fn profile(key: &str, at: &str, value: String, removes: &Edit) -> RepeatRow {
    let id = format!("cfg-f-{key}-p-{at}");
    let fields = Field::new(id.clone(), tr(Msg::Profile))
        .label_hidden()
        .view(view! {
            <input
                type="url"
                id=id.clone()
                name=format!("form[fhir][{key}][profiles]")
                value=value
                class=format!("{INPUT} fui-mono")
                autocomplete="off"
                spellcheck="false"
            />
        });
    RepeatRow {
        key: at.to_owned(),
        heading: None,
        fields,
        end: row_remove(removes, tr(Msg::RemoveThisProfile)),
        after: None,
    }
}

/// The profiles of the FHIR endpoint row `key`, a row of one field each,
/// and the line where a refusal of the list shows. The list is nested in
/// the endpoint's card, so its blank row is keyed by [`PROFILE`].
fn profiles(key: &str, held: &[String]) -> AnyView {
    let rows = held
        .iter()
        .enumerate()
        .map(|(at, value)| {
            profile(
                key,
                &at.to_string(),
                value.clone(),
                &Edit::RemoveProfile(key.to_owned(), at),
            )
        })
        .collect();
    let blank = profile(
        key,
        PROFILE,
        String::new(),
        &Edit::RemoveProfile(key.to_owned(), held.len()),
    );
    let list_id = format!("cfg-f-{key}-profiles");
    let refused = fhir_field(key, "add-profile");
    view! {
        {repeat(Repeat {
            id: list_id,
            label: tr(Msg::Profiles),
            fields: Fields::One,
            head: vec![tr(Msg::Profile)],
            rows,
            token: PROFILE,
            blank: Some(blank),
            add: Some(row_add(&Edit::AddProfile(key.to_owned()), tr(Msg::AddProfile))),
            empty: tr(Msg::NoProfileYet),
            form: None,
        })}
        {error_line(&refused)}
    }
    .into_any()
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

/// The heading of the FHIR endpoint row `row`.
fn fhir_named(row: &FhirEndpointInput) -> String {
    if row.id.trim().is_empty() {
        tr(Msg::NewFhirEndpoint)
    } else {
        tr_with(Msg::FhirEndpoint, &[("endpoint", row.id.clone().into())])
    }
}

/// A FHIR endpoint row (F7 sub-card), holding what `row` holds, its
/// profiles a list of its own.
fn fhir_row(row: &FhirEndpointInput) -> RepeatRow {
    let key = row.key.as_str();
    let named = fhir_named(row);
    let name = |part: &str| format!("form[fhir][{key}][{part}]");
    let fields = view! {
        <input type="hidden" name="form[fhir_rows]" value=key.to_owned() />
        <div class="fui-field-row fui-field-row-narrow-wide">
            {text(
                (&fhir_field(key, "id"), &name("id")),
                tr(Msg::EndpointId),
                row.id.clone(),
                Text {
                    required: true,
                    mono: true,
                    ..Text::default()
                },
            )}
            {text(
                (&fhir_field(key, "url"), &name("url")),
                tr(Msg::BaseUrl),
                row.url.clone(),
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
            row.system.clone(),
            Text {
                required: true,
                mono: true,
                hint: Some(tr(Msg::SystemEndpointToldEhrIdSuch)),
                ..Text::default()
            },
        )}
        {categories(key, &row.categories)}
        {profiles(key, &row.profiles)}
        {in_service(fhir_field(key, "service"), name("in_service"), row.in_service)}
    }
    .into_any();
    RepeatRow {
        key: key.to_owned(),
        heading: Some(named.clone()),
        fields,
        end: row_remove(
            &Edit::RemoveFhir(key.to_owned()),
            tr_with(Msg::Remove4, &[("named", named.into())]),
        ),
        after: None,
    }
}

/// The organisation and the node of the member form, filled in from
/// `form`; an edit's `node_id` stays as it is.
fn identity(form: &ConfigForm) -> AnyView {
    let editing = form.action == "save-member";
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
                    form.organisation.clone(),
                    required(None),
                )}
                {text(
                    (&field("organisation_name"), "form[organisation_name]"),
                    tr(Msg::OrganisationName),
                    form.organisation_name.clone(),
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
                    form.node.clone(),
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
                    form.system_id.clone(),
                    required(Some(tr_with(Msg::ConfigurationKey, &[("key", "system_id".into())]))),
                )}
                {text(
                    (&field("product"), "form[product]"),
                    tr(Msg::CdrProduct),
                    form.product.clone(),
                    Text::default(),
                )}
                {text(
                    (&field("product_version"), "form[product_version]"),
                    tr(Msg::CdrProductVersion),
                    form.product_version.clone(),
                    Text::default(),
                )}
            </div>
        </fieldset>
    }
    .into_any()
}

/// The Query API and FHIR endpoints of the member form, a sub-card each
/// (F7), and the lines where a refusal of either list shows.
fn endpoint_groups(form: &ConfigForm) -> AnyView {
    let query_rows = form.query_endpoints.iter().map(query_row).collect();
    let fhir_rows = form.fhir_endpoints.iter().map(fhir_row).collect();
    let query_blank = query_row(&QueryEndpointInput {
        key: String::from(ROW),
        in_service: true,
        ..QueryEndpointInput::default()
    });
    let fhir_blank = fhir_row(&FhirEndpointInput {
        key: String::from(ROW),
        in_service: true,
        ..FhirEndpointInput::default()
    });
    view! {
        <fieldset class=GROUP>
            <legend>{tr(Msg::QueryApiEndpoints)}</legend>
            <p class="fui-muted">{tr(Msg::RestEndpointsGatewaySendsEachMember)}</p>
            {repeat(Repeat {
                id: String::from("cfg-query-rows"),
                label: tr(Msg::QueryApiEndpoints),
                fields: Fields::Card,
                head: Vec::new(),
                rows: query_rows,
                token: ROW,
                blank: Some(query_blank),
                add: Some(row_add(&Edit::AddQuery, tr(Msg::AddEndpoint))),
                empty: tr(Msg::NoQueryEndpointYet),
                form: None,
            })}
            {error_line(ADD_QUERY)}
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::FhirEndpoints)}</legend>
            <p class="fui-muted">{tr(Msg::ExchangeFormatEndpointsServeCategoriesEach)}</p>
            {repeat(Repeat {
                id: String::from("cfg-fhir-rows"),
                label: tr(Msg::FhirEndpoints),
                fields: Fields::Card,
                head: Vec::new(),
                rows: fhir_rows,
                token: ROW,
                blank: Some(fhir_blank),
                add: Some(row_add(&Edit::AddFhir, tr(Msg::AddFhirEndpoint))),
                empty: tr(Msg::NoFhirEndpointYet),
                form: None,
            })}
            {error_line(ADD_FHIR)}
        </fieldset>
    }
    .into_any()
}

/// The form that adds a member, or edits the console member `member`, in
/// groups: the organisation, the node, its Query API and FHIR endpoints,
/// and the reason. A form the operator posted is drawn as they posted it.
pub(super) fn member_form(loaded: &Loaded<'_>, member: Option<&ConsoleMember>) -> AnyView {
    let action = if member.is_some() {
        "save-member"
    } else {
        "add-member"
    };
    let drafted = loaded
        .drafted(&[action])
        .filter(|form| member.is_none_or(|member| form.node == member.node.id.as_str()))
        .cloned();
    let form_shown = drafted.unwrap_or_else(|| draft::member_form(member, loaded.base()));
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
        {hidden("action", String::from(action))}
        {hidden("base", form_shown.base.clone())}
        {hidden("fhir_shown", String::from("yes"))}
        {identity(&form_shown)}
        {endpoint_groups(&form_shown)}
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
        {form("fui-stack", body)}
    }
    .into_any()
}
