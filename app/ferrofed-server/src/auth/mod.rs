// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Client authentication: every request to the ITS-REST surface and to
//! `OPTIONS {base}/` comes from a caller the gateway verified (§13.1, N25,
//! CP-17 inbound; §7a.2).
//!
//! `ferrohealth-smart` checks the credential. A caller presents an access
//! token in `Authorization: Bearer` (RFC 6750 §2.1), which the crate reads
//! and holds to the `b64token` syntax; its `iss` names an issuer `[auth]`
//! trusts, whose metadata and key set the crate reads (RFC 8414 §3) and
//! whose signature, `iss`, `aud`, `exp` and `nbf` it checks under the RFC
//! 9068 profile, with an asymmetric algorithm only; or the token is checked
//! by the one introspected issuer's RFC 7662 endpoint (`introspection`).
//! In the edge mode, a proxy authenticates the caller and the gateway
//! verifies the proxy's signed assertion, in the header the deployment
//! names, with the same checks (`verify`).
//!
//! The caller then needs what the operation requires ([`permission`]): a
//! SMART on openEHR resource scope, checked by the crate's
//! `Requirement::OpenEhr` with what the issuer's `[[trust.issuer]]` entry
//! lets count, and a purpose of use unless the deployment declared it
//! optional (§13.4). A request that reaches patient data also needs a
//! natural person behind the token, or a client its issuer declares as
//! acting for the professional the token names, and the authentication
//! assurance its issuer's entry requires (Regulation (EU) 2025/327 Annex II
//! 3.1). An issuer declared a national contact point vouches for the
//! professional of another Member State whose request its connector relays
//! ([`contact_point`]): such a request needs every Implementing Regulation
//! (EU) 2026/2099 Annex attribute and a purpose of use. A request that
//! fails is answered `400`, `401`, `403` or `503` and reaches nothing behind
//! the gate; one that passes carries its [`Caller`] in its extensions. The
//! caller's credential is never forwarded to a node: the node dispatch sends
//! each endpoint its own onward credentials and copies no client header
//! without a rule naming it.
//!
//! The [`Gate`] in place is held by a [`trust::Trust`], which a reload of
//! the configuration replaces whole; each request takes it once at the
//! [`guard`](guard::guard) and is admitted against that gate alone.
//!
//! The health family and `GET {base}/` stay open: they describe the process
//! and the product, no patient and no member (no specification governs this:
//! our own design). The JSON Web Key Set the gateway publishes for its own
//! client assertions is public too (RFC 7517), and is served beside them,
//! outside the gate.

pub mod caller;
mod claims;
pub mod contact_point;
pub mod guard;
mod introspection;
pub mod permission;
mod reference;
pub mod refusal;
pub mod trust;
mod verify;

use std::sync::Arc;

use ferrofed_engine::conveyance::caller::{Acting, AssuranceLevel};
use ferrofed_registry::id::EhrId;
use ferrohealth_smart::guard::{
    CredentialHeader, Gate as SmartGate, Requirement as SmartRequirement,
};
use ferrohealth_smart::openehr::smart_scopes::{Permission, ResourceFamily, SmartScope};
use ferrohealth_smart::openehr::{self, granted};
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::{Access, Caller as SmartCaller, Honoured};
use http::{HeaderMap, header};
use secrecy::SecretString;

use crate::auth::caller::{Caller, PatientContext, Stated, VerifiedBy};
use crate::auth::claims::Claims;
use crate::auth::contact_point::MissingAnnex;
use crate::auth::introspection::Introspector;
use crate::auth::permission::{Requirement, Resource};
use crate::auth::refusal::{BasicRefusal, Refusal, Refused};
use crate::auth::verify::Verifiers;
use crate::config::auth::{AuthMode, AuthSettings, IssuerSettings, Verification};
use crate::facade::options::ContactPoints;

/// The verifier of every caller, built from `[auth]` and `[trust]`.
#[derive(Debug)]
pub struct Gate {
    /// Where the credential travels.
    mode: AuthMode,
    /// The audience that names the gateway, `[auth]`'s first.
    audience: Option<String>,
    /// Whether a purpose of use is required.
    purpose_required: bool,
    /// The scope that admits a caller to the operator surface.
    product_scope: Option<String>,
    /// The realm every challenge names.
    realm: String,
    /// The issuers on the trust list, with their policy.
    issuers: Vec<IssuerSettings>,
    /// The crate's gate, which reads the credential from `Authorization`,
    /// or from the edge's header in the edge mode, and checks it with the
    /// issuers' verifiers.
    smart: SmartGate,
    /// The crate's gate over the Basic users of `[[auth.basic.users]]`,
    /// when any is written.
    basic: Option<SmartGate>,
    /// The key the security log's caller references are made under, kept
    /// by every gate that succeeds this one, so a reference stays stable for
    /// the life of the process.
    references: Arc<reference::References>,
    /// The national contact points `OPTIONS {base}/` declares.
    contact_points: Arc<ContactPoints>,
}

