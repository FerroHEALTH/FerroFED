// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver, `[pixm]`: the identity binding of N3 over ITI-83
//! (Annex A.1), built over the registry's members.
//!
//! `[[pixm.manager]]` names each PIX Manager's FHIR base, its credentials,
//! and each member it resolves mapped to that member's `ehr_id` domain;
//! `[pixm.namespaces]` maps a client's issuing namespace to the PIX
//! assigning authority when the namespace is not itself an absolute URI. No
//! specification governs the shape of the table: our own design.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_identity::ihe::pixm::{ManagerConfig, PixmResolver};
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::{Secret, SecretUrl};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ihe_iti::pixm::Invocation;
use serde::Deserialize;

use crate::binding::ihe::audit::config::AuditSettings;
use crate::config::Credentials;
use crate::config::error::Error;
use crate::config::service_grant::{ServiceContext, resolve_service};
use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;
use crate::config::transport;
use crate::federation::error::FederationError;
use crate::service;

/// The PIXm resolver: the identity binding of N3 over ITI-83 (Annex A.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pixm {
    /// The PIX Managers, one `[[pixm.manager]]` each; every registry member is
    /// resolved by exactly one of them.
    pub manager: Vec<PixManager>,
    /// A client's issuing namespace mapped to the PIX assigning authority it
    /// stands for (`"2.999.1" = "urn:oid:2.999.1"`). A namespace that is
    /// itself an absolute URI needs no entry.
    pub namespaces: BTreeMap<String, String>,
}

/// One PIX Manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PixManager {
    /// The Manager's FHIR base URL, which no rendering shows with its
    /// userinfo.
    pub url: SecretUrl,
    /// Each member this Manager resolves, mapped to its `ehr_id` domain: the
    /// assigning authority whose identifiers are that member's `ehr_id`s
    /// (Annex A.1).
    pub members: BTreeMap<String, String>,
    /// How the gateway authenticates to the Manager, when the transport does
    /// not.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS, inline or through `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the Manager's certificate chains to, beside
    /// the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// How the gateway asks the Manager: `"get"` or `"post"`.
    pub method: PixmMethod,
}

/// How the gateway invokes ITI-83 at a PIX Manager (`method`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PixmMethod {
    /// `GET [base]/Patient/$ihe-pix?sourceIdentifier=…`: the patient identifier
    /// is in the request URL.
    // NOTE: PIXm 3.1.0 §2:3.83.4.1.2 says "the HTTP GET operation shall be used", and
    // FHIR R4 Operations §3.2.0.1 requires a server to support that GET, with no such rule for POST.
    #[default]
    Get,
    /// `POST [base]/Patient/$ihe-pix` with the parameters in a `Parameters`
    /// body (FHIR R4 Operations §3.2.0.1), which keeps the patient identifier
    /// out of the request URL; for a Manager that accepts it.
    Post,
}

/// The PIXm resolver, resolved.
#[derive(Debug)]
pub struct PixmSettings {
    /// The PIX Managers.
    pub managers: Vec<PixManagerSettings>,
    /// A client's issuing namespace mapped to a PIX assigning authority.
    pub namespaces: BTreeMap<String, String>,
}

/// One PIX Manager, resolved.
#[derive(Debug)]
pub struct PixManagerSettings {
    /// The Manager's FHIR base URL, already known to parse.
    pub url: SecretUrl,
    /// Each member it resolves, mapped to that member's `ehr_id` domain.
    pub members: BTreeMap<String, String>,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
    /// The TLS material it is reached with.
    pub tls: TlsSettings,
    /// How the gateway asks it.
    pub method: PixmMethod,
}

/// Resolves `[pixm]`: every Manager URL parses, carries no userinfo and is
/// `https` outside the development `profile`, and every secret is read.
pub(super) fn resolve(pixm: &Pixm, context: &ServiceContext<'_>) -> Result<PixmSettings, Error> {
    let profile = context.profile;
    let mut managers = Vec::with_capacity(pixm.manager.len());
    for (index, manager) in pixm.manager.iter().enumerate() {
        let key = format!("pixm.manager[{index}]");
        let url = url::Url::parse(manager.url.expose()).map_err(|source| Error::Url {
            key: format!("{key}.url"),
            source,
        })?;
        // NOTE: no specification governs this: our own design; as the registry
        // refuses it on an endpoint URL, a credential goes in its own section.
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::UrlCredentials {
                key: format!("{key}.url"),
                section: format!("{key}.credentials"),
            });
        }
        let section = format!("{key}.credentials");
        let credentials = manager
            .credentials
            .as_ref()
            .map(|credentials| resolve_service(&section, credentials, context))
            .transpose()?;
        // NOTE: no specification governs this: our own design; the Manager is sent
        // patient identifiers, held to https at load as the XCPD and NVI tables are.
        let carried = credentials.is_some().then_some(section.as_str());
        transport::protected_payload(
            profile,
            manager.url.expose(),
            transport::identity_site(&key, carried),
        )?;
        let tls = crate::config::tls::resolve(
            &key,
            manager.client_identity.as_ref(),
            manager.client_identity_file.as_deref(),
            manager.trust_roots_file.as_ref(),
        )?;
        managers.push(PixManagerSettings {
            url: manager.url.clone(),
            members: manager.members.clone(),
            credentials,
            tls,
            method: manager.method,
        });
    }
    Ok(PixmSettings {
        managers,
        namespaces: pixm.namespaces.clone(),
    })
}

/// The PIXm resolver `[pixm]` describes over the members of `snapshot`.
pub(super) fn resolver(
    pixm: &PixmSettings,
    audit: &AuditSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Arc<PixmResolver>, FederationError> {
    let mut managers = Vec::with_capacity(pixm.managers.len());
    for (index, manager) in pixm.managers.iter().enumerate() {
        let mut members = BTreeMap::new();
        for (key, domain) in &manager.members {
            let member =
                NodeId::new(key.as_str()).map_err(|source| FederationError::PixmMember {
                    manager: index,
                    key: key.clone(),
                    source,
                })?;
            members.insert(member, domain.clone());
        }
        let key = format!("pixm.manager[{index}]");
        let tls = service::tls_of(&key, &manager.tls).map_err(FederationError::Tls)?;
        let auth = service::service_authentication(
            &format!("{key}.credentials"),
            manager.credentials.as_ref(),
            &tls,
        )?;
        managers.push(ManagerConfig {
            base: manager.url.clone(),
            auth,
            tls,
            members,
            invocation: match manager.method {
                PixmMethod::Get => Invocation::Get,
                PixmMethod::Post => Invocation::Post,
            },
        });
    }
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pixm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(FederationError::PixmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let resolver =
        PixmResolver::from_config(managers, namespaces, snapshot).map_err(FederationError::Pixm)?;
    // NOTE: PIXm §2:3.83.5.1.1: each ITI-83 exchange is audited, and one whose
    // record is refused fails, so the query fails closed.
    Ok(Arc::new(
        match crate::binding::ihe::audit::recorder(audit).map_err(FederationError::Audit)? {
            Some(recorder) => resolver.audited(&recorder),
            None => resolver,
        },
    ))
}
