// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token endpoint the two harness authorization servers share, the
//! OAuth 2.0 one ([`crate::oauth`]) and the Nuts node's ([`crate::nuts`]).
//!
//! Both read a token request the same way, a form body (RFC 6749 §4.4.2,
//! Nuts RFC021 §3), a `DPoP` proof header (RFC 9449 §4.1) and the Basic
//! credentials of the `Authorization` header (RFC 6749 §2.3.1), and both
//! answer a refusal with an RFC 6749 §5.2 error body. `Endpoint` does
//! that once: it records the form, answers a refusal a test set, checks the
//! proof where the grant requires one, and hands the request to the
//! `Grant` that tells the two apart. [`Issuance`] is what a test sets and
//! reads on either device: a refusal, a delay, a `DPoP` nonce, the tokens
//! issued and the verdict of every request. No specification governs the
//! device: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;
use wiremock::{Request, Respond, ResponseTemplate};

use crate::dpop;

/// What an endpoint made of one token request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A token was issued.
    Issued,
    /// The request was refused, for this reason.
    Refused(String),
}

/// A refusal the endpoint answers every request with, set by
/// [`Issuance::refuse`].
#[derive(Debug, Clone)]
struct Refusal {
    status: u16,
    error: String,
    description: String,
}

/// What an endpoint keeps across requests.
#[derive(Debug, Default)]
pub(crate) struct Ledger {
    /// The `expires_in` every token is issued with, none when `None`.
    pub(crate) expires_in: Option<u64>,
    nonce: Option<String>,
    refusal: Option<Refusal>,
    delay: Option<Duration>,
    /// The tokens issued and still accepted.
    pub(crate) accepted: BTreeSet<String>,
    /// The `DPoP` key thumbprint each bound token is bound to.
    pub(crate) bound: BTreeMap<String, String>,
    /// Every `jti` and `nonce` spent, each under the prefix of its kind.
    pub(crate) spent: BTreeSet<String>,
    verdicts: Vec<Verdict>,
    forms: Vec<Vec<(String, String)>>,
}

impl Ledger {
    /// Records `token` as issued, bound to the key of `jkt` when one is
    /// given.
    pub(crate) fn issue(&mut self, token: &str, jkt: Option<String>) {
        self.accepted.insert(token.to_owned());
        if let Some(jkt) = jkt {
            self.bound.insert(token.to_owned(), jkt);
        }
        self.verdicts.push(Verdict::Issued);
    }

    /// Records a request refused for `reason`, and answers it with `status`
    /// and the RFC 6749 §5.2 `error` and `reason`.
    pub(crate) fn refuse(&mut self, status: u16, error: &str, reason: &str) -> ResponseTemplate {
        self.verdicts.push(Verdict::Refused(reason.to_owned()));
        refused(status, error, reason)
    }

    /// The thumbprint of the key of `proof` for a request to `token_url`,
    /// or the answer to a request whose proof is refused: none, one that
    /// does not verify, one without the nonce a test requires (RFC 9449 §8)
    /// or one whose `jti` was spent before.
    fn proven(
        &mut self,
        proof: Option<&str>,
        token_url: &str,
    ) -> Result<String, Box<ResponseTemplate>> {
        let Some(proof) = proof else {
            return Err(Box::new(self.refuse(
                400,
                "invalid_dpop_proof",
                "no DPoP proof",
            )));
        };
        let verified = dpop::verify(proof, "POST", token_url, None)
            .map_err(|reason| Box::new(self.refuse(400, "invalid_dpop_proof", &reason)))?;
        if let Some(nonce) = self.nonce.clone()
            && verified.proof.nonce.as_ref() != Some(&nonce)
        {
            self.verdicts
                .push(Verdict::Refused("use_dpop_nonce".to_owned()));
            return Err(Box::new(
                refused(400, "use_dpop_nonce", "a nonce is required")
                    .insert_header(dpop::NONCE_HEADER, nonce.as_str()),
            ));
        }
        if !self.spent.insert(format!("dpop:{}", verified.proof.jti)) {
            self.verdicts.push(Verdict::Refused(
                "the proof's jti was used before".to_owned(),
            ));
            return Err(Box::new(refused(400, "invalid_dpop_proof", "jti")));
        }
        Ok(verified.jkt)
    }
}

/// What a test sets and reads on a harness token endpoint. Every setting
/// applies to the requests that arrive after it.
#[derive(Debug, Clone, Default)]
pub struct Issuance(Arc<Mutex<Ledger>>);

impl Issuance {
    /// An issuance whose tokens live `expires_in` seconds, stating none
    /// when `None`.
    pub(crate) fn new(expires_in: Option<u64>) -> Self {
        Self(Arc::new(Mutex::new(Ledger {
            expires_in,
            ..Ledger::default()
        })))
    }

