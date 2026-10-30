// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A form drawn again as the operator posted it.
//!
//! A form's Add and Remove buttons submit the form with `form[edit]`
//! naming the row they add or remove, so the rows grow and shrink with the
//! scripts refused (FerroUI F7): the page answers with the form as it
//! was posted, the one row changed, and runs nothing. A change no replica
//! was asked for, or one a replica refused, is drawn again the same way,
//! with the field a refusal names marked. A secret's value is never drawn
//! again. No specification governs this: our own design.

use ferrofed_registry::console::ConsoleMember;
use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};

use crate::configure::model::{ConfigForm, FhirEndpointInput, QueryEndpointInput, SettingInput};
use crate::configure::said::FieldNote;

/// The field the Add and Remove buttons of a form submit as.
pub const EDIT: &str = "form[edit]";

/// A form as the operator posted it, and the field a refusal names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    /// The form as it was posted, its rows changed by Add or Remove.
    pub form: ConfigForm,
    /// The field a refusal of the form names.
    pub field: Option<FieldNote>,
}

/// One edit of a form's rows, as `form[edit]` names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    /// Add a Query API endpoint row.
    AddQuery,
    /// Remove the Query API endpoint row of this key.
    RemoveQuery(String),
    /// Add a FHIR endpoint row.
    AddFhir,
    /// Remove the FHIR endpoint row of this key.
    RemoveFhir(String),
    /// Add a profile to the FHIR endpoint row of this key.
    AddProfile(String),
    /// Remove the profile at this place of the FHIR endpoint row of this
    /// key.
    RemoveProfile(String, usize),
    /// Add a value to the list setting of this row.
    AddValue(String),
    /// Remove the value at this place of the list setting of this row.
    RemoveValue(String, usize),
    /// Add a setting row.
    AddSetting,
    /// Remove the setting row of this key.
    RemoveSetting(String),
    /// Add a new secret row.
    AddSecret,
    /// Remove the secret row of this key.
    RemoveSecret(String),
}

impl Edit {
    /// The edit `text` names, `None` for one the console does not know.
    #[must_use]
    pub fn of(text: &str) -> Option<Self> {
        let mut parts = text.splitn(3, ':');
        let verb = parts.next()?;
        let key = parts.next().map(str::to_owned);
        let at = parts.next().and_then(|at| at.parse::<usize>().ok());
        Some(match (verb, key, at) {
            ("add-query", None, None) => Self::AddQuery,
            ("remove-query", Some(key), None) => Self::RemoveQuery(key),
            ("add-fhir", None, None) => Self::AddFhir,
            ("remove-fhir", Some(key), None) => Self::RemoveFhir(key),
            ("add-profile", Some(key), None) => Self::AddProfile(key),
            ("remove-profile", Some(key), Some(at)) => Self::RemoveProfile(key, at),
            ("add-value", Some(row), None) => Self::AddValue(row),
            ("remove-value", Some(row), Some(at)) => Self::RemoveValue(row, at),
            ("add-setting", None, None) => Self::AddSetting,
            ("remove-setting", Some(row), None) => Self::RemoveSetting(row),
            ("add-secret", None, None) => Self::AddSecret,
            ("remove-secret", Some(row), None) => Self::RemoveSecret(row),
            _ => return None,
        })
    }

    /// The value of `form[edit]` that names this edit.
    #[must_use]
    pub fn value(&self) -> String {
        match self {
            Self::AddQuery => String::from("add-query"),
            Self::RemoveQuery(key) => format!("remove-query:{key}"),
            Self::AddFhir => String::from("add-fhir"),
            Self::RemoveFhir(key) => format!("remove-fhir:{key}"),
            Self::AddProfile(key) => format!("add-profile:{key}"),
            Self::RemoveProfile(key, at) => format!("remove-profile:{key}:{at}"),
            Self::AddValue(row) => format!("add-value:{row}"),
            Self::RemoveValue(row, at) => format!("remove-value:{row}:{at}"),
            Self::AddSetting => String::from("add-setting"),
            Self::RemoveSetting(row) => format!("remove-setting:{row}"),
            Self::AddSecret => String::from("add-secret"),
            Self::RemoveSecret(row) => format!("remove-secret:{row}"),
        }
    }

    /// Applies the edit to `form`. A row or a place the form does not hold
    /// changes nothing.
    pub fn apply(&self, form: &mut ConfigForm) {
        match self {
            Self::AddQuery => {
                let key = fresh(form.query_endpoints.iter().map(|row| row.key.as_str()), "q");
                form.query_endpoints.push(QueryEndpointInput {
                    key,
                    in_service: true,
                    ..QueryEndpointInput::default()
                });
            }
            Self::RemoveQuery(key) => form.query_endpoints.retain(|row| &row.key != key),
            Self::AddFhir => {
                let key = fresh(form.fhir_endpoints.iter().map(|row| row.key.as_str()), "f");
                form.fhir_endpoints.push(FhirEndpointInput {
                    key,
                    in_service: true,
                    ..FhirEndpointInput::default()
                });
            }
            Self::RemoveFhir(key) => form.fhir_endpoints.retain(|row| &row.key != key),
            Self::AddProfile(key) => {
                if let Some(row) = form.fhir_endpoints.iter_mut().find(|row| &row.key == key) {
                    row.profiles.push(String::new());
                }
            }
            Self::RemoveProfile(key, at) => {
                if let Some(row) = form.fhir_endpoints.iter_mut().find(|row| &row.key == key)
                    && *at < row.profiles.len()
                {
                    row.profiles.remove(*at);
                }
            }
            Self::AddValue(row) => {
                if let Some(setting) = form.settings.iter_mut().find(|setting| &setting.row == row)
                {
                    setting.values.push(String::new());
                }
            }
            Self::RemoveValue(row, at) => {
                if let Some(setting) = form.settings.iter_mut().find(|setting| &setting.row == row)
                    && *at < setting.values.len()
                {
                    setting.values.remove(*at);
                }
            }
            Self::AddSetting => {
                let row = fresh(form.settings.iter().map(|row| row.row.as_str()), "n");
                form.settings.push(SettingInput {
                    row,
                    ..SettingInput::default()
                });
            }
            Self::RemoveSetting(row) => form.settings.retain(|setting| &setting.row != row),
            Self::AddSecret => {
                let row = fresh(form.secrets.iter().map(|row| row.row.as_str()), "n");
                form.secrets.push(crate::configure::model::SecretInput {
                    row,
                    choice: String::from("set"),
                    ..crate::configure::model::SecretInput::default()
                });
            }
            Self::RemoveSecret(row) => form.secrets.retain(|secret| &secret.row != row),
        }
    }
}

