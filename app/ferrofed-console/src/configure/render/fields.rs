// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The fields of a credential or binding section form: each setting, each
//! list of values, and each secret's choice of Keep, Set a value, Read from a
//! file or Clear, whose value field the page never fills (§13.1, N25).

use ferrofed_operator_api::resource::SecretSlot;
use ferrohealth_ui::html::Html;
use ferrohealth_ui::view::form::{
    Field as FormField, INPUT, Inner, SELECT, SWITCH, error_id, masked_input,
};
use ferrohealth_ui::view::masked::HoldWords;
use ferrohealth_ui::view::repeat::{Fields, ROW, Repeat, RepeatRow, repeat};

use crate::configure::draft::Edit;
use crate::configure::model::{ConfigForm, SettingInput};
use crate::configure::render::parts::{Text, error_line, row_add, row_remove, text};
use crate::configure::settings::build::{secret_field, setting_field};
use crate::configure::settings::{self, Field, Kind, Secret};
use crate::locale::{Msg, current, tr, tr_with};

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
        Html::raw(format!(
            "<label for=\"{id}\" class=\"fui-segment\"><input type=\"radio\" id=\"{id}\" \
             class=\"fui-sr-only\" name=\"{}\" value=\"{value}\"{}>{}</label>",
            Html::text(name("choice")),
            if value == chosen { " checked" } else { "" },
            Html::text(word),
            id = Html::text(&id),
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
    Html::raw(format!(
        "<div class=\"fui-reveal\"><fieldset class=\"fui-fieldset fui-choice\" id=\"{}\">\
         <legend class=\"fui-label\">{}</legend>\
         <input type=\"hidden\" name=\"form[secret_rows][]\" value=\"{}\">\
         <input type=\"hidden\" name=\"{}\" value=\"{}\">\
         <p class=\"fui-help\"><code>{}</code> · {}</p>\
         <div class=\"fui-segmented\">{segments}</div>{}</fieldset>\
         <div class=\"fui-reveal-parts\"><div class=\"fui-revealed\"></div>\
         <div class=\"fui-revealed\">{set}</div><div class=\"fui-revealed\">{file}</div></div></div>",
        Html::text(&group),
        Html::text(label),
        Html::text(row),
        Html::text(name("key")),
        Html::text(key),
        Html::text(key),
        Html::text(status),
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
pub(super) fn list_value(row: &str, at: &str, value: String, removes: &Edit) -> RepeatRow {
    let id = format!("{}-{at}", setting_field(row, "list"));
    let fields = FormField::new(id.clone(), tr(Msg::Value))
        .label_hidden()
        .view(Html::raw(format!(
            "<input type=\"text\" id=\"{}\" name=\"form[setting.{}.values][]\" value=\"{}\" \
             class=\"{INPUT} fui-mono\" autocomplete=\"off\">",
            Html::text(&id),
            Html::text(row),
            Html::text(value)
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
pub(super) fn list(row: &str, values: Vec<String>) -> Html {
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
pub(super) struct Shown<'a> {
    /// The settings the gateway holds.
    pub(super) held: &'a toml::Table,
    /// The form as posted, drawn in their place.
    pub(super) posted: Option<&'a ConfigForm>,
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
pub(super) fn setting(row: &str, field: &Field, shown: Shown<'_>) -> Html {
    let name = |part: &str| format!("form[setting.{row}.{part}]");
    let id = setting_field(row, "value");
    let value = shown.value(field.key);
    let control = match field.kind {
        Kind::Choice(options) => {
            let options = Html::join(options.iter().map(|(choice, label)| {
                Html::raw(format!(
                    "<option value=\"{}\"{}>{}</option>",
                    Html::text(choice),
                    if *choice == value.as_str() {
                        " selected"
                    } else {
                        ""
                    },
                    Html::text(tr(*label))
                ))
            }));
            FormField::new(id.clone(), tr(field.label))
                .after(error_line(&id))
                .view(Html::raw(format!(
                    "<select id=\"{}\" name=\"{}\" class=\"{SELECT}\" aria-describedby=\"{}\">{options}</select>",
                    Html::text(&id),
                    Html::text(name("value")),
                    Html::text(error_id(&id))
                )))
        }
        Kind::Switch => Html::raw(format!(
            "<div class=\"fui-check\"><input type=\"checkbox\" role=\"switch\" class=\"{SWITCH}\" \
             id=\"{id}\" name=\"{}\" value=\"true\"{}><label for=\"{id}\">{}</label></div>",
            Html::text(name("value")),
            if value == "true" { " checked" } else { "" },
            Html::text(tr(field.label)),
            id = Html::text(&id),
        )),
        Kind::List => Html::raw(format!(
            "<fieldset class=\"fui-fieldset fui-choice\"><legend class=\"fui-label\">{}</legend>{}</fieldset>",
            Html::text(tr(field.label)),
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
    Html::raw(format!(
        "<input type=\"hidden\" name=\"form[setting_rows][]\" value=\"{}\">\
         <input type=\"hidden\" name=\"{}\" value=\"{}\">{control}",
        Html::text(row),
        Html::text(name("key")),
        Html::text(field.key)
    ))
}

/// The fields and secrets of one part of the credential form, rows
/// numbered from `rows`.
pub(super) fn part(
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
    Html::raw(format!("<div class=\"fui-columns\">{fields}</div>")).then(Html::join(secrets))
}
