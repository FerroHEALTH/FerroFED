// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The form that adds or edits a binding section the console holds: each
//! setting it holds a field of its own, a row each for a new one, and a
//! choice for each secret, whose value the page never shows.

use ferrofed_registry::console::Owner;
use ferrofed_registry::resource::{BindingSection, Resource, SecretSlot};
use ferrohealth_ui::view::form::{Field, SELECT, error_id};
use ferrohealth_ui::widget::repeat::{Fields, ROW, Repeat, RepeatRow, repeat};
use leptos::prelude::*;

use crate::configure::draft::Edit;
use crate::configure::model::ConfigForm;
use crate::configure::render::Loaded;
use crate::configure::render::credentials::{addable, back, masked, secret_row};
use crate::configure::render::members::GROUP;
use crate::configure::render::parts::{
    Text, error_line, form, head, hidden, reason, row_add, row_remove, save_bar, text,
};
use crate::configure::settings;
use crate::configure::settings::build::{secret_field, setting_field};

use crate::locale::{Msg, tr, tr_with};
use crate::read::Refusal;

/// One setting row of a binding section as the form shows it.
struct SettingRow {
    /// The row's key on the page.
    row: String,
    /// The setting's key: fixed for one the section holds, typed for a new
    /// one.
    key: String,
    /// Whether the section holds the key, which then cannot be changed.
    held: bool,
    /// The value.
    value: String,
}

/// A setting row of a binding section (F7 sub-card): its key, fixed for
/// one the section holds and then its heading, and its value.
fn binding_row(setting: &SettingRow) -> RepeatRow {
    let row = setting.row.as_str();
    let name = |part: &str| format!("form[setting][{row}][{part}]");
    let value_id = setting_field(row, "value");
    let key = if setting.held {
        view! { <input type="hidden" name=name("key") value=setting.key.clone() /> }.into_any()
    } else {
        text(
            (&setting_field(row, "key"), &name("key")),
            tr(Msg::Key),
            setting.key.clone(),
            Text {
                mono: true,
                hint: Some(tr(Msg::PartsJoinedDotsSuchOauth2Scope)),
                ..Text::default()
            },
        )
        .into_any()
    };
    let (heading, label, named) = if setting.held {
        (
            setting.key.clone(),
            tr_with(Msg::ValueOf, &[("key", setting.key.clone().into())]),
            setting.key.clone(),
        )
    } else {
        (tr(Msg::NewSetting), tr(Msg::Value), tr(Msg::ThisNewSetting))
    };
    let fields = view! {
        <input type="hidden" name="form[setting_rows]" value=row.to_owned() />
        {key}
        {text(
            (&value_id, &name("value")),
            label,
            setting.value.clone(),
            Text {
                mono: true,
                ..Text::default()
            },
        )}
    }
    .into_any();
    RepeatRow {
        key: row.to_owned(),
        heading: Some(heading),
        fields,
        end: row_remove(
            &Edit::RemoveSetting(row.to_owned()),
            tr_with(Msg::Remove2, &[("named", named.into())]),
        ),
        after: None,
    }
}

/// A new secret row of a binding section (F7 sub-card), its key typed,
/// `key` filled in, and its value masked and never filled.
fn new_secret(row: &str, key: String) -> RepeatRow {
    let name = |part: &str| format!("form[secret][{row}][{part}]");
    let fields = view! {
        <input type="hidden" name="form[secret_rows]" value=row.to_owned() />
        {text(
            (&secret_field(row, "key"), &name("key")),
            tr(Msg::SecretKey),
            key,
            Text {
                mono: true,
                hint: Some(tr(Msg::SuchOauth2ClientSecret)),
                ..Text::default()
            },
        )}
        <input type="hidden" name=name("choice") value="set" />
        {masked(&secret_field(row, "value"), &name("value"))}
    }
    .into_any();
    RepeatRow {
        key: row.to_owned(),
        heading: Some(tr(Msg::NewSecret)),
        fields,
        end: row_remove(
            &Edit::RemoveSecret(row.to_owned()),
            tr(Msg::RemoveThisNewSecret),
        ),
        after: None,
    }
}