/// The first key `{prefix}1`, `{prefix}2` and so on that none of `held`
/// is; a list of `n` rows holds no more than `n` keys, so one of the first
/// `n + 1` is free.
fn fresh<'a>(held: impl Iterator<Item = &'a str>, prefix: &str) -> String {
    let held: Vec<&str> = held.collect();
    let bound = u32::try_from(held.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    (1..=bound)
        .map(|at| format!("{prefix}{at}"))
        .find(|key| !held.contains(&key.as_str()))
        .unwrap_or_default()
}

/// The member form for `member`, as the page first draws it: every
/// endpoint a row, and one empty Query API row for a new member.
#[must_use]
pub fn member_form(member: Option<&ConsoleMember>, base: String) -> ConfigForm {
    let Some(member) = member else {
        return ConfigForm {
            action: String::from("add-member"),
            base,
            query_endpoints: vec![QueryEndpointInput {
                key: String::from("0"),
                in_service: true,
                ..QueryEndpointInput::default()
            }],
            ..ConfigForm::default()
        };
    };
    let of_kind = |kind: ConnectionType| {
        member
            .endpoints
            .iter()
            .filter(move |endpoint| endpoint.connection_type == kind)
            .zip(0_u32..)
    };
    ConfigForm {
        action: String::from("save-member"),
        base,
        organisation: member.organisation.id.as_str().to_owned(),
        organisation_name: member.organisation.name.clone().unwrap_or_default(),
        node: member.node.id.as_str().to_owned(),
        system_id: member.node.system_id.as_str().to_owned(),
        product: member.node.product.clone().unwrap_or_default(),
        product_version: member.node.version.clone().unwrap_or_default(),
        query_endpoints: of_kind(ConnectionType::OpenehrRestQuery)
            .map(|(endpoint, key)| QueryEndpointInput {
                key: key.to_string(),
                id: endpoint.id.as_str().to_owned(),
                url: endpoint.url.clone(),
                in_service: endpoint.status != EndpointStatus::Suspended,
            })
            .collect(),
        fhir_endpoints: of_kind(ConnectionType::Hl7FhirRest)
            .map(|(endpoint, key)| FhirEndpointInput {
                key: key.to_string(),
                id: endpoint.id.as_str().to_owned(),
                url: endpoint.url.clone(),
                system: endpoint.ehr_id_system.clone().unwrap_or_default(),
                categories: endpoint.categories.clone(),
                profiles: endpoint.profiles.clone(),
                in_service: endpoint.status != EndpointStatus::Suspended,
            })
            .collect(),
        fhir_shown: String::from("yes"),
        ..ConfigForm::default()
    }
}

#[cfg(test)]
mod tests {
    use super::Edit;
    use crate::configure::model::{ConfigForm, FhirEndpointInput};

    #[test]
    fn each_edit_reads_back_from_the_value_it_submits() {
        for edit in [
            Edit::AddQuery,
            Edit::RemoveQuery(String::from("0")),
            Edit::AddFhir,
            Edit::RemoveFhir(String::from("f1")),
            Edit::AddProfile(String::from("0")),
            Edit::RemoveProfile(String::from("0"), 2),
            Edit::AddValue(String::from("c3")),
            Edit::RemoveValue(String::from("c3"), 0),
            Edit::AddSetting,
            Edit::RemoveSetting(String::from("s0")),
            Edit::AddSecret,
            Edit::RemoveSecret(String::from("n1")),
        ] {
            assert_eq!(Some(&edit), Edit::of(&edit.value()).as_ref(), "{edit:?}");
        }
        for unknown in ["", "drop-table", "remove-profile:0:x", "add-query:0"] {
            assert_eq!(None, Edit::of(unknown), "{unknown}");
        }
    }

    #[test]
    fn an_added_row_takes_a_fresh_key_and_a_removal_takes_one_place() {
        let mut form = ConfigForm::default();
        Edit::AddFhir.apply(&mut form);
        Edit::AddFhir.apply(&mut form);
        let keys: Vec<&str> = form
            .fhir_endpoints
            .iter()
            .map(|row| row.key.as_str())
            .collect();
        assert_eq!(vec!["f1", "f2"], keys);
        form.fhir_endpoints = vec![FhirEndpointInput {
            key: String::from("0"),
            profiles: vec![String::from("a"), String::from("b"), String::from("c")],
            ..FhirEndpointInput::default()
        }];
        Edit::RemoveProfile(String::from("0"), 1).apply(&mut form);
        Edit::RemoveProfile(String::from("0"), 9).apply(&mut form);
        assert_eq!(vec!["a", "c"], form.fhir_endpoints[0].profiles);
    }
}
