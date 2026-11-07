// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[auth]` and `[trust]`: how a caller authenticates to the gateway (§13.1,
//! N25), as written and as resolved.
//!
//! `[auth]` is the section `ferrohealth-smart` reads in every FerroHEALTH
//! product: one `[[auth.issuer]]` table per issuer the gateway trusts, with
//! its `kind`, its profile, its audiences and its token rules, each issuer
//! verified against the key set its metadata names (RFC 8414 §3), a key set
//! file, or RFC 7662 introspection; the Basic users of `[[auth.basic.users]]`;
//! and the product scope. `[trust]` is FerroFED's own policy beside it
//! ([`trust`]): what each issuer's tokens are honoured for, the edge mode,
//! and the purpose-of-use rule. There is no unauthenticated mode: `enabled = false`
//! is refused, and with no issuer and no Basic user every guarded request is
//! refused. The configuration keys are our own design; no specification
//! governs them.

pub mod assurance;
pub mod contact_point;
pub mod requester;
pub mod trust;

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrohealth_smart::basic::PasswordStore;
use ferrohealth_smart::config::{AuthSection, BasicSection, IssuerKey};
use ferrohealth_smart::federation::FederationSection;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use ferrohealth_smart::resource::introspection::IntrospectionSettings;
use ferrohealth_smart::sender::proof::ProofSettings;
use http::HeaderName;
use secrecy::ExposeSecret as _;
use serde::{Deserialize, Serialize};

use crate::config::Federation;
use crate::config::auth::assurance::Assurance;
use crate::config::auth::contact_point::ContactPoint;
use crate::config::auth::requester::RequesterClaims;
use crate::config::auth::trust::Trust;
use crate::config::error::Error;
use crate::config::public_url::PublicUrl;

/// The realm every challenge names when `[auth] realm` is empty (RFC 6750
/// §3, RFC 7617 §2).
pub const DEFAULT_REALM: &str = "ferrofed";

/// `[auth]` as written: the section `ferrohealth-smart` reads.
///
/// Two sections compare equal when they render alike and every Basic user
/// holds the same inline password hash, so a reload sees a changed hash.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Auth(pub AuthSection);

impl PartialEq for Auth {
    fn eq(&self, other: &Self) -> bool {
        // NOTE: no specification governs this: our own design; the section renders
        // every secret redacted, so the Basic users' hashes are compared besides.
        serde_json::to_value(&self.0).ok() == serde_json::to_value(&other.0).ok()
            && hashes(self.0.basic.as_ref()) == hashes(other.0.basic.as_ref())
    }
}

