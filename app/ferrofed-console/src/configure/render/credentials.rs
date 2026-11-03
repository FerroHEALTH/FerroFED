// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Credentials and bindings tab: each onward credential and binding
//! section with its owner, its state and its secrets by reference, and the
//! forms that add or edit one the console holds.
//!
//! A secret is shown as set, with the file it is read from, or not set;
//! its value is never on the page. Each secret's field is a choice of Keep,
//! Set a value, Read from a file or Clear, and its value field is a
//! password field the page never fills (§13.1, N25).

use ferrofed_operator_api::console::{ConsoleState, Owner};
use ferrofed_operator_api::resource::{BindingSection, Resource, SecretSlot, SectionResource};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::{ACTIONS, Column};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{Field as FormField, SELECT, described_by};

use crate::configure::plan::removal_dialog;
use crate::configure::render::Loaded;
use crate::configure::render::fields::{Shown, part};
use crate::configure::render::members::{GROUP, owner, state};
use crate::configure::render::parts::{
    Removal, confirmed, error_line, form, head, hidden, reason, removal_reason_field, row_actions,
    save_bar, trail,
};
use crate::configure::sections::choice;
use crate::configure::settings::fields::TLS;
use crate::configure::settings::{self, Grant, TLS_SECRET};
use crate::locale::{Msg, tr, tr_with};
use crate::pages::opt;
use crate::read::Refusal;

/// The path of `resource`'s form.
fn href(resource: &Resource) -> String {
    match resource {
        Resource::Credentials(endpoint) => format!("/configuration/credentials/{endpoint}"),
        Resource::Binding(section) => format!("/configuration/bindings/{}", section.as_str()),
        _ => String::from("/configuration/credentials"),
    }
}

/// The secrets `slots` name, each set and read from its file, or none.
fn secrets_said(slots: &[SecretSlot]) -> Html {
    if slots.is_empty() {
        return Html::raw(format!(
            "<span class=\"fui-muted\">{}</span>",
            Html::text(tr(Msg::NotSet2))
        ));
    }
    let items = Html::join(slots.iter().map(|slot| {
        Html::raw(format!(
            "<li><code>{}</code>{}<code>{}</code></li>",
            Html::text(&slot.key),
            Html::text(tr(Msg::SetReadFrom)),
            Html::text(&slot.reference)
        ))
    }));
    Html::raw(format!("<ul class=\"fui-divided\">{items}</ul>"))
}

/// The settings of `section` in a few words: its grant and how many keys
/// it sets.
fn settings_said(section: &SectionResource) -> String {
    let Ok(table) = settings::build::held(section.settings.as_deref()) else {
        return tr(Msg::NotReadable);
    };
    let count = match settings::build::scalars(&table).len() {
        0 => tr(Msg::NoSettings),
        1 => tr(Msg::N1Setting),
        many => tr_with(Msg::Settings2, &[("many", (many).to_string().into())]),
    };
    match &section.resource {
        Resource::Credentials(_) => {
            format!(
                "{} · {count}",
                tr(Grant::held(&table, &section.secrets).label())
            )
        }
        _ => count,
    }
}

/// The removal of `resource` from its own page, a confirmation step
/// asking for the reason.
pub(super) fn remove(loaded: &Loaded<'_>, resource: &Resource) -> Html {
    removal(
        loaded,
        resource,
        (String::from("remove-section"), tr(Msg::Remove)),
    )
}

/// The fields the removal of `resource` posts: what it removes and the
/// reason it asks for.
fn removal_fields(loaded: &Loaded<'_>, resource: &Resource) -> Html {
    let named = choice(resource);
    Html::join([
        hidden("action", String::from("remove-section")),
        hidden("base", loaded.base()),
        hidden("resource", named.clone()),
        removal_reason_field(&named),
    ])
}

