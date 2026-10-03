// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer: the [`Localizer`] seam over ITI-55 Cross Gateway
//! Patient Discovery, the proposed localization binding (N4, §14.1,
//! Annex A.3; ITI TF-2 §3.55).
//!
//! Discovery is a broadcast: every configured Responding Gateway is asked at
//! once, by the patient's shared identifier, within the localization
//! budget. Each community a gateway answers a match in maps, through the
//! configuration, to the registry member that serves it, and those members
//! are the candidates. A match is candidacy, never clearance (§14.3):
//! ITI-55 "carries no consent decision" (Annex A.3).
//!
//! The localizer fails closed with the seam. One gateway that does not
//! answer, answers an error, or asks for demographics the gateway never
//! sends leaves the whole discovery [`Localization::Unavailable`]: a
//! community behind that gateway might hold the patient, and a partial
//! candidate set would report its member `not-localized` with no error, as
//! if nothing had failed (§14.1). No specification governs a partial
//! broadcast: our own design.
//!
//! The patient identifier reaches the responding gateways only, which is the
//! transaction's purpose, inside `ihe_iti`'s redacting types; nothing here
//! logs it, and no error carries it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ihe_iti::xcpd::XcpdClient;
use ihe_iti::xcpd::audit::{self, AuditError, AuditEvent, AuditRecorder};
use ihe_iti::xcpd::discovery::Discovery;
use ihe_iti::xcpd::error::{InvalidInput, XcpdError};
use ihe_iti::xcpd::identifier::{HomeCommunityId, Oid, PatientIdentifier};
use ihe_iti::xcpd::request::{DiscoveryQuery, RespondingGateway};
use ihe_iti::xcpd::security::XuaAssertion;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::task::JoinSet;
use url::Url;

use crate::localizer::{Localization, Localizer, LocalizerError};
use crate::patient::{IdentifierNamespace, PatientRef};

/// The `tracing` target the [`LogAudit`] recorder writes to.
pub const AUDIT_TARGET: &str = "ferrofed::audit";

/// The audit recorder that writes each ITI-55 audit message as a structured
/// `tracing` event at [`AUDIT_TARGET`], for a deployment that routes its log
/// to its audit repository.
///
/// The event carries every field of the message but the query parameters,
/// which name the patient identifier: it says that they were recorded, and
/// never what they hold. It accepts every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct LogAudit;

impl AuditRecorder for LogAudit {
    fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let (access_point_type, access_point) = event
            .destination_access_point
            .as_ref()
            .map_or(("", String::new()), |point| (point.type_code(), point.id()));
        let mut destination = event.destination.clone();
        destination.set_query(None);
        tracing::info!(
            target: AUDIT_TARGET,
            event_id = audit::EVENT_ID.0,
            event_action = audit::EVENT_ACTION,
            event_type = audit::EVENT_TYPE.0,
            event_date_time = %event.date_time,
            event_outcome = event.outcome.code(),
            source_alternative_user_id = event.process_id,
            destination_user_id = %destination,
            destination_access_point_type = access_point_type,
            destination_access_point = %access_point,
            home_community = event.home_community.as_ref().map(ToString::to_string),
            participant_object_query = "recorded, not logged",
            "ITI-55 Cross Gateway Patient Discovery audit message"
        );
        Ok(())
    }
}

/// Where a deployment's XUA assertion comes from (ITI-40).
///
/// The gateway signs nothing: an identity provider or a security token
/// service issues and signs the assertion, and the localizer asks this
/// source for one before each discovery. An assertion the source cannot
/// give fails the discovery closed.
#[async_trait]
pub trait AssertionSource: Send + Sync {
    /// Returns the assertion the next discovery sends.
    ///
    /// # Errors
    ///
    /// An [`AssertionError`] when no valid assertion is available.
    async fn assertion(&self) -> Result<XuaAssertion, AssertionError>;
}

/// Why no XUA assertion is available.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AssertionError {
    /// The source could not provide one.
    #[error("the XUA assertion source could not provide an assertion")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// One fixed assertion, read once from the deployment's configuration.
///
/// An assertion expires (SAML 2.0 Core §2.5.1, `NotOnOrAfter`); a
/// deployment that holds a fixed one replaces it before it does and reloads
/// the gateway, and one that needs an assertion per request implements
/// [`AssertionSource`] over its token service.
#[derive(Debug, Clone)]
pub struct FixedAssertion(XuaAssertion);

