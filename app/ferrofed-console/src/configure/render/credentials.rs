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
use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::table::view::{ACTIONS, Column};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::form::{
    Field as FormField, INPUT, Inner, SELECT, SWITCH, described_by, error_id, masked_input,
};
use ferrohealth_ui::view::masked::HoldWords;
use ferrohealth_ui::view::repeat::{Fields, ROW, Repeat, RepeatRow, repeat};

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
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, opt, t};
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
        return h(format!(
            "<span class=\"fui-muted\">{}</span>",
            t(tr(Msg::NotSet2))
        ));
    }
    let items = Html::join(slots.iter().map(|slot| {
        h(format!(
            "<li><code>{}</code>{}<code>{}</code></li>",
            t(&slot.key),
            t(tr(Msg::SetReadFrom)),
            t(&slot.reference)
        ))
    }));
    h(format!("<ul class=\"fui-divided\">{items}</ul>"))
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
    h(format!(
        "<tr><th scope=\"row\"><code>{}</code></th>\
         <td data-label=\"{}\">{}</td><td data-label=\"{}\">{}</td>\
         <td data-label=\"{}\">{}</td><td data-label=\"{}\">{}</td>\
         <td data-label=\"{}\" class=\"{ACTIONS}\">{}</td></tr>",
        t(&shown),
        t(tr(Msg::Owner)),
        owner(section.owner),
        t(tr(Msg::State)),
        state(if console { section.state } else { None }),
        t(tr(Msg::Secrets)),
        secrets_said(&section.secrets),
        t(tr(Msg::Settings)),
        t(settings_said(section)),
        t(tr(Msg::Actions)),
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
        h(format!(
            "<tr><td colspan=\"6\" class=\"fui-muted\">{}</td></tr>",
            t(tr(Msg::NoOnwardCredentialBindingSectionSet))
        ))
    } else {
        Html::join(sections.iter().map(|section| row(loaded, section)))
    };
    let binding = (!addable(loaded).is_empty()).then(|| {
        h(format!(
            "<a href=\"/configuration/bindings/new\" class=\"{}\">{}</a>",
            Variant::Secondary.class(),
            t(tr(Msg::AddBindingSection))
        ))
    });
    head(
        ("credentials-title", tr(Msg::CredentialsAndBindings)),
        (
            tr(Msg::CredentialBindingSectionFileProvidesRead),
            "credentials-and-binding-sections",
        ),
        opt(binding).then(h(format!(
            "<a href=\"/configuration/credentials/new\" class=\"{}\">{}{}</a>",
            Variant::Primary.class(),
            icon(Glyph::PLUS, "16"),
            t(tr(Msg::AddCredentials))
        ))),
    )
    .then(crate::pages::table(
        &tr(Msg::CredentialsBindingSectionsTheirOwners),
        &section_columns(),
        rows,
    ))
}

/// The choice of the secret `secret` in row `row`, its slot `slot` when it
/// is set: Keep, Set a value, Read from a file or, when set, Clear.
///
/// The value field shows while Set a value is chosen and the file field
/// while Read from a file is, which the stylesheet decides by the place of
/// the radio chosen, so it works with the scripts refused.
pub(super) fn secret_row(
    row: &str,
    secret: (&str, String),
    slot: Option<&SecretSlot>,
    chosen: Option<&str>,
) -> Html {
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
    let segments = Html::join(choices.into_iter().map(|(value, word)| {
        let id = secret_field(row, value);
        h(format!(
            "<label for=\"{id}\" class=\"fui-segment\"><input type=\"radio\" id=\"{id}\" \
             class=\"fui-sr-only\" name=\"{}\" value=\"{value}\"{}>{}</label>",
            t(name("choice")),
            if value == chosen { " checked" } else { "" },
            t(word),
            id = t(&id),
        ))
    }));
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
    h(format!(
        "<div class=\"fui-reveal\"><fieldset class=\"fui-fieldset fui-choice\" id=\"{}\">\
         <legend class=\"fui-label\">{}</legend>\
         <input type=\"hidden\" name=\"form[secret_rows][]\" value=\"{}\">\
         <input type=\"hidden\" name=\"{}\" value=\"{}\">\
         <p class=\"fui-help\"><code>{}</code> · {}</p>\
         <div class=\"fui-segmented\">{segments}</div>{}</fieldset>\
         <div class=\"fui-reveal-parts\"><div class=\"fui-revealed\"></div>\
         <div class=\"fui-revealed\">{set}</div><div class=\"fui-revealed\">{file}</div></div></div>",
        t(&group),
        t(label),
        t(row),
        t(name("key")),
        t(key),
        t(key),
        t(status),
        error_line(&group),
    ))
}

/// The secret value field `id` named `name` (R10): masked, never filled,
/// with the eye that shows what was typed while it is held.
pub(super) fn masked(id: &str, name: &str) -> Html {
    FormField::new(id.to_owned(), tr(Msg::ValueNeverShownAgain))
        .trailing(Inner::HoldToShow(
            HoldWords::of(&current()).named(tr(Msg::ShowSecretWhileHeld)),
        ))
        .after(error_line(id).then(ferrohealth_ui::view::form::secret_mark(name)))
        .view(masked_input(id, name, Some(error_id(id))))
}

