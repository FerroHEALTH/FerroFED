// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stub NVI Localization Service of the Dutch Generic Functions.
//!
//! It answers the GF-Localization search (Annex B §B.1; the
//! `nl-gf-localization-repository` capability statement of `fhir.nl.gf`
//! 0.3.0).
//!
//! **This is a test device, not a Localization Service.** It holds an index
//! of pseudonymised BSN to care provider URAs that a test sets, and answers
//! `GET [base]/DocumentReference?patient.identifier=<pseudo-bsn>|<value>&
//! type=http://loinc.org|55188-7` with a `searchset` of localization records
//! in the shape of the IG's example, one per URA; a pseudonym it does not
//! index gets an empty `searchset`. It performs no access check at a data
//! holder; a test that needs a provider withheld leaves it out of the index.
//!
//! Started with [`LocalizationService::start_guarded`], it answers only a
//! search its guard admits, such as the `DPoP`-bound token the harness Nuts
//! node issued ([`crate::nuts::NutsNode::dpop_bound`]), as an Authenticated
//! Interaction of the IG's GFI-005, and `401` with a `DPoP` challenge to any
//! other (RFC 9449 §7.1). It can demand a nonce in every proof
//! ([`LocalizationService::require_nonce`]), answering a proof without it
//! with the challenge of RFC 9449 §9. No specification governs the device:
//! our own design.
//!
//! Every pseudonym and URA a test gives it is synthetic, and nothing here
//! logs one; its `Debug` shows counts only.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntrySearch};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::document_reference::{DocumentReference, DocumentReferenceContent};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, Request, Respond, ResponseTemplate};

use crate::dpop;
use crate::mock::Server;

/// The FHIR base path the device serves under.
const BASE: &str = "/fhir";

/// The FHIR JSON media type.
const FHIR_JSON: &str = "application/fhir+json";

/// The naming system of a pseudonymised BSN (the IG's `pseudo-bsn`).
pub const PSEUDO_BSN_SYSTEM: &str = "http://fhir.nl/fhir/NamingSystem/pseudo-bsn";

/// The naming system of a URA (the IG's `$ura`).
const URA_SYSTEM: &str = "http://fhir.nl/fhir/NamingSystem/ura";

/// What the device answers besides its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outage {
    /// Every search is a `503`.
    Refusing,
    /// Every search waits a minute before it answers.
    Silent,
}

/// What the device holds.
#[derive(Debug, Default)]
struct State {
    /// Each pseudonym's care providers, by URA.
    index: BTreeMap<String, BTreeSet<String>>,
    /// The outage the device plays, if any.
    outage: Option<Outage>,
    /// The nonce every proof must name, if any.
    nonce: Option<String>,
}

/// The stub Localization Service.
pub struct LocalizationService {
    server: Server,
    state: Arc<Mutex<State>>,
}

impl LocalizationService {
    /// Starts the device with an empty index.
    pub async fn start() -> Self {
        let server = Server::start().await;
        let state = Arc::new(Mutex::new(State::default()));
        Mock::given(method("GET"))
            .and(path(format!("{BASE}/DocumentReference")))
            .respond_with(Answer(Arc::clone(&state)))
            .mount(&server)
            .await;
        Self { server, state }
    }

    /// Starts the device with an empty index, answering only a search
    /// `guard` admits, and `401` with a `DPoP` challenge to any other.
    pub async fn start_guarded(guard: impl Match + 'static) -> Self {
        let server = Server::start().await;
        let state = Arc::new(Mutex::new(State::default()));
        Mock::given(method("GET"))
            .and(path(format!("{BASE}/DocumentReference")))
            .and(guard)
            .respond_with(Answer(Arc::clone(&state)))
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{BASE}/DocumentReference")))
            .respond_with(
                ResponseTemplate::new(401)
                    .insert_header("WWW-Authenticate", "DPoP algs=\"ES256 ES384\""),
            )
            .with_priority(2)
            .mount(&server)
            .await;
        Self { server, state }
    }

    /// Requires every later proof to name `nonce`, answering one that does
    /// not with the `401` challenge of RFC 9449 §9 and the nonce.
    pub fn require_nonce(&self, nonce: &str) {
        self.lock().nonce = Some(nonce.to_owned());
    }

    /// The device's FHIR base URL.
    #[must_use]
    pub fn base(&self) -> String {
        format!("{}{BASE}", self.server.uri())
    }

    /// Indexes care provider `ura` as holding data for the patient whose
    /// pseudonymised BSN is `pseudonym`.
    pub fn index(&self, pseudonym: &str, ura: &str) {
        self.lock()
            .index
            .entry(pseudonym.to_owned())
            .or_default()
            .insert(ura.to_owned());
    }

