// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The verified caller: who a request comes from, as the gateway verified it
//! (§13.1, N25, §13.4).
//!
//! A request the gateway admits carries one [`Caller`] in its extensions. It
//! holds what the verified token or edge assertion says about the caller and
//! no patient identifier: an IHE IUA `person_id` claim is never read
//! (§5.4.1, N33). The one patient context it may hold is the SMART on openEHR
//! `ehrId`, an `ehr_id` at the platform that issued the token, which N33
//! lets locate a node; it confines a `patient/` grant ([`PatientContext`]).
//!
//! It also holds who acts ([`Acting`]): the natural person the token names,
//! or a client application, which IHE IUA marks by a `sub` that is the
//! `client_id` (ITI TF-2 3.71.4.2.2.1) and SMART on openEHR by a `system/`
//! grant, "acting without a user context" (master08 §Resource Scopes). With
//! it come the professional's identification the token states
//! ([`Professional`]) and the assurance level its authentication reached
//! (Regulation (EU) 2025/327 Annex II 3.1). A caller a national contact point
//! vouched for also holds the professional and the provider of another
//! Member State it relays, as it asserts them ([`Relayed`]), and the
//! correlation identifier its connector sent.

use std::fmt;

use ferrofed_engine::conveyance::relayed::{Relayed, Role};
use ferrofed_engine::conveyance::{Acting, AssuranceLevel};
use ferrofed_identity::role::behalf::{self, OnBehalfOf};
use ferrofed_identity::role::consent::Requester;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_identity::session::SessionKey;
use ferrofed_registry::id::{EhrId, EndpointId};
use openehr_sdt::smart_scopes::SmartScope;
use secrecy::{ExposeSecret, SecretString};

/// A caller the gateway verified.
///
/// `Debug` shows how the caller was verified and what it was granted, and
/// none of its issuer, subject, client or token.
#[derive(Clone, PartialEq, Eq)]
pub struct Caller {
    /// The issuer that vouched for the caller (`iss`).
    issuer: String,
    /// The caller (`sub`).
    subject: String,
    /// The client the caller used (`client_id`).
    client_id: String,
    /// The audience the caller's token was admitted under: the one this
    /// gateway is known by, which every admitted token names in `aud`.
    audience: Option<String>,
    /// The caller's organisation: the IHE IUA `subject_organization_id`,
    /// when the token carries one.
    organisation: Option<String>,
    /// The `scope` claim as granted, space-separated.
    granted: String,
    /// The granted scopes, read in the SMART on openEHR grammar.
    scopes: Vec<SmartScope>,
    /// Every purpose of use the token declares (§13.4).
    purposes: Vec<PurposeOfUse>,
    /// How the gateway verified the caller.
    verified_by: VerifiedBy,
    /// The caller's verified access token, kept for a node whose grant
    /// exchanges it (RFC 8693); a caller the edge asserted has none.
    token: Option<VerifiedToken>,
    /// The granted scopes that cover the operation, space-separated, each
    /// in the canonical form of the SMART on openEHR grammar: the scope an
    /// exchanged token is asked for (N26).
    covering: String,
    /// The token's `ehrId` claim as written, read only to confine a
    /// `patient/` grant.
    launch_ehr_id: Option<String>,
    /// The patient the caller's grant is confined to, when only a
    /// `patient/` grant covers the operation.
    patient: Option<PatientContext>,
    /// Who asks for the data, as the claims the issuer's
    /// `[auth.issuer.requester]` names state it, when the token carries them
    /// all (§13.4).
    requester: Option<Requester>,
    /// The professional's identification the token states.
    professional: Professional,
    /// The assurance level the caller's authentication reached, as its
    /// issuer's `[auth.issuer.assurance]` reads the token, when the issuer
    /// declares one and the token carries a value it declares.
    assurance: Option<AssuranceLevel>,
    /// Who acts: the person the token names, or a client application.
    acting: Acting,
    /// Whether the issuer that vouched for the caller is declared a national
    /// contact point.
    contact_point: bool,
    /// The professional and the provider the contact point relays, when its
    /// token carries every Implementing Regulation (EU) 2026/2099 Annex
    /// attribute.
    relayed: Option<Box<Relayed>>,
    /// The correlation identifier the contact point's connector sent, when
    /// its issuer declares the header and the request carries it.
    correlation: Option<String>,
}

