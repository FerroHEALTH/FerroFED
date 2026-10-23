// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a configuration form asks for, read and checked.
//!
//! The change, its confirmation and each field are checked before any
//! replica is asked, a refusal naming the field it is about by its id on
//! the page and never what it holds. No specification governs this: our
//! own design.

use std::fmt;

use ferrofed_registry::console::ConsoleMember;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::resource::{BindingSection, Resource, SecretChange};
use ferrofed_registry::revision::RevisionId;

use crate::configure::member::Shown;
use crate::configure::model::{ConfigForm, SettingInput};
use crate::configure::settings::{self, Grant};
use crate::locale::{Msg, tr, tr_with};
use crate::read::Refusal;

/// The longest reason a revision records, in characters, as the
/// gateway holds it.
const REASON_LIMIT: usize = 1_000;

/// A form the console refuses before any replica is asked: the refusal,
/// and the id of the field it is about when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// The refusal.
    pub refusal: Refusal,
    /// The id on the page of the field it is about.
    pub field: Option<String>,
}

impl From<Refusal> for Refused {
    fn from(refusal: Refusal) -> Self {
        Self {
            refusal,
            field: None,
        }
    }
}

/// The refusal of the field `field` saying `reason`, never what the
/// field holds.
pub(crate) fn at(field: impl Into<String>, reason: impl Into<String>) -> Refused {
    Refused {
        refusal: Refusal::Invalid {
            reason: reason.into(),
        },
        field: Some(field.into()),
    }
}

/// What a section form saves: the grant's fields of a credential, or
/// the rows of a binding section.
#[derive(Clone, PartialEq, Eq)]
pub enum Settings {
    /// A credential's grant and the settings its form shows.
    Credentials(Grant, Vec<SettingInput>),
    /// A binding section's settings, a row each.
    Binding(Vec<SettingInput>),
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(grant, rows) => f
                .debug_tuple(&tr(Msg::Credentials))
                .field(grant)
                .field(&rows.len())
                .finish(),
            Self::Binding(rows) => f.debug_tuple("Binding").field(&rows.len()).finish(),
        }
    }
}

/// What a form asks for, read and checked.
///
/// Its `Debug` output names the action and the revisions alone, never a
/// reason, a member, a setting or an AQL text.
#[derive(Clone, PartialEq, Eq)]
pub enum Plan {
    /// Save a revision with `member` in place of the console's member
    /// of its `node_id`.
    SaveMember {
        /// The revision the change is based on.
        base: Option<RevisionId>,
        /// Why.
        reason: String,
        /// The member as the form gives it.
        member: Box<ConsoleMember>,
        /// How the member form was drawn.
        shown: Shown,
    },
    /// Save a revision without the console's member `node`.
    RemoveMember {
        /// The revision the change is based on.
        base: Option<RevisionId>,
        /// Why.
        reason: String,
        /// The member's `node_id`.
        node: String,
    },
    /// Apply revision `revision`.
    Apply {
        /// The revision.
        revision: RevisionId,
    },
    /// Roll the running revision `current` back.
    Rollback {
        /// The running revision the operator read.
        current: RevisionId,
        /// Why.
        reason: String,
    },
    /// Store a stored-query version.
    StoreQuery {
        /// The qualified name.
        name: String,
        /// The version.
        version: String,
        /// The AQL.
        aql: String,
    },
    /// Save a revision with a credential or a binding section.
    SaveSection {
        /// The credential or binding section.
        resource: Resource,
        /// The revision the change is based on.
        base: Option<RevisionId>,
        /// Why.
        reason: String,
        /// Its settings as the form gives them.
        settings: Settings,
        /// The changes to its secrets, a secret value among them.
        secrets: Vec<SecretChange>,
    },
    /// Save a revision without a credential or a binding section.
    RemoveSection {
        /// The revision the change is based on.
        base: Option<RevisionId>,
        /// Why.
        reason: String,
        /// The credential or binding section.
        resource: Resource,
    },
    /// Confirm the running revision `revision`.
    Confirm {
        /// The revision.
        revision: RevisionId,
    },
    /// Approve revision `revision` as an operator other than its
    /// author.
    Approve {
        /// The revision.
        revision: RevisionId,
    },
    /// Save `toml` as a dry-run revision.
    Import {
        /// The revision the import is based on.
        base: Option<RevisionId>,
        /// Why.
        reason: String,
        /// The configuration as TOML.
        toml: String,
    },
}

