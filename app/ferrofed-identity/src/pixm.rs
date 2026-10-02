// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver: the [`Resolver`] seam over ITI-83 against one or more
//! Patient Identifier Cross-reference Managers (N3, §5.2, Annex A.1).
//!
//! Each member is bound to one PIX Manager and to its `ehr_id` domain there:
//! the assigning authority whose identifier values are that member's
//! `ehr_id`s (Annex A.1, "`targetSystem=<the domain's ehr_id system>`"). The
//! resolver asks each Manager once per query, with one `targetSystem` per
//! member it serves (`targetSystem` is `0..*`), and reads the identifier the
//! answer holds in a member's domain as that member's `ehr_id`.
//!
//! The patient identifier reaches the PIX Manager only, which is the
//! transaction's purpose. It travels inside `ihe_iti`'s redacting
//! [`SourceIdentifier`]; nothing here logs it, and no error carries it.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use ihe_iti::pixm::PixmClient;
use ihe_iti::pixm::error::{InvalidInput, PixmError};
use ihe_iti::pixm::identifier::{CrossReference, SourceIdentifier, TargetSystem};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::task::JoinSet;
use url::Url;

use crate::patient::{IdentifierNamespace, PatientRef};
use crate::resolver::{Resolution, Resolver, ResolverError};

/// How the gateway authenticates to one PIX Manager (ITI TF-2 Appendix Z.8).
///
/// `Debug` redacts every secret, because [`SecretString`] does.
#[derive(Debug)]
#[non_exhaustive]
pub enum PixAuth {
    /// No `Authorization` header: the transport (mutual TLS, a private
    /// network) authenticates the gateway.
    None,
    /// An RFC 6750 bearer token.
    Bearer(SecretString),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
}

/// One PIX Manager as the configuration names it.
#[derive(Debug)]
pub struct ManagerConfig {
    /// The Manager's FHIR base URL.
    pub base: Url,
    /// How the gateway authenticates to it.
    pub auth: PixAuth,
    /// The members this Manager resolves, each with its `ehr_id` domain: the
    /// assigning authority whose identifiers are that member's `ehr_id`s.
    pub members: BTreeMap<NodeId, String>,
}

