// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer, `[xcpd]`: the responding gateways an undirected
//! patient query's node set is discovered at, and the communities each
//! registry member serves (N4, §14.1, Annex A.3; ITI TF-2 §3.55).
//!
//! ```toml
//! [xcpd]
//! sender_device = "2.999.40.1"
//! home_community = "2.999.40"
//! assertion_file = "/run/secrets/xua-assertion.xml"
//! client_identity_file = "/run/secrets/xcpd-client.pem"
//! trust_roots_file = "/etc/ferrofed/xcpd-roots.pem"
//!
//! [[xcpd.gateway]]
//! url = "https://xcpd.example.org/RespondingGateway"
//! device = "2.999.50.1"
//!
//! [xcpd.communities]
//! "2.999.50" = "node-a"
//! ```
//!
//! A gateway URL is `https` unless the configuration is marked
//! `profile = "development"`: the request carries the patient identifier and
//! the XUA assertion (ITI TF-1 §27.4.1), so it is held to the
//! protected-payload policy of [`transport`]. No specification governs the
//! shape of the table: our own design.
//!
//! A gateway marked `myhealth_eu = true` is a MyHealth@EU route, an eHDSI
//! link. It is `https` in every profile, needs `client_identity_file` and
//! `trust_roots_file` (Section II Security Services 10.0.0 §5.9.1 and
//! §5.9.2), and has its certificate chain checked for revocation before each
//! discovery (X.509 Certificates Profile 7.0.0 §3.2). A fixed `assertion`
//! never serves it: Messaging Profile 10.0.0 §1.5 admits no alternative
//! binding, and SAML Profile 10.0.0 §2.1 bounds the identity assertion to 4
//! hours, `sender-vouches`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use ferrofed_identity::profile::Profile;
use ferrohealth_secret::secret::Secret;
use ferrohealth_secret::secret_url::SecretUrl;
use serde::Deserialize;

use crate::config::audit::AuditDestination;
use crate::config::error::Error;
use crate::config::error::binding::{BindingFault, MyHealthEuNeed};
use crate::config::ihe::audit_repository::{AuditRepository, AuditRepositorySettings};
use crate::config::{Config, transport};
use ferrohealth_secret::file::read_secret;

/// The XCPD localizer, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Xcpd {
    /// The gateway's own device OID, the sender of every request.
    pub sender_device: String,
    /// The gateway's own `homeCommunityId`; required under
    /// `audit = "repository"`, whose audit message names it.
    pub home_community: Option<String>,
    /// The responding gateways, one `[[xcpd.gateway]]` each, every one asked
    /// on each discovery.
    pub gateway: Vec<XcpdGateway>,
    /// Each community, by its `homeCommunityId`, mapped to the registry
    /// member that serves it; every member needs one.
    pub communities: BTreeMap<String, String>,
    /// A client's issuing namespace mapped to the assigning authority OID it
    /// stands for, when the namespace is not itself an OID.
    pub namespaces: BTreeMap<String, String>,
    /// The signed SAML 2.0 XUA assertion every request carries (ITI-40),
    /// inline or through `assertion_file`.
    pub assertion: Option<Secret>,
    /// A file holding the assertion, read at boot.
    pub assertion_file: Option<PathBuf>,
    /// The gateway's client certificate chain and private key, PEM, for the
    /// mutual TLS of the ATNA secure channel, inline or through
    /// `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the responding gateways' certificates chain
    /// to, beside the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// Where the ITI-55 audit message of every exchange goes: `repository`,
    /// `log`, or `off`, which only `profile = "development"` admits. It has
    /// no default.
    pub audit: Option<AuditDestination>,
    /// The ATNA Audit Record Repository, under `audit = "repository"`.
    pub audit_repository: Option<AuditRepository>,
}

