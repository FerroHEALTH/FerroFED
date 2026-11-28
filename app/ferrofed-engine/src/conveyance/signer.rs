// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's signer of conveyances.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;

use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::keys::KeyRing;

/// The gateway's signer of conveyances: its keys, the `iss` each node knows
/// it by, and the key of its own each node that has one is signed for with.
#[derive(Debug)]
pub struct Signer {
    keys: Arc<KeyRing>,
    issuer: String,
    issuers: BTreeMap<EndpointId, String>,
    own_keys: BTreeMap<EndpointId, Arc<AssertionKey>>,
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
            own_keys: BTreeMap::new(),
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

    /// This signer, signing for `endpoint` with `key` in place of the
    /// current key of its ring, such as an ML-DSA key (RFC 9964) the node
    /// verifies.
    #[must_use]
    pub fn with_key_at(mut self, endpoint: EndpointId, key: Arc<AssertionKey>) -> Self {
        self.own_keys.insert(endpoint, key);
        self
    }

    /// The key a conveyance for `endpoint` is signed with: its own, or the
    /// current key of the ring.
    #[must_use]
    pub fn key_at(&self, endpoint: &EndpointId) -> &AssertionKey {
        self.own_keys
            .get(endpoint)
            .map_or_else(|| self.keys.current(), |key| key)
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
    use ferrohealth_smart::client::assertion::AssertionKey;
    use ferrohealth_smart::jws::algorithm::Algorithm;

    use super::Signer;
    use crate::conveyance::tests::KEYS;

    /// RFC 9964 §5: a node given a key of its own, an ML-DSA one here, is
    /// signed for with it, and every other node with the ring's current key.
    #[test]
    fn an_endpoint_with_a_key_of_its_own_is_signed_for_with_it() {
        let a = EndpointId::new("node-a-pub").expect("an id");
        let b = EndpointId::new("node-b-pub").expect("an id");
        let pem = ferrofed_testkit::oauth::MlDsa::MlDsa65
            .pem()
            .expect("a test key should generate");
        let own = AssertionKey::from_pem(pem.as_bytes(), None, None).expect("an ML-DSA-65 key");
        let signer = Signer::new(Arc::clone(&KEYS), "urn:example:gateway")
            .with_key_at(a.clone(), Arc::new(own.clone()));
        assert_eq!(own.kid(), signer.key_at(&a).kid());
        assert_eq!(Algorithm::MlDsa65, signer.key_at(&a).algorithm());
        assert_eq!(KEYS.current().kid(), signer.key_at(&b).kid());
    }

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