impl Gate {
    /// Returns the gate `settings` describe; no metadata or key set is read
    /// yet.
    #[must_use]
    pub fn new(settings: &AuthSettings) -> Self {
        Self::keyed(settings, Arc::new(reference::References::fresh()))
    }

    /// Returns the gate `settings` describe in place of this one, with the
    /// same caller references; no metadata or key set is read yet, so each
    /// issuer's is read again.
    #[must_use]
    pub fn succeeding(&self, settings: &AuthSettings) -> Self {
        Self::keyed(settings, Arc::clone(&self.references))
    }

    /// Returns the national contact points the gate's issuers declare, as
    /// `OPTIONS {base}/` declares them.
    #[must_use]
    pub fn contact_points(&self) -> &Arc<ContactPoints> {
        &self.contact_points
    }

    /// Returns the realm every challenge names.
    #[must_use]
    pub fn realm(&self) -> &str {
        &self.realm
    }

    /// Returns the gate `settings` describe, its caller references made
    /// under `references`.
    fn keyed(settings: &AuthSettings, references: Arc<reference::References>) -> Self {
        let mut keyed = Vec::new();
        let mut introspected = None;
        for issuer in &settings.issuers {
            let trusted = Arc::new(issuer.clone());
            match &issuer.verification {
                Verification::KeySet(server) => keyed.push(((**server).clone(), trusted)),
                Verification::Introspection(endpoint) => {
                    introspected = Some(Introspector::new(
                        issuer.issuer.clone(),
                        endpoint.clone(),
                        trusted,
                    ));
                }
            }
        }
        let edge = matches!(settings.mode, AuthMode::Edge(_));
        let bearer: Arc<dyn ferrohealth_smart::resource::Verifier> =
            Arc::new(Verifiers::new(keyed, introspected, edge));
        let smart = SmartGate::open()
            .with_bearer(bearer)
            .with_realm(settings.realm.clone());
        // NOTE: no specification governs the edge mode: our own design; the proxy's
        // assertion is the whole value of the header the deployment names.
        let smart = match &settings.mode {
            AuthMode::Token => smart,
            AuthMode::Edge(name) => {
                smart.with_credential_header(CredentialHeader::bare(name.clone()))
            }
        };
        let basic = settings.basic.as_ref().map(|users| {
            SmartGate::open()
                .with_basic(Arc::clone(users.store()))
                .with_realm(settings.realm.clone())
        });
        Self {
            mode: settings.mode.clone(),
            audience: settings.audience.first().cloned(),
            purpose_required: settings.purpose_required,
            product_scope: settings.product_scope.clone(),
            realm: settings.realm.clone(),
            issuers: settings.issuers.clone(),
            smart,
            basic,
            references,
            contact_points: Arc::new(ContactPoints::of(settings)),
        }
    }