/// One responding gateway.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct XcpdGateway {
    /// The SOAP endpoint, `https`; no rendering shows its userinfo.
    pub url: SecretUrl,
    /// The receiver device's OID.
    pub device: String,
    /// The one community to ask for, when the gateway serves several.
    pub community: Option<String>,
    /// Whether the gateway is a MyHealth@EU route: an eHDSI link that
    /// checks the gateway's certificate chain for revocation before each
    /// discovery (eHDSI X.509 Certificates Profile 7.0.0 §3.2), and that no
    /// fixed assertion serves. Off by default.
    pub myhealth_eu: bool,
}

/// The XCPD localizer, with every secret and file read.
pub struct XcpdSettings {
    /// The sender device OID.
    pub sender_device: String,
    /// The gateway's own `homeCommunityId`.
    pub home_community: Option<String>,
    /// The responding gateways.
    pub gateways: Vec<XcpdGateway>,
    /// The community map, as written.
    pub communities: BTreeMap<String, String>,
    /// The namespace map, as written.
    pub namespaces: BTreeMap<String, String>,
    /// The XUA assertion, not yet checked as one.
    pub assertion: Option<Secret>,
    /// The key the assertion was read from, for an error that names it.
    pub assertion_key: &'static str,
    /// The client certificate chain and key.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots.
    pub trust_roots: Option<String>,
    /// Where the audit messages go.
    pub audit: AuditDestination,
    /// The repository, under [`AuditDestination::Repository`].
    pub audit_repository: Option<AuditRepositorySettings>,
}

impl fmt::Debug for XcpdSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XcpdSettings")
            .field("sender_device", &self.sender_device)
            .field("gateways", &self.gateways)
            .field("communities", &self.communities)
            .field("assertion", &self.assertion.is_some())
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .field("audit", &self.audit)
            .field("audit_repository", &self.audit_repository)
            .finish_non_exhaustive()
    }
}

/// Resolves `[xcpd]`: a sender device, every gateway URL `https` outside
/// development, and every secret and file read.
///
/// # Errors
/// [`Error::Missing`] for no registry document, no `sender_device`, no
/// `audit`, no gateway, or no `home_community` under `audit = "repository"`,
/// [`Error::Url`]
/// for a gateway URL that does not parse, [`Error::Cleartext`] for a gateway
/// URL that is not `https` outside the development profile,
/// [`BindingFault::AuditOff`] for `audit = "off"` outside it,
/// [`BindingFault::FixedAssertionOnMyHealthEu`] and
/// [`BindingFault::MyHealthEuLink`] for a MyHealth@EU gateway a fixed
/// assertion would serve or that lacks what an eHDSI link needs, and the
/// errors of a secret or a file that cannot be read.
pub(crate) fn resolve(config: &Config) -> Result<Option<XcpdSettings>, Error> {
    let Some(xcpd) = &config.xcpd else {
        return Ok(None);
    };
    // NOTE: no specification governs this: our own design; the community map
    // names registry members, so it means nothing without a registry.
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    if xcpd.sender_device.is_empty() {
        return Err(Error::Missing {
            key: String::from("xcpd.sender_device"),
        });
    }
    if xcpd.gateway.is_empty() {
        return Err(Error::Missing {
            key: String::from("xcpd.gateway"),
        });
    }
    let (audit, audit_repository) = audit(config.profile, xcpd)?;
    let assertion_key = if xcpd.assertion_file.is_some() {
        "xcpd.assertion_file"
    } else {
        "xcpd.assertion"
    };
    let carried =
        (xcpd.assertion.is_some() || xcpd.assertion_file.is_some()).then_some(assertion_key);
    for (index, gateway) in xcpd.gateway.iter().enumerate() {
        let key = format!("xcpd.gateway[{index}]");
        url::Url::parse(gateway.url.expose()).map_err(|source| Error::Url {
            key: format!("{key}.url"),
            source,
        })?;
        // NOTE: ITI TF-1 §27.4.1: the request carries the identifier and the XUA
        // assertion; a site admitted under development is reported by transport::check.
        transport::protected_payload(
            config.profile,
            gateway.url.expose(),
            transport::identity_site(&key, carried),
        )?;
        if gateway.myhealth_eu {
            myhealth_eu(xcpd, gateway, &key)?;
        }
    }
    let assertion = read_secret(
        "xcpd.assertion",
        xcpd.assertion.clone(),
        xcpd.assertion_file.as_deref(),
    )?;
    let client_identity = read_secret(
        "xcpd.client_identity",
        xcpd.client_identity.clone(),
        xcpd.client_identity_file.as_deref(),
    )?;
    let trust_roots = xcpd
        .trust_roots_file
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: String::from("xcpd.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(Some(XcpdSettings {
        sender_device: xcpd.sender_device.clone(),
        home_community: xcpd.home_community.clone(),
        gateways: xcpd.gateway.clone(),
        communities: xcpd.communities.clone(),
        namespaces: xcpd.namespaces.clone(),
        assertion_key,
        assertion,
        client_identity,
        trust_roots,
        audit,
        audit_repository,
    }))
}

