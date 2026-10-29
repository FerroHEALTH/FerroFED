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

use ferrofed_registry::console::{ConsoleState, Owner};
use ferrofed_registry::resource::{BindingSection, Resource, SecretSlot, SectionResource};
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::{ACTIONS, Column};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{
    Field as FormField, INPUT, Inner, SELECT, SWITCH, described_by, error_id, masked_input,
};
use ferrohealth_ui::widget::masked::HoldWords;
use ferrohealth_ui::widget::repeat::{Fields, ROW, Repeat, RepeatRow, repeat};
use leptos::prelude::*;

use crate::configure::draft::Edit;
use crate::configure::model::{ConfigForm, SettingInput};
use crate::configure::plan::removal_dialog;
use crate::configure::render::Loaded;
use crate::configure::render::members::{GROUP, owner, state};
use crate::configure::render::parts::{
    Removal, Text, confirmed, error_line, form, head, hidden, reason, removal_reason_field,
    row_actions, row_add, row_remove, save_bar, text, trail,
};
use crate::configure::sections::choice;
use crate::configure::settings::build::{secret_field, setting_field};
use crate::configure::settings::fields::TLS;
use crate::configure::settings::{self, Field, Grant, Kind, Secret, TLS_SECRET};
use crate::locale::{Msg, tr, tr_with};
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
fn secrets_said(slots: &[SecretSlot]) -> AnyView {
    if slots.is_empty() {
        return view! { <span class="fui-muted">{tr(Msg::NotSet2)}</span> }.into_any();
    }
    let items = slots
        .iter()
        .map(|slot| {
            view! {
                <li>
                    <code>{slot.key.clone()}</code>
                    {tr(Msg::SetReadFrom)}
                    <code>{slot.reference.clone()}</code>
                </li>
            }
        })
        .collect_view();
    view! { <ul class="fui-divided">{items}</ul> }.into_any()
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
pub(super) fn remove(loaded: &Loaded<'_>, resource: &Resource) -> AnyView {
    removal(
        loaded,
        resource,
        (String::from("remove-section"), tr(Msg::Remove)),
    )
}

/// The fields the removal of `resource` posts: what it removes and the
/// reason it asks for.
fn removal_fields(loaded: &Loaded<'_>, resource: &Resource) -> AnyView {
    let named = choice(resource);
    view! {
        {hidden("action", String::from("remove-section"))}
        {hidden("base", loaded.base())}
        {hidden("resource", named.clone())}
        {removal_reason_field(&named)}
    }
    .into_any()
}

/// The removal of `resource`, a confirmation step in the dialog `dialog`
/// asking for the reason, opened by a button named `label`.
fn removal(loaded: &Loaded<'_>, resource: &Resource, (dialog, label): (String, String)) -> AnyView {
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
    .into_any()
}

/// One credential's or binding section's row; one the console holds ends
/// in its row menu, with Edit, and Remove… with its dialog.
fn row(loaded: &Loaded<'_>, section: &SectionResource) -> AnyView {
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
    let row = view! {
        <tr>
            <th scope="row">
                <code>{shown.clone()}</code>
            </th>
            <td data-label=tr(Msg::Owner)>{owner(section.owner)}</td>
            <td data-label=tr(Msg::State)>{state(if console { section.state } else { None })}</td>
            <td data-label=tr(Msg::Secrets)>{secrets_said(&section.secrets)}</td>
            <td data-label=tr(Msg::Settings)>{settings_said(section)}</td>
            <td data-label=tr(Msg::Actions) class=ACTIONS>
                {actions}
            </td>
        </tr>
    };
    row.into_any()
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
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let sections = &loaded.resources.sections;
    let rows = if sections.is_empty() {
        view! {
            <tr>
                <td colspan="6" class="fui-muted">
                    {tr(Msg::NoOnwardCredentialBindingSectionSet)}
                </td>
            </tr>
        }
        .into_any()
    } else {
        sections
            .iter()
            .map(|section| row(loaded, section))
            .collect_view()
            .into_any()
    };
    let binding = (!addable(loaded).is_empty()).then(|| {
        view! {
            <a href="/configuration/bindings/new" class=Variant::Secondary.class()>
                {tr(Msg::AddBindingSection)}
            </a>
        }
    });
    view! {
        {head(
            ("credentials-title", tr(Msg::CredentialsAndBindings)),
            (tr(Msg::CredentialBindingSectionFileProvidesRead), "credentials-and-binding-sections"),
            view! {
                {binding}
                <a href="/configuration/credentials/new" class=Variant::Primary.class()>
                    {icon(Glyph::PLUS, "16")}
                    {tr(Msg::AddCredentials)}
                </a>
            },
        )}
        {crate::pages::table(
            &tr(Msg::CredentialsBindingSectionsTheirOwners),
            &section_columns(),
            rows,
        )}
    }
    .into_any()
}

/// The choice of the secret `secret` in row `row`, its slot `slot` when it
/// is set: Keep, Set a value, Read from a file or, when set, Clear.
///
/// The value field shows while Set a value is chosen and the file field
/// while Read from a file is, which the stylesheet decides by the place of
/// the radio chosen, so it works before the page hydrates.
pub(super) fn secret_row(
    row: &str,
    secret: (&str, String),
    slot: Option<&SecretSlot>,
    chosen: Option<&str>,
) -> AnyView {
    let (key, label) = secret;
    let chosen = chosen.filter(|choice| !choice.is_empty()).unwrap_or("keep");
    let name = |part: &str| format!("form[secret.{row}.{part}]");
    let status = slot.map_or_else(
        || tr(Msg::NotSet),
        |slot| {
            tr_with(
                Msg::SetReadFrom2,
                &[("reference", (slot.reference).clone().into())],
            )
        },
    );
    let mut choices = vec![
        (
            "keep",
            if slot.is_some() {
                tr(Msg::Keep)
            } else {
                tr(Msg::LeaveUnset)
            },
        ),
        ("set", tr(Msg::SetAValue)),
        ("file", tr(Msg::ReadFromAFile)),
    ];
    if slot.is_some() {
        choices.push(("clear", tr(Msg::Clear)));
    }
    let segments = choices
        .into_iter()
        .map(|(value, word)| {
            let id = secret_field(row, value);
            let label_for = id.clone();
            view! {
                <label for=label_for class="fui-segment">
                    <input
                        type="radio"
                        id=id
                        class="fui-sr-only"
                        name=name("choice")
                        value=value
                        checked=value == chosen
                    />
                    {word}
                </label>
            }
        })
        .collect_view();
    let group = secret_field(row, "choice");
    let value_id = secret_field(row, "value");
    let set = masked(&value_id, &name("value"));
    let file = text(
        (&secret_field(row, "file"), &name("file")),
        tr(Msg::AbsolutePathFileReadFrom),
        String::new(),
        Text {
            mono: true,
            ..Text::default()
        },
    );
    view! {
        <div class="fui-reveal">
            <fieldset class="fui-fieldset fui-choice" id=group.clone()>
                <legend class="fui-label">{label}</legend>
                <input type="hidden" name="form[secret_rows][]" value=row.to_owned() />
                <input type="hidden" name=name("key") value=key.to_owned() />
                <p class="fui-help">
                    <code>{key.to_owned()}</code>
                    " · "
                    {status}
                </p>
                <div class="fui-segmented">{segments}</div>
                {error_line(&group)}
            </fieldset>
            <div class="fui-reveal-parts">
                <div class="fui-revealed"></div>
                <div class="fui-revealed">{set}</div>
                <div class="fui-revealed">{file}</div>
            </div>
        </div>
    }
    .into_any()
}

/// The secret value field `id` named `name` (R10): masked, never filled,
/// with the eye that shows what was typed while it is held.
pub(super) fn masked(id: &str, name: &str) -> AnyView {
    FormField::new(id.to_owned(), tr(Msg::ValueNeverShownAgain))
        .trailing(Inner::HoldToShow(
            HoldWords::of_page().named(tr(Msg::ShowSecretWhileHeld)),
        ))
        .after(error_line(id))
        .view(masked_input(id, name, Some(error_id(id))))
}

/// The value at `at` of the list setting in row `row`, holding `value`, a
/// row of one field (F7).
fn list_value(row: &str, at: &str, value: String, removes: &Edit) -> RepeatRow {
    let id = format!("{}-{at}", setting_field(row, "list"));
    let fields = FormField::new(id.clone(), tr(Msg::Value))
        .label_hidden()
        .view(view! {
            <input
                type="text"
                id=id.clone()
                name=format!("form[setting.{row}.values][]")
                value=value
                class=format!("{INPUT} fui-mono")
                autocomplete="off"
            />
        });
    RepeatRow {
        key: at.to_owned(),
        heading: None,
        fields,
        end: row_remove(removes, tr(Msg::RemoveThisValue)),
        after: None,
    }
}

/// The values of the list setting in row `row`, `values` filled in, a row
/// of one field each (F7).
fn list(row: &str, values: Vec<String>) -> AnyView {
    let count = values.len();
    let rows = values
        .into_iter()
        .enumerate()
        .map(|(at, value)| {
            list_value(
                row,
                &at.to_string(),
                value,
                &Edit::RemoveValue(row.to_owned(), at),
            )
        })
        .collect();
    let blank = list_value(
        row,
        ROW,
        String::new(),
        &Edit::RemoveValue(row.to_owned(), count),
    );
    view! {
        {repeat(Repeat {
            id: setting_field(row, "list"),
            label: tr(Msg::Value),
            fields: Fields::One,
            head: vec![tr(Msg::Value)],
            rows,
            token: ROW,
            blank: Some(blank),
            add: Some(row_add(&Edit::AddValue(row.to_owned()), tr(Msg::AddValue))),
            empty: tr(Msg::NoValueYet),
            form: None,
        })}
    }
    .into_any()
}

/// What a credential form shows: what the gateway holds, or the form the
/// operator posted.
#[derive(Clone, Copy)]
struct Shown<'a> {
    /// The settings the gateway holds.
    held: &'a toml::Table,
    /// The form as posted, drawn in their place.
    posted: Option<&'a ConfigForm>,
}