    /// Makes every search answer `503`.
    pub fn refuse(&self) {
        self.lock().outage = Some(Outage::Refusing);
    }

    /// Makes every search wait a minute before it answers.
    pub fn go_silent(&self) {
        self.lock().outage = Some(Outage::Silent);
    }

    /// The query string of every search the device received, in order.
    pub async fn searches(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| request.url.query().unwrap_or_default().to_owned())
            .collect()
    }

    /// The value of the `header` of every search the device received, in
    /// order, empty for a search without it.
    pub async fn headers(&self, header: &str) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| {
                request
                    .headers
                    .get(header)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for LocalizationService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.lock();
        f.debug_struct("LocalizationService")
            .field("patients", &state.index.len())
            .field("outage", &state.outage)
            .finish_non_exhaustive()
    }
}

/// The responder every search reaches.
struct Answer(Arc<Mutex<State>>);

impl Respond for Answer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match state.outage {
            Some(Outage::Refusing) => return ResponseTemplate::new(503),
            Some(Outage::Silent) => {
                return ResponseTemplate::new(200).set_delay(Duration::from_secs(60));
            }
            None => {}
        }
        if let Some(nonce) = &state.nonce
            && proof_nonce(request).as_ref() != Some(nonce)
        {
            return dpop::challenge(nonce);
        }
        let query: BTreeMap<String, String> = request.url.query_pairs().into_owned().collect();
        if query.get("type").map(String::as_str) != Some("http://loinc.org|55188-7") {
            return ResponseTemplate::new(400);
        }
        let Some(pseudonym) = query
            .get("patient.identifier")
            .and_then(|token| token.strip_prefix(PSEUDO_BSN_SYSTEM))
            .and_then(|rest| rest.strip_prefix('|'))
        else {
            return ResponseTemplate::new(400);
        };
        let entry = state
            .index
            .get(pseudonym)
            .into_iter()
            .flatten()
            .map(|ura| BundleEntry {
                resource: Some(Resource::DocumentReference(Box::new(record(
                    pseudonym, ura,
                )))),
                search: Some(BundleEntrySearch {
                    mode: Some("match".into()),
                    ..BundleEntrySearch::default()
                }),
                ..BundleEntry::default()
            })
            .collect();
        let bundle = Bundle {
            r#type: "searchset".into(),
            entry,
            ..Bundle::default()
        };
        match serde_json::to_vec(&Resource::Bundle(Box::new(bundle))) {
            Ok(body) => ResponseTemplate::new(200).set_body_raw(body, FHIR_JSON),
            Err(_unencodable) => ResponseTemplate::new(500),
        }
    }
}

/// The `nonce` claim of a proof, as the device reads it.
#[derive(Deserialize)]
struct Nonce {
    nonce: Option<String>,
}

/// The nonce the `DPoP` proof of `request` names, when it carries a proof
/// that names one.
fn proof_nonce(request: &Request) -> Option<String> {
    let proof = request.headers.get(dpop::HEADER)?.to_str().ok()?;
    let claims = crate::nuts::payload(proof).ok()?;
    serde_json::from_slice::<Nonce>(&claims).ok()?.nonce
}

/// The localization record of the IG's example: `pseudonym`'s data held by
/// care provider `ura`.
fn record(pseudonym: &str, ura: &str) -> DocumentReference {
    DocumentReference {
        status: "current".into(),
        r#type: Some(CodeableConcept {
            coding: vec![Coding {
                system: Some("http://loinc.org".into()),
                code: Some("55188-7".into()),
                display: Some("Patient data Document".into()),
                ..Coding::default()
            }],
            ..CodeableConcept::default()
        }),
        subject: Some(by_identifier(PSEUDO_BSN_SYSTEM, pseudonym)),
        custodian: Some(by_identifier(URA_SYSTEM, ura)),
        content: vec![DocumentReferenceContent {
            attachment: fhir_types::r4::attachment::Attachment {
                content_type: Some("application/json+fhir".into()),
                url: Some(
                    format!("https://{ura}.example.org/fhir/Patient/synthetic")
                        .as_str()
                        .into(),
                ),
                ..fhir_types::r4::attachment::Attachment::default()
            },
            ..DocumentReferenceContent::default()
        }],
        ..DocumentReference::default()
    }
}

/// A reference by an identifier in `system` with `value`.
fn by_identifier(system: &str, value: &str) -> Reference {
    Reference {
        identifier: Some(Box::new(Identifier {
            system: Some(system.into()),
            value: Some(value.into()),
            ..Identifier::default()
        })),
        ..Reference::default()
    }
}
