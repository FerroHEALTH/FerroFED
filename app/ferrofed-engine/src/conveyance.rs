// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a node is told about the caller: the [`HEADER`] every request to a
//! node carries, a compact JWS the gateway signs for that one node (§13.1,
//! N24, N25, §12.4, CP-16).
//!
//! The token is signed with the current key of the gateway's
//! [`KeyRing`], the key whose public half the gateway publishes as its JWK
//! Set and declares as `federation.auth.jwks_uri` (N30), so a node verifies
//! it the way it verifies the gateway's client assertions. Its JOSE header
//! names the key's algorithm, ES256 for a P-256 key and ES384 for a P-384
//! one, the key's `kid` and the type [`TYPE`] (RFC 8725 §3.11). A
//! deployment whose nodes hold to the FAPI 2.0 Security Profile, which
//! admits PS256, ES256 and `EdDSA` (§5.4.1), signs with a P-256 key. Its
//! claims:
//!
//! | Claim | Value |
//! |---|---|
//! | `iss` | the gateway, as the node knows it ([`Signer::issuer_at`]) |
//! | `aud` | the node's `endpoint_id` |
//! | `iat`, `exp` | whole seconds of the wall clock, `exp` [`LIFETIME`] after `iat` |
//! | `jti` | a fresh version 4 UUID |
//! | `sub` | the caller's subject; the gateway's `iss` for the gateway's own request |
//! | `iss_upstream` | the issuer that vouched for the caller |
//! | `verified_by` | how the gateway verified the caller: `signature`, `introspection` or `edge` |
//! | `subject_organization_id` | the caller's organisation, when its token names one (IHE IUA) |
//! | `purpose_of_use` | the caller's purposes of use, each a `system` and `code` (IHE IUA, HL7 v3 `PurposeOfUse`) |
//! | `scope` | the caller's scopes as granted, or the `patient/` scopes that cover the operation under a [`Confinement`] |
//! | `ehrId` | under a [`Confinement`] only: the patient's own `ehr_id` at the receiving node |
//!
//! It never carries `person_id` or any other patient identifier (§5.4.1,
//! N33): a [`Caller`] holds no field for one, and the outbound gate reads
//! every caller claim against the identifiers a request withholds
//! ([`crate::hygiene`]). The caller's own token is never in it. §13.1 leaves
//! end-user conveyance open, so the header and its claims are FerroFED's own
//! design.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::id::{EhrId, EndpointId};
use jsonwebtoken::Header;
use serde::Serialize;

use crate::onward::grant::exchange::SubjectToken;
use crate::onward::keys::KeyRing;

/// The header every request to a node carries the caller's identity in.
pub const HEADER: &str = "openEHR-federation-client";

/// The JOSE `typ` of the token (RFC 8725 §3.11), so a node cannot take it
/// for an access token or a client assertion.
pub const TYPE: &str = "openehr-federation-client+jwt";

/// How long a token is valid after it is signed.
// NOTE: no specification governs this: our own design; one minute outlasts any
// per-node budget and keeps a captured token short-lived.
pub const LIFETIME: Duration = Duration::from_secs(60);

/// How the gateway verified a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verification {
    /// A signed access token, against its issuer's key set (RFC 9068).
    Signature,
    /// An access token its issuer's introspection endpoint called active
    /// (RFC 7662).
    Introspection,
    /// The edge's signed assertion: the edge authenticated the caller, and
    /// the gateway verified what the edge asserted.
    Edge,
}

impl Verification {
    /// The `verified_by` claim's value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Signature => "signature",
            Self::Introspection => "introspection",
            Self::Edge => "edge",
        }
    }
}

/// One purpose of use: a code and the system that defines it, the HL7 v3
/// `PurposeOfUse` coding IHE IUA conveys (§13.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Purpose {
    /// The code system, when the caller's token names one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