impl Shown<'_> {
    /// The posted setting of `key`.
    fn posted(&self, key: &str) -> Option<&SettingInput> {
        self.posted?
            .settings
            .iter()
            .find(|setting| setting.key == key)
    }

    /// The value of the setting `key`.
    fn value(&self, key: &str) -> String {
        self.posted.map_or_else(
            || settings::build::shown(self.held, key),
            |_| {
                self.posted(key)
                    .map(|setting| setting.value.clone())
                    .unwrap_or_default()
            },
        )
    }

    /// The values of the list setting `key`.
    fn values(&self, key: &str) -> Vec<String> {
        self.posted.map_or_else(
            || settings::build::listed(self.held, key),
            |_| {
                self.posted(key)
                    .map(|setting| setting.values.clone())
                    .unwrap_or_default()
            },
        )
    }

    /// The row the posted form names the setting `key` in.
    fn setting_row(&self, key: &str) -> Option<String> {
        self.posted(key).map(|setting| setting.row.clone())
    }

    /// The row the posted form names the secret `key` in.
    fn secret_row(&self, key: &str) -> Option<String> {
        self.posted?
            .secrets
            .iter()
            .find(|secret| secret.key == key)
            .map(|secret| secret.row.clone())
    }

    /// The choice posted for the secret `key`.
    fn choice(&self, key: &str) -> Option<&str> {
        self.posted?
            .secrets
            .iter()
            .find(|secret| secret.key == key)
            .map(|secret| secret.choice.as_str())
    }
}

