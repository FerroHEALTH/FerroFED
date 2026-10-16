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
use leptos::prelude::*;

use crate::configure::plan::removal_reason;
use crate::configure::render::Loaded;
use crate::configure::render::members::{GROUP, GROUP_TITLE, owner, state};
use crate::configure::render::parts::{
    Text, add_button, confirmed, error_line, form, head, hidden, reason, remove_button, save_bar,
    template, text,
};
use crate::configure::sections::choice;
use crate::configure::settings::build::{secret_field, setting_field};
use crate::configure::settings::fields::TLS;
use crate::configure::settings::{self, Field, Grant, Kind, Secret, TLS_SECRET};
use crate::styles::{self, Button};
use crate::views::model::Refusal;

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
        return view! { <span class="text-muted">"not set"</span> }.into_any();
    }
    slots
        .iter()
        .map(|slot| {
            view! {
                <span class="block">
                    <code>{slot.key.clone()}</code>
                    ": set, read from "
                    <code>{slot.reference.clone()}</code>
                </span>
            }
        })
        .collect_view()
        .into_any()
}

/// The settings of `section` in a few words: its grant and how many keys
/// it sets.
fn settings_said(section: &SectionResource) -> String {
    let Ok(table) = settings::build::held(section.settings.as_deref()) else {
        return String::from("not readable");
    };
    let count = match settings::build::scalars(&table).len() {
        0 => String::from("no settings"),
        1 => String::from("1 setting"),
        many => format!("{many} settings"),
    };
    match &section.resource {
        Resource::Credentials(_) => {
            format!(
                "{} · {count}",
                Grant::held(&table, &section.secrets).label()
            )
        }
        _ => count,
    }
}

