// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a binding builds for the roles it fills.
//!
//! The resolver and localizer seams, the health indicators, the onward grants
//! and the public documents it has the gateway serve. No specification governs
//! the module layout: our own design.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "binding-ihe")]
use crate::audit::AuditTrailError;
use crate::config::service::{GrantRefused, TlsRefused};
use crate::health::dependencies::Indicator;
use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_identity::role::demographics::Demographics;
use ferrofed_identity::role::localizer::Localizer;
use ferrofed_identity::role::resolver::Resolver;
use ferrohealth_smart::client::dpop::DpopKey;

/// The resolver a binding builds, with the localizer it doubles as.
#[derive(Clone)]
pub struct ResolverSeam {
    /// The resolver.
    pub resolver: Arc<dyn Resolver>,
    /// The resolver as a localizer, with the `localization.mode` it is
    /// declared as, when it names the members that hold the patient.
    pub localizer: Option<(Arc<dyn Localizer>, &'static str)>,
}

impl fmt::Debug for ResolverSeam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolverSeam")
            .field("localizer", &self.localizer.as_ref().map(|(_, mode)| mode))
            .finish_non_exhaustive()
    }
}

/// The localizer a binding builds as its own role (§14, N4).
#[derive(Clone)]
pub struct LocalizerSeam {
    /// The localizer.
    pub localizer: Arc<dyn Localizer>,
    /// The `localization.mode` `OPTIONS {base}/` declares it as.
    pub mode: &'static str,
    /// Where its audit messages go, as `localization.audit` declares it.
    pub audit: Option<&'static str>,
    /// The health indicators of what it records through.
    pub indicators: Vec<Arc<dyn Indicator>>,
}

impl fmt::Debug for LocalizerSeam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalizerSeam")
            .field("mode", &self.mode)
            .field("audit", &self.audit)
            .field("indicators", &self.indicators)
            .finish_non_exhaustive()
    }
}

/// A public document a binding has the gateway serve.
///
/// One example is the DID document its onward grants' keys are resolved by.
/// It is public material, served with no client authentication outside the
/// ITS-REST surface.
///
/// `Debug` shows the path and the media type, never the body.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicDocument {
    /// The absolute request path the document is served at.
    pub path: String,
    /// The `Content-Type` it is served as.
    pub media_type: &'static str,
    /// The document's bytes.
    pub body: Vec<u8>,
}

impl fmt::Debug for PublicDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicDocument")
            .field("path", &self.path)
            .field("media_type", &self.media_type)
            .finish_non_exhaustive()
    }
}

/// Why an onward grant cannot provide its node client.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProvideError {
    /// A grant no compiled binding added (§13.1, N25).
    #[error(
        "{section} names a grant it cannot use: a node takes one with [signing], an IHE FHIR service an oauth2 client-credentials grant"
    )]
    Unclaimed {
        /// The credentials section.
        section: String,
    },
    /// The HTTP client of an endpoint's Nuts grant could not be built.
    #[error("the HTTP client of the Nuts grant of {section} could not be built")]
    #[cfg(feature = "binding-nl")]
    NutsClient {
        /// The credentials section.
        section: String,
        /// Why the client could not be built.
        #[source]
        source: reqwest::Error,
    },
}

/// What one endpoint's binding grant provides its node client.
#[derive(Debug)]
pub struct Provided {
    /// The credentials every request to the node carries.
    pub credentials: SharedCredentials,
    /// The key the node requests are proven with, when the grant binds its
    /// tokens with `DPoP` (RFC 9449).
    pub dpop: Option<Arc<DpopKey>>,
}

