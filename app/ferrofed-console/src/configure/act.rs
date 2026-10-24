// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration's reads and the run of each of its forms.
//!
//! Each runs while the page is drawn, as the operator of the request: the
//! form read into what is sent, checked before any replica is asked, and
//! the gateway's answers read into what the page draws. The first replica
//! configured is called with the operator's own access token, which never
//! leaves the server.

use crate::locale::{Msg, tr};
use ferrofed_registry::console::ConsoleMember;
use ferrofed_registry::operator::PageRequest;
use ferrofed_registry::resource::{Resource, SecretChange, SectionRequest, SectionResource};
use ferrofed_registry::revision::{Revision, RevisionId};
use ferrofed_registry::running::SettingsRequest;

use crate::config::settings::ReplicaSettings;
use crate::configure::member::Shown;
use crate::configure::model::{ConfigView, Screen};
use crate::configure::plan::{Plan, Refused, Settings, at, invalid};
use crate::configure::render::{self, Loaded, SavedLine, StoredLine};
use crate::configure::said::ActionAnswer;
use crate::configure::sections;
use crate::configure::settings;
use crate::gateway::config::Change;
use crate::gateway::{AccessToken, Gateway, GatewayError};
use crate::read::Refusal;
use crate::read::Stop;

/// How many revisions the history shows.
pub const HISTORY: u64 = 20;

/// The stable code of a gateway whose configuration is not managed.
pub const UNMANAGED: &str = "configuration-unmanaged";

/// The first of `replicas`, which the configuration page reads and
/// changes.
///
/// # Errors
/// Returns [`Refusal::Invalid`] when the console names no replica.
pub fn replica(replicas: &[ReplicaSettings]) -> Result<&ReplicaSettings, Refusal> {
    replicas
        .first()
        .ok_or_else(|| invalid(&tr(Msg::ConsoleConfiguredNoAdminListenerSo2)))
}

/// The part of the configuration page `screen` names, as `replica`
/// answers it, its form drawn as `draft` holds it when the operator posted
/// one, or the page saying its configuration is not managed.
///
/// # Errors
/// Returns the [`Stop`] of a refused or failed read, and of a path that
/// names no part of the page.
pub async fn page(
    gateway: &Gateway,
    replica: &ReplicaSettings,
    token: &AccessToken,
    screen: &Screen,
    draft: Option<&crate::configure::draft::Draft>,
) -> Result<ConfigView, Stop> {
    let shown = render::Shown::of(screen)
        .ok_or_else(|| Stop::Refused(invalid(&tr(Msg::PathNamesNoPartConfigurationPage))))?;
    let standing = match gateway.config_standing(replica, token).await {
        Ok(standing) => standing,
        Err(error) if unmanaged(&error) => {
            return Ok(match gateway.config_running(replica, token).await {
                Ok(running) => render::file_only(&running, shown.file_section()),
                Err(error) => {
                    unread(&error);
                    render::unmanaged(replica)
                }
            });
        }
        Err(error) => return Err(error.into()),
    };
    let settings = if shown.reads_running() {
        match gateway.config_running(replica, token).await {
            Ok(running) => Some(running),
            Err(error) => {
                unread(&error);
                None
            }
        }
    } else {
        None
    };
    let resources = gateway.config_resources(replica, token).await?;
    let history = if shown.reads_history() {
        Some(
            gateway
                .config_history(
                    replica,
                    token,
                    PageRequest {
                        offset: 0,
                        limit: HISTORY,
                    },
                )
                .await?,
        )
    } else {
        None
    };
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
    let compared = match (running, shown.compared()) {
        (Some(running), Some(other)) if running != other => Some((
            other,
            gateway
                .config_diff(replica, token, (running, other))
                .await?
                .changes,
        )),
        _ => None,
    };
    let loaded = Loaded {
        replica,
        standing: &standing,
        resources: &resources,
        history: history.as_ref(),
        pending: pending.as_deref(),
        compared: compared
            .as_ref()
            .map(|(other, changes)| (*other, changes.as_slice())),
        draft,
        running: settings.as_ref(),
    };
    render::page(&loaded, &shown).map_err(Stop::Refused)
}