impl fmt::Debug for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SaveMember { base, .. } => f
                .debug_struct("SaveMember")
                .field("base", base)
                .finish_non_exhaustive(),
            Self::RemoveMember { base, .. } => f
                .debug_struct("RemoveMember")
                .field("base", base)
                .finish_non_exhaustive(),
            Self::Apply { revision } => {
                f.debug_struct("Apply").field("revision", revision).finish()
            }
            Self::Rollback { current, .. } => f
                .debug_struct("Rollback")
                .field("current", current)
                .finish_non_exhaustive(),
            Self::StoreQuery { .. } => f.debug_struct("StoreQuery").finish_non_exhaustive(),
            Self::SaveSection { resource, base, .. } => f
                .debug_struct("SaveSection")
                .field("resource", resource)
                .field("base", base)
                .finish_non_exhaustive(),
            Self::RemoveSection { base, resource, .. } => f
                .debug_struct("RemoveSection")
                .field("resource", resource)
                .field("base", base)
                .finish_non_exhaustive(),
            Self::Confirm { revision } => f
                .debug_struct("Confirm")
                .field("revision", revision)
                .finish(),
            Self::Approve { revision } => f
                .debug_struct("Approve")
                .field("revision", revision)
                .finish(),
            Self::Import { base, .. } => f
                .debug_struct("Import")
                .field("base", base)
                .finish_non_exhaustive(),
        }
    }
}

/// A refusal of the form, naming the field and never its value.
pub(crate) fn invalid(reason: &str) -> Refusal {
    Refusal::Invalid {
        reason: reason.to_owned(),
    }
}

/// The text of `field`, trimmed, or `None` when it is empty.
pub(crate) fn given(field: &str) -> Option<&str> {
    let text = field.trim();
    (!text.is_empty()).then_some(text)
}

/// The revision number `field` holds, `None` for an empty field.
fn revision(field: &str, what: &str) -> Result<Option<RevisionId>, Refused> {
    given(field)
        .map(|text| {
            text.parse::<RevisionId>().map_err(|_not_a_number| {
                Refused::from(Refusal::Invalid {
                    reason: tr_with(
                        Msg::NotRevisionNumber,
                        &[("what", (what).to_string().into())],
                    ),
                })
            })
        })
        .transpose()
}

/// The revision number `field` holds, which the form must name.
fn named(field: &str, what: &str) -> Result<RevisionId, Refused> {
    revision(field, what)?.ok_or_else(|| {
        Refused::from(Refusal::Invalid {
            reason: tr_with(Msg::NameThe, &[("what", (what).to_string().into())]),
        })
    })
}

/// The id on the page of the reason field of every form.
pub const REASON: &str = "cfg-reason";

/// The reason the form gives, 1 to the limit of characters.
fn reason(form: &ConfigForm) -> Result<String, Refused> {
    let field = reason_field(form);
    let reason = given(&form.reason).ok_or_else(|| at(field.clone(), tr(Msg::GiveReasonChange)))?;
    if reason.chars().count() > REASON_LIMIT {
        return Err(at(field, tr(Msg::ReasonLongerThan1000Characters)));
    }
    Ok(reason.to_owned())
}

/// The id on the page of the reason field of the form `form`: a
/// removal's and a rollback's sit in their confirmation step.
fn reason_field(form: &ConfigForm) -> String {
    match form.action.as_str() {
        "rollback" => String::from(crate::configure::render::ROLLBACK_REASON),
        "remove-member" => removal_reason(form.node.trim()),
        "remove-section" => removal_reason(form.resource.trim()),
        _ => String::from(REASON),
    }
}

/// The id on the page of the reason field of the removal of `named`, a
/// member's `node_id` or a section as a removal names it.
#[must_use]
pub fn removal_reason(named: &str) -> String {
    let slug: String = named
        .chars()
        .map(|letter| {
            if letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_' | '.') {
                letter
            } else {
                '-'
            }
        })
        .collect();
    format!("cfg-remove-{slug}-reason")
}

/// The id on the page of the dialog that confirms the removal of `named`
/// from its row of a tab, one per row.
#[must_use]
pub fn removal_dialog(named: &str) -> String {
    let reason = removal_reason(named);
    format!("{}-dialog", reason.trim_end_matches("-reason"))
}

/// The actions that change the running gateway or the stored-query
/// store, which the operator confirms in a step of their own; a save
/// is not applied, and needs none.
const CONFIRMED: [&str; 7] = [
    "apply",
    "rollback",
    "confirm",
    "approve",
    "remove-member",
    "remove-section",
    "store-query",
];