/// The removal of `resource`, a confirmation step in the dialog `dialog`
/// asking for the reason, opened by a button named `label`.
fn removal(loaded: &Loaded<'_>, resource: &Resource, (dialog, label): (String, String)) -> Html {
    let shown = resource.to_string();
    form(
        "",
        confirmed(
            dialog,
            (Glyph::TRASH, label, Variant::Danger),
            (
                tr_with(
                    Msg::RemoveSavedWithoutWhichYouApply,
                    &[
                        ("shown", (shown).clone().into()),
                        ("loaded", (loaded.next()).clone().into()),
                    ],
                ),
                tr(Msg::SaveTheRemoval),
            ),
            removal_fields(loaded, resource),
        ),
    )
}

/// One credential's or binding section's row; one the console holds ends
/// in its row menu, with Edit, and Remove… with its dialog.
fn row(loaded: &Loaded<'_>, section: &SectionResource) -> Html {
    let shown = section.resource.to_string();
    let console = section.owner == Owner::Console;
    let editable = console
        && matches!(
            section.state,
            Some(ConsoleState::Running | ConsoleState::Pending)
        );
    let actions = editable.then(|| {
        let removes = Removal {
            title: tr_with(Msg::RemoveSectionTitle, &[("shown", shown.clone().into())]),
            message: tr_with(
                Msg::RemovalSavedWithoutIt,
                &[("loaded", loaded.next().clone().into())],
            ),
            fields: removal_fields(loaded, &section.resource),
        };
        row_actions(
            &removal_dialog(&choice(&section.resource)),
            &shown,
            href(&section.resource),
            Some(removes),
        )
    });
    Html::raw(format!(
        "<tr><th scope=\"row\"><code>{}</code></th>\
         <td data-label=\"{}\">{}</td><td data-label=\"{}\">{}</td>\
         <td data-label=\"{}\">{}</td><td data-label=\"{}\">{}</td>\
         <td data-label=\"{}\" class=\"{ACTIONS}\">{}</td></tr>",
        Html::text(&shown),
        Html::text(tr(Msg::Owner)),
        owner(section.owner),
        Html::text(tr(Msg::State)),
        state(if console { section.state } else { None }),
        Html::text(tr(Msg::Secrets)),
        secrets_said(&section.secrets),
        Html::text(tr(Msg::Settings)),
        Html::text(settings_said(section)),
        Html::text(tr(Msg::Actions)),
        opt(actions),
    ))
}

/// The binding sections none of the sections sets yet.
pub(super) fn addable(loaded: &Loaded<'_>) -> Vec<BindingSection> {
    loaded
        .resources
        .bindings
        .iter()
        .copied()
        .filter(|binding| {
            !loaded
                .resources
                .sections
                .iter()
                .any(|section| section.resource == Resource::Binding(*binding))
        })
        .collect()
}

/// The columns of the Credentials and bindings tab.
fn section_columns() -> [Column; 6] {
    [
        Column::text("resource", tr(Msg::Resource)),
        Column::text("owner", tr(Msg::Owner)),
        Column::text("state", tr(Msg::State)),
        Column::text("secrets", tr(Msg::Secrets)),
        Column::text("settings", tr(Msg::Settings)),
        Column::text("actions", tr(Msg::Actions)),
    ]
}