/// Logs once that the running configuration could not be read, as every
/// failed read of the page is logged.
fn unread(error: &GatewayError) {
    let _stopped = crate::read::stopped(error, crate::read::Asked::View);
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
            shown,
        } => {
            let change = Change {
                base,
                reason: &reason,
            };
            save_member(gateway, replica, token, (change, *member, shown)).await
        }
        Plan::RemoveMember { base, reason, node } => {
            let change = Change {
                base,
                reason: &reason,
            };
            let saved = gateway.remove_member(replica, token, change, &node).await;
            let saved = previewed(gateway, replica, token, saved).await;
            render::answers::saved(&acted((name, "member removal"), saved, |line| line))
        }
        Plan::Apply { revision } => {
            let applied = gateway.apply_revision(replica, token, revision).await;
            render::answers::applied(&acted((name, "apply"), applied, |line| line), "apply")
        }
        Plan::Rollback { current, reason } => {
            let applied = gateway.rollback(replica, token, current, &reason).await;
            render::answers::applied(&acted((name, "rollback"), applied, |line| line), "rollback")
        }
        Plan::StoreQuery {
            name: query,
            version,
            aql,
        } => store(gateway, (replica, token), (&query, &version, &aql)).await,
        Plan::SaveSection {
            resource,
            base,
            reason,
            settings,
            secrets,
        } => {
            let change = (base, reason, settings, secrets);
            save_section(gateway, (replica, token), &resource, change).await
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
            render::answers::saved(&acted((name, "section removal"), saved, |line| line))
        }
        Plan::SaveSettings {
            base,
            reason,
            settings,
        } => {
            let request = SettingsRequest {
                base,
                reason,
                settings,
            };
            let saved = gateway.save_settings(replica, token, &request).await;
            let saved = previewed(gateway, replica, token, saved).await;
            render::answers::saved(&acted((name, "settings-save"), saved, |line| line))
        }
        Plan::Confirm { revision } => {
            let confirmed = gateway.confirm_revision(replica, token, revision).await;
            sections::confirmed(&acted((name, "confirmation"), confirmed, |mark| mark))
        }
        Plan::Approve { revision } => {
            let approved = gateway.approve_revision(replica, token, revision).await;
            sections::approved(&acted((name, "approval"), approved, |review| review))
        }
        Plan::Import { base, reason, toml } => {
            import(gateway, (replica, token), (base, &reason, &toml)).await
        }
    }
}

/// Stores the stored-query version `query` names with its AQL at
/// `replica` as the operator, and renders the answer.
async fn store(
    gateway: &Gateway,
    (replica, token): (&ReplicaSettings, &AccessToken),
    (query, version, aql): (&str, &str, &str),
) -> ActionAnswer {
    let stored = gateway
        .store_query(replica, token, (query, version), aql)
        .await;
    render::answers::stored(&acted(
        (replica.name.as_str(), &tr(Msg::StoredQueryStore)),
        stored,
        |stored| StoredLine {
            name: stored.name,
            version: stored.version,
        },
    ))
}

/// Saves `resource` with the settings and secrets `change` gives,
/// merged onto what the newest revision holds, and renders the answer
/// with its preview.
async fn save_section(
    gateway: &Gateway,
    (replica, token): (&ReplicaSettings, &AccessToken),
    resource: &Resource,
    (base, reason, settings, secrets): (Option<RevisionId>, String, Settings, Vec<SecretChange>),
) -> ActionAnswer {
    let at = (replica.name.as_str(), "section save");
    let (settings, cleared) =
        match section_request(gateway, (replica, token), resource, settings).await {
            Ok(Ok(made)) => made,
            Ok(Err(refused)) => return render::answers::refused(&refused),
            Err(error) => {
                let failed = Err::<SavedLine, GatewayError>(error);
                return render::answers::saved(&acted(at, failed, |line| line));
            }
        };
    let mut changes = secrets;
    for key in cleared {
        if !changes.iter().any(|change| change.key == key) {
            changes.push(SecretChange {
                key,
                value: None,
                file: None,
                clear: true,
            });
        }
    }
    let request = SectionRequest {
        base,
        reason,
        settings,
        secrets: changes,
    };
    let saved = gateway
        .save_section(replica, token, resource, &request)
        .await;
    let saved = previewed(gateway, replica, token, saved).await;
    render::answers::saved(&acted(at, saved, |line| line))
}