/// The binding section of the form: `held` as it stands, or a choice of
/// those not set yet.
fn section_choice(
    loaded: &Loaded<'_>,
    held: Option<BindingSection>,
    chosen: Option<&str>,
) -> AnyView {
    if let Some(binding) = held {
        return hidden("section", binding.as_str().to_owned()).into_any();
    }
    let options = addable(loaded)
        .into_iter()
        .map(|binding| {
            let selected = chosen == Some(binding.as_str());
            view! {
                <option value=binding.as_str() selected=selected>
                    {Resource::Binding(binding).to_string()}
                </option>
            }
        })
        .collect_view();
    let select = view! {
        <select
            id="cfg-section"
            name="form[section]"
            class=SELECT
            aria-describedby=error_id("cfg-section")
        >
            {options}
        </select>
    };
    Field::new("cfg-section", tr(Msg::BindingSection))
        .after(error_line("cfg-section"))
        .view(select)
}

/// The setting rows of a binding section: what `table` holds, or the rows
/// the operator posted.
fn setting_rows(table: &toml::Table, posted: Option<&ConfigForm>) -> Vec<SettingRow> {
    let scalars = settings::build::scalars(table);
    match posted {
        Some(form) => form
            .settings
            .iter()
            .map(|setting| SettingRow {
                row: setting.row.clone(),
                key: setting.key.clone(),
                held: scalars.iter().any(|(key, _)| key == &setting.key),
                value: setting.value.clone(),
            })
            .collect(),
        None => scalars
            .into_iter()
            .zip(0_u32..)
            .map(|((key, value), row)| SettingRow {
                row: format!("s{row}"),
                key,
                held: true,
                value: match value {
                    toml::Value::String(text) => text,
                    other => other.to_string(),
                },
            })
            .collect(),
    }
}

/// The settings of a binding section, a sub-card each (F7) with Add under
/// them; its lists are kept and named.
fn settings_group(table: &toml::Table, posted: Option<&ConfigForm>) -> AnyView {
    let rows = setting_rows(table, posted)
        .iter()
        .map(binding_row)
        .collect();
    let blank = binding_row(&SettingRow {
        row: String::from(ROW),
        key: String::new(),
        held: false,
        value: String::new(),
    });
    let kept: Vec<String> = table
        .iter()
        .filter(|(_, value)| value.is_array())
        .map(|(key, _)| key.clone())
        .collect();
    let kept = (!kept.is_empty()).then(|| {
        view! {
            <p class="fui-muted">
                {tr_with(Msg::KeptRevisionHoldsThem, &[("kept", (kept.join(", ")).clone().into())])}
            </p>
        }
    });
    view! {
        <fieldset class=GROUP>
            <legend>{tr(Msg::Settings)}</legend>
            {kept}
            {repeat(Repeat {
                id: String::from("cfg-setting-rows"),
                label: tr(Msg::Settings),
                fields: Fields::Card,
                head: Vec::new(),
                rows,
                blank: Some(blank),
                add: Some(row_add(&Edit::AddSetting, tr(Msg::AddSetting))),
                empty: tr(Msg::NoSettingYet),
                form: None,
            })}
        </fieldset>
    }
    .into_any()
}