/// The password hashes `basic`'s users carry inline, in order.
fn hashes(basic: Option<&BasicSection>) -> Vec<Option<String>> {
    basic
        .map(|section| {
            section
                .users
                .iter()
                .map(|user| {
                    user.password_hash
                        .as_ref()
                        .map(|hash| hash.expose_secret().to_owned())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `[auth]` and `[trust]`, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSettings {
    /// Where the credential travels.
    pub mode: AuthMode,
    /// Whether a token must carry a purpose of use (§13.4).
    pub purpose_required: bool,
    /// The scope that admits a caller to the operator surface and the admin
    /// write actions, `[auth] product_scope`; `None` admits no caller there.
    pub product_scope: Option<String>,
    /// The realm every challenge names.
    pub realm: String,
    /// The issuers on the trust list, `[auth]`'s key-set issuers in the order
    /// written, then the introspected one.
    pub issuers: Vec<IssuerSettings>,
    /// The Basic users of `[[auth.basic.users]]`, when any is written.
    pub basic: Option<BasicUsers>,
    /// How a `DPoP` proof is checked (`[auth.dpop]`, RFC 9449 §4.3), when
    /// the table is written; without it, no `DPoP` request is admitted.
    pub dpop: Option<ProofSettings>,
}

impl Default for AuthSettings {
    /// The settings of an absent `[auth]`: no issuer, so every guarded
    /// request is refused.
    fn default() -> Self {
        Self {
            mode: AuthMode::Token,
            purpose_required: true,
            product_scope: None,
            realm: DEFAULT_REALM.to_owned(),
            issuers: Vec::new(),
            basic: None,
            dpop: None,
        }
    }
}

impl AuthSettings {
    /// What `config check` notes about the issuers: each one that declares
    /// no `[trust.issuer.assurance]`, so no assurance level is read from its
    /// tokens or required of them (Regulation (EU) 2025/327 Annex II 3.1).
    #[must_use]
    pub fn notes(&self) -> Vec<String> {
        self.issuers
            .iter()
            .filter(|issuer| issuer.assurance.is_none())
            .map(|issuer| {
                format!(
                    "the issuer {} declares no [trust.issuer.assurance]: the gateway reads no authentication assurance from its tokens and requires none for patient data",
                    issuer.issuer
                )
            })
            .collect()
    }
}

/// The Basic users of `[[auth.basic.users]]`, resolved.
///
/// Two compare equal when they were resolved from the same written table,
/// so a reload sees a changed user.
#[derive(Debug, Clone)]
pub struct BasicUsers {
    /// The users, their hashes checked.
    store: Arc<PasswordStore>,
    /// The table as written.
    written: Auth,
}

impl BasicUsers {
    /// Returns the users `store` holds, resolved from `written`.
    #[must_use]
    pub fn new(store: PasswordStore, written: BasicSection) -> Self {
        Self {
            store: Arc::new(store),
            written: Auth(AuthSection {
                basic: Some(written),
                ..AuthSection::default()
            }),
        }
    }

    /// Returns the password store the users are checked against.
    #[must_use]
    pub fn store(&self) -> &Arc<PasswordStore> {
        &self.store
    }

    /// Whether a user `user` is written.
    #[must_use]
    pub fn names(&self, user: &str) -> bool {
        self.written
            .0
            .basic
            .as_ref()
            .is_some_and(|basic| basic.users.iter().any(|written| written.user == user))
    }
}

impl PartialEq for BasicUsers {
    fn eq(&self, other: &Self) -> bool {
        self.written == other.written
    }
}

impl Eq for BasicUsers {}

/// Where a caller's credential travels, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthMode {
    /// An RFC 6750 bearer token in `Authorization`.
    Token,
    /// The edge's signed assertion, in this header.
    Edge(HeaderName),
}

/// One issuer on the trust list, resolved: how its tokens are verified and
/// what they are honoured for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerSettings {
    /// The issuer identifier its tokens carry in `iss`.
    pub issuer: String,
    /// How its tokens are verified.
    pub verification: Verification,
    /// The backend clients whose `system/aql-*` grant is honoured.
    pub backend_clients: BTreeSet<String>,
    /// The clients admitted to the DEMOGRAPHIC API.
    pub demographic_clients: BTreeSet<String>,
    /// Where this issuer's `patient/` grants are confined, when they are
    /// honoured at all.
    pub patient: Option<PatientBinding>,
    /// The claims that name who asks for the data, when this issuer's tokens
    /// carry them.
    pub requester: Option<RequesterClaims>,
    /// How this issuer's tokens state their authentication assurance, and
    /// the least level a patient-data request needs, when the deployment
    /// declares it.
    pub assurance: Option<Assurance>,
    /// Whether this issuer's client tokens act for the professional they
    /// name.
    pub client_tokens_act_for_professional: bool,
    /// The claim that carries the agency that issued the professional's
    /// identifier, when this issuer's tokens carry one.
    pub professional_issuing_authority: Option<String>,
    /// This issuer as a national contact point, when its `[[auth.federation]]`
    /// table sets `contact_point = true`, with its
    /// `[[federation.contact_point]]`: its requests are served with consent
    /// exclusions withheld, and its tokens relay the professional and the
    /// provider of another Member State.
    pub national_contact_point: Option<ContactPoint>,
}

impl IssuerSettings {
    /// Returns the issuer `verification` verifies, honoured for nothing
    /// beyond its tokens' `user/` and `system/` grants.
    #[must_use]
    pub fn new(issuer: impl Into<String>, verification: Verification) -> Self {
        Self {
            issuer: issuer.into(),
            verification,
            backend_clients: BTreeSet::new(),
            demographic_clients: BTreeSet::new(),
            patient: None,
            requester: None,
            assurance: None,
            client_tokens_act_for_professional: false,
            professional_issuing_authority: None,
            national_contact_point: None,
        }
    }
}

/// An issuer's patient tokens bound to one member, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatientBinding {
    /// The endpoint of the member whose platform issues the tokens.
    pub endpoint: EndpointId,
    /// The identifier system of that member's `ehr_id`s at the
    /// cross-reference.
    pub ehr_id_system: IdentifierNamespace,
    /// Whether the `act` claim of this issuer's patient tokens names a
    /// representative who acts for the patient.
    pub act_is_representative: bool,
}

/// How an issuer's tokens are verified.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Verification {
    /// As signed JWTs, by `ferrohealth-smart`, against the key set these
    /// settings name.
    KeySet(Box<KeySetSettings>),
    /// By RFC 7662 introspection at the issuer, by `ferrohealth-smart`, under
    /// these settings.
    Introspection(Box<IntrospectionSettings>),
}

impl Verification {
    /// Returns the audiences a token of the issuer must name one of; the
    /// first names the gateway in its access records.
    #[must_use]
    pub fn audience(&self) -> &[String] {
        match self {
            Self::KeySet(settings) => &settings.audience,
            Self::Introspection(settings) => &settings.audience,
        }
    }

    /// Returns the issuer's `[[auth.federation]]` table, when it is a
    /// `federation` issuer.
    #[must_use]
    pub fn federation(&self) -> Option<&FederationSection> {
        match self {
            Self::KeySet(settings) => settings.federation.as_ref(),
            Self::Introspection(settings) => settings.federation.as_ref(),
        }
    }

    /// Whether the issuer is a MyHealth@EU contact point, its
    /// `[[auth.federation]]` table setting `contact_point = true`.
    #[must_use]
    pub fn is_contact_point(&self) -> bool {
        self.federation()
            .is_some_and(|section| section.contact_point == Some(true))
    }

    /// Returns the audiences, to be changed.
    pub fn audience_mut(&mut self) -> &mut Vec<String> {
        match self {
            Self::KeySet(settings) => &mut settings.audience,
            Self::Introspection(settings) => &mut settings.audience,
        }
    }
}

impl PartialEq for Verification {
    fn eq(&self, other: &Self) -> bool {
        // NOTE: no specification governs this: our own design; the introspection
        // settings hold a credential, so two compare by what their rendering shows.
        match (self, other) {
            (Self::KeySet(left), Self::KeySet(right)) => left == right,
            (Self::Introspection(left), Self::Introspection(right)) => {
                format!("{left:?}") == format!("{right:?}")
            }
            _ => false,
        }
    }
}

impl Eq for Verification {}

/// Why `[auth]` or `[trust]` is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthFault {
    /// `[auth] enabled = false`: the gateway has no unauthenticated mode.
    Disabled,
    /// Two `[[trust.issuer]]` entries name the same issuer.
    DuplicateIssuer,
    /// A `[[trust.issuer]]` entry names no `[[auth.issuer]]`, so no token of
    /// it could be verified.
    UnknownIssuer,
    /// More than one issuer is introspected, so an opaque token, which names
    /// no issuer, could go to either.
    SeveralIntrospection,
    /// The header is no HTTP field name.
    HeaderName,
    /// The edge mode needs exactly one issuer, verified by its key set.
    EdgeIssuer,
    /// `[trust.edge]` is set without `mode = "edge"`.
    EdgeWithoutMode,
    /// A key `[auth]` carries is read by no part of the gateway: it serves
    /// no SMART discovery document and grants nothing without a scope.
    Unread,
    /// An assurance value is empty, or is declared at two levels.
    AssuranceValue,
    /// No value is declared at the minimum level or above it, so no token
    /// could reach patient data.
    AssuranceUnreachable,
    /// The correlation header names a header that carries a credential,
    /// which the access record would then hold.
    CorrelationCredential,
    /// An additional country is no ISO 3166-1 alpha-2 code, or is a Member
    /// State, which every contact point already admits; or a contact point's
    /// `countries` names a code that is neither.
    CountryCode,
    /// A `[[federation.contact_point]]` names no issuer whose
    /// `[[auth.federation]]` table sets `contact_point = true`.
    NotContactPoint,
    /// A contact-point issuer has no `[[federation.contact_point]]`.
    ContactPointUndeclared,
    /// `[trust.issuer.national_contact_point]`, removed in favour of
    /// `[[auth.federation]]` and `[[federation.contact_point]]`.
    ContactPointMoved,
}

