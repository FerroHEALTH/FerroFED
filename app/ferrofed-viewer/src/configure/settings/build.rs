// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings a form gives, built onto those the newest revision holds
//! as the one TOML inline table the gateway takes, and the secret changes
//! it asks for.

use ferrofed_registry::resource::{SECRETS, SecretChange};
use ferrofed_registry::secret::WriteOnly;

use crate::configure::model::{SecretInput, SettingInput};
use crate::configure::plan::{Refused, at, given, invalid};
use crate::configure::settings::fields::TLS;
use crate::configure::settings::{Field, Grant, Kind};
use crate::locale::{Msg, tr, tr_with};

/// The settings `text` holds, one inline table, or none for an empty text.
///
/// # Errors
/// Returns a [`Refused`] when the gateway's settings do not read as TOML.
pub fn held(text: Option<&str>) -> Result<toml::Table, Refused> {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(toml::Table::new());
    };
    let wrapped: Result<toml::Table, _> = toml::from_str(&tr_with(
        Msg::Settings4,
        &[("text", (text).to_string().into())],
    ));
    match wrapped.map(|mut table| table.remove("settings")) {
        Ok(Some(toml::Value::Table(table))) => Ok(table),
        _ => Err(Refused::from(invalid(&tr(
            Msg::GatewaySSettingsResourceDoNot,
        )))),
    }
}

/// The value at the dotted `key` of `table`, if it holds one.
#[must_use]
pub fn lookup<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut parts = key.split('.');
    let first = parts.next()?;
    let mut value = table.get(first)?;
    for part in parts {
        value = value.as_table()?.get(part)?;
    }
    Some(value)
}

/// The value at `key` of `table` as a field shows it.
#[must_use]
pub fn shown(table: &toml::Table, key: &str) -> String {
    match lookup(table, key) {
        Some(toml::Value::String(text)) => text.clone(),
        Some(toml::Value::Boolean(on)) => on.to_string(),
        Some(toml::Value::Integer(number)) => number.to_string(),
        Some(toml::Value::Float(number)) => number.to_string(),
        Some(toml::Value::Datetime(at)) => at.to_string(),
        Some(toml::Value::Array(_) | toml::Value::Table(_)) | None => String::new(),
    }
}