/// The secrets of a binding section: a choice for each of `slots`, the new
/// ones the operator posted, and a button that adds a new one.
fn secrets_group(slots: &[SecretSlot], posted: Option<&ConfigForm>) -> AnyView {
    let chosen = |key: &str| {
        posted.and_then(|form| {
            form.secrets
                .iter()
                .find(|secret| secret.key == key)
                .map(|secret| secret.choice.clone())
        })
    };
    let secrets = slots
        .iter()
        .zip(0_u32..)
        .map(|(slot, row)| {
            secret_row(
                &format!("x{row}"),
                (&slot.key, slot.key.clone()),
                Some(slot),
                chosen(&slot.key).as_deref(),
            )
        })
        .collect_view();
    let fresh = posted
        .map(|form| {
            form.secrets
                .iter()
                .filter(|secret| !slots.iter().any(|slot| slot.key == secret.key))
                .map(|secret| new_secret(&secret.row, secret.key.clone()))
                .collect()
        })
        .unwrap_or_default();
    view! {
        <fieldset class=GROUP>
            <legend>{tr(Msg::Secrets)}</legend>
            {secrets}
            {repeat(Repeat {
                id: String::from("cfg-secret-rows"),
                label: tr(Msg::Secrets),
                fields: Fields::Card,
                head: Vec::new(),
                rows: fresh,
                blank: Some(new_secret(ROW, String::new())),
                add: Some(row_add(&Edit::AddSecret, tr(Msg::AddSecret))),
                empty: tr(Msg::NoNewSecretYet),
                form: None,
            })}
        </fieldset>
    }
    .into_any()
}

/// The form that adds a binding section, or edits the section `section`.
///
/// # Errors
/// Returns [`Refusal::Invalid`] for a section the console does not hold or
/// does not know, and settings that do not read.
pub(super) fn binding_form(loaded: &Loaded<'_>, section: Option<&str>) -> Result<AnyView, Refusal> {
    let held = match section {
        Some(name) => {
            let binding = BindingSection::named(name).ok_or_else(|| Refusal::Invalid {
                reason: tr_with(
                    Msg::NoBindingSectionCalled,
                    &[("name", (name).to_string().into())],
                ),
            })?;
            let found = loaded
                .resources
                .sections
                .iter()
                .find(|held| {
                    held.owner == Owner::Console && held.resource == Resource::Binding(binding)
                })
                .ok_or_else(|| Refusal::Invalid {
                    reason: tr_with(
                        Msg::ConsoleHoldsNoSection,
                        &[("name", (name).to_string().into())],
                    ),
                })?;
            Some((binding, found))
        }
        None => None,
    };
    let table = settings::build::held(held.and_then(|(_, held)| held.settings.as_deref()))
        .map_err(|refused| refused.refusal)?;
    let slots = held
        .map(|(_, held)| held.secrets.as_slice())
        .unwrap_or_default();
    let posted = loaded
        .drafted(&["save-binding"])
        .filter(|form| section.is_none_or(|section| form.section == section));
    let chosen = section_choice(
        loaded,
        held.map(|(binding, _)| binding),
        posted.map(|form| form.section.as_str()),
    );
    let title = held.map_or_else(
        || tr(Msg::AddABindingSection),
        |(binding, _)| {
            tr_with(
                Msg::Edit2,
                &[("value0", (Resource::Binding(binding)).to_string().into())],
            )
        },
    );
    let body = view! {
        {hidden("action", String::from("save-binding"))}
        {hidden("base", loaded.base())}
        <fieldset class=GROUP>
            <legend>{tr(Msg::Section)}</legend>
            {chosen}
        </fieldset>
        {settings_group(&table, posted)}
        {secrets_group(slots, posted)}
        {reason()}
        {save_bar(loaded, "/configuration/credentials")}
    };
    Ok(view! {
        {back(title.clone())}
        {head(
            ("binding-title", title),
            (tr(Msg::EachSettingOwnFieldNumberTrue), "credentials-and-binding-sections"),
            (),
        )}
        {form("fui-stack", body)}
        {held
            .map(|(binding, _)| {
                view! {
                    <div class="fui-actions">
                        {crate::configure::render::credentials::remove(
                            loaded,
                            &Resource::Binding(binding),
                        )}
                    </div>
                }
            })}
    }
    .into_any())
}
