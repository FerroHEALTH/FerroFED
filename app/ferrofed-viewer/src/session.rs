// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's sign-in sessions, held on the server.
//!
//! The browser carries one opaque, random session id in an `HttpOnly`
//! cookie and nothing else; every fact about the session, the pending
//! sign-in's `state` and PKCE verifier included, stays in this process
//! (RFC 6749 §10.12, RFC 7636 §4.1). A session that sees no request for the
//! idle timeout is dropped, and the store holds at most the configured
//! number at once, refusing a new one rather than evicting a live one. No
//! specification governs the session store: our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use aws_lc_rs::rand::{SecureRandom as _, SystemRandom};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use cookie::{Cookie, SameSite};
use secrecy::{ExposeSecret as _, SecretString};

use crate::config::settings::SessionSettings;

/// The name of the session cookie.
pub const COOKIE: &str = "ferrofed_viewer_session";

/// How many random bytes a session id, a `state` and a PKCE verifier carry:
/// 256 bits, the entropy RFC 7636 §7.1 recommends for the verifier.
const RANDOM_BYTES: usize = 32;

/// Why a session could not be begun or read.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The system random source failed.
    #[error("the system random source failed")]
    Random,
    /// The store holds as many sessions as it is allowed to.
    #[error("the session store is full")]
    Full,
    /// A thread panicked while it held the store.
    #[error("the session store is unusable after a panic")]
    Poisoned,
}

/// An opaque session id, the value of the session cookie.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId(String);

impl SessionId {
    /// Takes the value a browser sent in the session cookie.
    #[must_use]
    pub fn from_cookie(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// The id as the cookie carries it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionId(..)")
    }
}

/// A sign-in the console sent to the OpenID Provider and is waiting on.
pub struct PendingSignIn {
    /// The `state` the authorization request carried (RFC 6749 §4.1.1).
    state: String,
    /// The PKCE code verifier its challenge was made from (RFC 7636 §4.1).
    verifier: SecretString,
}

impl PendingSignIn {
    /// Mints a fresh `state` and PKCE verifier.
    ///
    /// # Errors
    /// Returns [`SessionError::Random`] when the system random source fails.
    pub fn mint() -> Result<Self, SessionError> {
        Ok(Self {
            state: random_token()?,
            verifier: SecretString::from(random_token()?),
        })
    }

    /// The `state` the authorization request carries.
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    /// The PKCE code verifier.
    #[must_use]
    pub fn verifier(&self) -> &SecretString {
        &self.verifier
    }

    /// Whether `state` is the one this sign-in sent, compared in constant
    /// time.
    #[must_use]
    pub fn answers(&self, state: &str) -> bool {
        aws_lc_rs::constant_time::verify_slices_are_equal(self.state.as_bytes(), state.as_bytes())
            .is_ok()
    }
}

impl fmt::Debug for PendingSignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingSignIn").finish_non_exhaustive()
    }
}

/// One session.
#[derive(Debug)]
struct Session {
    /// When the session last saw a request.
    touched: Instant,
    /// The sign-in it is waiting on, until the callback takes it.
    pending: Option<PendingSignIn>,
}

/// The session store, shared by every request.
#[derive(Debug, Clone)]
pub struct Sessions {
    settings: SessionSettings,
    store: Arc<Mutex<BTreeMap<SessionId, Session>>>,
}

impl Sessions {
    /// An empty store under `settings`.
    #[must_use]
    pub fn new(settings: SessionSettings) -> Self {
        Self {
            settings,
            store: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Begins a session waiting on `pending` and returns its id.
    ///
    /// # Errors
    /// Returns [`SessionError::Full`] when the store is at its bound once the
    /// idle sessions are dropped, [`SessionError::Random`] when no id could
    /// be minted, and [`SessionError::Poisoned`] when the store is unusable.
    pub fn begin(&self, pending: PendingSignIn) -> Result<SessionId, SessionError> {
        let id = SessionId(random_token()?);
        let now = Instant::now();
        let mut store = self.lock()?;
        let idle = self.settings.idle_timeout;
        store.retain(|_, session| now.saturating_duration_since(session.touched) < idle);
        if store.len() >= self.settings.max_sessions {
            return Err(SessionError::Full);
        }
        store.insert(
            id.clone(),
            Session {
                touched: now,
                pending: Some(pending),
            },
        );
        Ok(id)
    }

    /// Takes the sign-in the session `id` is waiting on, once.
    ///
    /// Answers `None` for an unknown or idle session and for one with no
    /// sign-in pending.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn take_pending(&self, id: &SessionId) -> Result<Option<PendingSignIn>, SessionError> {
        let now = Instant::now();
        let mut store = self.lock()?;
        let idle = self.settings.idle_timeout;
        let Some(session) = store.get_mut(id) else {
            return Ok(None);
        };
        if now.saturating_duration_since(session.touched) >= idle {
            store.remove(id);
            return Ok(None);
        }
        session.touched = now;
        Ok(session.pending.take())
    }

    /// How many sessions the store holds, idle ones not yet dropped
    /// included.
    ///
    /// # Errors
    /// Returns [`SessionError::Poisoned`] when the store is unusable.
    pub fn count(&self) -> Result<usize, SessionError> {
        Ok(self.lock()?.len())
    }

    /// The `Set-Cookie` value that hands the browser the session `id`.
    ///
    /// The cookie is `HttpOnly`, so no script reads it, `SameSite=Lax`, so a
    /// cross-site subrequest does not carry it while the provider's redirect
    /// back does, and `Secure` unless the configuration turns it off.
    #[must_use]
    pub fn cookie(&self, id: &SessionId) -> Cookie<'static> {
        Cookie::build((COOKIE, id.as_str().to_owned()))
            .path("/")
            .http_only(true)
            .secure(self.settings.secure_cookie)
            .same_site(SameSite::Lax)
            .build()
    }