    /// Verifies the caller of a request with `headers` and checks it against
    /// `requirement`, where `named` is the resource the request path names.
    ///
    /// The caller keeps its verified access token only when `exchanging`, a
    /// node's grant exchanging it (RFC 8693 §2.1), and only when the gateway
    /// verified it by signature or introspection; otherwise the token is
    /// dropped with the request's headers.
    ///
    /// # Errors
    /// Returns the [`Refused`] the request is answered with.
    pub async fn admit(
        &self,
        headers: &HeaderMap,
        (requirement, named): (Requirement, Option<&str>),
        exchanging: bool,
    ) -> Result<Caller, Refused> {
        // NOTE: no specification governs this: our own design; an operation no
        // caller may reach is refused before any credential is read.
        if requirement == Requirement::Refused {
            return Err(Refusal::Operation.into());
        }
        let (smart, credential) = self.authenticate(headers).await?;
        // NOTE: no specification governs this: our own design; the verifier that
        // routed the token by its `iss` attached that issuer's trust entry.
        let entry = smart
            .extension::<Arc<IssuerSettings>>()
            .map(Arc::clone)
            .ok_or(Refused {
                refusal: Refusal::Invalid,
                check: Some(Reason::UntrustedIssuer),
                missing: MissingAnnex::default(),
            })?;
        let trusted: &IssuerSettings = &entry;
        let caller = self.caller(&smart, trusted)?;
        let correlation = match &trusted.national_contact_point {
            Some(declared) => contact_point::correlation(headers, declared)?,
            None => None,
        };
        let caller = caller
            .with_audience(self.audience.clone())
            .with_correlation(correlation);
        let mut caller = if exchanging {
            caller.with_token(SecretString::from(credential))
        } else {
            caller
        };
        let access = Access::Caller(Arc::new(smart));
        let none = Honoured::default();
        match requirement {
            Requirement::Caller => return Ok(caller),
            Requirement::Refused => return Err(Refusal::Operation.into()),
            Requirement::Operator => {
                let admitted = self.product_scope.as_ref().is_some_and(|scope| {
                    SmartRequirement::Scope(scope.clone()).met_by(&access, none)
                });
                return admitted
                    .then_some(caller)
                    .ok_or_else(|| Refusal::Scope.into());
            }
            Requirement::Demographic => {
                let listed = SmartRequirement::Client {
                    client_ids: trusted.demographic_clients.iter().cloned().collect(),
                    basic_users: Vec::new(),
                };
                if !listed.met_by(&access, none) {
                    return Err(Refusal::Demographic.into());
                }
                // NOTE: SMART on openEHR master08 §Resource Scopes: a patient scope reaches "data
                // within that patient's EHR", so listing its client never widens it to parties.
                if openehr::patient_grant(caller.scopes()) {
                    return Err(Refusal::PatientDemographic.into());
                }
            }
            Requirement::Scope {
                family,
                permission,
                resource,
            } => {
                let resources: Vec<Option<String>> = match resource {
                    Resource::Unnamed => vec![None],
                    Resource::Path(_) => vec![named.map(str::to_owned)],
                    Resource::Sections => permission::sections()
                        .ok_or(Refusal::Scope)?
                        .into_iter()
                        .map(Some)
                        .collect(),
                };
                caller = scoped(caller, &access, trusted, (family, permission), &resources)?;
            }
        }
        if requirement.reaches_patient_data() {
            contact_point::relaying(&caller)?;
            professional(&caller, trusted)?;
        }
        // NOTE: §13.4 authn-purpose-of-use, a node must never be left to infer
        // the purpose, so a request that reaches past the gate carries one.
        if self.purpose_required && caller.purposes().is_empty() {
            return Err(Refusal::PurposeOfUse.into());
        }
        Ok(caller)
    }

    /// Returns the caller the credential of `headers` authenticates and the
    /// credential itself: the bearer token `ferrohealth-smart` reads from
    /// `Authorization`, or the edge's assertion from the header the
    /// deployment names, sent exactly once.
    async fn authenticate(&self, headers: &HeaderMap) -> Result<(SmartCaller, String), Refused> {
        let access = self
            .smart
            .authenticate(headers)
            .await
            .map_err(|denied| Refused::from(&denied))?;
        let Access::Caller(caller) = access else {
            return Err(Refusal::Missing.into());
        };
        // NOTE: RFC 6750 §2.1, the credential the crate verified is the one field's
        // text after its scheme, or the whole value of the edge's header.
        let credential = self
            .smart
            .credential(headers)
            .ok_or(Refusal::Malformed)?
            .to_owned();
        Ok((Arc::unwrap_or_clone(caller), credential))
    }