impl fmt::Display for AuthFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disabled => "is false, and the gateway admits no caller it did not authenticate",
            Self::DuplicateIssuer => "names an issuer another [[trust.issuer]] already names",
            Self::UnknownIssuer => {
                "names an issuer no [[auth.issuer]] table trusts, so no token of it could be verified"
            }
            Self::SeveralIntrospection => {
                "introspects at a second issuer: an opaque token names no issuer, so one issuer at most is introspected"
            }
            Self::HeaderName => "is not an HTTP field name",
            Self::EdgeIssuer => {
                "needs exactly one [auth] issuer, the edge, verified by its key set, and no introspected issuer"
            }
            Self::EdgeWithoutMode => "is set, but mode is not \"edge\"",
            Self::Unread => {
                "is set, but the gateway serves no SMART discovery document and grants nothing without a scope"
            }
            Self::AssuranceValue => "is empty, or is declared at two levels",
            Self::AssuranceUnreachable => {
                "declares no value at the minimum level or above it, so no token could reach patient data"
            }
            Self::CorrelationCredential => {
                "names a header that carries a credential, which the access record would then hold"
            }
            Self::CountryCode => {
                "names a code that is not two upper-case letters (ISO 3166-1 alpha-2), or a Member State, which every contact point admits; or, in a contact point's countries, a code that is no Member State and not one of its [[federation.contact_point]] additional_countries"
            }
            Self::NotContactPoint => {
                "names no [[auth.issuer]] of kind \"federation\" whose [[auth.federation]] table sets contact_point = true"
            }
            Self::ContactPointUndeclared => {
                "is a contact point, and no [[federation.contact_point]] table names it"
            }
            Self::ContactPointMoved => {
                "is removed: declare a contact point as a kind = \"federation\" [[auth.issuer]] whose [[auth.federation]] table sets contact_point = true, with its correlation_header and additional_countries in [[federation.contact_point]]"
            }
        })
    }
}