    /// The ledger, locked. A caller that also locks its device's own state
    /// locks this first.
    pub(crate) fn lock(&self) -> MutexGuard<'_, Ledger> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Requires every later proof to name `nonce`, answering one that does
    /// not with `400` `use_dpop_nonce` and the nonce (RFC 9449 §8).
    pub fn require_nonce(&self, nonce: &str) {
        self.lock().nonce = Some(nonce.to_owned());
    }

    /// Answers every later token request with the RFC 6749 §5.2 error
    /// `error`, `description` and `status`.
    pub fn refuse(&self, status: u16, error: &str, description: &str) {
        self.lock().refusal = Some(Refusal {
            status,
            error: error.to_owned(),
            description: description.to_owned(),
        });
    }

    /// Answers every later token request after `delay`.
    pub fn delay(&self, delay: Duration) {
        self.lock().delay = Some(delay);
    }

    /// Stops accepting every token issued so far, as a node whose
    /// authorization server revoked them would.
    pub fn revoke_all(&self) {
        self.lock().accepted.clear();
    }

    /// Returns whether `token` is one the endpoint issued and still accepts,
    /// for a test that reads the `Authorization` a real node received from a
    /// proxy journal rather than through a `wiremock` matcher.
    #[must_use]
    pub fn accepts(&self, token: &str) -> bool {
        self.lock().accepted.contains(token)
    }

    /// The verdict of every token request, in arrival order.
    #[must_use]
    pub fn verdicts(&self) -> Vec<Verdict> {
        self.lock().verdicts.clone()
    }

    /// The tokens issued so far.
    #[must_use]
    pub fn issued(&self) -> usize {
        self.lock()
            .verdicts
            .iter()
            .filter(|verdict| **verdict == Verdict::Issued)
            .count()
    }

    /// Every form parameter of every token request, in arrival order.
    #[must_use]
    pub fn forms(&self) -> Vec<Vec<(String, String)>> {
        self.lock().forms.clone()
    }
}

/// One token request as an endpoint reads it.
#[derive(Debug)]
pub(crate) struct TokenRequest {
    /// The form parameters of its body, in order.
    pub(crate) form: Vec<(String, String)>,
    /// Its `DPoP` proof header.
    pub(crate) proof: Option<String>,
    /// The credentials of its `Authorization: Basic` header, still encoded.
    pub(crate) basic: Option<String>,
}

impl TokenRequest {
    fn read(request: &Request) -> Self {
        let header = |name| {
            request
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
        };
        Self {
            form: url::form_urlencoded::parse(&request.body)
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect(),
            proof: header(dpop::HEADER).map(str::to_owned),
            basic: header(http::header::AUTHORIZATION.as_str())
                .and_then(|value| value.strip_prefix("Basic "))
                .map(str::to_owned),
        }
    }

    /// The value of the form parameter `name`, the first when it repeats.
    pub(crate) fn field(&self, name: &str) -> Option<String> {
        self.form
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }
}

/// The grant a harness endpoint checks, the part the two devices do not
/// share.
pub(crate) trait Grant: Send + Sync + 'static {
    /// The URL the endpoint answers on, the `htu` of every proof.
    fn token_url(&self) -> &str;

    /// Whether every request must carry a `DPoP` proof.
    fn requires_proof(&self) -> bool;

    /// Answers `request`, whose proof, when one was required, the key of
    /// `jkt` signed: a token recorded with [`Ledger::issue`], or a refusal
    /// recorded with [`Ledger::refuse`].
    fn answer(
        &self,
        ledger: &mut Ledger,
        request: &TokenRequest,
        jkt: Option<String>,
    ) -> ResponseTemplate;
}

/// The `wiremock` responder of a harness token endpoint.
pub(crate) struct Endpoint<G> {
    /// What the endpoint keeps across requests.
    pub(crate) issuance: Issuance,
    /// The grant it checks.
    pub(crate) grant: Arc<G>,
}

impl<G: Grant> Respond for Endpoint<G> {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let request = TokenRequest::read(request);
        let mut ledger = self.issuance.lock();
        ledger.forms.push(request.form.clone());
        let delay = ledger.delay;
        let answer = if let Some(refusal) = ledger.refusal.clone() {
            ledger
                .verdicts
                .push(Verdict::Refused(refusal.error.clone()));
            refused(refusal.status, &refusal.error, &refusal.description)
        } else if self.grant.requires_proof() {
            match ledger.proven(request.proof.as_deref(), self.grant.token_url()) {
                Ok(jkt) => self.grant.answer(&mut ledger, &request, Some(jkt)),
                Err(answer) => *answer,
            }
        } else {
            self.grant.answer(&mut ledger, &request, None)
        };
        match delay {
            Some(delay) => answer.set_delay(delay),
            None => answer,
        }
    }
}

#[derive(Serialize)]
struct Refused<'a> {
    error: &'a str,
    error_description: &'a str,
}

/// An RFC 6749 §5.2 error answer with `status`.
pub(crate) fn refused(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(Refused {
        error,
        error_description: description,
    })
}