/// The text values of the list at `key` of `table`.
#[must_use]
pub fn listed(table: &toml::Table, key: &str) -> Vec<String> {
    match lookup(table, key) {
        Some(toml::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether `part` is a key segment the forms take: letters, digits, `_`
/// and `-`.
fn segment(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-'))
}

/// Sets `value` at the dotted `key` of `table`, or removes the key for
/// `None`, making the tables on its way.
fn place(table: &mut toml::Table, key: &str, value: Option<toml::Value>) {
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(last) = parts.pop() else {
        return;
    };
    let mut at = table;
    for part in parts {
        let entry = at
            .entry(part.to_owned())
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        if !entry.is_table() {
            *entry = toml::Value::Table(toml::Table::new());
        }
        let toml::Value::Table(inner) = entry else {
            return;
        };
        at = inner;
    }
    match value {
        Some(value) => {
            at.insert(last.to_owned(), value);
        }
        None => {
            at.remove(last);
        }
    }
}

/// Removes every table of `table` left empty.
fn pruned(table: &mut toml::Table) {
    for (_, value) in table.iter_mut() {
        if let toml::Value::Table(inner) = value {
            pruned(inner);
        }
    }
    let empty: Vec<String> = table
        .iter()
        .filter(|(_, value)| matches!(value, toml::Value::Table(inner) if inner.is_empty()))
        .map(|(key, _)| key.clone())
        .collect();
    for key in empty {
        table.remove(&key);
    }
}

/// Returns the settings `rows` give a credential for `grant`.
///
/// They are built on the held settings `held`: the other grants' settings
/// left out, each field of `grant` and of the TLS material set or, left
/// empty, removed, and every other key kept.
///
/// # Errors
/// Returns a [`Refused`] for a row the grant does not show.
pub fn credentials(
    held: &toml::Table,
    grant: Grant,
    rows: &[SettingInput],
) -> Result<String, Refused> {
    let mut table = held.clone();
    for other in Grant::ALL.into_iter().filter(|other| *other != grant) {
        for key in other.own() {
            table.remove(*key);
        }
    }
    let fields: Vec<&Field> = grant.fields().iter().chain(TLS).collect();
    for row in rows {
        let Some(field) = fields.iter().find(|field| field.key == row.key) else {
            if Grant::ALL
                .iter()
                .any(|other| other.fields().iter().any(|field| field.key == row.key))
            {
                continue;
            }
            return Err(Refused::from(invalid(&tr(
                Msg::FormNamesSettingGrantDoesNot,
            ))));
        };
        place(&mut table, field.key, value_of(field.kind, row));
    }
    pruned(&mut table);
    Ok(toml::Value::Table(table).to_string())
}

/// The value `row` gives a field of `kind`, `None` when it is left empty.
fn value_of(kind: Kind, row: &SettingInput) -> Option<toml::Value> {
    match kind {
        Kind::Switch => (row.value.trim() == "true").then_some(toml::Value::Boolean(true)),
        Kind::List => {
            let values: Vec<toml::Value> = row
                .values
                .iter()
                .filter_map(|value| given(value).map(toml::Value::from))
                .collect();
            (!values.is_empty()).then_some(toml::Value::Array(values))
        }
        Kind::Text | Kind::Url | Kind::Path | Kind::Choice(_) => {
            given(&row.value).map(toml::Value::from)
        }
    }
}

/// The scalar settings `table` holds, by dotted key, in key order: what a
/// binding section's form shows a field each for. A list or a list of
/// tables is kept as it is held and not shown.
#[must_use]
pub fn scalars(table: &toml::Table) -> Vec<(String, toml::Value)> {
    let mut found = Vec::new();
    gather(table, "", &mut found);
    found
}

/// Adds the scalar settings of `table` under `prefix` to `found`.
fn gather(table: &toml::Table, prefix: &str, found: &mut Vec<(String, toml::Value)>) {
    for (key, value) in table {
        let dotted = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(inner) => gather(inner, &dotted, found),
            toml::Value::Array(_) => {}
            scalar => found.push((dotted, scalar.clone())),
        }
    }
}

/// The id on the page of the value field of setting row `row`.
#[must_use]
pub fn setting_field(row: &str, part: &str) -> String {
    format!("cfg-s-{row}-{part}")
}

/// The id on the page of the field `part` of secret row `row`.
#[must_use]
pub fn secret_field(row: &str, part: &str) -> String {
    format!("cfg-x-{row}-{part}")
}

/// Returns the settings `rows` give a binding section.
///
/// They are built on the held settings `held`: every scalar the form showed is set from its row, one whose row
/// was removed is removed, a new row adds its key, and every list is kept.
///
/// A value keeps the type the held setting has; a new one is a boolean for
/// `true` or `false`, a whole number for one, and text otherwise.
///
/// # Errors
/// Returns a [`Refused`] at the field of a key that is not dotted letters,
/// digits, `_` and `-`, a key named twice, or a value that does not read
/// as its held type.
pub fn binding(held: &toml::Table, rows: &[SettingInput]) -> Result<String, Refused> {
    let mut table = held.clone();
    let kept = scalars(held);
    for (key, _) in &kept {
        place(&mut table, key, None);
    }
    let mut named: Vec<&str> = Vec::new();
    for row in rows {
        let key = row.key.trim();
        if key.is_empty() && row.value.trim().is_empty() {
            continue;
        }
        if !key.split('.').all(segment) {
            return Err(at(
                setting_field(&row.row, "key"),
                tr(Msg::SettingSKeyLettersDigitsParts),
            ));
        }
        if named.contains(&key) {
            return Err(at(
                setting_field(&row.row, "key"),
                tr_with(Msg::SettingNamedTwice, &[("key", (key).to_string().into())]),
            ));
        }
        if is_secret(key) {
            return Err(at(
                setting_field(&row.row, "key"),
                tr_with(
                    Msg::SecretSetUnderSecrets,
                    &[("key", (key).to_string().into())],
                ),
            ));
        }
        named.push(key);
        let was = kept
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, value)| value);
        let value = typed(row.value.trim(), was).ok_or_else(|| {
            at(
                setting_field(&row.row, "value"),
                tr_with(
                    Msg::ValueDoesNotReadSettingS,
                    &[("key", (key).to_string().into())],
                ),
            )
        })?;
        place(&mut table, key, Some(value));
    }
    pruned(&mut table);
    Ok(toml::Value::Table(table).to_string())
}

/// The value `text` stands for, as the type of `was`, or as its own for a
/// new setting.
fn typed(text: &str, was: Option<&toml::Value>) -> Option<toml::Value> {
    match was {
        Some(toml::Value::Boolean(_)) => text.parse::<bool>().ok().map(toml::Value::Boolean),
        Some(toml::Value::Integer(_)) => text.parse::<i64>().ok().map(toml::Value::Integer),
        Some(toml::Value::Float(_)) => text.parse::<f64>().ok().map(toml::Value::Float),
        Some(_) => Some(toml::Value::from(text)),
        None => Some(
            text.parse::<bool>()
                .map(toml::Value::Boolean)
                .or_else(|_| text.parse::<i64>().map(toml::Value::Integer))
                .unwrap_or_else(|_| toml::Value::from(text)),
        ),
    }
}