impl FixedAssertion {
    /// The source that always gives `assertion`.
    #[must_use]
    pub fn new(assertion: XuaAssertion) -> Self {
        Self(assertion)
    }

    /// The source that always gives the assertion `xml` holds.
    ///
    /// # Errors
    /// [`InvalidInput::Assertion`] when `xml` is not one SAML 2.0
    /// `Assertion` element ([`XuaAssertion::new`]).
    pub fn from_xml(xml: &SecretString) -> Result<Self, InvalidInput> {
        XuaAssertion::new(xml.expose_secret()).map(Self)
    }
}

#[async_trait]
impl AssertionSource for FixedAssertion {
    async fn assertion(&self) -> Result<XuaAssertion, AssertionError> {
        Ok(self.0.clone())
    }
}

/// How the gateway reaches the responding gateways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// `https` only: the identifier and the assertion never cross the
    /// network in clear text (ITI TF-1 §27.4.1).
    Encrypted,
    /// `https` or `http`, for a configuration marked for development only.
    UnencryptedForDevelopment,
}

/// The TLS material of the ATNA secure channel (ITI TF-1 Table 27.1.3-1).
///
/// `Debug` shows neither.
#[derive(Default)]
pub struct Tls {
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS.
    pub identity: Option<SecretString>,
    /// Trust roots the network uses beside the platform's, PEM.
    pub roots: Option<String>,
}

impl fmt::Debug for Tls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tls")
            .field("identity", &self.identity.is_some())
            .field("roots", &self.roots.is_some())
            .finish()
    }
}

/// One responding gateway as the configuration names it.
#[derive(Debug)]
pub struct GatewayConfig {
    /// The SOAP endpoint, which `Debug` shows without its userinfo.
    pub endpoint: SecretUrl,
    /// The receiver device's OID.
    pub device: String,
    /// The one community to ask for, when the gateway serves several
    /// (§3.55.4.1.2.5).
    pub community: Option<String>,
}

/// The XCPD localizer as the configuration names it.
#[derive(Debug)]
pub struct XcpdConfig {
    /// The gateway's own device OID, the sender of every request.
    pub sender_device: String,
    /// The gateway's own `homeCommunityId`, when it has one
    /// (§3.55.4.1.2.4).
    pub home_community: Option<String>,
    /// The responding gateways, every one asked on each discovery.
    pub gateways: Vec<GatewayConfig>,
    /// Each community, by its `homeCommunityId`, mapped to the registry
    /// member that serves it.
    pub communities: BTreeMap<String, NodeId>,
    /// A client's issuing namespace mapped to the assigning authority OID it
    /// stands for, when the namespace is not itself an OID.
    pub namespaces: BTreeMap<IdentifierNamespace, String>,
    /// How the responding gateways are reached.
    pub transport: Transport,
    /// The TLS material.
    pub tls: Tls,
}