/// What `form` asks for.
///
/// # Errors
/// Returns a [`Refused`] for an action the operator did not confirm, an
/// action the page does not offer, or a field that does not read,
/// naming the field and never what it holds.
pub fn plan(form: &mut ConfigForm) -> Result<Plan, Refused> {
    if CONFIRMED.contains(&form.action.as_str()) && given(&form.confirm).is_none() {
        return Err(Refused::from(invalid(&tr(
            Msg::ConfirmChangeConfirmationStepRun,
        ))));
    }
    match form.action.as_str() {
        "add-member" | "save-member" => {
            let (member, shown) = crate::configure::member::member(form)?;
            Ok(Plan::SaveMember {
                base: revision(&form.base, &tr(Msg::BaseRevision))?,
                reason: reason(form)?,
                member: Box::new(member),
                shown,
            })
        }
        "remove-member" => Ok(Plan::RemoveMember {
            base: revision(&form.base, &tr(Msg::BaseRevision))?,
            reason: reason(form)?,
            node: given(&form.node)
                .ok_or_else(|| Refused::from(invalid(&tr(Msg::ChooseConsoleMemberRemove))))?
                .to_owned(),
        }),
        "apply" => Ok(Plan::Apply {
            revision: named(&form.revision, &tr(Msg::RevisionToApply))?,
        }),
        "rollback" => Ok(Plan::Rollback {
            current: named(&form.current, &tr(Msg::RunningRevisionRollBack))?,
            reason: reason(form)?,
        }),
        "store-query" => Ok(Plan::StoreQuery {
            name: given(&form.name)
                .ok_or_else(|| at("cfg-name", tr(Msg::NameTheStoredQuery)))?
                .to_owned(),
            version: given(&form.version)
                .ok_or_else(|| at("cfg-version", tr(Msg::NameVersionStore)))?
                .to_owned(),
            aql: given(&form.aql)
                .ok_or_else(|| at("cfg-aql", tr(Msg::GiveAqlStore)))?
                .to_owned(),
        }),
        "save-credentials" => {
            let endpoint = given(&form.endpoint)
                .ok_or_else(|| at("cfg-endpoint", tr(Msg::ChooseEndpointCredentials)))?;
            EndpointId::new(endpoint)
                .map_err(|_bad| at("cfg-endpoint", tr(Msg::EndpointIdNot164Letters)))?;
            let grant = Grant::named(form.grant.trim())
                .ok_or_else(|| at("cfg-grant", tr(Msg::ChooseHowGatewayReachesEndpoint)))?;
            section_plan(form, Resource::Credentials(endpoint.to_owned()), |form| {
                Settings::Credentials(grant, std::mem::take(&mut form.settings))
            })
        }
        "save-binding" => {
            let section = BindingSection::named(form.section.trim())
                .ok_or_else(|| at("cfg-section", tr(Msg::ChooseTheBindingSection)))?;
            section_plan(form, Resource::Binding(section), |form| {
                Settings::Binding(std::mem::take(&mut form.settings))
            })
        }
        "remove-section" => Ok(Plan::RemoveSection {
            base: revision(&form.base, &tr(Msg::BaseRevision))?,
            reason: reason(form)?,
            resource: chosen(&form.resource)?,
        }),
        "confirm" => Ok(Plan::Confirm {
            revision: named(&form.revision, &tr(Msg::RevisionToConfirm))?,
        }),
        "approve" => Ok(Plan::Approve {
            revision: named(&form.revision, &tr(Msg::RevisionToApprove))?,
        }),
        "import" => Ok(Plan::Import {
            base: revision(&form.base, &tr(Msg::BaseRevision))?,
            reason: reason(form)?,
            toml: toml(form)?,
        }),
        _ => Err(Refused::from(invalid(&tr(
            Msg::FormNamesNoActionPageOffers,
        )))),
    }
}

/// The save of `resource` the form gives, its settings read by
/// `settings` and its secrets moved out of the form.
fn section_plan(
    form: &mut ConfigForm,
    resource: Resource,
    settings: impl FnOnce(&mut ConfigForm) -> Settings,
) -> Result<Plan, Refused> {
    let base = revision(&form.base, &tr(Msg::BaseRevision))?;
    let reason = reason(form)?;
    let secrets = settings::build::secret_changes(&mut form.secrets, &[])?;
    Ok(Plan::SaveSection {
        resource,
        base,
        reason,
        settings: settings(form),
        secrets,
    })
}

/// The TOML an import sends, not empty and within the gateway's limit.
fn toml(form: &mut ConfigForm) -> Result<String, Refused> {
    let text = std::mem::take(&mut form.toml);
    if text.trim().is_empty() {
        return Err(at("cfg-toml", tr(Msg::PasteTomlImport)));
    }
    if text.len() > crate::configure::import::IMPORT_LIMIT {
        return Err(at(
            "cfg-toml",
            tr_with(
                Msg::TomlLargerThanBytes,
                &[(
                    "value0",
                    (crate::configure::import::IMPORT_LIMIT).to_string().into(),
                )],
            ),
        ));
    }
    Ok(text)
}

/// The credential or binding section a removal names, as
/// [`crate::configure::sections::choice`] writes it.
fn chosen(field: &str) -> Result<Resource, Refused> {
    let refused = || Refused::from(invalid(&tr(Msg::ChooseCredentialBindingSectionRemove)));
    match given(field).and_then(|text| text.split_once(':')) {
        Some(("credentials", endpoint)) if EndpointId::new(endpoint).is_ok() => {
            Ok(Resource::Credentials(endpoint.to_owned()))
        }
        Some(("binding", section)) => BindingSection::named(section)
            .map(Resource::Binding)
            .ok_or_else(refused),
        _ => Err(refused()),
    }
}