impl fmt::Debug for Caller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const REDACTED: &str = "<redacted>";
        f.debug_struct("Caller")
            .field("issuer", &REDACTED)
            .field("subject", &REDACTED)
            .field("client_id", &REDACTED)
            .field("granted", &self.granted)
            .field("purposes", &self.purposes)
            .field("verified_by", &self.verified_by)
            .field("token", &self.token)
            .field("covering", &self.covering)
            .field("confined", &self.patient.is_some())
            .field("assurance", &self.assurance)
            .field("acting", &self.acting)
            .field("contact_point", &self.contact_point)
            .field("relayed", &self.relayed)
            .finish_non_exhaustive()
    }
}

/// The patient a caller's `patient/` grant is confined to: the token's
/// `ehrId` at the member its issuer is bound to.
///
/// The `ehrId` is the SMART on openEHR launch context (master07 §Context
/// Selection), an `ehr_id` at that member alone (§12.5). The gateway resolves
/// it through the cross-reference as an identifier in that member's `ehr_id`
/// system (§5.2), so it never compares the bare `ehrId` across members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientContext {
    /// The endpoint of the member whose platform issued the token.
    endpoint: EndpointId,
    /// The identifier system of that member's `ehr_id`s at the
    /// cross-reference.
    ehr_id_system: IdentifierNamespace,
    /// The token's `ehrId`.
    ehr_id: EhrId,
}

impl PatientContext {
    /// Returns the context of `ehr_id`, an `ehr_id` in `ehr_id_system`, at
    /// the member `endpoint` reaches.
    #[must_use]
    pub fn new(endpoint: EndpointId, ehr_id_system: IdentifierNamespace, ehr_id: EhrId) -> Self {
        Self {
            endpoint,
            ehr_id_system,
            ehr_id,
        }
    }

    /// Returns the endpoint of the member whose platform issued the token.
    #[must_use]
    pub fn endpoint(&self) -> &EndpointId {
        &self.endpoint
    }

    /// Returns the identifier system of that member's `ehr_id`s.
    #[must_use]
    pub fn ehr_id_system(&self) -> &IdentifierNamespace {
        &self.ehr_id_system
    }

    /// Returns the token's `ehrId`.
    #[must_use]
    pub fn ehr_id(&self) -> &EhrId {
        &self.ehr_id
    }
}

/// The professional's identification a token states.
///
/// It is read from the IHE IUA extension (ITI TF-2 3.71.4.2.2.1.1), and the
/// agency that issued the identifier from the claim the issuer's
/// `professional_issuing_authority` names. `Debug` shows which members are present and none of their values.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Professional {
    /// `national_provider_identifier`: "A unique identifier issued to health
    /// care providers by their national authority".
    pub identifier: Option<String>,
    /// `subject_name`: "The user's name as String".
    pub name: Option<String>,
    /// The name of the agency that issued `identifier`: Implementing
    /// Regulation (EU) 2026/2099 Annex Table 1 `issuing_authority_name`.
    pub issuing_authority: Option<String>,
    /// `subject_role`, each role that has a code.
    pub roles: Vec<Role>,
}

impl fmt::Debug for Professional {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Professional")
            .field("identifier", &self.identifier.is_some())
            .field("name", &self.name.is_some())
            .field("issuing_authority", &self.issuing_authority.is_some())
            .field("roles", &self.roles.len())
            .finish()
    }
}

/// A caller's verified access token.
///
/// `Debug` shows nothing of it, and it is never logged or conveyed to a
/// node: it reaches only the authorization server of a node whose grant
/// exchanges it (RFC 8693 §2.1).
#[derive(Clone)]
pub struct VerifiedToken(SecretString);

impl VerifiedToken {
    /// Returns the token text.
    #[must_use]
    pub fn secret(&self) -> &SecretString {
        &self.0
    }
}