    /// Returns the gateway's caller of `smart`, the crate's, vouched for by
    /// `trusted`, with every claim the gateway reads.
    fn caller(&self, smart: &SmartCaller, trusted: &IssuerSettings) -> Result<Caller, Refused> {
        let invalid = |check: &'static str| Refused {
            refusal: Refusal::Invalid,
            check: Some(Reason::Other(check)),
            missing: MissingAnnex::default(),
        };
        // NOTE: RFC 9068 §2.2 requires `client_id`, and IHE IUA ITI TF-2 3.71.4.2.2.1
        // tells a client from a person by it, so a token without one does not hold.
        let client_id = smart
            .client_id()
            .filter(|client| !client.is_empty())
            .ok_or_else(|| invalid("client_id"))?
            .to_owned();
        let claims = Claims::read(smart.claims()).map_err(|_shape| invalid("claims"))?;
        let verified_by = match (&self.mode, &trusted.verification) {
            (AuthMode::Edge(_), _) => VerifiedBy::Edge,
            (_, Verification::Introspection(_)) => VerifiedBy::Introspection,
            (_, Verification::KeySet(_)) => VerifiedBy::Signature,
        };
        let subject = smart.subject().to_owned();
        let acting = acting(&subject, &client_id);
        let stated = Stated {
            issuer: trusted.issuer.clone(),
            subject,
            client_id,
            organisation: claims.organisation(),
            granted: smart.scope().join(" "),
            purposes: claims.purposes(),
        };
        let relayed = contact_point::read(trusted, claims.annex(), |claim| claims.text(claim));
        let caller = Caller::new(stated, verified_by)
            .with_launch_ehr_id(smart.ehr_id().map(str::to_owned))
            .with_requester(claims.requester(trusted.requester.as_ref()))
            .with_professional(
                claims.professional(trusted.professional_issuing_authority.as_deref()),
            )
            .with_assurance(assurance_of(trusted, |claim| claims.text(claim)))
            .with_acting(acting)
            .with_representative(claims.representative(trusted));
        Ok(contact_point::vouched(caller, relayed))
    }

    /// Admits a request whose Basic credential (RFC 7617) names one of
    /// `users`, written in `[[auth.basic.users]]`, and returns the user.
    ///
    /// # Errors
    /// Returns [`BasicRefusal::Missing`] when the request carries no Basic
    /// credential or no Basic user is written, [`BasicRefusal::Invalid`] for
    /// one that does not verify, and [`BasicRefusal::Unlisted`] for a user
    /// not in `users`.
    pub async fn admit_basic(
        &self,
        headers: &HeaderMap,
        users: &[String],
    ) -> Result<String, BasicRefusal> {
        let basic = self.basic.as_ref().ok_or(BasicRefusal::Missing)?;
        let access =
            basic
                .authenticate(headers)
                .await
                .map_err(|denied| match denied.refusal() {
                    ferrohealth_smart::refusal::Refusal::Missing(_) => BasicRefusal::Missing,
                    _ => BasicRefusal::Invalid,
                })?;
        if access.caller().is_none() {
            return Err(BasicRefusal::Missing);
        }
        access
            .basic_user_in(users)
            .map(str::to_owned)
            .ok_or(BasicRefusal::Unlisted)
    }

    /// Whether any caller can be admitted: some issuer is on the trust list.
    #[must_use]
    pub fn admits_any(&self) -> bool {
        !self.issuers.is_empty()
    }

    /// Whether `headers` carry the field a credential travels in, read or
    /// not: `Authorization`, or the edge's header.
    #[must_use]
    pub fn presents_credential(&self, headers: &HeaderMap) -> bool {
        headers.contains_key(self.smart.credential_header().name())
    }
}

/// Returns `caller`, admitted to `permission` on a resource of `family`
/// named each of `resources`, when the crate's `Requirement::OpenEhr` finds
/// every one granted, with the scopes that cover it, and confined to its
/// patient when only `patient/` grants do.
///
/// # Errors
///
/// Returns [`Refusal::Scope`] when a resource is not granted, and the
/// refusals of [`patient_context`].
fn scoped(
    mut caller: Caller,
    access: &Access,
    trusted: &IssuerSettings,
    (family, permission): (ResourceFamily, Permission),
    resources: &[Option<String>],
) -> Result<Caller, Refusal> {
    let honoured = Honoured {
        backend: trusted.backend_clients.contains(caller.client_id()),
        patient: trusted.patient.is_some(),
    };
    let met = resources.iter().all(|name| {
        SmartRequirement::OpenEhr {
            family,
            permission,
            resource: name.clone(),
        }
        .met_by(access, honoured)
    });
    if !met {
        return Err(Refusal::Scope);
    }
    // NOTE: N26, RFC 8693 §2.1 scope: an exchanged token asks for the granted
    // scopes that cover the operation, never the whole grant.
    let covering: Vec<SmartScope> = caller
        .scopes()
        .iter()
        .filter(|scope| {
            resources.iter().any(|name| {
                granted(
                    std::slice::from_ref(*scope),
                    family,
                    permission,
                    name.as_deref(),
                    honoured,
                )
            })
        })
        .cloned()
        .collect();
    let confined = openehr::confined(&covering);
    let backend = openehr::backend_only(&covering);
    caller = caller.with_covering(SmartScope::format_all(&covering));
    // NOTE: SMART on openEHR master08 §Resource Scopes: a `system/` scope is
    // granted "to backend applications acting without a user context".
    if backend {
        caller = caller.with_acting(Acting::Client);
    }
    if confined {
        let context = patient_context(&caller, trusted)?;
        caller = caller.with_patient(context);
    }
    Ok(caller)
}