/// Refuses `gateway`, the MyHealth@EU gateway at `key` of `xcpd`, when a
/// fixed assertion would serve it or it lacks what an eHDSI link needs.
fn myhealth_eu(xcpd: &Xcpd, gateway: &XcpdGateway, key: &str) -> Result<(), Error> {
    let refused = |needs| {
        Err(Error::Binding(BindingFault::MyHealthEuLink {
            gateway: key.to_owned(),
            needs,
        }))
    };
    if xcpd.assertion.is_some() || xcpd.assertion_file.is_some() {
        return Err(Error::Binding(BindingFault::FixedAssertionOnMyHealthEu {
            gateway: key.to_owned(),
        }));
    }
    // NOTE: Section II Security Services 10.0.0 §5.9.1 and §5.9.2: an NCP link is TLS
    // with mutual authentication under the eHDSI PKI, in development as anywhere.
    if !url::Url::parse(gateway.url.expose()).is_ok_and(|url| url.scheme() == "https") {
        return refused(MyHealthEuNeed::Https);
    }
    if xcpd.client_identity.is_none() && xcpd.client_identity_file.is_none() {
        return refused(MyHealthEuNeed::ClientIdentity);
    }
    if xcpd.trust_roots_file.is_none() {
        return refused(MyHealthEuNeed::TrustRoots);
    }
    Ok(())
}

/// Where `xcpd`'s audit messages go under `profile`, with the repository
/// they go to when it is one.
fn audit(
    profile: Profile,
    xcpd: &Xcpd,
) -> Result<(AuditDestination, Option<AuditRepositorySettings>), Error> {
    // NOTE: ITI TF-2 §3.55.5.1, ITI TF-1 Table 27.1.3-1: the actor records every
    // exchange, so no audit at all is a declared, development-only choice.
    let audit = match xcpd.audit {
        None => {
            return Err(Error::Missing {
                key: String::from("xcpd.audit"),
            });
        }
        Some(AuditDestination::Off) if profile != Profile::Development => {
            return Err(Error::Binding(BindingFault::AuditOff {
                key: String::from("xcpd.audit"),
            }));
        }
        Some(destination) => destination,
    };
    // NOTE: ITI TF-2 §3.55.5.1.1: the Initiating Gateway's audit message carries the
    // ihe:homeCommunityID ParticipantObjectDetail, so a repository-bound audit needs it.
    let named = xcpd
        .home_community
        .as_deref()
        .is_some_and(|community| !community.is_empty());
    if audit == AuditDestination::Repository && !named {
        return Err(Error::Missing {
            key: String::from("xcpd.home_community"),
        });
    }
    let repository = match (audit, &xcpd.audit_repository) {
        (AuditDestination::Repository, Some(table)) => Some(
            crate::config::ihe::audit_repository::resolve(profile, table)?,
        ),
        (AuditDestination::Repository, None) => {
            return Err(Error::Missing {
                key: String::from("xcpd.audit_repository"),
            });
        }
        (_, Some(_)) => return Err(Error::Binding(BindingFault::AuditRepositoryUnused)),
        (_, None) => None,
    };
    Ok((audit, repository))
}