/// A PIXm resolver that cannot be built.
///
/// The errors name a member, a domain or a Manager, never an identifier value.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PixmConfigError {
    /// No PIX Manager is configured.
    #[error("the PIXm resolver names no PIX Manager")]
    NoManager,
    /// A Manager names a member the registry does not hold.
    #[error("a PIX Manager names member {0}, which is not in the registry")]
    UnknownMember(NodeId),
    /// Two Managers, or two entries, name one member.
    #[error("member {0} is resolved by more than one PIX Manager")]
    DuplicateMember(NodeId),
    /// A registry member has no PIX Manager, so no patient query could be
    /// scoped there and every one would fail.
    #[error("registry member {0} has no PIX Manager and ehr_id domain")]
    UnresolvedMember(NodeId),
    /// A member's `ehr_id` domain is not an absolute URI.
    #[error("the ehr_id domain of member {member} is not an absolute URI")]
    Domain {
        /// The member.
        member: NodeId,
        /// What the PIXm client reported.
        #[source]
        source: InvalidInput,
    },
    /// A Manager's base URL is not an `http(s)` URL without a query.
    #[error("a PIX Manager base URL is not an http(s) URL without a query or a fragment")]
    Base(#[source] InvalidInput),
    /// A namespace mapping does not name an absolute URI.
    #[error("the PIX domain mapped from namespace {0} is not an absolute URI")]
    Namespace(IdentifierNamespace),
    /// A credential cannot travel in an HTTP header.
    #[error("the credentials of a PIX Manager cannot travel in an HTTP header")]
    Credentials,
    /// The HTTP client could not be built.
    #[error("the HTTP client for a PIX Manager could not be built")]
    Client(#[source] reqwest::Error),
}

/// Why the PIXm resolver could not answer for a member.
///
/// None of them carries an identifier value: a value the Manager answered
/// with is never repeated, only the fact that it broke a rule.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PixmResolveError {
    /// The patient's issuing namespace maps to no PIX assigning authority.
    #[error("the namespace {0} maps to no PIX assigning authority")]
    UnmappedNamespace(IdentifierNamespace),
    /// The ITI-83 exchange failed.
    #[error("the PIX Manager could not cross-reference the patient")]
    Exchange(#[source] PixmError),
    /// The member's domain holds an identifier that is not an `ehr_id`.
    #[error("the PIX Manager answered an identifier for member {0} that is not an ehr_id")]
    NotAnEhrId(NodeId),
    /// The member's domain holds more than one identifier, so the gateway
    /// cannot choose the `ehr_id` and does not guess.
    #[error("the PIX Manager answered more than one identifier for member {0}")]
    Ambiguous(NodeId),
    /// The PIXm client read an answer this resolver does not know how to
    /// interpret.
    #[error("the PIX Manager answered in a form this resolver does not interpret")]
    UnexpectedAnswer,
}

/// One PIX Manager and the members it resolves.
struct Manager {
    client: PixmClient,
    members: Vec<(NodeId, TargetSystem)>,
}

/// The [`Resolver`] over ITI-83.
///
/// It answers [`Resolution::Resolved`] when a member's domain holds exactly
/// one identifier that reads as an `ehr_id`, [`Resolution::Unknown`] when the
/// Manager does not know the patient or the member's domain holds nothing,
/// and [`Resolution::Unavailable`] for every failure, a namespace it cannot
/// map included, so the query fails closed (§11.3 covers only an answered
/// lookup; no specification governs this: our own design).
pub struct PixmResolver {
    managers: Vec<Arc<Manager>>,
    namespaces: BTreeMap<IdentifierNamespace, String>,
}

impl PixmResolver {
    /// Builds the resolver over `managers`, whose members must cover every
    /// member of `registry` exactly once.
    ///
    /// `namespaces` maps a client's issuing namespace to the PIX assigning
    /// authority it stands for; a namespace that is itself an absolute URI
    /// needs no entry.
    ///
    /// # Errors
    /// A [`PixmConfigError`] for an unknown, doubled or uncovered member, a
    /// domain or base URL the PIXm client refuses, a namespace mapping that
    /// is not a URI, credentials no header can carry, or an HTTP client that
    /// cannot be built.
    pub fn from_config(
        managers: Vec<ManagerConfig>,
        namespaces: BTreeMap<IdentifierNamespace, String>,
        registry: &RegistrySnapshot,
    ) -> Result<Self, PixmConfigError> {
        if managers.is_empty() {
            return Err(PixmConfigError::NoManager);
        }
        let mut seen: Vec<NodeId> = Vec::new();
        let mut built = Vec::with_capacity(managers.len());
        for manager in managers {
            let mut members = Vec::with_capacity(manager.members.len());
            for (member, domain) in manager.members {
                if registry.node(&member).is_none() {
                    return Err(PixmConfigError::UnknownMember(member));
                }
                if seen.contains(&member) {
                    return Err(PixmConfigError::DuplicateMember(member));
                }
                let target =
                    TargetSystem::new(domain).map_err(|source| PixmConfigError::Domain {
                        member: member.clone(),
                        source,
                    })?;
                seen.push(member.clone());
                members.push((member, target));
            }
            let http = http_client(&manager.auth)?;
            let client = PixmClient::new(manager.base, http).map_err(PixmConfigError::Base)?;
            built.push(Arc::new(Manager { client, members }));
        }
        if let Some(uncovered) = registry
            .nodes()
            .map(ferrofed_registry::snapshot::Node::id)
            .find(|id| !seen.contains(id))
        {
            return Err(PixmConfigError::UnresolvedMember(uncovered.clone()));
        }
        for (namespace, system) in &namespaces {
            if TargetSystem::new(system.clone()).is_err() {
                return Err(PixmConfigError::Namespace(namespace.clone()));
            }
        }
        Ok(Self {
            managers: built,
            namespaces,
        })
    }

    /// The PIX assigning authority `namespace` stands for.
    fn system(&self, namespace: &IdentifierNamespace) -> Option<String> {
        if let Some(system) = self.namespaces.get(namespace) {
            return Some(system.clone());
        }
        // NOTE: no specification governs this: our own design. A namespace
        // that is itself an absolute URI names the assigning authority.
        TargetSystem::new(namespace.as_str())
            .ok()
            .map(|_absolute| namespace.as_str().to_owned())
    }
}

impl fmt::Debug for PixmResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PixmResolver")
            .field("managers", &self.managers.len())
            .field("namespaces", &self.namespaces.len())
            .finish()
    }
}

/// The HTTP client one Manager is asked through: no redirects, because the
/// request URL holds the source identifier, and the credentials sent as a
/// sensitive default header.
fn http_client(auth: &PixAuth) -> Result<reqwest::Client, PixmConfigError> {
    let mut headers = HeaderMap::new();
    let value = match auth {
        PixAuth::None => None,
        PixAuth::Bearer(token) => Some(format!("Bearer {}", token.expose_secret())),
        PixAuth::Basic { user, password } => Some(format!(
            "Basic {}",
            STANDARD.encode(format!("{user}:{}", password.expose_secret()))
        )),
    };
    if let Some(value) = value {
        let mut header =
            HeaderValue::from_str(&value).map_err(|_invalid| PixmConfigError::Credentials)?;
        header.set_sensitive(true);
        headers.insert(AUTHORIZATION, header);
    }
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(headers)
        .build()
        .map_err(PixmConfigError::Client)
}