/// The field of the credential setting `field`, in row `row`, holding what
/// `held` sets.
fn setting(row: &str, field: &Field, shown: Shown<'_>) -> AnyView {
    let name = |part: &str| format!("form[setting.{row}.{part}]");
    let id = setting_field(row, "value");
    let value = shown.value(field.key);
    let control = match field.kind {
        Kind::Choice(options) => {
            let options = options
                .iter()
                .map(|(choice, label)| {
                    view! {
                        <option value=*choice selected=*choice == value.as_str()>
                            {tr(*label)}
                        </option>
                    }
                })
                .collect_view();
            FormField::new(id.clone(), tr(field.label))
                .after(error_line(&id))
                .view(view! {
                    <select
                        id=id.clone()
                        name=name("value")
                        class=SELECT
                        aria-describedby=error_id(&id)
                    >
                        {options}
                    </select>
                })
        }
        Kind::Switch => view! {
            <div class="fui-check">
                <input
                    type="checkbox"
                    role="switch"
                    class=SWITCH
                    id=id.clone()
                    name=name("value")
                    value="true"
                    checked=value == "true"
                />
                <label for=id.clone()>{tr(field.label)}</label>
            </div>
        }
        .into_any(),
        Kind::List => view! {
            <fieldset class="fui-fieldset fui-choice">
                <legend class="fui-label">{tr(field.label)}</legend>
                {list(row, shown.values(field.key))}
            </fieldset>
        }
        .into_any(),
        Kind::Text | Kind::Url | Kind::Path => text(
            (&id, &name("value")),
            tr(field.label),
            value,
            Text {
                kind: if field.kind == Kind::Url {
                    "url"
                } else {
                    "text"
                },
                mono: field.kind != Kind::Text,
                hint: (field.kind == Kind::Path).then_some(tr(Msg::AbsolutePathGatewaySHost)),
                ..Text::default()
            },
        )
        .into_any(),
    };
    view! {
        <input type="hidden" name="form[setting_rows][]" value=row.to_owned() />
        <input type="hidden" name=name("key") value=field.key />
        {control}
    }
    .into_any()
}