impl PartialEq for VerifiedToken {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret() == other.0.expose_secret()
    }
}

impl Eq for VerifiedToken {}

impl fmt::Debug for VerifiedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("VerifiedToken(<redacted>)")
    }
}

/// The facts a verified credential states about its caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stated {
    /// `iss`.
    pub issuer: String,
    /// `sub`.
    pub subject: String,
    /// `client_id`.
    pub client_id: String,
    /// The IHE IUA `subject_organization_id`.
    pub organisation: Option<String>,
    /// The `scope` claim, or empty.
    pub granted: String,
    /// The purposes of use.
    pub purposes: Vec<PurposeOfUse>,
}

impl Caller {
    /// Returns the caller `stated` describes, verified as `verified_by`
    /// says, its scopes read with
    /// [`SmartScope::parse_all`](openehr_sdt::smart_scopes::SmartScope::parse_all).
    #[must_use]
    pub fn new(stated: Stated, verified_by: VerifiedBy) -> Self {
        let scopes = SmartScope::parse_all(&stated.granted);
        Self {
            issuer: stated.issuer,
            subject: stated.subject,
            client_id: stated.client_id,
            audience: None,
            organisation: stated.organisation,
            granted: stated.granted,
            scopes,
            purposes: stated.purposes,
            verified_by,
            token: None,
            covering: String::new(),
            launch_ehr_id: None,
            patient: None,
            requester: None,
            professional: Professional::default(),
            assurance: None,
            acting: Acting::Person,
            contact_point: false,
            relayed: None,
            correlation: None,
        }
    }

    /// Returns this caller, vouched for by an issuer declared a national
    /// contact point, its token relaying `relayed` when it carries every
    /// Annex attribute.
    #[must_use]
    pub fn relayed_by_contact_point(mut self, relayed: Option<Box<Relayed>>) -> Self {
        self.contact_point = true;
        self.relayed = relayed;
        self
    }

    /// Returns whether the issuer that vouched for the caller is declared a
    /// national contact point, whose requests are served with consent
    /// exclusions withheld.
    #[must_use]
    pub fn is_contact_point(&self) -> bool {
        self.contact_point
    }

    /// Returns the professional and the provider a contact point relays, as
    /// it asserts them, when its token carries every Annex attribute.
    #[must_use]
    pub fn relayed(&self) -> Option<&Relayed> {
        self.relayed.as_deref()
    }

    /// Returns this caller, its request carrying the correlation identifier
    /// `correlation`.
    #[must_use]
    pub fn with_correlation(mut self, correlation: Option<String>) -> Self {
        self.correlation = correlation;
        self
    }

    /// Returns the correlation identifier the contact point's connector sent,
    /// when it sent one.
    #[must_use]
    pub fn correlation(&self) -> Option<&str> {
        self.correlation.as_deref()
    }

    /// Returns this caller, its token stating `professional`.
    #[must_use]
    pub fn with_professional(mut self, professional: Professional) -> Self {
        self.professional = professional;
        self
    }

    /// Returns the professional's identification the token states.
    #[must_use]
    pub fn professional(&self) -> &Professional {
        &self.professional
    }

    /// Returns this caller, its authentication having reached `assurance`.
    #[must_use]
    pub fn with_assurance(mut self, assurance: Option<AssuranceLevel>) -> Self {
        self.assurance = assurance;
        self
    }

    /// Returns the assurance level the caller's authentication reached, when
    /// the issuer declares how its tokens state one and the token states a
    /// value it declares.
    #[must_use]
    pub fn assurance(&self) -> Option<AssuranceLevel> {
        self.assurance
    }

    /// Returns this caller, `acting` behind its token.
    #[must_use]
    pub fn with_acting(mut self, acting: Acting) -> Self {
        self.acting = acting;
        self
    }

    /// Returns who acts behind the caller's token.
    #[must_use]
    pub fn acting(&self) -> Acting {
        self.acting
    }

    /// Returns whether the token names the professional a client acts for:
    /// an IUA `national_provider_identifier`, or the professional of the
    /// requester its issuer's `[auth.issuer.requester]` reads.
    #[must_use]
    pub fn names_professional(&self) -> bool {
        self.professional.identifier.is_some() || self.requester.is_some()
    }

