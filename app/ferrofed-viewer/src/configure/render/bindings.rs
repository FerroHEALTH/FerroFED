// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The form that adds or edits a binding section the console holds: each
//! setting it holds a field of its own, a row each for a new one, and a
//! choice for each secret, whose value the page never shows.

use ferrofed_registry::console::Owner;
use ferrofed_registry::resource::{BindingSection, Resource, SecretSlot};
use ferrohealth_ui::view::form::{INPUT, SELECT};
use leptos::prelude::*;

use crate::configure::render::Loaded;
use crate::configure::render::credentials::{addable, back, secret_row};
use crate::configure::render::members::GROUP;
use crate::configure::render::parts::{
    Text, add_button, error_line, form, head, hidden, reason, remove_button, save_bar, template,
    text,
};
use crate::configure::settings;
use crate::configure::settings::build::{secret_field, setting_field};

use crate::views::model::Refusal;

/// A setting row `row` of a binding section: its key, fixed for one the
/// section holds, and its value.
fn binding_row(row: &str, held: Option<(&str, &toml::Value)>) -> AnyView {
    let name = |part: &str| format!("form[setting][{row}][{part}]");
    let line = format!("cfg-s-{row}-row");
    let value_id = setting_field(row, "value");
    let key = match held {
        Some((key, _)) => view! {
            <input type="hidden" name=name("key") value=key.to_owned() />
            <p class="fui-label">
                <code>{key.to_owned()}</code>
            </p>
        }
        .into_any(),
        None => text(
            (&setting_field(row, "key"), &name("key")),
            String::from("Key"),
            String::new(),
            Text {
                mono: true,
                hint: Some("Its parts joined by dots, such as oauth2.scope."),
                ..Text::default()
            },
        )
        .into_any(),
    };
    let label = held.map_or_else(
        || String::from("Value"),
        |(key, _)| format!("Value of {key}"),
    );
    let value = held
        .map(|(_, value)| match value {
            toml::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let named = held.map_or_else(
        || String::from("this new setting"),
        |(key, _)| key.to_owned(),
    );
    view! {
        <div id=line.clone() class="fui-well fui-stack">
            <input type="hidden" name="form[setting_rows]" value=row.to_owned() />
            {key}
            {text(
                (&value_id, &name("value")),
                label,
                value,
                Text {
                    mono: true,
                    ..Text::default()
                },
            )}
            {remove_button(&line, format!("Remove {named}"))}
        </div>
    }
    .into_any()
}

/// A new secret row of a binding section, its key typed.
fn new_secret(row: &str) -> AnyView {
    let name = |part: &str| format!("form[secret][{row}][{part}]");
    let line = format!("cfg-x-{row}-row");
    view! {
        <div id=line.clone() class="fui-well fui-stack">
            <input type="hidden" name="form[secret_rows]" value=row.to_owned() />
            {text(
                (&secret_field(row, "key"), &name("key")),
                String::from("Secret key"),
                String::new(),
                Text {
                    mono: true,
                    hint: Some("Such as oauth2.client_secret."),
                    ..Text::default()
                },
            )}
            <input type="hidden" name=name("choice") value="set" />
            <label for=secret_field(row, "value") class="fui-label">
                "Value, never shown again"
            </label>
            <input
                type="password"
                id=secret_field(row, "value")
                name=name("value")
                class=INPUT
                autocomplete="new-password"
                aria-describedby=format!("{}-error", secret_field(row, "value"))
            />
            {error_line(&secret_field(row, "value"))}
            {remove_button(&line, String::from("Remove this new secret"))}
        </div>
    }
    .into_any()
}

/// The binding section of the form: `held` as it stands, or a choice of
/// those not set yet.
fn section_choice(loaded: &Loaded<'_>, held: Option<BindingSection>) -> AnyView {
    if let Some(binding) = held {
        return hidden("section", binding.as_str().to_owned()).into_any();
    }
    let options = addable(loaded)
        .into_iter()
        .map(|binding| {
            view! { <option value=binding.as_str()>{Resource::Binding(binding).to_string()}</option> }
        })
        .collect_view();
    view! {
        <div class="fui-field">
            <label for="cfg-section" class="fui-label">"Binding section"</label>
            <select id="cfg-section" name="form[section]" class=SELECT aria-describedby="cfg-section-error">
                {options}
            </select>
            {error_line("cfg-section")}
        </div>
    }
    .into_any()
}

/// The settings of a binding section, a row each for what `table` holds
/// and a button that adds one; its lists are kept and named.
fn settings_group(table: &toml::Table) -> AnyView {
    let rows = settings::build::scalars(table)
        .iter()
        .zip(0_u32..)
        .map(|((key, value), row)| binding_row(&format!("s{row}"), Some((key.as_str(), value))))
        .collect_view();
    let kept: Vec<String> = table
        .iter()
        .filter(|(_, value)| value.is_array())
        .map(|(key, _)| key.clone())
        .collect();
    let kept = (!kept.is_empty()).then(|| {
        view! {
            <p class="fui-muted">
                {format!("Kept as the revision holds them: {}.", kept.join(", "))}
            </p>
        }
    });
    view! {
        <fieldset class=GROUP>
            <legend>"Settings"</legend>
            {kept}
            <div id="cfg-setting-rows" class="fui-stack">
                {rows}
            </div>
            {add_button(
                "cfg-add-setting",
                ("tpl-setting", "cfg-setting-rows"),
                ("Add setting", "__KEY__"),
            )}
            {template(String::from("tpl-setting"), binding_row("__KEY__", None))}
        </fieldset>
    }
    .into_any()
}

/// The secrets of a binding section: a choice for each of `slots`, and a
/// button that adds a new one.
fn secrets_group(slots: &[SecretSlot]) -> AnyView {
    let secrets = slots
        .iter()
        .zip(0_u32..)
        .map(|(slot, row)| {
            secret_row(
                &format!("x{row}"),
                (&slot.key, slot.key.clone()),
                Some(slot),
            )
        })
        .collect_view();
    view! {
        <fieldset class=GROUP>
            <legend>"Secrets"</legend>
            {secrets}
            <div id="cfg-secret-rows" class="fui-stack"></div>
            {add_button(
                "cfg-add-secret",
                ("tpl-secret", "cfg-secret-rows"),
                ("Add secret", "__KEY__"),
            )}
            {template(String::from("tpl-secret"), new_secret("__KEY__"))}
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
                reason: format!("No binding section is called {name}."),
            })?;
            let found = loaded
                .resources
                .sections
                .iter()
                .find(|held| {
                    held.owner == Owner::Console && held.resource == Resource::Binding(binding)
                })
                .ok_or_else(|| Refusal::Invalid {
                    reason: format!("The console holds no {name} section."),
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
    let chosen = section_choice(loaded, held.map(|(binding, _)| binding));
    let title = held.map_or_else(
        || String::from("Add a binding section"),
        |(binding, _)| format!("Edit {}", Resource::Binding(binding)),
    );
    let body = view! {
        {hidden("action", String::from("save-binding"))}
        {hidden("base", loaded.base())}
        <fieldset class=GROUP>
            <legend>"Section"</legend>
            {chosen}
        </fieldset>
        {settings_group(&table)}
        {secrets_group(slots)}
        {reason()}
        {save_bar(loaded, "/configuration/credentials")}
    };
    Ok(view! {
        {back(title.clone())}
        {head(
            ("binding-title", title),
            (
                "Each setting is its own field; a number or true or false is read as one.",
                "credentials-and-binding-sections",
            ),
            (),
        )}
        {form("main", "fui-stack", body)}
    }
    .into_any())
}