/// The Credentials and bindings tab.
pub(super) fn tab(loaded: &Loaded<'_>) -> Html {
    let sections = &loaded.resources.sections;
    let rows = if sections.is_empty() {
        Html::raw(format!(
            "<tr><td colspan=\"6\" class=\"fui-muted\">{}</td></tr>",
            Html::text(tr(Msg::NoOnwardCredentialBindingSectionSet))
        ))
    } else {
        Html::join(sections.iter().map(|section| row(loaded, section)))
    };
    let binding = (!addable(loaded).is_empty()).then(|| {
        Html::raw(format!(
            "<a href=\"/configuration/bindings/new\" class=\"{}\">{}</a>",
            Variant::Secondary.class(),
            Html::text(tr(Msg::AddBindingSection))
        ))
    });
    head(
        ("credentials-title", tr(Msg::CredentialsAndBindings)),
        (
            tr(Msg::CredentialBindingSectionFileProvidesRead),
            "credentials-and-binding-sections",
        ),
        opt(binding).then(Html::raw(format!(
            "<a href=\"/configuration/credentials/new\" class=\"{}\">{}{}</a>",
            Variant::Primary.class(),
            icon(Glyph::PLUS, "16"),
            Html::text(tr(Msg::AddCredentials))
        ))),
    )
    .then(crate::pages::table(
        &tr(Msg::CredentialsBindingSectionsTheirOwners),
        &section_columns(),
        rows,
    ))
}

/// The endpoints the members declare that hold no credentials yet.
fn uncredentialed(loaded: &Loaded<'_>) -> Vec<String> {
    let mut endpoints: Vec<String> = loaded
        .resources
        .members
        .iter()
        .flat_map(|member| member.endpoints.iter().map(|id| id.as_str().to_owned()))
        .filter(|endpoint| {
            !loaded
                .resources
                .sections
                .iter()
                .any(|section| section.resource == Resource::Credentials(endpoint.clone()))
        })
        .collect();
    endpoints.sort();
    endpoints.dedup();
    endpoints
}

/// The endpoint of the credentials: `endpoint` as it stands, or a choice
/// of those that hold none.
fn endpoint_choice(loaded: &Loaded<'_>, endpoint: Option<&str>, chosen: Option<&str>) -> Html {
    if let Some(endpoint) = endpoint {
        return hidden("endpoint", endpoint.to_owned()).then(Html::raw(format!(
            "<p>{} <code>{}</code></p>",
            Html::text(tr(Msg::Endpoint2)),
            Html::text(endpoint)
        )));
    }
    let options = Html::join(uncredentialed(loaded).into_iter().map(|endpoint| {
        let selected = chosen == Some(endpoint.as_str());
        Html::raw(format!(
            "<option value=\"{endpoint}\"{}>{endpoint}</option>",
            if selected { " selected" } else { "" },
            endpoint = Html::text(&endpoint),
        ))
    }));
    let select = Html::raw(format!(
        "<select id=\"cfg-endpoint\" class=\"{SELECT}\" name=\"form[endpoint]\" required \
         aria-describedby=\"{}\">{options}</select>",
        Html::text(described_by("cfg-endpoint", true, true).unwrap_or_default())
    ));
    FormField::new("cfg-endpoint", tr(Msg::Endpoint))
        .help(tr(Msg::EndpointsMembersDeclareHoldNoCredentials))
        .after(error_line("cfg-endpoint"))
        .view(select)
}

/// The select of the grant, `grant` chosen; the form shows the chosen
/// grant's fields alone.
fn grant_choice(grant: Grant) -> Html {
    let grants = Html::join(Grant::ALL.into_iter().map(|option| {
        Html::raw(format!(
            "<option value=\"{}\"{}>{}</option>",
            Html::text(option.as_str()),
            if option == grant { " selected" } else { "" },
            Html::text(tr(option.label()))
        ))
    }));
    let select = Html::raw(format!(
        "<select id=\"cfg-grant\" class=\"{SELECT}\" name=\"form[grant]\" data-grant=\"\" \
         aria-describedby=\"{}\">{grants}</select>",
        Html::text(described_by("cfg-grant", true, true).unwrap_or_default())
    ));
    FormField::new("cfg-grant", tr(Msg::GrantType))
        .help(tr(Msg::HowGatewayReachesEndpointFormShows))
        .after(error_line("cfg-grant"))
        .view(select)
}