    /// Returns this caller, admitted under `audience`, the audience their
    /// token names this gateway by.
    #[must_use]
    pub fn with_audience(mut self, audience: Option<String>) -> Self {
        self.audience = audience;
        self
    }

    /// Returns the audience the caller's token names this gateway by, when
    /// the gate checked one.
    #[must_use]
    pub fn audience(&self) -> Option<&str> {
        self.audience.as_deref()
    }

    /// Returns whom an identity exchange made for this caller's request is
    /// on behalf of: this caller, as an IHE audit record names its user from
    /// the token (PIXm §2:3.83.5.2.1, BALP 1.1.4 §3:5.7.5.4).
    #[must_use]
    pub fn on_behalf(&self) -> OnBehalfOf {
        OnBehalfOf::Caller(
            behalf::Caller::new(
                self.issuer.clone(),
                self.subject.clone(),
                self.client_id.clone(),
            )
            .with_audience(self.audience.clone())
            .with_purposes(
                self.purposes
                    .iter()
                    .map(|purpose| behalf::Purpose {
                        system: purpose.system.clone(),
                        code: purpose.code.clone(),
                    })
                    .collect(),
            ),
        )
    }

    /// Returns this caller, asking for the data as `requester` states.
    #[must_use]
    pub fn with_requester(mut self, requester: Option<Requester>) -> Self {
        self.requester = requester;
        self
    }

    /// Returns who asks for the data, when the caller's token states it.
    #[must_use]
    pub fn requester(&self) -> Option<&Requester> {
        self.requester.as_ref()
    }

    /// Returns this caller with `claim`, its token's `ehrId` claim as
    /// written (SMART on openEHR master04 §Capabilities).
    #[must_use]
    pub fn with_launch_ehr_id(mut self, claim: Option<String>) -> Self {
        self.launch_ehr_id = claim;
        self
    }

    /// Returns the token's `ehrId` claim as written, when it carries one.
    #[must_use]
    pub fn launch_ehr_id(&self) -> Option<&str> {
        self.launch_ehr_id.as_deref()
    }

    /// Returns this caller, its grant confined to the patient of `context`.
    #[must_use]
    pub fn with_patient(mut self, context: PatientContext) -> Self {
        self.patient = Some(context);
        self
    }

    /// Returns the patient the caller's grant is confined to, when only a
    /// `patient/` grant covers the operation.
    #[must_use]
    pub fn patient(&self) -> Option<&PatientContext> {
        self.patient.as_ref()
    }

    /// Returns the issuer that vouched for the caller.
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Returns the caller's subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Returns the client the caller used.
    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Returns the caller's organisation, when the token names one.
    #[must_use]
    pub fn organisation(&self) -> Option<&str> {
        self.organisation.as_deref()
    }

    /// Returns the `scope` claim as granted.
    #[must_use]
    pub fn granted(&self) -> &str {
        &self.granted
    }

    /// Returns the granted scopes.
    #[must_use]
    pub fn scopes(&self) -> &[SmartScope] {
        &self.scopes
    }

    /// Returns every purpose of use the token declares.
    #[must_use]
    pub fn purposes(&self) -> &[PurposeOfUse] {
        &self.purposes
    }

    /// Returns how the gateway verified the caller.
    #[must_use]
    pub const fn verified_by(&self) -> VerifiedBy {
        self.verified_by
    }

    /// Returns this caller, keeping `token`, the access token the gateway
    /// verified, for a node whose grant exchanges it (RFC 8693 §2.1).
    ///
    /// Only a caller verified by its token's signature or by introspection
    /// keeps it. A caller the edge asserted keeps none: the gateway verified
    /// the edge's assertion, which is no token of the caller's.
    #[must_use]
    pub fn with_token(mut self, token: SecretString) -> Self {
        if matches!(
            self.verified_by,
            VerifiedBy::Signature | VerifiedBy::Introspection
        ) {
            self.token = Some(VerifiedToken(token));
        }
        self
    }