/// The caller as the gateway verified it, the facts a node is told.
///
/// It holds no patient identifier and no credential. `Debug` shows how the
/// caller was verified and no value.
#[derive(Clone, PartialEq, Eq)]
pub struct Caller {
    /// The issuer that vouched for the caller.
    pub issuer: String,
    /// The caller's subject.
    pub subject: String,
    /// The caller's organisation, when its token names one.
    pub organisation: Option<String>,
    /// The caller's purposes of use.
    pub purposes: Vec<Purpose>,
    /// The caller's scopes as granted, space-separated; for a caller whose
    /// grant is confined to one patient, the granted `patient/` scopes that
    /// cover the operation.
    pub scope: String,
    /// How the gateway verified the caller.
    pub verified_by: Verification,
}

impl fmt::Debug for Caller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Caller")
            .field("verified_by", &self.verified_by)
            .finish_non_exhaustive()
    }
}

/// On whose behalf a request reaches a node.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Principal {
    /// A caller the gateway verified.
    Caller(Caller),
    /// The gateway itself, for its operator, with no client caller: the
    /// admission check and the operator's stored-query distribution.
    Gateway,
}

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

/// A conveyance that could not be signed, so nothing was sent.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConveyanceError {
    /// The token could not be signed.
    #[error("the caller's identity could not be signed for the node")]
    Sign(#[source] jsonwebtoken::errors::Error),
    /// The caller's grant is confined to one patient, and the patient has no
    /// `ehr_id` at the node this endpoint reaches.
    #[error(
        "the caller's patient/ grant is confined to one patient, who has no ehr_id at endpoint {0}"
    )]
    Unconfined(EndpointId),
    /// The caller's grant is confined to one patient, and the request to
    /// this endpoint is composed for no node-local `ehr_id`, or for another
    /// one than the patient's `ehr_id` the conveyance tells that node.
    #[error(
        "the request to endpoint {0} is not composed for the confined patient's own ehr_id at its node"
    )]
    NotOwn(EndpointId),
}

/// A caller's `patient/` grant confined to one patient: that patient's own
/// `ehr_id` at each endpoint the grant reaches, and no other endpoint.
///
/// The node-local `ehr_id` is the one value N33 lets locate a node, so it is
/// the patient context a node is told, never an identifier of the patient.
/// No specification defines a patient-confined grant across nodes, so the
/// confinement is FerroFED's own design.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confinement {
    bound: EndpointId,
    at: BTreeMap<EndpointId, EhrId>,
}

impl Confinement {
    /// The confinement to `at`, the patient's `ehr_id` at each endpoint the
    /// grant reaches, of a token issued at the member `bound` reaches.
    #[must_use]
    pub fn new(bound: EndpointId, at: BTreeMap<EndpointId, EhrId>) -> Self {
        Self { bound, at }
    }

    /// The endpoint of the member whose platform issued the token, where
    /// the token's own `ehrId` names the patient's EHR.
    #[must_use]
    pub fn bound(&self) -> &EndpointId {
        &self.bound
    }

    /// The patient's `ehr_id` at the node `endpoint` reaches, or `None` when
    /// the grant does not reach it.
    #[must_use]
    pub fn ehr_id_at(&self, endpoint: &EndpointId) -> Option<&EhrId> {
        self.at.get(endpoint)
    }

    /// Whether the grant reaches `ehr_id` at the node `endpoint` reaches:
    /// the pair, never the `ehr_id` alone, since one `ehr_id` can name
    /// another patient's EHR at another node (§12.5, §12.5.2).
    #[must_use]
    pub fn admits(&self, endpoint: &EndpointId, ehr_id: &EhrId) -> bool {
        self.ehr_id_at(endpoint) == Some(ehr_id)
    }

    /// Every endpoint whose node holds `ehr_id` for the patient.
    pub fn holding<'a>(&'a self, ehr_id: &'a EhrId) -> impl Iterator<Item = &'a EndpointId> {
        self.at
            .iter()
            .filter(move |(_, held)| *held == ehr_id)
            .map(|(endpoint, _)| endpoint)
    }
}

/// The identity one client request conveys to every node it reaches: the
/// signer and the principal.
///
/// `Debug` names the principal's kind and no value.
#[derive(Debug, Clone)]
pub struct Conveyance(Arc<Conveyed>);

/// What a [`Conveyance`] shares among the requests that carry it.
#[derive(Debug)]
struct Conveyed {
    signer: Arc<Signer>,
    principal: Principal,
    subject: Option<SubjectToken>,
    confinement: Option<Confinement>,
}

