// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a call made for a verified caller hands its node's onward credentials.
//!
//! That is the caller's verified token (§13.1, N25, N26), the source of
//! credentials that depend on whom a call is made for, and the check that
//! no withheld identifier leaves in the caller's token (§5.4.1, N33).
//!
//! The one source of that kind is a node's onward lane
//! ([`lane`](crate::onward::grant::lane)), which `ferrohealth-smart`'s
//! onward client serves. No specification governs the grouping: our own
//! design.

use std::fmt;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrohealth_smart::client::onward::SubjectToken as OnwardSubject;
use secrecy::{ExposeSecret, SecretString};

use crate::conveyance::Conveyance;
use crate::conveyance::caller::{Principal, Verification};
use crate::dispatch::SharedCredentials;
use crate::hygiene::Withheld;

/// The caller's verified access token, and the scope its exchange asks for.
///
/// It may also carry the caller as `ferrohealth-smart` verified it, the
/// subject of an endpoint's onward lane ([`lane`](crate::onward::grant::lane)).
/// `Debug` shows none of it.
#[derive(Clone)]
pub struct SubjectToken {
    token: SecretString,
    scope: String,
    onward: Option<OnwardSubject>,
}

impl SubjectToken {
    /// The caller's verified `token`, exchanged for `scope`: the caller's
    /// granted scopes that cover the operation, space-separated; an empty
    /// scope is an operation no scope covers.
    #[must_use]
    pub fn new(token: SecretString, scope: impl Into<String>) -> Self {
        Self {
            token,
            scope: scope.into(),
            onward: None,
        }
    }

    /// This subject, carrying `onward`, the caller as `ferrohealth-smart`
    /// verified it, for a node reached through an onward lane.
    #[must_use]
    pub fn with_onward(mut self, onward: OnwardSubject) -> Self {
        self.onward = Some(onward);
        self
    }

    /// The caller as `ferrohealth-smart` verified it, when the subject
    /// carries it.
    #[must_use]
    pub fn onward(&self) -> Option<&OnwardSubject> {
        self.onward.as_ref()
    }

    /// The caller's token.
    #[must_use]
    pub fn token(&self) -> &SecretString {
        &self.token
    }

    /// The scope the exchange asks for.
    #[must_use]
    pub fn scope(&self) -> &str {
        &self.scope
    }
}

impl fmt::Debug for SubjectToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubjectToken").finish_non_exhaustive()
    }
}

/// A source of onward credentials that depend on whom a call is made for.
pub trait OnBehalf: Send + Sync + fmt::Debug {
    /// The credentials provider of one call conveying `conveyance`, which
    /// refuses to send the caller's token when it carries an identifier of
    /// `withheld` (§5.4.1, N33).
    fn provider(&self, conveyance: &Conveyance, withheld: &Arc<Withheld>) -> SharedCredentials;

    /// The kind of tokens the source's issuer signs, when the source is an
    /// onward lane, whose node derives each commit's committer from them.
    fn committer_kind(&self) -> Option<ferrohealth_smart::kind::IssuerKind> {
        None
    }

    /// Whether a call conveying `conveyance` reaches the node read-only
    /// through the source, so a write is refused before any token is asked
    /// for.
    fn read_only(&self, _conveyance: &Conveyance) -> bool {
        false
    }
}

/// A shared [`OnBehalf`] source of one endpoint.
pub type SharedOnBehalf = Arc<dyn OnBehalf>;

/// What one call made for a verified caller hands its onward credentials.
pub(crate) struct ForCall {
    /// Whether the edge asserted the caller, so the gateway holds no token
    /// of it.
    pub(crate) edge: bool,
    /// The caller's verified token, when the call carries it.
    pub(crate) subject: Option<SubjectToken>,
    /// The identifiers the request withholds.
    pub(crate) withheld: Arc<Withheld>,
}

impl ForCall {
    /// Returns the call `conveyance` makes for a verified caller, or `None`
    /// for the gateway's own request.
    pub(crate) fn of(conveyance: &Conveyance, withheld: &Arc<Withheld>) -> Option<Self> {
        match conveyance.principal() {
            Principal::Gateway => None,
            Principal::Caller(caller) => Some(Self {
                // NOTE: RFC 8693 §2.1, only a token the gateway itself verified is a
                // caller's subject_token; an edge assertion never is.
                edge: !matches!(
                    caller.verified_by,
                    Verification::Signature | Verification::Introspection
                ),
                subject: conveyance.subject().cloned(),
                withheld: Arc::clone(withheld),
            }),
        }
    }
}

/// Whether `subject`'s token or scope carries an identifier of `withheld`,
/// raw or in the decoded payload of a JWS.
pub(crate) fn carries_withheld(subject: &SubjectToken, withheld: &Withheld) -> bool {
    let token = subject.token().expose_secret();
    if withheld.carried_by(token) || withheld.carried_by(subject.scope()) {
        return true;
    }
    // NOTE: RFC 7515 §7.1, the payload is the second segment, base64url; a token
    // that is no JWS is opaque, and its raw text is all there is to read.
    let payload = token
        .split('.')
        .nth(1)
        .and_then(|segment| URL_SAFE_NO_PAD.decode(segment).ok());
    payload.is_some_and(|payload| withheld.carried_by(&String::from_utf8_lossy(&payload)))
}