impl Auth {
    /// Resolves `[auth]` with `trust`, its policy, and the
    /// `[[federation.contact_point]]` tables of `federation`, reading every
    /// `_file` secret.
    ///
    /// An `[[auth.issuer]]` that names no audience takes `public`, the public
    /// base URL as written, when that is set. A section that names no issuer
    /// and no Basic user resolves to no issuer, and every guarded request is
    /// then refused.
    ///
    /// # Errors
    /// Returns [`Error::Smart`] for a section `ferrohealth-smart` refuses,
    /// [`Error::Auth`] naming the key and the [`AuthFault`] for one FerroFED
    /// refuses, and the errors of `[trust]` and of
    /// [`contact_point::declare`].
    pub fn resolve(
        &self,
        trust: &Trust,
        federation: &Federation,
        public: Option<&PublicUrl>,
    ) -> Result<AuthSettings, Error> {
        let mut section = self.0.clone();
        if !section.enabled {
            return Err(fault("auth.enabled", AuthFault::Disabled));
        }
        for (key, empty) in [
            (
                "auth.scopeless_subjects",
                section.scopeless_subjects.is_empty(),
            ),
            ("auth.capabilities", section.capabilities.is_empty()),
            ("auth.scopes_supported", section.scopes_supported.is_empty()),
        ] {
            if !empty {
                return Err(fault(key, AuthFault::Unread));
            }
        }
        if let (IssuerKey::List(entries), Some(public)) = (&mut section.issuer, public) {
            for entry in entries {
                if entry.audience.iter().all(String::is_empty) {
                    entry.audience = vec![public.as_str().to_owned()];
                }
            }
        }
        let no_issuer = matches!(&section.issuer, IssuerKey::List(entries) if entries.is_empty());
        let no_basic = section
            .basic
            .as_ref()
            .is_none_or(|basic| basic.users.is_empty());
        let mut keyed = Vec::new();
        let mut introspected = Vec::new();
        let mut basic = None;
        let mut realm = None;
        let mut product_scope = None;
        let mut dpop = None;
        if !(no_issuer && no_basic) {
            let resolved = section.resolve().map_err(|source| Error::Smart {
                section: String::from("auth"),
                source: Box::new(source),
            })?;
            keyed = resolved.issuers;
            introspected = resolved.introspection;
            realm = resolved.realm;
            product_scope = resolved.product_scope;
            dpop = resolved.dpop;
            if let (Some(store), Some(written)) = (resolved.basic, section.basic.clone())
                && !written.users.is_empty()
            {
                basic = Some(BasicUsers::new(store, written));
            }
        }
        let (mode, purpose_required, mut issuers) = trust.resolve(keyed, introspected)?;
        contact_point::declare(&mut issuers, &federation.contact_point)?;
        Ok(AuthSettings {
            mode,
            purpose_required,
            product_scope,
            realm: realm.unwrap_or_else(|| DEFAULT_REALM.to_owned()),
            issuers,
            basic,
            dpop,
        })
    }
}

/// The refusal of `key` for `fault`.
pub(crate) fn fault(key: &str, fault: AuthFault) -> Error {
    Error::Auth {
        key: key.to_owned(),
        fault,
    }
}