/// The removal of `resource`, a confirmation step asking for the reason.
fn remove(loaded: &Loaded<'_>, resource: &Resource) -> AnyView {
    let named = choice(resource);
    let reason_id = removal_reason(&named);
    let shown = resource.to_string();
    form(
        "main",
        "inline",
        view! {
            {hidden("action", String::from("remove-section"))}
            {hidden("base", loaded.base())}
            {hidden("resource", named.clone())}
            {confirmed(
                format!("remove-{}", named.replace(':', "-")),
                (
                    view! {
                        "Remove"
                        <span class="sr-only">{format!(" {shown}")}</span>
                    },
                    Button::Ghost,
                ),
                (
                    format!(
                        "Remove {shown}? It is saved as {} without it, which you apply separately.",
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

/// One credential's or binding section's row.
fn row(loaded: &Loaded<'_>, section: &SectionResource) -> AnyView {
    let shown = section.resource.to_string();
    let console = section.owner == Owner::Console;
    let editable = console
        && matches!(
            section.state,
            Some(ConsoleState::Running | ConsoleState::Pending)
        );
    let actions = editable.then(|| {
        view! {
            <a href=href(&section.resource) class=styles::BUTTON_GHOST>
                "Edit"
                <span class="sr-only">{format!(" {shown}")}</span>
            </a>
            {remove(loaded, &section.resource)}
        }
    });
    view! {
        <tr>
            <th scope="row">
                <code>{shown.clone()}</code>
            </th>
            <td>{owner(section.owner)}</td>
            <td>{state(if console { section.state } else { None })}</td>
            <td>{secrets_said(&section.secrets)}</td>
            <td>{settings_said(section)}</td>
            <td>
                <div class="flex flex-wrap items-center justify-end gap-1">{actions}</div>
            </td>
        </tr>
    }
    .into_any()
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

/// The Credentials and bindings tab.
pub(super) fn tab(loaded: &Loaded<'_>) -> AnyView {
    let sections = &loaded.resources.sections;
    let rows = if sections.is_empty() {
        view! {
            <tr>
                <td colspan="6" class="text-muted">
                    "No onward credential or binding section is set."
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
            <a href="/configuration/bindings/new" class=styles::BUTTON_SECONDARY>
                "Add binding section"
            </a>
        }
    });
    view! {
        {head(
            ("credentials-title", String::from("Credentials and bindings")),
            (
                "A credential or a binding section the file provides is read-only here; a secret shows as set, never by value.",
                "credentials-and-binding-sections",
            ),
            view! {
                {binding}
                <a href="/configuration/credentials/new" class=styles::BUTTON_PRIMARY>
                    {crate::components::icon(icondata_lu::LuPlus, "16")}
                    "Add credentials"
                </a>
            },
        )}
        <div
            class=format!("{} relative", styles::TABLE_SCROLL)
            tabindex="0"
            role="region"
            aria-labelledby="credentials-title"
        >
            <table>
                <caption class="sr-only">
                    "Credentials and binding sections and their owners"
                </caption>
                <thead>
                    <tr>
                        <th scope="col">"Resource"</th>
                        <th scope="col">"Owner"</th>
                        <th scope="col">"State"</th>
                        <th scope="col">"Secrets"</th>
                        <th scope="col">"Settings"</th>
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

/// The classes of a choice of a secret, one segment of a segmented control:
/// raised and edged while chosen, so colour is never its only mark.
const SEGMENT: &str = "inline-flex cursor-pointer items-center rounded-[0.25rem] border \
                       border-transparent px-2.5 py-1 text-xs font-medium text-muted \
                       has-[:checked]:border-control has-[:checked]:bg-raised \
                       has-[:checked]:text-ink has-[:checked]:shadow-card \
                       has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-accent";

/// The choice of the secret `secret` in row `row`, its slot `slot` when it
/// is set: Keep, Set a value, Read from a file or, when set, Clear.
pub(super) fn secret_row(row: &str, secret: (&str, String), slot: Option<&SecretSlot>) -> AnyView {
    let (key, label) = secret;
    let name = |part: &str| format!("form[secret][{row}][{part}]");
    let status = slot.map_or_else(
        || String::from("Not set."),
        |slot| format!("Set, read from {}.", slot.reference),
    );
    let mut choices = vec![
        (
            "keep",
            if slot.is_some() {
                "Keep"
            } else {
                "Leave unset"
            },
        ),
        ("set", "Set a value"),
        ("file", "Read from a file"),
    ];
    if slot.is_some() {
        choices.push(("clear", "Clear"));
    }
    let segments = choices
        .into_iter()
        .map(|(value, word)| {
            let id = secret_field(row, value);
            let label_for = id.clone();
            view! {
                <label for=label_for class=SEGMENT>
                    <input
                        type="radio"
                        id=id
                        class="sr-only"
                        name=name("choice")
                        value=value
                        checked=value == "keep"
                    />
                    {word}
                </label>
            }
        })
        .collect_view();
    let group = secret_field(row, "choice");
    let value_id = secret_field(row, "value");
    view! {
        <fieldset
            class="secret m-0 mb-4 min-w-0 rounded-none border-0 bg-transparent p-0 shadow-none"
            id=group.clone()
        >
            <legend class="mb-1 text-sm font-medium">{label}</legend>
            <input type="hidden" name="form[secret_rows]" value=row.to_owned() />
            <input type="hidden" name=name("key") value=key.to_owned() />
            <p class="clear-both mt-0 mb-2 text-xs text-muted">
                <code>{key.to_owned()}</code>
                " · "
                {status}
            </p>
            <div class=styles::SEGMENTED role="presentation">
                {segments}
            </div>
            <div class="secret-set mt-2">
                <label for=value_id.clone()>"Value, never shown again"</label>
                <input
                    type="password"
                    id=value_id.clone()
                    name=name("value")
                    autocomplete="new-password"
                    aria-describedby=format!("{value_id}-error")
                />
                {error_line(&value_id)}
            </div>
            <div class="secret-file mt-2">
                {text(
                    (&secret_field(row, "file"), &name("file")),
                    String::from("Absolute path of the file it is read from"),
                    String::new(),
                    Text {
                        mono: true,
                        ..Text::default()
                    },
                )}
            </div>
            {error_line(&group)}
        </fieldset>
    }
    .into_any()
}

/// The values of the list setting in row `row`, `values` filled in.
fn list(row: &str, values: Vec<String>) -> AnyView {
    let into = setting_field(row, "list");
    let item = |item: &str, value: String| {
        let id = format!("{into}-{item}");
        view! {
            <div id=format!("{id}-row") class="flex items-end gap-2">
                <div class="min-w-0 flex-1">
                    <label for=id.clone() class="sr-only">
                        "Value"
                    </label>
                    <input
                        type="text"
                        id=id.clone()
                        name=format!("form[setting][{row}][values]")
                        value=value
                        class=styles::MONO
                        autocomplete="off"
                    />
                </div>
                {remove_button(&format!("{id}-row"), String::from("Remove this value"))}
            </div>
        }
    };
    let items = values
        .into_iter()
        .zip(0_u32..)
        .map(|(value, at)| item(&at.to_string(), value))
        .collect_view();
    let tpl = format!("tpl-{into}");
    view! {
        <div id=into.clone() class="mb-2 flex flex-col gap-2">
            {items}
        </div>
        {add_button(&setting_field(row, "add"), (&tpl, &into), ("Add value", "__ITEM__"))}
        {template(tpl.clone(), item("__ITEM__", String::new()))}
    }
    .into_any()
}

/// The field of the credential setting `field`, in row `row`, holding what
/// `held` sets.
fn setting(row: &str, field: &Field, held: &toml::Table) -> AnyView {
    let name = |part: &str| format!("form[setting][{row}][{part}]");
    let id = setting_field(row, "value");
    let value = settings::build::shown(held, field.key);
    let control = match field.kind {
        Kind::Choice(options) => {
            let options = options
                .iter()
                .map(|(choice, label)| {
                    view! {
                        <option value=*choice selected=*choice == value.as_str()>
                            {*label}
                        </option>
                    }
                })
                .collect_view();
            view! {
                <div class=styles::FIELD>
                    <label for=id.clone()>{field.label}</label>
                    <select id=id.clone() name=name("value") aria-describedby=format!("{id}-error")>
                        {options}
                    </select>
                    {error_line(&id)}
                </div>
            }
            .into_any()
        }
        Kind::Switch => view! {
            <div class="mb-4">
                <label for=id.clone() class="inline-flex items-center gap-2 text-sm font-medium">
                    <input
                        type="checkbox"
                        role="switch"
                        id=id.clone()
                        name=name("value")
                        value="true"
                        checked=value == "true"
                    />
                    {field.label}
                </label>
            </div>
        }
        .into_any(),
        Kind::List => view! {
            <fieldset class="m-0 mb-4 min-w-0 rounded-none border-0 bg-transparent p-0 shadow-none">
                <legend class="mb-1 text-sm font-medium">{field.label}</legend>
                {list(row, settings::build::listed(held, field.key))}
            </fieldset>
        }
        .into_any(),
        Kind::Text | Kind::Url | Kind::Path => text(
            (&id, &name("value")),
            field.label.to_owned(),
            value,
            Text {
                kind: if field.kind == Kind::Url {
                    "url"
                } else {
                    "text"
                },
                mono: field.kind != Kind::Text,
                hint: (field.kind == Kind::Path)
                    .then_some("An absolute path on the gateway's host."),
                ..Text::default()
            },
        )
        .into_any(),
    };
    view! {
        <input type="hidden" name="form[setting_rows]" value=row.to_owned() />
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
    (held, slots): (&toml::Table, &[SecretSlot]),
    rows: &mut u32,
) -> AnyView {
    let mut next = || {
        let row = rows.to_string();
        *rows = rows.saturating_add(1);
        row
    };
    let fields = fields
        .iter()
        .map(|field| setting(&format!("c{}", next()), field, held))
        .collect_view();
    let secrets = secrets
        .iter()
        .map(|secret| {
            let slot = slots.iter().find(|slot| slot.key == secret.key);
            secret_row(
                &format!("x{}", next()),
                (secret.key, secret.label.to_owned()),
                slot,
            )
        })
        .collect_view();
    view! {
        <div class="grid grid-cols-1 gap-x-4 md:grid-cols-2">{fields}</div>
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
fn endpoint_choice(loaded: &Loaded<'_>, endpoint: Option<&str>) -> AnyView {
    if let Some(endpoint) = endpoint {
        return view! {
            {hidden("endpoint", endpoint.to_owned())}
            <p class="mt-0 mb-4 text-sm">"Endpoint " <code>{endpoint.to_owned()}</code></p>
        }
        .into_any();
    }
    let options = uncredentialed(loaded)
        .into_iter()
        .map(|endpoint| {
            let shown = endpoint.clone();
            view! { <option value=endpoint>{shown}</option> }
        })
        .collect_view();
    view! {
        <div class=styles::FIELD>
            <label for="cfg-endpoint">"Endpoint"</label>
            <select
                id="cfg-endpoint"
                name="form[endpoint]"
                required
                aria-describedby="cfg-endpoint-hint cfg-endpoint-error"
            >
                {options}
            </select>
            <p id="cfg-endpoint-hint" class=styles::HELP>
                "The endpoints the members declare that hold no credentials yet."
            </p>
            {error_line("cfg-endpoint")}
        </div>
    }
    .into_any()
}

/// The select of the grant, `grant` chosen; the form shows the chosen
/// grant's fields alone.
fn grant_choice(grant: Grant) -> AnyView {
    let grants = Grant::ALL
        .into_iter()
        .map(|option| {
            view! {
                <option value=option.as_str() selected=option == grant>
                    {option.label()}
                </option>
            }
        })
        .collect_view();
    view! {
        <div class=styles::FIELD>
            <label for="cfg-grant">"Grant type"</label>
            <select
                id="cfg-grant"
                name="form[grant]"
                data-grant=""
                aria-describedby="cfg-grant-hint cfg-grant-error"
            >
                {grants}
            </select>
            <p id="cfg-grant-hint" class=styles::HELP>
                "How the gateway reaches the endpoint; the form shows that grant's fields."
            </p>
            {error_line("cfg-grant")}
        </div>
    }
    .into_any()
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
                    reason: format!("The console holds no credentials of {endpoint}."),
                })?,
        ),
        None => None,
    };
    let table = settings::build::held(held.and_then(|held| held.settings.as_deref()))
        .map_err(|refused| refused.refusal)?;
    let slots = held.map(|held| held.secrets.as_slice()).unwrap_or_default();
    let grant = Grant::held(&table, slots);
    let mut rows = 0_u32;
    let parts = Grant::ALL
        .into_iter()
        .filter(|option| !option.fields().is_empty() || !option.secrets().is_empty())
        .map(|option| {
            let inner = part(
                option.fields(),
                option.secrets(),
                (&table, slots),
                &mut rows,
            );
            view! {
                <div class="grant-part" data-grant=option.as_str()>
                    {inner}
                </div>
            }
        })
        .collect_view();
    let tls = part(TLS, &[TLS_SECRET], (&table, slots), &mut rows);
    let title = endpoint.map_or_else(
        || String::from("Add onward credentials"),
        |endpoint| format!("Edit the credentials of {endpoint}"),
    );
    let body = view! {
        {hidden("action", String::from("save-credentials"))}
        {hidden("base", loaded.base())}
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Endpoint and grant"</legend>
            {endpoint_choice(loaded, endpoint)}
            {grant_choice(grant)}
        </fieldset>
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Grant settings"</legend>
            {parts}
        </fieldset>
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"TLS"</legend>
            {tls}
        </fieldset>
        <fieldset class=GROUP>
            <legend class=GROUP_TITLE>"Reason"</legend>
            {reason()}
        </fieldset>
        {save_bar(loaded, "/configuration/credentials")}
    };
    Ok(view! {
        {back()}
        {head(
            ("credential-title", title),
            (
                "Saved as a revision; once applied it awaits a confirmation, or the gateway rolls it back.",
                "commit-confirm",
            ),
            (),
        )}
        {form("main", "", body)}
    }
    .into_any())
}

/// The link back to the tab.
pub(super) fn back() -> AnyView {
    view! {
        <p class="mt-0 mb-2 text-sm">
            <a href="/configuration/credentials">"Credentials and bindings"</a>
        </p>
    }
    .into_any()
}
