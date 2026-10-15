// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server functions of the configuration page: its load, and the
//! submission of each of its forms.
//!
//! Each is a public HTTP endpoint of the console, so each checks the
//! operator's session itself before anything else; a form's function then
//! checks that the request came from the hydrated page, then the form and
//! its confirmation, before any replica is asked (`server/25_server_functions`,
//! the Leptos book). The first replica configured is called with the
//! operator's own access token, which never leaves the server.

use leptos::prelude::*;

use crate::admin::model::ActionAnswer;
use crate::configure::model::{ConfigForm, ConfigView};
use crate::views::model::{Outcome, ViewError};

/// Loads the configuration page from the first replica's admin listener.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// console's fault as its [`ViewError`]; the gateway's refusal, and a
/// console configured with no admin listener, are an [`Outcome::Refused`].
#[server(endpoint = "configuration")]
pub async fn configuration() -> Result<Outcome<ConfigView>, ViewError> {
    crate::views::load::server::settled(async {
        let (state, token) = crate::views::load::server::signed_in()?;
        let replica = server::replica(&state.settings().admin)?;
        server::page(state.gateway(), replica, &token).await
    })
    .await
}

/// Runs the configuration form `form` names at the first replica, as the
/// operator.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, the console's
/// fault as its [`ViewError`], and [`ViewError::PlainPost`] for a plain
/// form post, before any replica is asked. A form that cannot be sent and a
/// replica's refusal are answers.
#[server(endpoint = "configure")]
pub async fn configure(
    /// The configuration form.
    form: ConfigForm,
) -> Result<ActionAnswer, ViewError> {
    let (state, token) = crate::views::load::server::signed_in()?;
    crate::query::run::server::from_the_hydrated_page()?;
    let planned = server::replica(&state.settings().admin)
        .and_then(|replica| server::plan(&form).map(|plan| (replica, plan)));
    let (replica, plan) = match planned {
        Ok(planned) => planned,
        Err(refused) => return Ok(crate::admin::answer::refused(&refused)),
    };
    Ok(server::run(state.gateway(), replica, &token, plan).await)
}