/// An XCPD localizer that cannot be built.
///
/// The errors name a gateway by its position, a community or a member, never
/// an identifier value or a credential.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum XcpdConfigError {
    /// No responding gateway is configured.
    #[error("the XCPD localizer names no responding gateway")]
    NoGateway,
    /// The sender device is not an OID.
    #[error("the sender device is not an ISO OID")]
    SenderDevice(#[source] InvalidInput),
    /// The gateway's own home community is not an OID.
    #[error("the home community is not an ISO OID")]
    HomeCommunity(#[source] InvalidInput),
    /// A gateway's endpoint does not parse as a URL.
    #[error("the endpoint of responding gateway {index} is not a URL")]
    EndpointUrl {
        /// The gateway's position, counted from 0.
        index: usize,
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// A gateway's endpoint is refused: not `https` outside development, or
    /// not `http(s)` at all.
    #[error(
        "the endpoint of responding gateway {index} is not an https URL (http only under the development profile)"
    )]
    Endpoint {
        /// The gateway's position, counted from 0.
        index: usize,
        /// What the XCPD client reported.
        #[source]
        source: InvalidInput,
    },
    /// A gateway's receiver device or target community is not an OID.
    #[error("responding gateway {index} names a device or community that is not an ISO OID")]
    GatewayOid {
        /// The gateway's position, counted from 0.
        index: usize,
        /// What the XCPD client reported.
        #[source]
        source: InvalidInput,
    },
    /// A community key is not an OID.
    #[error("the community {community:?} is not an ISO OID")]
    Community {
        /// The key as written, an OID and never a patient value.
        community: String,
        /// What the XCPD client reported.
        #[source]
        source: InvalidInput,
    },
    /// Two keys name one community.
    #[error("the community {0} is mapped twice")]
    DuplicateCommunity(HomeCommunityId),
    /// A community maps to a member the registry does not hold.
    #[error("a community maps to member {0}, which is not in the registry")]
    UnknownMember(NodeId),
    /// A registry member is served by no community, so no discovery could
    /// ever name it and every undirected query would leave it
    /// `not-localized`.
    #[error("registry member {0} is served by no XCPD community")]
    UnlocatedMember(NodeId),
    /// A namespace mapping is not an OID.
    #[error("the assigning authority mapped from namespace {0} is not an ISO OID")]
    Namespace(IdentifierNamespace),
    /// The client certificate and key do not read as PEM.
    #[error("the XCPD client certificate and key are not PEM")]
    Identity(#[source] reqwest::Error),
    /// The trust roots do not read as PEM certificates.
    #[error("the XCPD trust roots are not PEM certificates")]
    Roots(#[source] reqwest::Error),
    /// The HTTP client could not be built.
    #[error("the HTTP client for the responding gateways could not be built")]
    Client(#[source] reqwest::Error),
}

/// Why the XCPD localizer could not answer.
///
/// None carries an identifier value or a gateway's free text.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum XcpdLocalizeError {
    /// The patient's issuing namespace maps to no assigning authority OID.
    #[error("the namespace {0} maps to no assigning authority OID")]
    UnmappedNamespace(IdentifierNamespace),
    /// No XUA assertion was available.
    #[error("no XUA assertion was available")]
    Assertion(#[source] AssertionError),
    /// A responding gateway's discovery failed.
    #[error("responding gateway {index} could not discover the patient")]
    Gateway {
        /// The gateway's position, counted from 0.
        index: usize,
        /// What the XCPD client reported.
        #[source]
        source: XcpdError,
    },
    /// A responding gateway asked for demographics, which the gateway never
    /// sends (§3.55.4.2.3 Case 3), so whether its communities hold the
    /// patient is unknown.
    #[error("responding gateway {index} asked for demographics, which the gateway never sends")]
    DemographicsRequested {
        /// The gateway's position, counted from 0.
        index: usize,
    },
    /// The task asking a responding gateway did not finish.
    #[error("the discovery at responding gateway {index} did not finish")]
    Task {
        /// The gateway's position, counted from 0.
        index: usize,
    },
}

/// The [`Localizer`] over ITI-55.
pub struct XcpdLocalizer {
    client: XcpdClient,
    sender: Oid,
    home: Option<HomeCommunityId>,
    gateways: Vec<RespondingGateway>,
    communities: BTreeMap<HomeCommunityId, NodeId>,
    namespaces: BTreeMap<IdentifierNamespace, Oid>,
    assertion: Option<Arc<dyn AssertionSource>>,
}

impl XcpdLocalizer {
    /// Builds the localizer `config` describes over the members of
    /// `registry`, sending the assertions `assertion` gives when it is
    /// set.
    ///
    /// # Errors
    /// An [`XcpdConfigError`] for no gateway, an identifier that is not an
    /// OID, an endpoint refused under `config.transport`, a community mapped
    /// twice or to an unknown member, a member no community serves, TLS
    /// material that does not read as PEM, or an HTTP client that cannot be
    /// built.
    pub fn from_config(
        config: XcpdConfig,
        assertion: Option<Arc<dyn AssertionSource>>,
        registry: &RegistrySnapshot,
    ) -> Result<Self, XcpdConfigError> {
        if config.gateways.is_empty() {
            return Err(XcpdConfigError::NoGateway);
        }
        let sender = Oid::new(&config.sender_device).map_err(XcpdConfigError::SenderDevice)?;
        let home = config
            .home_community
            .as_deref()
            .map(Oid::new)
            .transpose()
            .map_err(XcpdConfigError::HomeCommunity)?
            .map(HomeCommunityId::new);
        let gateways = config
            .gateways
            .iter()
            .enumerate()
            .map(|(index, gateway)| responding(index, gateway, config.transport))
            .collect::<Result<Vec<_>, _>>()?;
        let communities = communities(config.communities, registry)?;
        let mut namespaces = BTreeMap::new();
        for (namespace, authority) in config.namespaces {
            let Ok(oid) = Oid::new(&authority) else {
                return Err(XcpdConfigError::Namespace(namespace));
            };
            namespaces.insert(namespace, oid);
        }
        Ok(Self {
            client: XcpdClient::new(http_client(&config.tls)?),
            sender,
            home,
            gateways,
            communities,
            namespaces,
            assertion,
        })
    }

    /// This localizer, recording the ITI-55 audit message of every exchange
    /// through `recorder` (ITI TF-2 §3.55.5.1.1).
    #[must_use]
    pub fn audited(mut self, recorder: Arc<dyn AuditRecorder>) -> Self {
        self.client = self.client.audited(recorder);
        self
    }

    /// The assigning authority `namespace` stands for.
    fn authority(&self, namespace: &IdentifierNamespace) -> Option<Oid> {
        if let Some(oid) = self.namespaces.get(namespace) {
            return Some(oid.clone());
        }
        // NOTE: no specification governs this: our own design. A namespace
        // that is itself an OID, dotted or as a URN, names the authority.
        Oid::new(namespace.as_str()).ok()
    }

    /// The discovery of `patient`, or why there is none.
    async fn discover(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> Result<Localization, LocalizerError> {
        let backend = |error: XcpdLocalizeError| LocalizerError::Backend(Box::new(error));
        let authority = self.authority(patient.namespace()).ok_or_else(|| {
            backend(XcpdLocalizeError::UnmappedNamespace(
                patient.namespace().clone(),
            ))
        })?;
        let identifier = PatientIdentifier::new(authority, SecretString::from(patient.value()))
            .map_err(|_empty| {
                backend(XcpdLocalizeError::UnmappedNamespace(
                    patient.namespace().clone(),
                ))
            })?;
        let mut query = DiscoveryQuery::new(self.sender.clone(), identifier);
        if let Some(home) = &self.home {
            query = query.sent_for(home.clone());
        }
        let assertion = match &self.assertion {
            Some(source) => Some(
                source
                    .assertion()
                    .await
                    .map_err(|error| backend(XcpdLocalizeError::Assertion(error)))?,
            ),
            None => None,
        };
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(LocalizerError::DeadlineExceeded);
        }
        let mut answers = self.broadcast(&query, assertion, timeout).await;
        // NOTE: ITI TF-2 §3.55.5.1, §14.1: an exchange whose audit was not recorded is
        // reported as that before any other failure, since no failure policy may widen it.
        let unaudited = answers
            .iter()
            .position(|answer| matches!(answer, Some(Err(XcpdError::Audit(_)))));
        if let Some(index) = unaudited
            && let Some(Some(Err(source))) = answers.get_mut(index).map(Option::take)
        {
            let error = XcpdLocalizeError::Gateway { index, source };
            return Err(LocalizerError::AuditFailed(Box::new(error)));
        }
        let mut candidates = BTreeSet::new();
        for (index, answer) in answers.into_iter().enumerate() {
            match answer {
                Some(Ok(Discovery::Matched(found))) => {
                    for record in &found {
                        // NOTE: §3.55.4.2.2.4: a community outside the federation names no member, so it adds none.
                        if let Some(member) = self.communities.get(record.community())
                            && members.contains(member)
                        {
                            candidates.insert(member.clone());
                        }
                    }
                }
                Some(Ok(Discovery::NoMatch)) => {}
                Some(Ok(_)) => {
                    // NOTE: §3.55.4.2.3 Case 3 is answered with a 200 (§3.55.4.2.2.6).
                    return Err(LocalizerError::Answered {
                        status: http::StatusCode::OK,
                        source: Box::new(XcpdLocalizeError::DemographicsRequested { index }),
                    });
                }
                Some(Err(XcpdError::Timeout)) => return Err(LocalizerError::DeadlineExceeded),
                Some(Err(source)) => {
                    let status = source.status();
                    let error = XcpdLocalizeError::Gateway { index, source };
                    return Err(match status {
                        Some(status) => LocalizerError::Answered {
                            status,
                            source: Box::new(error),
                        },
                        None => backend(error),
                    });
                }
                None => return Err(backend(XcpdLocalizeError::Task { index })),
            }
        }
        Ok(if candidates.is_empty() {
            Localization::NoRecords
        } else {
            Localization::Candidates(candidates)
        })
    }

    /// Asks every gateway at once, within `timeout`; each answer in gateway
    /// order, `None` for a task that did not finish.
    async fn broadcast(
        &self,
        query: &DiscoveryQuery,
        assertion: Option<XuaAssertion>,
        timeout: Duration,
    ) -> Vec<Option<Result<Discovery, XcpdError>>> {
        let mut tasks = JoinSet::new();
        for (index, gateway) in self.gateways.iter().enumerate() {
            let client = self.client.clone();
            let gateway = gateway.clone();
            let query = query.clone();
            let assertion = assertion.clone();
            tasks.spawn(async move {
                let answer = client
                    .discover(&gateway, &query, assertion.as_ref(), timeout)
                    .await;
                (index, answer)
            });
        }
        let mut answers: Vec<Option<Result<Discovery, XcpdError>>> =
            std::iter::repeat_with(|| None)
                .take(self.gateways.len())
                .collect();
        while let Some(joined) = tasks.join_next().await {
            if let Ok((index, answer)) = joined
                && let Some(slot) = answers.get_mut(index)
            {
                *slot = Some(answer);
            }
        }
        answers
    }
}

impl fmt::Debug for XcpdLocalizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XcpdLocalizer")
            .field("sender", &self.sender)
            .field("gateways", &self.gateways)
            .field("communities", &self.communities)
            .field("assertion", &self.assertion.is_some())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Localizer for XcpdLocalizer {
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        deadline: Instant,
    ) -> Localization {
        match self.discover(patient, members, deadline).await {
            Ok(localization) => localization,
            Err(error) => Localization::Unavailable(error),
        }
    }
}

/// The responding gateway at position `index`, reached as `transport` allows.
fn responding(
    index: usize,
    gateway: &GatewayConfig,
    transport: Transport,
) -> Result<RespondingGateway, XcpdConfigError> {
    let endpoint = Url::parse(gateway.endpoint.expose())
        .map_err(|source| XcpdConfigError::EndpointUrl { index, source })?;
    let device = Oid::new(&gateway.device)
        .map_err(|source| XcpdConfigError::GatewayOid { index, source })?;
    let built = match transport {
        Transport::Encrypted => RespondingGateway::new(endpoint, device),
        Transport::UnencryptedForDevelopment => {
            RespondingGateway::unencrypted_for_development(endpoint, device)
        }
    }
    .map_err(|source| XcpdConfigError::Endpoint { index, source })?;
    match &gateway.community {
        Some(community) => {
            let oid = Oid::new(community)
                .map_err(|source| XcpdConfigError::GatewayOid { index, source })?;
            Ok(built.targeting(HomeCommunityId::new(oid)))
        }
        None => Ok(built),
    }
}

/// The community map, every member of `registry` served by one community.
fn communities(
    written: BTreeMap<String, NodeId>,
    registry: &RegistrySnapshot,
) -> Result<BTreeMap<HomeCommunityId, NodeId>, XcpdConfigError> {
    let mut mapped = BTreeMap::new();
    for (community, member) in written {
        let oid = Oid::new(&community).map_err(|source| XcpdConfigError::Community {
            community: community.clone(),
            source,
        })?;
        if registry.node(&member).is_none() {
            return Err(XcpdConfigError::UnknownMember(member));
        }
        let home = HomeCommunityId::new(oid);
        if mapped.contains_key(&home) {
            return Err(XcpdConfigError::DuplicateCommunity(home));
        }
        mapped.insert(home, member);
    }
    let served: BTreeSet<&NodeId> = mapped.values().collect();
    if let Some(unlocated) = registry
        .nodes()
        .map(ferrofed_registry::snapshot::Node::id)
        .find(|id| !served.contains(id))
    {
        return Err(XcpdConfigError::UnlocatedMember(unlocated.clone()));
    }
    Ok(mapped)
}

/// The HTTP client the gateways are asked through: no redirects, because the
/// request body holds the patient identifier, and the network's TLS
/// material.
fn http_client(tls: &Tls) -> Result<reqwest::Client, XcpdConfigError> {
    let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
    if let Some(identity) = &tls.identity {
        let identity = reqwest::Identity::from_pem(identity.expose_secret().as_bytes())
            .map_err(XcpdConfigError::Identity)?;
        builder = builder.identity(identity);
    }
    if let Some(roots) = &tls.roots {
        let roots = reqwest::Certificate::from_pem_bundle(roots.as_bytes())
            .map_err(XcpdConfigError::Roots)?;
        builder = builder.tls_certs_merge(roots);
    }
    builder.build().map_err(XcpdConfigError::Client)
}