    /// Locks the store.
    fn lock(&self) -> Result<MutexGuard<'_, BTreeMap<SessionId, Session>>, SessionError> {
        self.store
            .lock()
            .map_err(|_poisoned| SessionError::Poisoned)
    }
}

/// The PKCE `S256` challenge of `verifier`: the base64url of its SHA-256
/// (RFC 7636 §4.2).
#[must_use]
pub fn challenge(verifier: &SecretString) -> String {
    let digest = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        verifier.expose_secret().as_bytes(),
    );
    URL_SAFE_NO_PAD.encode(digest.as_ref())
}

/// Returns [`RANDOM_BYTES`] fresh random bytes as base64url, 43 characters
/// of the unreserved set RFC 7636 §4.1 admits for a verifier.
fn random_token() -> Result<String, SessionError> {
    let mut bytes = [0_u8; RANDOM_BYTES];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_unspecified| SessionError::Random)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::{PendingSignIn, SessionError, SessionId, Sessions, challenge};
    use crate::config::settings::SessionSettings;
    use secrecy::SecretString;
    use std::time::Duration;

    fn settings(max_sessions: usize, idle_timeout: Duration) -> SessionSettings {
        SessionSettings {
            secure_cookie: true,
            idle_timeout,
            max_sessions,
        }
    }

    #[test]
    fn the_s256_challenge_is_the_one_rfc_7636_appendix_b_derives() {
        let verifier = SecretString::from("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            challenge(&verifier)
        );
    }

    #[test]
    fn a_minted_sign_in_carries_a_verifier_of_the_length_rfc_7636_admits() {
        let pending = PendingSignIn::mint().expect("the random source works");
        let verifier = secrecy::ExposeSecret::expose_secret(pending.verifier()).len();
        assert!((43..=128).contains(&verifier), "{verifier}");
        assert_ne!(
            pending.state(),
            secrecy::ExposeSecret::expose_secret(pending.verifier())
        );
    }

    #[test]
    fn a_pending_sign_in_is_taken_once_and_only_with_its_own_id() {
        let sessions = Sessions::new(settings(4, Duration::from_secs(60)));
        let pending = PendingSignIn::mint().expect("the random source works");
        let state = pending.state().to_owned();
        let id = sessions.begin(pending).expect("room for a session");
        assert!(
            sessions
                .take_pending(&SessionId::from_cookie("not-a-session"))
                .expect("usable")
                .is_none()
        );
        let taken = sessions
            .take_pending(&id)
            .expect("usable")
            .expect("pending");
        assert!(taken.answers(&state));
        assert!(!taken.answers("another-state"));
        assert!(sessions.take_pending(&id).expect("usable").is_none());
    }

    #[test]
    fn a_full_store_refuses_a_new_session() {
        let sessions = Sessions::new(settings(1, Duration::from_secs(60)));
        sessions
            .begin(PendingSignIn::mint().expect("random"))
            .expect("room for one");
        let refused = sessions.begin(PendingSignIn::mint().expect("random"));
        assert!(matches!(refused, Err(SessionError::Full)), "{refused:?}");
    }

    #[test]
    fn an_idle_session_is_dropped_and_makes_room() {
        let sessions = Sessions::new(settings(1, Duration::ZERO));
        let first = sessions
            .begin(PendingSignIn::mint().expect("random"))
            .expect("room for one");
        sessions
            .begin(PendingSignIn::mint().expect("random"))
            .expect("the idle one was dropped");
        assert_eq!(1, sessions.count().expect("usable"));
        assert!(sessions.take_pending(&first).expect("usable").is_none());
    }

    #[test]
    fn the_cookie_is_http_only_lax_and_secure() {
        let sessions = Sessions::new(settings(1, Duration::from_secs(60)));
        let cookie = sessions.cookie(&SessionId::from_cookie("abc")).to_string();
        assert_eq!(
            "ferrofed_viewer_session=abc; HttpOnly; SameSite=Lax; Secure; Path=/",
            cookie
        );
    }

    #[test]
    fn neither_an_id_nor_a_sign_in_shows_its_value_in_debug_output() {
        let pending = PendingSignIn::mint().expect("random");
        let shown = format!("{:?} {pending:?}", SessionId::from_cookie("abc"));
        assert!(!shown.contains("abc"), "{shown}");
        assert!(!shown.contains(pending.state()), "{shown}");
    }
}