/// The claims of one token.
#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
    jti: String,
    sub: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    iss_upstream: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verified_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject_organization_id: Option<&'a str>,
    #[serde(skip_serializing_if = "<[Purpose]>::is_empty")]
    purpose_of_use: &'a [Purpose],
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<&'a str>,
    #[serde(rename = "ehrId", skip_serializing_if = "Option::is_none")]
    ehr_id: Option<&'a str>,
}

impl Conveyance {
    /// The conveyance of `principal`, signed by `signer`.
    #[must_use]
    pub fn new(signer: Arc<Signer>, principal: Principal) -> Self {
        Self(Arc::new(Conveyed {
            signer,
            principal,
            subject: None,
            confinement: None,
        }))
    }

    /// This conveyance, carrying the caller's verified token for a node
    /// whose grant exchanges it (RFC 8693 §2.1).
    ///
    /// The token reaches that node's authorization server alone, never the
    /// node, and is no claim of the [`HEADER`].
    #[must_use]
    pub fn with_subject(self, subject: SubjectToken) -> Self {
        Self(Arc::new(Conveyed {
            signer: Arc::clone(&self.0.signer),
            principal: self.0.principal.clone(),
            subject: Some(subject),
            confinement: self.0.confinement.clone(),
        }))
    }

    /// This conveyance, its caller's grant confined to one patient by
    /// `confinement`.
    ///
    /// Every node it is signed for is told the patient's own `ehr_id` there,
    /// in the `ehrId` claim, so the node can enforce the confinement (N26);
    /// an endpoint the confinement does not reach is signed nothing, so no
    /// request reaches its node.
    #[must_use]
    pub fn with_confinement(self, confinement: Confinement) -> Self {
        Self(Arc::new(Conveyed {
            signer: Arc::clone(&self.0.signer),
            principal: self.0.principal.clone(),
            subject: self.0.subject.clone(),
            confinement: Some(confinement),
        }))
    }

    /// The patient the caller's grant is confined to, when it is.
    #[must_use]
    pub fn confinement(&self) -> Option<&Confinement> {
        self.0.confinement.as_ref()
    }

    /// On whose behalf the request reaches a node.
    #[must_use]
    pub fn principal(&self) -> &Principal {
        &self.0.principal
    }

    /// The caller's verified token, when the conveyance carries it.
    #[must_use]
    pub fn subject(&self) -> Option<&SubjectToken> {
        self.0.subject.as_ref()
    }

    /// The patient's `ehr_id` this conveyance tells the node `endpoint`
    /// reaches, in its `ehrId` claim: the one the confinement holds for that
    /// node, or `None` for a caller whose grant is not confined.
    ///
    /// # Errors
    ///
    /// Returns [`ConveyanceError::Unconfined`] when the caller's grant is
    /// confined to a patient who has no `ehr_id` at that node.
    pub fn confined_ehr_id(
        &self,
        endpoint: &EndpointId,
    ) -> Result<Option<&EhrId>, ConveyanceError> {
        match &self.0.confinement {
            Some(confinement) => confinement
                .ehr_id_at(endpoint)
                .map(Some)
                .ok_or_else(|| ConveyanceError::Unconfined(endpoint.clone())),
            None => Ok(None),
        }
    }

    /// Holds the `ehrId` this conveyance tells `endpoint` to `composed`, the
    /// node-local `ehr_id` the request to it is composed for; nothing to
    /// hold for a caller whose grant is not confined.
    ///
    /// The `ehrId` is the one carrier a node is told about the patient, so
    /// it must be that node's own `ehr_id`, the same the request is located
    /// by, and never another node's (§5.4.1, N33, §12.5).
    ///
    /// # Errors
    ///
    /// Returns [`ConveyanceError::Unconfined`] as [`Conveyance::confined_ehr_id`]
    /// does, and [`ConveyanceError::NotOwn`] when the request is composed for
    /// no node-local `ehr_id`, or another one.
    pub fn holds_own(
        &self,
        endpoint: &EndpointId,
        composed: Option<&EhrId>,
    ) -> Result<(), ConveyanceError> {
        match self.confined_ehr_id(endpoint)? {
            None => Ok(()),
            Some(told) if composed == Some(told) => Ok(()),
            Some(_) => Err(ConveyanceError::NotOwn(endpoint.clone())),
        }
    }

