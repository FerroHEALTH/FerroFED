// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's signer of conveyances.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;

use crate::onward::keys::KeyRing;

/// The gateway's signer of conveyances: its keys, and the `iss` each node
/// knows it by.
#[derive(Debug)]
pub struct Signer {
    keys: Arc<KeyRing>,
    issuer: String,
    issuers: BTreeMap<EndpointId, String>,
}

impl Signer {
    /// A signer with the current key of `keys`, naming the gateway `issuer`
    /// to every node.
    #[must_use]
    pub fn new(keys: Arc<KeyRing>, issuer: impl Into<String>) -> Self {
        Self {
            keys,
            issuer: issuer.into(),
            issuers: BTreeMap::new(),
        }
    }

    /// This signer, naming the gateway `issuer` to `endpoint`.
    ///
    /// An endpoint the gateway obtains an OAuth 2.0 token from knows the
    /// gateway as the `client_id` of its client assertions, so the two name
    /// the gateway the same way.
    #[must_use]
    pub fn with_issuer_at(mut self, endpoint: EndpointId, issuer: impl Into<String>) -> Self {
        self.issuers.insert(endpoint, issuer.into());
        self
    }

    /// The `iss` `endpoint` receives.
    #[must_use]
    pub fn issuer_at(&self, endpoint: &EndpointId) -> &str {
        self.issuers.get(endpoint).unwrap_or(&self.issuer)
    }

    /// The keys the signer signs with.
    #[must_use]
    pub fn keys(&self) -> &KeyRing {
        &self.keys
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ferrofed_registry::id::EndpointId;

    use super::Signer;
    use crate::conveyance::tests::KEYS;

    #[test]
    fn an_endpoint_without_an_issuer_of_its_own_gets_the_gateways() {
        let a = EndpointId::new("node-a-pub").expect("an id");
        let b = EndpointId::new("node-b-pub").expect("an id");
        let signer = Signer::new(Arc::clone(&KEYS), "urn:example:gateway")
            .with_issuer_at(a.clone(), "client-a");
        assert_eq!("client-a", signer.issuer_at(&a));
        assert_eq!("urn:example:gateway", signer.issuer_at(&b));
    }
}