/// What one Manager answered, per member it was asked about.
fn read(
    answer: Result<CrossReference, PixmError>,
    members: &[(NodeId, TargetSystem)],
) -> Vec<(NodeId, Resolution)> {
    match answer {
        Ok(CrossReference::Matched(found)) => members
            .iter()
            .map(|(member, domain)| {
                let mut in_domain = found.in_domain(domain.as_str());
                let resolution = match (in_domain.next(), in_domain.next()) {
                    (None, _) => Resolution::Unknown,
                    (Some(identifier), None) => {
                        match EhrId::new(identifier.value().expose_secret()) {
                            Ok(ehr_id) => Resolution::Resolved(ehr_id),
                            Err(_not_an_ehr_id) => {
                                unavailable(PixmResolveError::NotAnEhrId(member.clone()))
                            }
                        }
                    }
                    (Some(_), Some(_)) => unavailable(PixmResolveError::Ambiguous(member.clone())),
                };
                (member.clone(), resolution)
            })
            .collect(),
        Ok(CrossReference::SourceNotFound) => members
            .iter()
            .map(|(member, _)| (member.clone(), Resolution::Unknown))
            .collect(),
        Ok(_) => members
            .iter()
            .map(|(member, _)| {
                (
                    member.clone(),
                    unavailable(PixmResolveError::UnexpectedAnswer),
                )
            })
            .collect(),
        Err(PixmError::Timeout) => members
            .iter()
            .map(|(member, _)| {
                (
                    member.clone(),
                    Resolution::Unavailable(ResolverError::DeadlineExceeded),
                )
            })
            .collect(),
        Err(error) => {
            let shared = Arc::new(error);
            members
                .iter()
                .map(|(member, _)| {
                    (
                        member.clone(),
                        Resolution::Unavailable(ResolverError::Backend(Box::new(SharedExchange(
                            Arc::clone(&shared),
                        )))),
                    )
                })
                .collect()
        }
    }
}

fn unavailable(error: PixmResolveError) -> Resolution {
    Resolution::Unavailable(ResolverError::Backend(Box::new(error)))
}

/// One failed exchange, reported once for every member the Manager serves.
#[derive(Debug)]
struct SharedExchange(Arc<PixmError>);

impl fmt::Display for SharedExchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the PIX Manager could not cross-reference the patient")
    }
}

impl std::error::Error for SharedExchange {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

#[async_trait]
impl Resolver for PixmResolver {
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        let mut out = BTreeMap::new();
        let Some(system) = self.system(patient.namespace()) else {
            for member in members {
                out.insert(
                    member.clone(),
                    unavailable(PixmResolveError::UnmappedNamespace(
                        patient.namespace().clone(),
                    )),
                );
            }
            return out;
        };
        let source = match SourceIdentifier::new(system, SecretString::from(patient.value())) {
            Ok(source) => source,
            Err(_refused) => {
                for member in members {
                    out.insert(
                        member.clone(),
                        unavailable(PixmResolveError::UnmappedNamespace(
                            patient.namespace().clone(),
                        )),
                    );
                }
                return out;
            }
        };
        let timeout = deadline.saturating_duration_since(Instant::now());
        let mut tasks = JoinSet::new();
        for manager in &self.managers {
            let asked: Vec<(NodeId, TargetSystem)> = manager
                .members
                .iter()
                .filter(|(member, _)| members.contains(member))
                .cloned()
                .collect();
            if asked.is_empty() {
                continue;
            }
            let manager = Arc::clone(manager);
            let source = source.clone();
            tasks.spawn(async move { ask(&manager, &source, asked, timeout).await });
        }
        while let Some(joined) = tasks.join_next().await {
            // NOTE: a task that panicked leaves its members out of the map,
            // which the resolution step reads as no answer and fails closed.
            if let Ok(resolutions) = joined {
                out.extend(resolutions);
            }
        }
        out
    }
}

/// Asks one Manager about `asked`, within `timeout`.
async fn ask(
    manager: &Manager,
    source: &SourceIdentifier,
    asked: Vec<(NodeId, TargetSystem)>,
    timeout: Duration,
) -> Vec<(NodeId, Resolution)> {
    if timeout.is_zero() {
        return asked
            .into_iter()
            .map(|(member, _)| {
                (
                    member,
                    Resolution::Unavailable(ResolverError::DeadlineExceeded),
                )
            })
            .collect();
    }
    let targets: Vec<TargetSystem> = asked.iter().map(|(_, domain)| domain.clone()).collect();
    let answer = manager
        .client
        .cross_reference(source, &targets, timeout)
        .await;
    read(answer, &asked)
}
