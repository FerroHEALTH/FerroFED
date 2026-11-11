// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[pixm]` section: the PIX Managers the PIXm resolver asks over ITI-83
//! (Annex A.1), as the configuration writes them and resolved, which the
//! IHE binding builds the resolver from.
//!
//! `[[pixm.manager]]` names each PIX Manager and each member it resolves
//! mapped to that member's `ehr_id` domain, and its `endpoint` table, the
//! `ferrohealth-smart` `EndpointSection`, names its FHIR base, its TLS,
//! its timeouts and retries and its credentials;
//! `[pixm.namespaces]` maps a client's issuing namespace to the PIX
//! assigning authority when the namespace is not itself an absolute URI. No
//! specification governs the shape of the table: our own design.

use ferrohealth_config::units;
use ferrohealth_smart::config::endpoint::EndpointSection;
use std::collections::BTreeMap;

use serde::Deserialize;
use url::Url;

use crate::config::error::Error;
use crate::config::service_grant::ServiceContext;
use crate::config::settings::Scheme;
use crate::config::transport;

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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PixManager {
    /// The Manager's FHIR base and how it is reached: `https`, `http` under
    /// development only.
    pub endpoint: EndpointSection,
    /// Each member this Manager resolves, mapped to its `ehr_id` domain: the
    /// assigning authority whose identifiers are that member's `ehr_id`s
    /// (Annex A.1).
    pub members: BTreeMap<String, String>,
    /// How the gateway asks the Manager: `"get"` or `"post"`.
    pub method: PixmMethod,
}

impl Default for PixManager {
    fn default() -> Self {
        Self {
            endpoint: crate::config::endpoint::tried_once(units::Duration::from_secs(10)),
            members: BTreeMap::new(),
            method: PixmMethod::default(),
        }
    }
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
    /// The Manager's FHIR base URL, read from `endpoint`.
    pub url: Url,
    /// Each member it resolves, mapped to that member's `ehr_id` domain.
    pub members: BTreeMap<String, String>,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
    /// How the Manager is reached, as `ferrohealth-smart`'s outbound client
    /// reads it.
    pub endpoint: EndpointSection,
    /// How the gateway asks it.
    pub method: PixmMethod,
}

/// Resolves `[pixm]`: every Manager's endpoint table is one
/// `ferrohealth-smart` accepts, its URL carries no userinfo and is `https`
/// outside the development `profile`, and every secret is read.
pub(crate) fn resolve(pixm: &Pixm, context: ServiceContext) -> Result<PixmSettings, Error> {
    let profile = context.profile;
    let mut managers = Vec::with_capacity(pixm.manager.len());
    for (index, manager) in pixm.manager.iter().enumerate() {
        let key = format!("pixm.manager[{index}].endpoint");
        let endpoint = crate::config::endpoint::service(&key, &manager.endpoint, context)?;
        // NOTE: no specification governs this: our own design; the Manager is sent
        // patient identifiers, held to https at load as the XCPD and NVI tables are.
        let section = format!("{key}.credentials");
        let carried = endpoint.credentials.is_some().then_some(section.as_str());
        transport::protected_payload(
            profile,
            endpoint.url.as_str(),
            transport::identity_site(&key, carried),
        )?;
        managers.push(PixManagerSettings {
            url: endpoint.url,
            members: manager.members.clone(),
            credentials: endpoint.credentials,
            endpoint: manager.endpoint.clone(),
            method: manager.method,
        });
    }
    Ok(PixmSettings {
        managers,
        namespaces: pixm.namespaces.clone(),
    })
}
