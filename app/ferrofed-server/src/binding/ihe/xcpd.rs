// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer (N4, §14.1, Annex A.3; ITI TF-2 §3.55), built over
//! the registry's members from the `[xcpd]` settings [`crate::config::ihe::xcpd`]
//! resolves.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::ihe::xcpd::{
    AssertionSource, FixedAssertion, GatewayConfig, LogAudit, Transport, XcpdConfig, XcpdLocalizer,
};
use ferrofed_identity::profile::Profile;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrohealth_server::revocation::{RevocationCheck, RevocationSetupError};
use ferrohealth_server::section::TlsVersion;

use crate::binding::ihe::audit::{RepositoryTrail, trail};
use crate::binding::seam::LocalizationError;
use crate::binding::seam::LocalizerSeam;
use crate::config::audit::AuditDestination;
use crate::config::ihe::xcpd::XcpdSettings;
use crate::config::service;
use crate::health::dependencies::Indicator;

/// The XCPD localizer `xcpd` describes over the members of `snapshot`, which
/// admits an `http` gateway only when `profile` is development, audited
/// where `[xcpd] audit` says.
pub(super) fn localizer(
    xcpd: &XcpdSettings,
    profile: Profile,
    snapshot: &RegistrySnapshot,
) -> Result<LocalizerSeam, LocalizationError> {
    let assertion = xcpd
        .assertion
        .as_ref()
        .map(|written| {
            FixedAssertion::from_xml(&written.to_secret_string()).map_err(|_refused| {
                LocalizationError::XcpdAssertion {
                    key: xcpd.assertion_key,
                }
            })
        })
        .transpose()?
        .map(|assertion| -> Arc<dyn AssertionSource> { Arc::new(assertion) });
    let mut communities = BTreeMap::new();
    for (community, member) in &xcpd.communities {
        let member =
            NodeId::new(member.as_str()).map_err(|source| LocalizationError::XcpdMember {
                community: community.clone(),
                source,
            })?;
        communities.insert(community.clone(), member);
    }
    let mut namespaces = BTreeMap::new();
    for (namespace, authority) in &xcpd.namespaces {
        let namespace = IdentifierNamespace::new(namespace.as_str())
            .map_err(LocalizationError::XcpdNamespace)?;
        namespaces.insert(namespace, authority.clone());
    }
    let config = XcpdConfig {
        sender_device: xcpd.sender_device.clone(),
        home_community: xcpd.home_community.clone(),
        gateways: xcpd
            .gateways
            .iter()
            .map(|gateway| GatewayConfig {
                endpoint: gateway.url.clone(),
                device: gateway.device.clone(),
                community: gateway.community.clone(),
                myhealth_eu: gateway.myhealth_eu,
            })
            .collect(),
        communities,
        namespaces,
        transport: if profile == Profile::Development {
            Transport::UnencryptedForDevelopment
        } else {
            Transport::Encrypted
        },
        tls: service::tls(
            "xcpd",
            xcpd.client_identity.as_ref(),
            xcpd.trust_roots.as_deref(),
        )
        .map_err(LocalizationError::Tls)?,
        revocation: revocation(xcpd)?,
    };
    let localizer =
        XcpdLocalizer::from_config(config, assertion, snapshot).map_err(LocalizationError::Xcpd)?;
    let (localizer, indicators) = match (xcpd.audit, &xcpd.audit_repository) {
        (AuditDestination::Repository, Some(repository)) => {
            let recorder = trail(repository).map_err(LocalizationError::AuditTrail)?;
            let indicator: Arc<dyn Indicator> = Arc::new(RepositoryTrail(Arc::clone(&recorder)));
            (localizer.audited(recorder), vec![indicator])
        }
        (AuditDestination::Repository, None) => return Err(LocalizationError::NoAuditRepository),
        (AuditDestination::Log, _) => (localizer.audited(Arc::new(LogAudit)), Vec::new()),
        (AuditDestination::Off, _) => {
            tracing::warn!(
                "[xcpd] audit = \"off\": no ITI-55 audit message is recorded (development only)"
            );
            (localizer, Vec::new())
        }
    };
    Ok(LocalizerSeam {
        localizer: Arc::new(localizer),
        mode: super::XCPD,
        audit: Some(xcpd.audit.as_str()),
        indicators,
    })
}

/// The revocation check of the gateways `xcpd` marks as MyHealth@EU routes,
/// over its client identity and trust roots, which configuration load
/// requires of such a gateway; none when no gateway is marked.
fn revocation(xcpd: &XcpdSettings) -> Result<Option<RevocationCheck>, LocalizationError> {
    if !xcpd.gateways.iter().any(|gateway| gateway.myhealth_eu) {
        return Ok(None);
    }
    let identity = xcpd
        .client_identity
        .as_ref()
        .ok_or(LocalizationError::XcpdRevocation(
            RevocationSetupError::NoClientCertificate,
        ))?;
    let roots = xcpd
        .trust_roots
        .as_deref()
        .ok_or(LocalizationError::XcpdRevocation(
            RevocationSetupError::NoRoots,
        ))?;
    RevocationCheck::from_pem(roots, Some(identity), TlsVersion::Tls12)
        .map(Some)
        .map_err(LocalizationError::XcpdRevocation)
}
