// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a binding builds for the roles it fills.
//!
//! The resolver and localizer seams, the health indicators, the onward grants
//! and the public documents it has the gateway serve. No specification governs
//! the module layout: our own design.

use std::fmt;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_identity::role::localizer::Localizer;
use ferrofed_identity::role::resolver::Resolver;
use ferrofed_operator_api::health::Indication;

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

/// A source of indications a binding adds to `GET {base}/operator/dependencies`.
pub trait Indicator: fmt::Debug + Send + Sync {
    /// Returns its indications, each under the key the report names it by.
    fn indicate(&self) -> Vec<(&'static str, Indication)>;
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
    pub dpop: Option<Arc<Prover>>,
}