/// The form that adds onward credentials, or edits those of `endpoint`.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for an endpoint whose credentials the
/// console does not hold, and settings that do not read.
pub(super) fn credential_form(
    loaded: &Loaded<'_>,
    endpoint: Option<&str>,
) -> Result<Html, Refusal> {
    let held = match endpoint {
        Some(endpoint) => Some(
            loaded
                .resources
                .sections
                .iter()
                .find(|section| {
                    section.owner == Owner::Console
                        && section.resource == Resource::Credentials(endpoint.to_owned())
                })
                .ok_or_else(|| Refusal::Invalid {
                    reason: tr_with(
                        Msg::ConsoleHoldsNoCredentials,
                        &[("endpoint", (endpoint).to_string().into())],
                    ),
                })?,
        ),
        None => None,
    };
    let table = settings::build::held(held.and_then(|held| held.settings.as_deref()))
        .map_err(|refused| refused.refusal)?;
    let slots = held.map(|held| held.secrets.as_slice()).unwrap_or_default();
    let posted = loaded
        .drafted(&["save-credentials"])
        .filter(|form| endpoint.is_none_or(|endpoint| form.endpoint == endpoint));
    let shown = Shown {
        held: &table,
        posted,
    };
    let grant = posted
        .and_then(|form| Grant::named(&form.grant))
        .unwrap_or_else(|| Grant::held(&table, slots));
    let mut rows = 0_u32;
    let parts = Grant::ALL
        .into_iter()
        .filter(|option| !option.fields().is_empty() || !option.secrets().is_empty())
        .map(|option| {
            let inner = part(option.fields(), option.secrets(), (shown, slots), &mut rows);
            // NOTE: no specification governs this: our own design; each grant's settings
            // fold in one exclusive disclosure, the grant held open, with no script.
            Html::raw(format!(
                "<details name=\"cfg-grant-parts\"{}><summary>{}</summary>{inner}</details>",
                if option == grant { " open" } else { "" },
                Html::text(tr(option.label()))
            ))
        })
        .collect::<Vec<_>>();
    let parts = Html::join(parts);
    let tls = part(TLS, &[TLS_SECRET], (shown, slots), &mut rows);
    let title = endpoint.map_or_else(
        || tr(Msg::AddOnwardCredentials),
        |endpoint| {
            tr_with(
                Msg::EditTheCredentialsOf,
                &[("endpoint", (endpoint).to_string().into())],
            )
        },
    );
    let body = Html::join([
        hidden("action", String::from("save-credentials")),
        hidden("base", loaded.base()),
        Html::raw(format!(
            "<fieldset class=\"{GROUP}\"><legend>{}</legend>{}{}</fieldset>\
             <fieldset class=\"{GROUP}\"><legend>{}</legend><div class=\"fui-stack\">{parts}</div></fieldset>\
             <fieldset class=\"{GROUP}\"><legend>{}</legend>{tls}</fieldset>",
            Html::text(tr(Msg::EndpointAndGrant)),
            endpoint_choice(loaded, endpoint, posted.map(|form| form.endpoint.as_str())),
            grant_choice(grant),
            Html::text(tr(Msg::GrantSettings)),
            Html::text(tr(Msg::Tls)),
        )),
        reason(),
        save_bar(loaded, "/configuration/credentials"),
    ]);
    Ok(Html::join([
        back(title.clone()),
        head(
            ("credential-title", title),
            (
                tr(Msg::SavedRevisionOnceAppliedAwaitsConfirmation),
                "commit-confirm",
            ),
            Html::empty(),
        ),
        form("fui-stack", body),
        opt(endpoint.map(|endpoint| {
            Html::raw(format!(
                "<div class=\"fui-actions\">{}</div>",
                remove(loaded, &Resource::Credentials(endpoint.to_owned()))
            ))
        })),
    ]))
}

/// The trail from the tab to the form titled `current`.
pub(super) fn back(current: String) -> Html {
    trail(
        (
            tr(Msg::CredentialsAndBindings),
            "/configuration/credentials",
        ),
        current,
    )
}