    /// Returns the caller's verified access token, when the gateway keeps
    /// one.
    #[must_use]
    pub fn token(&self) -> Option<&VerifiedToken> {
        self.token.as_ref()
    }

    /// Returns this caller, its granted scopes that cover the operation
    /// being `covering`, space-separated, each printed by `openehr-sdt` in
    /// the canonical form of the SMART on openEHR grammar.
    #[must_use]
    pub fn with_covering(mut self, covering: String) -> Self {
        self.covering = covering;
        self
    }

    /// Returns the granted scopes that cover the operation, the scope an
    /// exchanged token is asked for (N26), or empty for an operation no
    /// scope covers.
    #[must_use]
    pub fn covering(&self) -> &str {
        &self.covering
    }

    /// Returns the session the caller's resolution bindings belong to
    /// (§12.5.1 step 2): its issuer, subject and client together, so another
    /// caller, or the same subject through another client, never shares it.
    #[must_use]
    pub fn session(&self) -> SessionKey {
        // NOTE: no specification governs this: our own design; each part is
        // length-prefixed, so no two callers' parts run together into one key.
        let mut key = String::new();
        for part in [&self.issuer, &self.subject, &self.client_id] {
            key.push_str(&part.len().to_string());
            key.push(':');
            key.push_str(part);
        }
        SessionKey::new(key)
    }
}

/// How the gateway verified a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum VerifiedBy {
    /// A signed access token, against its issuer's key set (RFC 9068).
    Signature,
    /// An access token its issuer's introspection endpoint called active
    /// (RFC 7662).
    Introspection,
    /// The edge's signed assertion: the edge authenticated the caller, and
    /// the gateway verified what the edge asserted.
    Edge,
}

impl VerifiedBy {
    /// The name the security log records.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Signature => "signature",
            Self::Introspection => "introspection",
            Self::Edge => "edge",
        }
    }
}

/// One purpose of use: a code and the system that defines it, the HL7 v3
/// `PurposeOfUse` coding of IHE IUA (§13.4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PurposeOfUse {
    /// The code system, when the token names one.
    pub system: Option<String>,
    /// The code.
    pub code: String,
}

#[cfg(test)]
mod tests {
    use super::{Caller, Stated, VerifiedBy};

    fn caller(issuer: &str, subject: &str, client_id: &str) -> Caller {
        Caller::new(
            Stated {
                issuer: issuer.to_owned(),
                subject: subject.to_owned(),
                client_id: client_id.to_owned(),
                organisation: None,
                granted: String::from("system/aql-*.s"),
                purposes: Vec::new(),
            },
            VerifiedBy::Signature,
        )
    }

    #[test]
    fn one_caller_keeps_one_session_whatever_its_scopes_or_verification() {
        let mut other = caller("https://issuer.example.test", "clinician-1", "app");
        other.granted = String::from("system/composition-*.r");
        other.verified_by = VerifiedBy::Introspection;
        assert_eq!(
            caller("https://issuer.example.test", "clinician-1", "app").session(),
            other.session()
        );
    }

    #[test]
    fn another_issuer_subject_or_client_is_another_session() {
        let one = caller("https://issuer.example.test", "clinician-1", "app").session();
        for other in [
            caller("https://other.example.test", "clinician-1", "app"),
            caller("https://issuer.example.test", "clinician-2", "app"),
            caller("https://issuer.example.test", "clinician-1", "other-app"),
        ] {
            assert_ne!(one, other.session());
        }
    }

    #[test]
    fn parts_that_run_together_alike_are_still_two_sessions() {
        assert_ne!(
            caller("ab", "c", "d").session(),
            caller("a", "bc", "d").session()
        );
    }

    #[test]
    fn debug_shows_no_issuer_subject_or_client() {
        let shown = format!(
            "{:?}",
            caller("https://issuer.example.test", "Qz7-sub-71", "Qz7-client-72")
        );
        for value in ["issuer.example.test", "Qz7-sub-71", "Qz7-client-72"] {
            assert!(!shown.contains(value), "{value} in {shown}");
        }
        assert!(shown.contains("Signature"), "{shown}");
    }
}