/// Whether `headers` carry `expected` as their one bearer token (RFC 6750
/// §2.1), compared in constant time: the check of the PMIR feed token.
#[must_use]
pub fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return false;
    };
    let Some((scheme, token)) = value.to_str().ok().and_then(|text| text.split_once(' ')) else {
        return false;
    };
    scheme.eq_ignore_ascii_case("bearer")
        && aws_lc_rs::constant_time::verify_slices_are_equal(
            token.trim_start_matches(' ').as_bytes(),
            expected.as_bytes(),
        )
        .is_ok()
}

/// The patient `caller`'s `patient/` grant is confined to: its token's
/// `ehrId` at the member `trusted` is bound to.
///
/// # Errors
///
/// Returns [`Refusal::PatientContext`] when the token carries no `ehrId`, or
/// one that is no openEHR `HIER_OBJECT_ID`, and [`Refusal::Scope`] when the
/// issuer is bound to no member, whose `patient/` grants then count for
/// nothing.
fn patient_context(caller: &Caller, trusted: &IssuerSettings) -> Result<PatientContext, Refusal> {
    let binding = trusted.patient.as_ref().ok_or(Refusal::Scope)?;
    // NOTE: SMART on openEHR master04 §Capabilities conveys the context "via the `ehrId`
    // token claim"; one absent, or no HIER_OBJECT_ID, names no patient to confine to.
    let ehr_id = caller
        .launch_ehr_id()
        .and_then(|claim| EhrId::new(claim).ok())
        .ok_or(Refusal::PatientContext)?;
    Ok(PatientContext::new(
        binding.endpoint.clone(),
        binding.ehr_id_system.clone(),
        ehr_id,
    ))
}

/// Holds `caller`, admitted to patient data, to who acts and how they
/// authenticated, as `issuer`'s entry requires.
///
/// Regulation (EU) 2025/327 Annex II 3.1 asks for "reliable mechanisms for
/// the identification and authentication of health professionals", and
/// Implementing Regulation (EU) 2026/2099 Art 6(3) sets the assurance level
/// of that authentication for a cross-border exchange. Which claim values
/// stand for which level, and the least level, are the deployment's per
/// issuer: no specification governs this: our own design.
///
/// # Errors
///
/// Returns [`Refusal::NaturalPerson`] when a client acts and the issuer does
/// not declare its client tokens as acting for the professional they name,
/// or the token names none, and [`Refusal::Assurance`] when the issuer
/// requires a level and the token states none at or above it.
fn professional(caller: &Caller, issuer: &IssuerSettings) -> Result<(), Refusal> {
    // NOTE: 2026/2099 Art 7: a contact point's connector relays the professional its token
    // names, so its client tokens act for that professional as a declared issuer's do.
    let acts_for = issuer.client_tokens_act_for_professional || caller.is_contact_point();
    if caller.acting() == Acting::Client && !(acts_for && caller.names_professional()) {
        return Err(Refusal::NaturalPerson);
    }
    if let Some(assurance) = &issuer.assurance
        && caller
            .assurance()
            .is_none_or(|level| level < assurance.minimum)
    {
        return Err(Refusal::Assurance);
    }
    Ok(())
}

/// The assurance level `stated`, the value of the claim `issuer`'s entry
/// names, stands for, when the entry declares a mapping and the value is in
/// it.
fn assurance_of(
    issuer: &IssuerSettings,
    stated: impl Fn(&str) -> Option<String>,
) -> Option<AssuranceLevel> {
    let assurance = issuer.assurance.as_ref()?;
    // NOTE: no specification governs this: our own design; a value the entry declares at
    // no level states no level, which no minimum admits.
    stated(&assurance.claim).and_then(|value| assurance.level_of(&value))
}

/// Who acts behind a token whose `sub` is `subject` and whose `client_id`
/// is `client_id`.
fn acting(subject: &str, client_id: &str) -> Acting {
    // NOTE: IHE IUA ITI TF-2 3.71.4.2.2.1: `sub` is the "unique identifier of the
    // user" if known, "the client_id otherwise".
    if subject == client_id {
        Acting::Client
    } else {
        Acting::Person
    }
}