/// Whether the dotted `key` names a secret or its reference.
#[must_use]
pub fn is_secret(key: &str) -> bool {
    let last = key.rsplit('.').next().unwrap_or(key);
    let last = last.strip_suffix("_file").unwrap_or(last);
    SECRETS.contains(&last)
}

/// Returns the secret changes `inputs` ask for.
///
/// Each `keep` is left out, each `set`
/// carried in a type that never renders it, each `file` by its path and
/// each `clear` as one, and `cleared`, the held secrets the chosen grant no
/// longer uses, cleared.
///
/// # Errors
/// Returns a [`Refused`] at the field of a secret set with no value, read
/// from no file or from a relative one, or a choice the form does not
/// offer.
pub fn secret_changes(
    inputs: &mut [SecretInput],
    cleared: &[String],
) -> Result<Vec<SecretChange>, Refused> {
    let mut changes = Vec::new();
    for input in inputs.iter_mut() {
        let key = input.key.trim().to_owned();
        let untouched = input.value.is_empty() && input.file.trim().is_empty();
        if key.is_empty() && (input.choice == "keep" || untouched) {
            continue;
        }
        if !key.split('.').all(segment) || !is_secret(&key) {
            return Err(at(
                secret_field(&input.row, "key"),
                tr(Msg::NameSecretResourceTakesSuchOauth2),
            ));
        }
        let change = match input.choice.as_str() {
            "keep" | "" => continue,
            "set" => {
                let typed = std::mem::take(&mut input.value);
                if typed.is_empty() {
                    return Err(at(
                        secret_field(&input.row, "value"),
                        tr_with(Msg::TypeValueChooseKeep, &[("key", (key).clone().into())]),
                    ));
                }
                SecretChange {
                    key,
                    value: Some(WriteOnly::from(typed.into_secret())),
                    file: None,
                    clear: false,
                }
            }
            "file" => {
                let file = given(&input.file).ok_or_else(|| {
                    at(
                        secret_field(&input.row, "file"),
                        tr_with(
                            Msg::GiveAbsolutePathFileReadFrom,
                            &[("key", (key).clone().into())],
                        ),
                    )
                })?;
                if !std::path::Path::new(file).is_absolute() {
                    return Err(at(
                        secret_field(&input.row, "file"),
                        tr_with(
                            Msg::FileReadFromNotAbsolutePath,
                            &[("key", (key).clone().into())],
                        ),
                    ));
                }
                SecretChange {
                    key,
                    value: None,
                    file: Some(file.to_owned()),
                    clear: false,
                }
            }
            "clear" => SecretChange {
                key,
                value: None,
                file: None,
                clear: true,
            },
            _ => {
                return Err(at(
                    secret_field(&input.row, "choice"),
                    tr(Msg::ChooseKeepSetValueReadFrom),
                ));
            }
        };
        changes.push(change);
    }
    for key in cleared {
        if !changes.iter().any(|change| &change.key == key) {
            changes.push(SecretChange {
                key: key.clone(),
                value: None,
                file: None,
                clear: true,
            });
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::model::Typed;

    /// A setting row of `key` holding `value`.
    fn row(key: &str, value: &str) -> SettingInput {
        SettingInput {
            row: String::from("0"),
            key: key.to_owned(),
            value: value.to_owned(),
            values: Vec::new(),
        }
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_credential_keeps_what_its_form_does_not_show_and_drops_the_other_grants()
    -> Result<(), Refused> {
        let held = held(Some(
            r#"{ user = "old", trust_roots_file = "/run/roots.pem", oauth2 = { client_id = "a", extra = 1 } }"#,
        ))?;
        assert_eq!(Grant::OAuth2, Grant::held(&held, &[]));
        let text = credentials(
            &held,
            Grant::OAuth2,
            &[
                row("oauth2.client_id", "gateway"),
                row("oauth2.scope", "system/aql-*.s"),
                row("oauth2.tls_client_certificate_bound_access_tokens", ""),
                row("trust_roots_file", ""),
            ],
        )?;
        let built = super::held(Some(&text))?;
        assert_eq!("gateway", shown(&built, "oauth2.client_id"));
        assert_eq!("system/aql-*.s", shown(&built, "oauth2.scope"));
        assert_eq!("1", shown(&built, "oauth2.extra"));
        assert!(lookup(&built, "user").is_none(), "{text}");
        assert!(lookup(&built, "trust_roots_file").is_none(), "{text}");
        assert!(
            lookup(&built, "oauth2.tls_client_certificate_bound_access_tokens").is_none(),
            "{text}"
        );
        Ok(())
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_list_and_a_switch_are_written_as_their_types() -> Result<(), Refused> {
        let mut hosts = row("mtls_alias_hosts", "");
        hosts.values = vec![String::from("mtls.example.org"), String::new()];
        let text = credentials(
            &toml::Table::new(),
            Grant::Fapi2,
            &[
                hosts,
                row("fapi2.tls_client_certificate_bound_access_tokens", "true"),
            ],
        )?;
        let built = held(Some(&text))?;
        assert_eq!(
            vec![String::from("mtls.example.org")],
            listed(&built, "mtls_alias_hosts")
        );
        assert_eq!(
            Some(&toml::Value::Boolean(true)),
            lookup(&built, "fapi2.tls_client_certificate_bound_access_tokens")
        );
        Ok(())
    }

    #[test]
    #[expect(clippy::panic_in_result_fn, reason = "test assertions")]
    fn a_binding_keeps_its_held_types_and_lists_and_reads_new_values() -> Result<(), Refused> {
        let held = held(Some(
            r#"{ timeout_ms = 2000, base = "https://pix.example.org", hosts = ["a"], oauth2 = { scope = "s" } }"#,
        ))?;
        let rows = [
            row("timeout_ms", "2500"),
            row("oauth2.scope", "t"),
            row("best_effort", "true"),
            row("budget", "12"),
        ];
        let built = held_after(&held, &rows)?;
        assert_eq!(
            Some(&toml::Value::Integer(2500)),
            lookup(&built, "timeout_ms")
        );
        assert!(
            lookup(&built, "base").is_none(),
            "a removed row removes its key"
        );
        assert_eq!(vec![String::from("a")], listed(&built, "hosts"));
        assert_eq!(
            Some(&toml::Value::Boolean(true)),
            lookup(&built, "best_effort")
        );
        assert_eq!(Some(&toml::Value::Integer(12)), lookup(&built, "budget"));
        let refused = binding(&held, &[row("timeout_ms", "soon")]).err();
        assert_eq!(
            Some(String::from("cfg-s-0-value")),
            refused.and_then(|refused| refused.field)
        );
        let refused = binding(&held, &[row("oauth2.client_secret", "x")]).err();
        assert_eq!(
            Some(String::from("cfg-s-0-key")),
            refused.and_then(|refused| refused.field)
        );
        Ok(())
    }

    /// The settings `rows` give a binding on `held`, read back.
    fn held_after(held: &toml::Table, rows: &[SettingInput]) -> Result<toml::Table, Refused> {
        super::held(Some(&binding(held, rows)?))
    }

    /// A secret input of `key` with `choice`.
    fn secret(key: &str, choice: &str) -> SecretInput {
        SecretInput {
            row: String::from("1"),
            key: key.to_owned(),
            choice: choice.to_owned(),
            value: Typed::default(),
            file: String::new(),
        }
    }

    #[test]
    fn each_secret_choice_becomes_its_change_and_a_kept_one_none() {
        let mut set = secret("oauth2.client_secret", "set");
        set.value = Typed::new(String::from("synthetic-value"));
        let mut file = secret("client_identity", "file");
        file.file = String::from("/run/secrets/id.pem");
        let mut inputs = [
            secret("oauth.client_secret", "keep"),
            set,
            file,
            secret("password", "clear"),
        ];
        let changes = secret_changes(&mut inputs, &[String::from("oauth.client_secret")]);
        let Ok(changes) = changes else {
            panic!("the choices should read");
        };
        let keys: Vec<(&str, bool, bool, bool)> = changes
            .iter()
            .map(|change| {
                (
                    change.key.as_str(),
                    change.value.is_some(),
                    change.file.is_some(),
                    change.clear,
                )
            })
            .collect();
        assert_eq!(
            vec![
                ("oauth2.client_secret", true, false, false),
                ("client_identity", false, true, false),
                ("password", false, false, true),
                ("oauth.client_secret", false, false, true),
            ],
            keys
        );
        assert!(
            inputs[1].value.is_empty(),
            "the value moved into the change"
        );
        assert!(!format!("{changes:?}").contains("synthetic-value"));
    }

    #[test]
    fn a_secret_set_with_nothing_or_from_a_relative_file_is_refused_at_its_field() {
        let mut inputs = [secret("password", "set")];
        assert_eq!(
            Some(String::from("cfg-x-1-value")),
            secret_changes(&mut inputs, &[])
                .err()
                .and_then(|refused| refused.field)
        );
        let mut relative = secret("password", "file");
        relative.file = String::from("secrets/p");
        assert_eq!(
            Some(String::from("cfg-x-1-file")),
            secret_changes(&mut [relative], &[])
                .err()
                .and_then(|refused| refused.field)
        );
        assert_eq!(
            Some(String::from("cfg-x-1-key")),
            secret_changes(&mut [secret("user", "set")], &[])
                .err()
                .and_then(|refused| refused.field)
        );
    }
}