/// The server half of the configuration page: the form read into what is
/// sent, the gateway's answers read into what the page renders.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use std::fmt;

    use ferrofed_registry::console::{
        ConsoleEndpoint, ConsoleMember, ConsoleNode, ConsoleOrganisation,
    };
    use ferrofed_registry::id::{EndpointId, NodeId, OrganisationId, SystemId};
    use ferrofed_registry::operator::PageRequest;
    use ferrofed_registry::resource::{BindingSection, Resource, SecretChange, SectionRequest};
    use ferrofed_registry::revision::{Revision, RevisionId};
    use ferrofed_registry::secret::WriteOnly;
    use ferrofed_registry::snapshot::{ConnectionType, EndpointStatus};

    use crate::admin::act::server::acted;
    use crate::admin::model::ActionAnswer;
    use crate::config::settings::ReplicaSettings;
    use crate::configure::model::{ConfigForm, ConfigView};
    use crate::configure::render::{self, SavedLine, StoredLine};
    use crate::configure::sections;
    use crate::gateway::config::Change;
    use crate::gateway::{AccessToken, Gateway, GatewayError};
    use crate::views::load::server::Stop;
    use crate::views::model::Refusal;

    /// How many revisions the history shows.
    pub const HISTORY: u64 = 20;

    /// The stable code of a gateway whose configuration is not managed.
    pub const UNMANAGED: &str = "configuration-unmanaged";

    /// The longest reason a revision records, in characters, as the
    /// gateway holds it.
    const REASON_LIMIT: usize = 1_000;

    /// What a form asks for, read and checked.
    ///
    /// Its `Debug` output names the action and the revisions alone, never a
    /// reason, a member or an AQL text.
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
            /// The save as the gateway takes it, a secret value among it.
            request: Box<SectionRequest>,
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
                Self::SaveSection { resource, request } => f
                    .debug_struct("SaveSection")
                    .field("resource", resource)
                    .field("base", &request.base)
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
            }
        }
    }

    /// A refusal of the form, naming the field and never its value.
    fn invalid(reason: &str) -> Refusal {
        Refusal::Invalid {
            reason: reason.to_owned(),
        }
    }

    /// The text of `field`, trimmed, or `None` when it is empty.
    fn given(field: &str) -> Option<&str> {
        let text = field.trim();
        (!text.is_empty()).then_some(text)
    }

    /// The first of `replicas`, which the configuration page reads and
    /// changes.
    ///
    /// # Errors
    /// Returns [`Refusal::Invalid`] when the console names no replica.
    pub fn replica(replicas: &[ReplicaSettings]) -> Result<&ReplicaSettings, Refusal> {
        replicas.first().ok_or_else(|| {
            invalid(
                "This console is configured with no admin listener, so it shows no configuration.",
            )
        })
    }

    /// The revision number `field` holds, `None` for an empty field.
    fn revision(field: &str, what: &str) -> Result<Option<RevisionId>, Refusal> {
        given(field)
            .map(|text| {
                text.parse::<RevisionId>()
                    .map_err(|_not_a_number| Refusal::Invalid {
                        reason: format!("The {what} is not a revision number."),
                    })
            })
            .transpose()
    }

    /// The reason the form gives, 1 to the limit of characters.
    fn reason(form: &ConfigForm) -> Result<String, Refusal> {
        let reason =
            given(&form.reason).ok_or_else(|| invalid("Give the reason for the change."))?;
        if reason.chars().count() > REASON_LIMIT {
            return Err(invalid("The reason is longer than 1000 characters."));
        }
        Ok(reason.to_owned())
    }

    /// What `form` asks for.
    ///
    /// # Errors
    /// Returns [`Refusal::Invalid`] for a form not confirmed, an action the
    /// page does not offer, or a field that does not read, naming the field
    /// and never what it holds.
    pub fn plan(form: &ConfigForm) -> Result<Plan, Refusal> {
        if given(&form.confirm).is_none() {
            return Err(invalid("Tick the confirmation to run this change."));
        }
        match form.action.as_str() {
            "save-member" => Ok(Plan::SaveMember {
                base: revision(&form.base, "base revision")?,
                reason: reason(form)?,
                member: Box::new(member(form)?),
            }),
            "remove-member" => Ok(Plan::RemoveMember {
                base: revision(&form.base, "base revision")?,
                reason: reason(form)?,
                node: given(&form.node)
                    .ok_or_else(|| invalid("Choose the console member to remove."))?
                    .to_owned(),
            }),
            "apply" => Ok(Plan::Apply {
                revision: revision(&form.revision, "revision to apply")?
                    .ok_or_else(|| invalid("Name the revision to apply."))?,
            }),
            "rollback" => Ok(Plan::Rollback {
                current: revision(&form.current, "running revision")?
                    .ok_or_else(|| invalid("Name the running revision to roll back."))?,
                reason: reason(form)?,
            }),
            "store-query" => Ok(Plan::StoreQuery {
                name: given(&form.name)
                    .ok_or_else(|| invalid("Name the stored query."))?
                    .to_owned(),
                version: given(&form.version)
                    .ok_or_else(|| invalid("Name the version to store."))?
                    .to_owned(),
                aql: given(&form.aql)
                    .ok_or_else(|| invalid("Give the AQL to store."))?
                    .to_owned(),
            }),
            "save-credentials" => {
                let endpoint = given(&form.endpoint)
                    .ok_or_else(|| invalid("Name the endpoint_id of the credentials."))?;
                EndpointId::new(endpoint).map_err(|_bad| {
                    invalid("The endpoint_id is not 1 to 64 letters, digits, '.', '-' or '_'.")
                })?;
                Ok(Plan::SaveSection {
                    resource: Resource::Credentials(endpoint.to_owned()),
                    request: Box::new(section_request(form)?),
                })
            }
            "save-binding" => {
                let section = BindingSection::named(form.section.trim())
                    .ok_or_else(|| invalid("Choose the binding section."))?;
                Ok(Plan::SaveSection {
                    resource: Resource::Binding(section),
                    request: Box::new(section_request(form)?),
                })
            }
            "remove-section" => Ok(Plan::RemoveSection {
                base: revision(&form.base, "base revision")?,
                reason: reason(form)?,
                resource: chosen(&form.resource)?,
            }),
            "confirm" => Ok(Plan::Confirm {
                revision: revision(&form.revision, "revision to confirm")?
                    .ok_or_else(|| invalid("Name the revision to confirm."))?,
            }),
            "approve" => Ok(Plan::Approve {
                revision: revision(&form.revision, "revision to approve")?
                    .ok_or_else(|| invalid("Name the revision to approve."))?,
            }),
            _ => Err(invalid("The form names no action this page offers.")),
        }
    }

    /// The credential or binding section a removal names, as
    /// [`crate::configure::sections::choice`] writes it.
    fn chosen(field: &str) -> Result<Resource, Refusal> {
        let refused = || invalid("Choose the credential or binding section to remove.");
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

    /// The save of a credential or a binding section the form gives: its
    /// settings, and at most one secret set by its value or a file, or
    /// cleared.
    fn section_request(form: &ConfigForm) -> Result<SectionRequest, Refusal> {
        let value = (!form.secret_value.is_empty()).then(|| WriteOnly::new(&form.secret_value));
        let file = given(&form.secret_file).map(str::to_owned);
        let clear = given(&form.secret_clear).is_some();
        let named = [value.is_some(), file.is_some(), clear]
            .into_iter()
            .filter(|named| *named)
            .count();
        let secrets = match (given(&form.secret_key), named) {
            (None, 0) => Vec::new(),
            (None, _) => return Err(invalid("Name the key of the secret to set or clear.")),
            (Some(_), 0) => {
                return Err(invalid(
                    "Give the secret's value or file, or tick its clear, for the key you named.",
                ));
            }
            (Some(key), 1) => vec![SecretChange {
                key: key.to_owned(),
                value,
                file,
                clear,
            }],
            (Some(_), _) => {
                return Err(invalid(
                    "Give exactly one of the secret's value, its file and its clear.",
                ));
            }
        };
        Ok(SectionRequest {
            base: revision(&form.base, "base revision")?,
            reason: reason(form)?,
            settings: form.settings.trim().to_owned(),
            secrets,
        })
    }

    /// The member the form describes.
    fn member(form: &ConfigForm) -> Result<ConsoleMember, Refusal> {
        let id_form = "1 to 64 letters, digits, '.', '-' or '_', starting with a letter or digit";
        let organisation =
            OrganisationId::new(form.organisation.trim()).map_err(|_bad| Refusal::Invalid {
                reason: format!("The organisation id is not {id_form}."),
            })?;
        let node = NodeId::new(form.node.trim()).map_err(|_bad| Refusal::Invalid {
            reason: format!("The node_id is not {id_form}."),
        })?;
        let system_id = SystemId::new(form.system_id.trim())
            .map_err(|_bad| invalid("The system_id is not an openEHR UID."))?;
        let mut endpoints = Vec::new();
        for line in form
            .endpoints
            .split(';')
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            endpoints.push(endpoint(line)?);
        }
        if endpoints.is_empty() {
            return Err(invalid("Give at least one Query API endpoint."));
        }
        Ok(ConsoleMember {
            organisation: ConsoleOrganisation {
                id: organisation,
                name: given(&form.organisation_name).map(str::to_owned),
            },
            node: ConsoleNode {
                id: node,
                system_id,
                product: given(&form.product).map(str::to_owned),
                version: given(&form.product_version).map(str::to_owned),
            },
            endpoints,
        })
    }

    /// One Query API endpoint: its `endpoint_id`, its base URL and
    /// optionally `suspended`.
    fn endpoint(line: &str) -> Result<ConsoleEndpoint, Refusal> {
        let shape = "Each endpoint is an endpoint_id and a base URL, then suspended if it is out of service.";
        let mut words = line.split_whitespace();
        let (Some(id), Some(url)) = (words.next(), words.next()) else {
            return Err(invalid(shape));
        };
        let status = match words.next() {
            None => EndpointStatus::Active,
            Some("suspended") => EndpointStatus::Suspended,
            Some(_) => return Err(invalid(shape)),
        };
        if words.next().is_some() {
            return Err(invalid(shape));
        }
        let id = EndpointId::new(id).map_err(|_bad| {
            invalid("An endpoint_id is not 1 to 64 letters, digits, '.', '-' or '_'.")
        })?;
        Ok(ConsoleEndpoint {
            id,
            url: url.to_owned(),
            connection_type: ConnectionType::OpenehrRestQuery,
            status,
            consent_refusal_codes: Vec::new(),
            categories: Vec::new(),
            profiles: Vec::new(),
            ehr_id_system: None,
        })
    }

    /// Returns `form`, the member the page's form gives, merged onto `held`.
    ///
    /// `held` is the member the newest revision holds of its `node_id`:
    /// each endpoint the form names keeps every key the form does not show (its consent
    /// refusal codes, categories, profiles and `ehr_id` system), and each
    /// `hl7-fhir-rest` endpoint the form does not name is kept whole.
    #[must_use]
    pub fn merged(mut form: ConsoleMember, held: Option<&ConsoleMember>) -> ConsoleMember {
        let Some(held) = held else {
            return form;
        };
        for endpoint in &mut form.endpoints {
            if let Some(kept) = held.endpoints.iter().find(|kept| kept.id == endpoint.id) {
                *endpoint = ConsoleEndpoint {
                    id: endpoint.id.clone(),
                    url: endpoint.url.clone(),
                    status: endpoint.status,
                    ..kept.clone()
                };
            }
        }
        let natives: Vec<ConsoleEndpoint> = held
            .endpoints
            .iter()
            .filter(|kept| kept.connection_type == ConnectionType::Hl7FhirRest)
            .filter(|kept| !form.endpoints.iter().any(|named| named.id == kept.id))
            .cloned()
            .collect();
        form.endpoints.extend(natives);
        form
    }

    /// The configuration page `replica` answers, or the page saying its
    /// configuration is not managed.
    ///
    /// # Errors
    /// Returns the [`Stop`] of a refused or failed read.
    pub async fn page(
        gateway: &Gateway,
        replica: &ReplicaSettings,
        token: &AccessToken,
    ) -> Result<ConfigView, Stop> {
        let standing = match gateway.config_standing(replica, token).await {
            Ok(standing) => standing,
            Err(error) if unmanaged(&error) => return Ok(render::unmanaged(replica)),
            Err(error) => return Err(error.into()),
        };
        let resources = gateway.config_resources(replica, token).await?;
        let history = gateway
            .config_history(
                replica,
                token,
                PageRequest {
                    offset: 0,
                    limit: HISTORY,
                },
            )
            .await?;
        let running = standing.current.as_ref().map(|mark| mark.revision);
        let pending = match (running, standing.head) {
            (Some(running), Some(head)) if running != head => Some(
                gateway
                    .config_diff(replica, token, (running, head))
                    .await?
                    .changes,
            ),
            _ => None,
        };
        Ok(render::page(
            replica,
            &standing,
            &resources,
            (&history, pending.as_deref()),
        ))
    }

    /// Whether `error` is the `404` (`configuration-unmanaged`) of an admin
    /// listener that serves no configuration revisions; a `404` without
    /// that code, as from a wrong admin URL, is not.
    fn unmanaged(error: &GatewayError) -> bool {
        error.refusal().is_some_and(|(status, code)| {
            status == http::StatusCode::NOT_FOUND && code.as_deref() == Some(UNMANAGED)
        })
    }

    /// Runs `plan` at `replica` as the operator, and renders its answer.
    pub async fn run(
        gateway: &Gateway,
        replica: &ReplicaSettings,
        token: &AccessToken,
        plan: Plan,
    ) -> ActionAnswer {
        let name = replica.name.as_str();
        match plan {
            Plan::SaveMember {
                base,
                reason,
                member,
            } => {
                let change = Change {
                    base,
                    reason: &reason,
                };
                let saved = save(gateway, replica, token, (change, *member)).await;
                let saved = previewed(gateway, replica, token, saved).await;
                render::saved(&acted(name, "save", saved, |line| line))
            }
            Plan::RemoveMember { base, reason, node } => {
                let change = Change {
                    base,
                    reason: &reason,
                };
                let saved = gateway.remove_member(replica, token, change, &node).await;
                let saved = previewed(gateway, replica, token, saved).await;
                render::saved(&acted(name, "save", saved, |line| line))
            }
            Plan::Apply { revision } => {
                let applied = gateway.apply_revision(replica, token, revision).await;
                render::applied(&acted(name, "apply", applied, |line| line), "apply")
            }
            Plan::Rollback { current, reason } => {
                let applied = gateway.rollback(replica, token, current, &reason).await;
                render::applied(&acted(name, "rollback", applied, |line| line), "rollback")
            }
            Plan::StoreQuery {
                name: query,
                version,
                aql,
            } => {
                let stored = gateway
                    .store_query(replica, token, (&query, &version), &aql)
                    .await;
                render::stored(&acted(name, "store", stored, |stored| StoredLine {
                    name: stored.name,
                    version: stored.version,
                }))
            }
            Plan::SaveSection { resource, request } => {
                let saved = gateway
                    .save_section(replica, token, &resource, &request)
                    .await;
                drop(request);
                let saved = previewed(gateway, replica, token, saved).await;
                render::saved(&acted(name, "save", saved, |line| line))
            }
            Plan::RemoveSection {
                base,
                reason,
                resource,
            } => {
                let change = Change {
                    base,
                    reason: &reason,
                };
                let saved = gateway
                    .remove_section(replica, token, change, &resource)
                    .await;
                let saved = previewed(gateway, replica, token, saved).await;
                render::saved(&acted(name, "save", saved, |line| line))
            }
            Plan::Confirm { revision } => {
                let confirmed = gateway.confirm_revision(replica, token, revision).await;
                sections::confirmed(&acted(name, "confirmation", confirmed, |mark| mark))
            }
            Plan::Approve { revision } => {
                let approved = gateway.approve_revision(replica, token, revision).await;
                sections::approved(&acted(name, "approval", approved, |review| review))
            }
        }
    }

    /// Saves `member` merged onto the member the newest revision holds of
    /// its `node_id` ([`merged`]); when the resources cannot be read the
    /// failure is the answer and nothing is sent, since a save without the
    /// held member would drop the keys the form does not show.
    async fn save(
        gateway: &Gateway,
        replica: &ReplicaSettings,
        token: &AccessToken,
        (change, member): (Change<'_>, ConsoleMember),
    ) -> Result<Revision, GatewayError> {
        let resources = gateway.config_resources(replica, token).await?;
        let held = resources
            .members
            .iter()
            .filter_map(|resource| resource.member.as_ref())
            .find(|held| held.node.id == member.node.id);
        let member = merged(member, held);
        gateway.save_member(replica, token, change, &member).await
    }

    /// The saved revision `saved` with the preview of what it changes
    /// against the running revision.
    async fn previewed(
        gateway: &Gateway,
        replica: &ReplicaSettings,
        token: &AccessToken,
        saved: Result<Revision, GatewayError>,
    ) -> Result<SavedLine, GatewayError> {
        let revision = saved?;
        // NOTE: no specification governs this: our own design; the save stands without its
        // preview, so a preview that cannot be read is shown as unread, never as no change.
        let running = gateway
            .config_standing(replica, token)
            .await
            .ok()
            .and_then(|standing| standing.current)
            .map(|mark| mark.revision);
        let changes = match running {
            Some(running) => gateway
                .config_diff(replica, token, (running, revision.id))
                .await
                .ok()
                .map(|diff| diff.changes),
            None => None,
        };
        Ok(SavedLine {
            revision,
            running,
            changes,
        })
    }
}