/// The value at `at` of the list setting in row `row`, holding `value`, a
/// row of one field (F7).
fn list_value(row: &str, at: &str, value: String, removes: &Edit) -> RepeatRow {
    let id = format!("{}-{at}", setting_field(row, "list"));
    let fields = FormField::new(id.clone(), tr(Msg::Value))
        .label_hidden()
        .view(h(format!(
            "<input type=\"text\" id=\"{}\" name=\"form[setting.{}.values][]\" value=\"{}\" \
             class=\"{INPUT} fui-mono\" autocomplete=\"off\">",
            t(&id),
            t(row),
            t(value)
        )));
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
fn list(row: &str, values: Vec<String>) -> Html {
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
    repeat(Repeat {
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
    })
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
fn setting(row: &str, field: &Field, shown: Shown<'_>) -> Html {
    let name = |part: &str| format!("form[setting.{row}.{part}]");
    let id = setting_field(row, "value");
    let value = shown.value(field.key);
    let control = match field.kind {
        Kind::Choice(options) => {
            let options = Html::join(options.iter().map(|(choice, label)| {
                h(format!(
                    "<option value=\"{}\"{}>{}</option>",
                    t(choice),
                    if *choice == value.as_str() {
                        " selected"
                    } else {
                        ""
                    },
                    t(tr(*label))
                ))
            }));
            FormField::new(id.clone(), tr(field.label))
                .after(error_line(&id))
                .view(h(format!(
                    "<select id=\"{}\" name=\"{}\" class=\"{SELECT}\" aria-describedby=\"{}\">{options}</select>",
                    t(&id),
                    t(name("value")),
                    t(error_id(&id))
                )))
        }
        Kind::Switch => h(format!(
            "<div class=\"fui-check\"><input type=\"checkbox\" role=\"switch\" class=\"{SWITCH}\" \
             id=\"{id}\" name=\"{}\" value=\"true\"{}><label for=\"{id}\">{}</label></div>",
            t(name("value")),
            if value == "true" { " checked" } else { "" },
            t(tr(field.label)),
            id = t(&id),
        )),
        Kind::List => h(format!(
            "<fieldset class=\"fui-fieldset fui-choice\"><legend class=\"fui-label\">{}</legend>{}</fieldset>",
            t(tr(field.label)),
            list(row, shown.values(field.key))
        )),
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
        ),
    };
    h(format!(
        "<input type=\"hidden\" name=\"form[setting_rows][]\" value=\"{}\">\
         <input type=\"hidden\" name=\"{}\" value=\"{}\">{control}",
        t(row),
        t(name("key")),
        t(field.key)
    ))
}

/// The fields and secrets of one part of the credential form, rows
/// numbered from `rows`.
fn part(
    fields: &[Field],
    secrets: &[Secret],
    (shown, slots): (Shown<'_>, &[SecretSlot]),
    rows: &mut u32,
) -> Html {
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
        .collect::<Vec<_>>();
    let fields = Html::join(fields);
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
        .collect::<Vec<_>>();
    h(format!("<div class=\"fui-columns\">{fields}</div>")).then(Html::join(secrets))
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
        return hidden("endpoint", endpoint.to_owned()).then(h(format!(
            "<p>{} <code>{}</code></p>",
            t(tr(Msg::Endpoint2)),
            t(endpoint)
        )));
    }
    let options = Html::join(uncredentialed(loaded).into_iter().map(|endpoint| {
        let selected = chosen == Some(endpoint.as_str());
        h(format!(
            "<option value=\"{endpoint}\"{}>{endpoint}</option>",
            if selected { " selected" } else { "" },
            endpoint = t(&endpoint),
        ))
    }));
    let select = h(format!(
        "<select id=\"cfg-endpoint\" class=\"{SELECT}\" name=\"form[endpoint]\" required \
         aria-describedby=\"{}\">{options}</select>",
        t(described_by("cfg-endpoint", true, true).unwrap_or_default())
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
        h(format!(
            "<option value=\"{}\"{}>{}</option>",
            t(option.as_str()),
            if option == grant { " selected" } else { "" },
            t(tr(option.label()))
        ))
    }));
    let select = h(format!(
        "<select id=\"cfg-grant\" class=\"{SELECT}\" name=\"form[grant]\" data-grant=\"\" \
         aria-describedby=\"{}\">{grants}</select>",
        t(described_by("cfg-grant", true, true).unwrap_or_default())
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
            h(format!(
                "<details name=\"cfg-grant-parts\"{}><summary>{}</summary>{inner}</details>",
                if option == grant { " open" } else { "" },
                t(tr(option.label()))
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
        h(format!(
            "<fieldset class=\"{GROUP}\"><legend>{}</legend>{}{}</fieldset>\
             <fieldset class=\"{GROUP}\"><legend>{}</legend><div class=\"fui-stack\">{parts}</div></fieldset>\
             <fieldset class=\"{GROUP}\"><legend>{}</legend>{tls}</fieldset>",
            t(tr(Msg::EndpointAndGrant)),
            endpoint_choice(loaded, endpoint, posted.map(|form| form.endpoint.as_str())),
            grant_choice(grant),
            t(tr(Msg::GrantSettings)),
            t(tr(Msg::Tls)),
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
            h(format!(
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
