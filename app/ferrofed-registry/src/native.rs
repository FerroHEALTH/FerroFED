// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member's own endpoint for the European exchange format: a second
//! endpoint of the node, of connection type `hl7-fhir-rest`, that serves the
//! documents of the categories it names itself.
//!
//! A node may expose several endpoints, or two connection types (§ The four
//! identifiers), and a member that serves a category's documents itself is
//! asked for them there, in the `native` mode, instead of having its openEHR
//! data mapped by the gateway, the `map` mode every other member and
//! category is in. Such an endpoint is never an openEHR Query API endpoint
//! (N19): the snapshot keeps it apart from [`Endpoint`](crate::snapshot::Endpoint),
//! so no query is dispatched to it, `OPTIONS {base}/` never lists it (§7a.2)
//! and a directive or a targeting header naming it names no endpoint (§8).
//! The member is located there by its own `ehr_id` alone, written as an
//! identifier in the system the endpoint declares for it (N33), which no
//! specification defines for a FHIR face. No specification governs the
//! `native` mode or its keys: our own design.

use std::collections::{BTreeMap, BTreeSet};

use url::Url;

use crate::document::EndpointDoc;
use crate::error::LoadError;
use crate::id::{EndpointId, NodeId, OrganisationId};
use crate::snapshot::EndpointStatus;

/// A member's own `hl7-fhir-rest` endpoint, serving the exchange-format
/// documents of the categories it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeEndpoint {
    id: EndpointId,
    node: NodeId,
    url: Url,
    managing_organisation: OrganisationId,
    status: EndpointStatus,
    categories: BTreeSet<String>,
    profiles: Vec<String>,
    ehr_id_system: String,
}

impl NativeEndpoint {
    /// The endpoint's stable `endpoint_id`, unique beside every other
    /// endpoint's.
    #[must_use]
    pub fn id(&self) -> &EndpointId {
        &self.id
    }

    /// The node the endpoint belongs to.
    #[must_use]
    pub fn node(&self) -> &NodeId {
        &self.node
    }

    /// The endpoint's FHIR base URL.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// The one organisation that manages the endpoint (N20).
    #[must_use]
    pub fn managing_organisation(&self) -> &OrganisationId {
        &self.managing_organisation
    }

    /// Whether the endpoint is in service.
    #[must_use]
    pub fn status(&self) -> EndpointStatus {
        self.status
    }

    /// The categories it serves, each by the code the deployment names it
    /// with, such as `Patient-Summaries`.
    #[must_use]
    pub fn categories(&self) -> &BTreeSet<String> {
        &self.categories
    }

    /// Whether it serves the category `code`.
    #[must_use]
    pub fn serves(&self, code: &str) -> bool {
        self.categories.contains(code)
    }

    /// The canonical URLs of the profiles its documents follow, in document
    /// order.
    #[must_use]
    pub fn profiles(&self) -> &[String] {
        &self.profiles
    }

    /// The identifier system the member's `ehr_id` is written in when the
    /// endpoint is asked by identifier.
    #[must_use]
    pub fn ehr_id_system(&self) -> &str {
        &self.ehr_id_system
    }
}

/// The native endpoint `doc` declares, at the base URL `url` already held
/// to the registry's rules.
///
/// # Errors
///
/// [`LoadError::MisplacedKey`] for a consent refusal code, which only an
/// ITS-REST `Error` carries, [`LoadError::NativeWithoutIdentifierSystem`]
/// for no `ehr_id_system`, or an empty one, and
/// [`LoadError::NativeWithout`] for no category or no profile, or an empty
/// one.
pub(crate) fn declared(doc: EndpointDoc, url: Url) -> Result<NativeEndpoint, LoadError> {
    if !doc.consent_refusal_codes.is_empty() {
        return Err(LoadError::MisplacedKey {
            endpoint: doc.id,
            key: "consent_refusal_codes",
        });
    }
    let Some(ehr_id_system) = doc.ehr_id_system.filter(|system| !system.is_empty()) else {
        return Err(LoadError::NativeWithoutIdentifierSystem(doc.id));
    };
    for (key, values) in [("categories", &doc.categories), ("profiles", &doc.profiles)] {
        if values.is_empty() || values.iter().any(String::is_empty) {
            return Err(LoadError::NativeWithout {
                endpoint: doc.id,
                key,
            });
        }
    }
    Ok(NativeEndpoint {
        id: doc.id,
        node: doc.node,
        url,
        managing_organisation: doc.managing_organisation,
        status: doc.status,
        categories: doc.categories.into_iter().collect(),
        profiles: doc.profiles,
        ehr_id_system,
    })
}

/// Refuses an `openehr-rest-query` endpoint that sets a key only a native
/// endpoint takes.
///
/// # Errors
///
/// [`LoadError::MisplacedKey`] naming the first such key.
pub(crate) fn held_to_query(doc: &EndpointDoc) -> Result<(), LoadError> {
    let set = [
        ("categories", !doc.categories.is_empty()),
        ("profiles", !doc.profiles.is_empty()),
        ("ehr_id_system", doc.ehr_id_system.is_some()),
    ];
    match set.into_iter().find(|(_, set)| *set) {
        Some((key, _)) => Err(LoadError::MisplacedKey {
            endpoint: doc.id.clone(),
            key,
        }),
        None => Ok(()),
    }
}

/// Refuses a second native endpoint of one node for one category, since a
/// member is in one mode per category.
///
/// # Errors
///
/// [`LoadError::NativeCategoryTwice`] naming the node, the category and both
/// endpoints.
pub(crate) fn one_per_category<'a>(
    natives: impl Iterator<Item = &'a NativeEndpoint>,
) -> Result<(), LoadError> {
    let mut seen: BTreeMap<(&NodeId, &str), &EndpointId> = BTreeMap::new();
    for native in natives {
        for category in &native.categories {
            if let Some(first) = seen.insert((&native.node, category), &native.id) {
                return Err(LoadError::NativeCategoryTwice {
                    node: native.node.clone(),
                    category: category.clone(),
                    first: first.clone(),
                    second: native.id.clone(),
                });
            }
        }
    }
    Ok(())
}