    /// The [`HEADER`] value for `endpoint`: a compact JWS, its `aud` the
    /// endpoint's id, valid for [`LIFETIME`] from now, carrying the
    /// patient's `ehr_id` there when the caller's grant is confined.
    ///
    /// # Errors
    ///
    /// Returns [`ConveyanceError::Sign`] when the key cannot sign, and
    /// [`ConveyanceError::Unconfined`] when the caller's grant is confined
    /// to a patient who has no `ehr_id` at the node `endpoint` reaches.
    pub fn signed_for(&self, endpoint: &EndpointId) -> Result<String, ConveyanceError> {
        let ehr_id = self.confined_ehr_id(endpoint)?;
        let iss = self.0.signer.issuer_at(endpoint);
        let iat = jiff::Timestamp::now().as_second();
        let lifetime = i64::try_from(LIFETIME.as_secs()).unwrap_or(i64::MAX);
        let mut claims = Claims {
            iss,
            aud: endpoint.as_str(),
            iat,
            exp: iat.saturating_add(lifetime),
            jti: uuid::Uuid::new_v4().to_string(),
            sub: iss,
            iss_upstream: None,
            verified_by: None,
            subject_organization_id: None,
            purpose_of_use: &[],
            scope: None,
            ehr_id: None,
        };
        if let Principal::Caller(caller) = &self.0.principal {
            claims.sub = &caller.subject;
            claims.iss_upstream = Some(&caller.issuer);
            claims.verified_by = Some(caller.verified_by.as_str());
            claims.subject_organization_id = caller.organisation.as_deref();
            claims.purpose_of_use = &caller.purposes;
            claims.scope = Some(caller.scope.as_str()).filter(|scope| !scope.is_empty());
            // NOTE: SMART on openEHR master04 §Capabilities names the claim `ehrId`, and N33
            // lets a node be located by its own ehr_id, so each node is told its own.
            claims.ehr_id = ehr_id.map(EhrId::as_str);
        }
        let key = self.0.signer.keys.current();
        let mut header = Header::new(key.algorithm());
        header.typ = Some(TYPE.to_owned());
        header.kid = Some(key.kid().to_owned());
        jsonwebtoken::encode(&header, &claims, key.private()).map_err(ConveyanceError::Sign)
    }

    /// Every claim value that comes from the caller's credential, which the
    /// outbound gate reads against the identifiers a request withholds.
    ///
    /// The gateway's own `iss`, the node's `aud` and the minted `iat`,
    /// `exp` and `jti` come from no request, and are not among them.
    #[must_use]
    pub fn carried(&self) -> Vec<&str> {
        let Principal::Caller(caller) = &self.0.principal else {
            return Vec::new();
        };
        let mut carried = vec![
            caller.subject.as_str(),
            caller.issuer.as_str(),
            caller.scope.as_str(),
        ];
        carried.extend(caller.organisation.as_deref());
        for purpose in &caller.purposes {
            carried.extend(purpose.system.as_deref());
            carried.push(&purpose.code);
        }
        carried
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, LazyLock};
    use std::time::Duration;

    use ferrofed_registry::id::{EhrId, EndpointId};
    use secrecy::SecretString;

    use super::{
        Caller, Confinement, Conveyance, ConveyanceError, Principal, Purpose, Signer, Verification,
    };
    use crate::onward::SystemClock;
    use crate::onward::keys::{KeyRing, SigningKey};

    /// The keys of every unit test's conveyance, generated once.
    static KEYS: LazyLock<Arc<KeyRing>> = LazyLock::new(|| {
        let pem = ferrofed_testkit::oauth::es384_pem().expect("a test key should generate");
        let key = SigningKey::from_pem(&SecretString::from(pem)).expect("an ES384 key");
        Arc::new(
            KeyRing::new(key, None, Duration::ZERO, Arc::new(SystemClock))
                .expect("one key is a ring"),
        )
    });