/// A localization configuration that cannot be set up.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LocalizationError {
    /// `node_selection = "localized"` is declared and no localizer is
    /// configured, so no undirected patient query could find its node set
    /// (N4).
    #[error(
        "federation.node_selection = \"localized\" needs a localizer: {} (N4, §14.1)",
        crate::binding::localizer_list()
    )]
    NoLocalizer,
    /// `[federation.localization]` is set under a node selection that uses
    /// no localizer.
    #[error(
        "[federation.localization] applies only under federation.node_selection = \"localized\"; remove it, or declare the localized selection"
    )]
    NotLocalized,
    /// A binding's own localizer is configured under a node selection that
    /// uses no localizer.
    #[error(
        "{section} is a localizer and applies only under federation.node_selection = \"localized\"; remove it, or declare the localized selection"
    )]
    Unused {
        /// The localizer's section, such as `[xcpd]`.
        section: &'static str,
    },
    /// An `[nl_gf.nvi.custodians]` value is not a node id.
    #[error("nl_gf.nvi.custodians.{ura:?} is not a node id")]
    #[cfg(feature = "binding-nl")]
    NviMember {
        /// The custodian key, a URA and never a patient value.
        ura: String,
        /// What the id rules reported.
        #[source]
        source: ferrofed_registry::error::IdError,
    },
    /// An `nl_gf.nvi.namespaces` entry is empty.
    #[error("nl_gf.nvi.namespaces has an empty namespace")]
    #[cfg(feature = "binding-nl")]
    NviNamespace(#[source] ferrofed_identity::role::patient::PatientRefError),
    /// A credentials section names a grant, which only a node takes.
    #[error("the localizer's credentials cannot be used")]
    Grant(#[source] GrantRefused),
    /// The localizer's TLS material does not read.
    #[error("the localizer's TLS material cannot be used")]
    Tls(#[source] TlsRefused),
    /// The NVI localizer refuses its configuration.
    #[error("the [nl_gf.nvi] localizer cannot be enabled")]
    #[cfg(feature = "binding-nl")]
    Nvi(#[source] ferrofed_identity::nl::nvi::NviConfigError),
    /// The HTTP client the Nuts grant of `[nl_gf.nvi.credentials]` sends its
    /// token requests through could not be built.
    #[error("the HTTP client of nl_gf.nvi.credentials.nuts could not be built")]
    #[cfg(feature = "binding-nl")]
    NviNutsClient(#[source] reqwest::Error),
    /// The TLS material of the MyHealth@EU gateways cannot build their
    /// revocation check.
    #[error("the TLS material of the MyHealth@EU responding gateways cannot be used")]
    #[cfg(feature = "binding-ihe")]
    XcpdRevocation(#[source] ferrohealth_server::revocation::RevocationSetupError),
    /// The XUA assertion is not one SAML 2.0 `Assertion` element.
    #[error("{key} is not one SAML 2.0 Assertion element")]
    #[cfg(feature = "binding-ihe")]
    XcpdAssertion {
        /// The key the assertion was read from.
        key: &'static str,
    },
    /// An `[xcpd.communities]` value is not a node id.
    #[error("xcpd.communities.{community:?} is not a node id")]
    #[cfg(feature = "binding-ihe")]
    XcpdMember {
        /// The community key, an OID.
        community: String,
        /// What the id rules reported.
        #[source]
        source: ferrofed_registry::error::IdError,
    },
    /// An `[xcpd.namespaces]` key is empty.
    #[error("xcpd.namespaces has an empty namespace")]
    #[cfg(feature = "binding-ihe")]
    XcpdNamespace(#[source] ferrofed_identity::role::patient::PatientRefError),
    /// The XCPD localizer refuses its configuration.
    #[error("the [xcpd] localizer cannot be enabled")]
    #[cfg(feature = "binding-ihe")]
    Xcpd(#[source] ferrofed_identity::ihe::xcpd::XcpdConfigError),
    /// The audit trail to the ATNA Audit Record Repository cannot start.
    #[error("the ITI-20 audit trail of the [xcpd] localizer cannot start")]
    #[cfg(feature = "binding-ihe")]
    AuditTrail(#[source] AuditTrailError),
    /// `audit = "repository"` names no `[xcpd.audit_repository]`.
    #[error("xcpd.audit = \"repository\" needs [xcpd.audit_repository]")]
    #[cfg(feature = "binding-ihe")]
    NoAuditRepository,
}

/// The demographics step a patient identifier the cross-reference does not
/// map is taken to before localization and resolution, with its budget
/// (Annex A §A.2).
///
/// A registry reload builds it again from `[pdqm]`, as it builds the resolver.
#[derive(Clone)]
pub struct DemographicsStep {
    step: Arc<dyn Demographics>,
    timeout: Duration,
}

impl DemographicsStep {
    /// The step `step`, each exchange of which may take `timeout`.
    #[must_use]
    pub fn new(step: Arc<dyn Demographics>, timeout: Duration) -> Self {
        Self { step, timeout }
    }

    /// The demographics service.
    #[must_use]
    pub fn step(&self) -> &dyn Demographics {
        self.step.as_ref()
    }

    /// How long one exchange may take.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl fmt::Debug for DemographicsStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DemographicsStep")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// The gateway a binding's identity feed route serves in: its feed, its
/// counter and the federation a message changes.
pub trait FeedHost: Send + Sync + 'static {
    /// The PMIR identity feed, when `[pmir]` is set.
    #[cfg(feature = "binding-ihe")]
    fn identity_feed(&self) -> Option<&Arc<crate::binding::ihe::pmir::IdentityFeed>>;

    /// Counts an ITI-93 message that ended as `result`.
    #[cfg(feature = "binding-ihe")]
    fn feed_counted(&self, result: crate::metrics::identity_feed::FeedResult);

    /// Applies `change` to the resolution bindings of the running federation
    /// and returns how many bindings it dropped, or `None` while no
    /// federation runs.
    #[cfg(feature = "binding-ihe")]
    fn identity_changed(
        &self,
        change: Option<&ferrofed_identity::session::IdentityChange>,
    ) -> Option<usize>;
}

/// What a binding that keeps the registry in step with a source of its own
/// reports a change of that source to: the gateway's reload.
pub trait RegistrySink: Send + Sync + 'static {
    /// What a change the gateway applied reports.
    type Applied: fmt::Debug + Send;
    /// Why the gateway refused a change; the running registry stays.
    type Refused: fmt::Debug + Send;

    /// Puts the registry `snapshot` the source changed to in place of the
    /// running one, checked as a reload is.
    ///
    /// # Errors
    ///
    /// The [`RegistrySink::Refused`] of a federation that cannot be built
    /// over `snapshot`.
    fn directory_changed(
        &self,
        snapshot: ferrofed_registry::snapshot::RegistrySnapshot,
    ) -> Result<Self::Applied, Self::Refused>;

    /// The refusal of a change the source answered with that `refusal`
    /// stopped before the gateway saw a registry.
    fn directory_refused(&self, refusal: crate::binding::error::BindingError) -> Self::Refused;

    /// Whether `refused` is of a registry that breaks a rule of its own,
    /// apart from a sound one the rest of the configuration does not fit.
    fn registry_invalid(refused: &Self::Refused) -> bool;
}
