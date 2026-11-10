// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer (N4, §14.1, Annex A.3; ITI TF-2 §3.55), built over
//! the registry's members from the `[xcpd]` settings [`crate::config::ihe::xcpd`]
//! resolves.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_identity::ihe::xcpd::{
    AssertionSource, FixedAssertion, GatedTls, GatewayConfig, LogAudit, Transport, XcpdConfig,
    XcpdLocalizer,
};
use ferrofed_identity::profile::Profile;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrohealth_server::revocation::{RevocationCheck, RevocationSetupError};
use ferrohealth_server::tls_client::TlsClient;
use ferrohealth_smart::client::outbound::OutboundClient;
use ferrohealth_smart::config::{ServerTls, TlsBuilder};

use crate::binding::ihe::audit::{RepositoryTrail, trail};
use crate::binding::seam::LocalizationError;
use crate::binding::seam::LocalizerSeam;
use crate::config::audit::AuditDestination;
use crate::config::ihe::xcpd::{XcpdGateway, XcpdSettings};
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
            .enumerate()
            .map(|(index, written)| gateway(index, written))
            .collect::<Result<_, _>>()?,
        communities,
        namespaces,
        transport: if profile == Profile::Development {
            Transport::UnencryptedForDevelopment
        } else {
            Transport::Encrypted
        },
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

/// The responding gateway `written` at `index`, asked through the outbound
/// client its endpoint table builds: over the gated TLS of its revocation
/// check for a MyHealth@EU route, over the table's own TLS otherwise.
fn gateway(index: usize, written: &XcpdGateway) -> Result<GatewayConfig, LocalizationError> {
    let table = format!("xcpd.gateway[{index}].endpoint");
    let revocation = written
        .myhealth_eu
        .then(|| revocation(&table, written))
        .transpose()?;
    let tls: &dyn TlsBuilder = match &revocation {
        Some(gated) => gated,
        None => &ServerTls,
    };
    let client = OutboundClient::build(&written.endpoint, "xcpd.gateway.endpoint", tls).map_err(
        |source| LocalizationError::Outbound {
            table: table.clone(),
            source: Box::new(source),
        },
    )?;
    Ok(GatewayConfig {
        client,
        device: written.device.clone(),
        community: written.community.clone(),
        myhealth_eu: written.myhealth_eu,
        revocation: revocation.map(|gated| gated.check().clone()),
    })
}

/// The gated TLS of the MyHealth@EU route `written`, whose endpoint table is
/// at `table`: its revocation check over the client identity, trust roots
/// and minimum version of its `tls`, which configuration load requires of
/// such a gateway.
fn revocation(table: &str, written: &XcpdGateway) -> Result<GatedTls, LocalizationError> {
    let key = format!("{table}.tls");
    let tls = TlsClient::read(&key, &written.endpoint.tls).map_err(|source| {
        LocalizationError::XcpdTls {
            table: key.clone(),
            source,
        }
    })?;
    let refused = |source| LocalizationError::XcpdRevocation {
        table: key.clone(),
        source,
    };
    if !tls.has_identity() {
        return Err(refused(RevocationSetupError::NoClientCertificate));
    }
    let check = RevocationCheck::from_tls_client(&tls).map_err(refused)?;
    Ok(GatedTls::new(check))
}