    fn caller() -> Caller {
        Caller {
            issuer: "https://issuer.example.test".to_owned(),
            subject: "clinician-0042".to_owned(),
            organisation: Some("urn:oid:2.999.7".to_owned()),
            purposes: vec![Purpose {
                system: Some("http://terminology.hl7.org/CodeSystem/v3-ActReason".to_owned()),
                code: "TREAT".to_owned(),
            }],
            scope: "user/aql-*.s".to_owned(),
            verified_by: Verification::Edge,
        }
    }

    fn signer() -> Arc<Signer> {
        Arc::new(Signer::new(Arc::clone(&KEYS), "urn:example:gateway"))
    }

    /// A conveyance of a synthetic verified caller, for a unit test that
    /// dispatches.
    pub(crate) fn conveyance() -> Conveyance {
        Conveyance::new(signer(), Principal::Caller(caller()))
    }

    #[test]
    fn every_caller_claim_is_carried_to_the_gate_and_no_gateway_claim_is() {
        let conveyance = conveyance();
        let carried = conveyance.carried();
        for claim in [
            "clinician-0042",
            "https://issuer.example.test",
            "urn:oid:2.999.7",
            "http://terminology.hl7.org/CodeSystem/v3-ActReason",
            "TREAT",
            "user/aql-*.s",
        ] {
            assert!(carried.contains(&claim), "{claim} in {carried:?}");
        }
        let gateway = Conveyance::new(signer(), Principal::Gateway);
        assert!(gateway.carried().is_empty(), "the gateway's own claims");
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

    /// The confinement claims of a signed token, read without verifying it.
    #[derive(serde::Deserialize)]
    struct Confined {
        #[serde(rename = "ehrId")]
        ehr_id: Option<String>,
        scope: Option<String>,
    }

    fn confined(token: &str) -> Confined {
        jsonwebtoken::dangerous::insecure_decode_claims::<Confined>(token)
            .expect("a token the conveyance signed should read")
    }

    // NOTE: N26, N33: each node is told its own ehr_id for the confined patient, never another's.
    #[test]
    fn a_confined_conveyance_tells_each_node_its_own_ehr_id_and_reaches_no_other() {
        let a = EndpointId::new("node-a-pub").expect("an id");
        let b = EndpointId::new("node-b-pub").expect("an id");
        let c = EndpointId::new("node-c-pub").expect("an id");
        let ehr_a = EhrId::new("2222aaaa-2222-4222-8222-222222222222").expect("an ehr_id");
        let ehr_b = EhrId::new("1111bbbb-1111-4111-8111-111111111111").expect("an ehr_id");
        let confinement = Confinement::new(
            a.clone(),
            BTreeMap::from([(a.clone(), ehr_a.clone()), (b.clone(), ehr_b.clone())]),
        );
        assert!(confinement.admits(&a, &ehr_a));
        assert!(
            !confinement.admits(&a, &ehr_b),
            "a pair, never a bare ehr_id"
        );
        let conveyance = conveyance().with_confinement(confinement);
        for (endpoint, ehr_id) in [(&a, &ehr_a), (&b, &ehr_b)] {
            let token = conveyance.signed_for(endpoint).expect("a reached node");
            assert_eq!(Some(ehr_id.as_str()), confined(&token).ehr_id.as_deref());
        }
        assert!(matches!(
            conveyance.signed_for(&c),
            Err(ConveyanceError::Unconfined(endpoint)) if endpoint == c
        ));
    }

    #[test]
    fn an_unconfined_conveyance_carries_no_ehr_id() {
        let token = conveyance()
            .signed_for(&EndpointId::new("node-a-pub").expect("an id"))
            .expect("a signed token");
        let claims = confined(&token);
        assert_eq!(None, claims.ehr_id);
        assert_eq!(Some("user/aql-*.s"), claims.scope.as_deref());
    }

    #[test]
    fn a_conveyance_shows_no_caller_value_in_debug() {
        let shown = format!("{:?}", conveyance());
        assert!(!shown.contains("clinician-0042"), "{shown}");
        assert!(shown.contains("Edge"), "{shown}");
    }
}