/// The fields and secrets of one part of the credential form, rows
/// numbered from `rows`.
fn part(
    fields: &[Field],
    secrets: &[Secret],
    (shown, slots): (Shown<'_>, &[SecretSlot]),
    rows: &mut u32,
) -> AnyView {
    let mut next = || {
        let row = rows.to_string();
        *rows = rows.saturating_add(1);
        row
    };
    let fields = fields
        .iter()
        .map(|field| {
            let counted = format!("c{}", next());
            setting(
                &shown.setting_row(field.key).unwrap_or(counted),
                field,
                shown,
            )
        })
        .collect_view();
    let secrets = secrets
        .iter()
        .map(|secret| {
            let slot = slots.iter().find(|slot| slot.key == secret.key);
            let counted = format!("x{}", next());
            secret_row(
                &shown.secret_row(secret.key).unwrap_or(counted),
                (secret.key, tr(secret.label)),
                slot,
                shown.choice(secret.key),
            )
        })
        .collect_view();
    view! {
        <div class="fui-columns">{fields}</div>
        {secrets}
    }
    .into_any()
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
fn endpoint_choice(loaded: &Loaded<'_>, endpoint: Option<&str>, chosen: Option<&str>) -> AnyView {
    if let Some(endpoint) = endpoint {
        return view! {
            {hidden("endpoint", endpoint.to_owned())}
            <p>{tr(Msg::Endpoint2)} <code>{endpoint.to_owned()}</code></p>
        }
        .into_any();
    }
    let options = uncredentialed(loaded)
        .into_iter()
        .map(|endpoint| {
            let shown = endpoint.clone();
            let selected = chosen == Some(endpoint.as_str());
            view! {
                <option value=endpoint selected=selected>
                    {shown}
                </option>
            }
        })
        .collect_view();
    let select = view! {
        <select
            id="cfg-endpoint"
            class=SELECT
            name="form[endpoint]"
            required
            aria-describedby=described_by("cfg-endpoint", true, true)
        >
            {options}
        </select>
    };
    FormField::new("cfg-endpoint", tr(Msg::Endpoint))
        .help(tr(Msg::EndpointsMembersDeclareHoldNoCredentials))
        .after(error_line("cfg-endpoint"))
        .view(select)
}

/// The select of the grant, `grant` chosen; the form shows the chosen
/// grant's fields alone.
fn grant_choice(grant: Grant) -> AnyView {
    let grants = Grant::ALL
        .into_iter()
        .map(|option| {
            view! {
                <option value=option.as_str() selected=option == grant>
                    {tr(option.label())}
                </option>
            }
        })
        .collect_view();
    let select = view! {
        <select
            id="cfg-grant"
            class=SELECT
            name="form[grant]"
            data-grant=""
            aria-describedby=described_by("cfg-grant", true, true)
        >
            {grants}
        </select>
    };
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
) -> Result<AnyView, Refusal> {
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
            view! {
                <details name="cfg-grant-parts" open=option == grant>
                    <summary>{tr(option.label())}</summary>
                    {inner}
                </details>
            }
        })
        .collect_view();
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
    let body = view! {
        {hidden("action", String::from("save-credentials"))}
        {hidden("base", loaded.base())}
        <fieldset class=GROUP>
            <legend>{tr(Msg::EndpointAndGrant)}</legend>
            {endpoint_choice(loaded, endpoint, posted.map(|form| form.endpoint.as_str()))}
            {grant_choice(grant)}
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::GrantSettings)}</legend>
            <div class="fui-stack">{parts}</div>
        </fieldset>
        <fieldset class=GROUP>
            <legend>{tr(Msg::Tls)}</legend>
            {tls}
        </fieldset>
        {reason()}
        {save_bar(loaded, "/configuration/credentials")}
    };
    Ok(view! {
        {back(title.clone())}
        {head(
            ("credential-title", title),
            (tr(Msg::SavedRevisionOnceAppliedAwaitsConfirmation), "commit-confirm"),
            (),
        )}
        {form("fui-stack", body)}
        {endpoint
            .map(|endpoint| {
                view! {
                    <div class="fui-actions">
                        {remove(loaded, &Resource::Credentials(endpoint.to_owned()))}
                    </div>
                }
            })}
    }
    .into_any())
}

/// The trail from the tab to the form titled `current`.
pub(super) fn back(current: String) -> AnyView {
    trail(
        (
            tr(Msg::CredentialsAndBindings),
            "/configuration/credentials",
        ),
        current,
    )
}