/// The settings `settings` give `resource`, merged onto those the
/// newest revision holds, and the held secrets a change of grant leaves
/// behind, which are cleared.
///
/// # Errors
/// Returns the gateway's failure when the resources cannot be read: a
/// save without the held settings would drop the keys the form does
/// not show. A form that does not read is the inner [`Refused`].
async fn section_request(
    gateway: &Gateway,
    (replica, token): (&ReplicaSettings, &AccessToken),
    resource: &Resource,
    settings: Settings,
) -> Result<Result<(String, Vec<String>), Refused>, GatewayError> {
    let resources = gateway.config_resources(replica, token).await?;
    let held: Option<&SectionResource> = resources
        .sections
        .iter()
        .find(|section| &section.resource == resource);
    Ok(merged_settings(held, settings))
}

/// The settings `settings` give, merged onto `held`, and the held
/// secrets the chosen grant does not use.
fn merged_settings(
    held: Option<&SectionResource>,
    settings: Settings,
) -> Result<(String, Vec<String>), Refused> {
    let table = settings::build::held(held.and_then(|held| held.settings.as_deref()))?;
    let slots = held.map(|held| held.secrets.as_slice()).unwrap_or_default();
    match settings {
        Settings::Credentials(grant, rows) => {
            let text = settings::build::credentials(&table, grant, &rows)?;
            let kept: Vec<&str> = grant
                .secrets()
                .iter()
                .map(|secret| secret.key)
                .chain([settings::TLS_SECRET.key])
                .collect();
            let cleared = slots
                .iter()
                .filter(|slot| !kept.contains(&slot.key.as_str()))
                .map(|slot| slot.key.clone())
                .collect();
            Ok((text, cleared))
        }
        Settings::Binding(rows) => Ok((settings::build::binding(&table, &rows)?, Vec::new())),
    }
}

/// Imports `toml` as a dry-run revision on `base` for `reason` at
/// `replica` as the operator, and renders its answer.
async fn import(
    gateway: &Gateway,
    (replica, token): (&ReplicaSettings, &AccessToken),
    (base, reason, toml): (Option<RevisionId>, &str, &str),
) -> ActionAnswer {
    let name = replica.name.as_str();
    let change = Change { base, reason };
    let imported = gateway.import_config(replica, token, change, toml).await;
    render::answers::imported(&acted((name, "import"), imported, |line| line))
}

/// Saves `member` as [`save`] does, and renders the answer with its preview.
async fn save_member(
    gateway: &Gateway,
    replica: &ReplicaSettings,
    token: &AccessToken,
    (change, member, shown): (Change<'_>, ConsoleMember, Shown),
) -> ActionAnswer {
    let what = if shown.adding {
        "member add"
    } else {
        "member save"
    };
    match save(gateway, replica, token, (change, member, shown)).await {
        Ok(saved) => {
            let saved = previewed(gateway, replica, token, saved).await;
            let at = (replica.name.as_str(), what);
            render::answers::saved(&acted(at, saved, |line| line))
        }
        Err(refused) => render::answers::refused(&refused),
    }
}

/// Saves `member` merged onto the member the newest revision holds of
/// its `node_id` ([`merged`](crate::configure::member::merged)); when the
/// resources cannot be read the failure is the answer and nothing is
/// sent, since a save without the held member would drop the keys the
/// form does not show.
///
/// # Errors
/// Returns a [`Refused`] at the `node_id` field, asking nothing more,
/// for the form that adds a member naming a `node_id` the gateway
/// already holds.
async fn save(
    gateway: &Gateway,
    replica: &ReplicaSettings,
    token: &AccessToken,
    (change, member, shown): (Change<'_>, ConsoleMember, Shown),
) -> Result<Result<Revision, GatewayError>, Refused> {
    let resources = match gateway.config_resources(replica, token).await {
        Ok(resources) => resources,
        Err(error) => return Ok(Err(error)),
    };
    let node = member.node.id.as_str();
    if shown.adding
        && resources
            .members
            .iter()
            .any(|resource| resource.node.as_str() == node)
    {
        return Err(at(
            crate::configure::member::field("node"),
            tr(Msg::MemberNodeIdExistsChangeOwn),
        ));
    }
    let held = resources
        .members
        .iter()
        .filter_map(|resource| resource.member.as_ref())
        .find(|held| held.node.id == member.node.id);
    let member = crate::configure::member::merged(member, held, shown.fhir);
    Ok(gateway.save_member(replica, token, change, &member).await)
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

/// What `replica` did with the configuration action `what` that
/// `answered`, its report read by `read`.
fn acted<A, T>(
    at: (&str, &str),
    answered: Result<A, GatewayError>,
    read: impl FnOnce(A) -> T,
) -> crate::act::ReplicaAnswer<T> {
    crate::act::acted(crate::read::ActionKind::Configuration, at, answered, read)
}
