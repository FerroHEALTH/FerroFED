// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness Nuts node, a care provider's authorization server on the
//! GF-Authentication track.
//!
//! Annex B §B.4 of the Federation Tier specification, the IG's GFI-004 and
//! Nuts RFC021 define what it answers.
//!
//! **This is a test device, not an authorization server.** It serves, under
//! one tenant path:
//!
//! - its metadata at the RFC 8414 §3.1 well-known URL, naming its token and
//!   Presentation Definition endpoints, `vp_formats` with `jwt_vp` (RFC021
//!   §3.1) and the `DPoP` algorithms it takes;
//! - the Presentation Definition a test set ([`NutsNode::define`]) for the
//!   one scope it grants (RFC021 §5);
//! - the token endpoint, which answers the `vp_token-bearer` grant (RFC021
//!   §3). It verifies the presentation against the holder key a test trusts
//!   ([`NutsNode::trust_holder`]), as the holder's DID document would
//!   publish it (GFI-001): the signature by the key its `kid` names, `iss`
//!   and `sub` the holder's DID, `aud` its issuer, `nbf` and `exp` at most
//!   five seconds apart around now, and a `nonce` it has not seen (RFC021
//!   §4.2, §4.4). It reads each credential the submission maps (path
//!   `$.verifiableCredential[i]`, format `jwt_vc`), verifies it against the
//!   credential issuer key a test trusts ([`NutsNode::trust_issuer`]) and
//!   requires its `sub` to be the holder; and it requires every input
//!   descriptor of the definition to be answered. It requires a `DPoP` proof
//!   of every request ([`crate::dpop::verify`]) and binds the token it
//!   issues to the proof's key, typed `DPoP` (RFC 9449 §5).
//!
//! [`NutsNode::dpop_bound`] is the matcher a mock node requires an issued
//! token with, proven by the bound key. [`credential`] mints a synthetic
//! JWT Verifiable Credential (VC Data Model 1.1 §6.3.1) with a key a test
//! generates at run time ([`crate::oauth::p256_pem`]), so no key and no real
//! identifier is ever committed. No specification governs the device: our
//! own design.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, Request, Respond, ResponseTemplate};

use crate::dpop;
use crate::mock::Server;

/// The `grant_type` the token endpoint answers (Nuts RFC021 §3).
pub const GRANT_TYPE: &str = "vp_token-bearer";

/// The longest a presentation may be valid, `exp` less `nbf`, in seconds
/// (Nuts RFC021 §4.2 item 9).
pub const PRESENTATION_LIFETIME_S: i64 = 5;

/// The clock skew the device allows, in seconds (Nuts RFC021 §4.1 item 4).
pub const SKEW_S: i64 = 5;

/// A key could not be read or a credential could not be signed.
#[derive(Debug, thiserror::Error)]
#[error("the synthetic key could not be used: {0}")]
pub struct KeyError(String);

/// The public half of the private key `pem` (PKCS#8 PEM) as a JWK, for
/// `algorithm`.
///
/// # Errors
///
/// Returns [`KeyError`] when `pem` is no EC key for `algorithm`.
pub fn public_jwk(pem: &str, algorithm: Algorithm) -> Result<Jwk, KeyError> {
    let key = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| KeyError(e.to_string()))?;
    Jwk::from_encoding_key(&key, algorithm).map_err(|e| KeyError(e.to_string()))
}

/// The claims of a synthetic credential.
#[derive(Serialize)]
struct CredentialClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    nbf: i64,
    exp: i64,
    jti: String,
    vc: CredentialBody<'a>,
}

#[derive(Serialize)]
struct CredentialBody<'a> {
    #[serde(rename = "@context")]
    context: [&'static str; 1],
    #[serde(rename = "type")]
    types: [&'a str; 2],
    #[serde(rename = "credentialSubject")]
    subject: Subject<'a>,
}

#[derive(Serialize)]
struct Subject<'a> {
    id: &'a str,
    name: &'a str,
}

/// Mints a synthetic JWT Verifiable Credential (VC Data Model 1.1 §6.3.1).
///
/// The credential is of type `credential_type`, issued by `issuer` with the ES256 key
/// `pem` named `kid`, to `holder`, naming it `name`, valid until `exp`
/// (seconds since the epoch).
///
/// # Errors
///
/// Returns [`KeyError`] when the key cannot sign.
pub fn credential(
    (pem, kid): (&str, &str),
    issuer: &str,
    holder: &str,
    (credential_type, label): (&str, &str),
    exp: i64,
) -> Result<String, KeyError> {
    let key = EncodingKey::from_ec_pem(pem.as_bytes()).map_err(|e| KeyError(e.to_string()))?;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(kid.to_owned());
    let claims = CredentialClaims {
        iss: issuer,
        sub: holder,
        nbf: jiff::Timestamp::now().as_second().saturating_sub(60),
        exp,
        jti: format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        vc: CredentialBody {
            context: ["https://www.w3.org/2018/credentials/v1"],
            types: ["VerifiableCredential", credential_type],
            subject: Subject {
                id: holder,
                name: label,
            },
        },
    };
    jsonwebtoken::encode(&header, &claims, &key).map_err(|e| KeyError(e.to_string()))
}

/// What the device made of one token request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A token was issued.
    Issued,
    /// The request was refused, for this reason.
    Refused(String),
}

/// A key the device trusts, by the DID URL that names it.
#[derive(Debug, Clone)]
struct Trusted {
    did: String,
    kid: String,
    jwk: Jwk,
}

#[derive(Debug, Clone)]
struct Refusal {
    status: u16,
    error: String,
    description: String,
}

#[derive(Debug, Default)]
struct State {
    definition: Option<String>,
    holder: Option<Trusted>,
    issuer: Option<Trusted>,
    client_id: Option<String>,
    expires_in: Option<u64>,
    nonce: Option<String>,
    refusal: Option<Refusal>,
    delay: Option<Duration>,
    bearer: bool,
    seen: BTreeSet<String>,
    accepted: BTreeSet<String>,
    bound: std::collections::BTreeMap<String, String>,
    verdicts: Vec<Verdict>,
    forms: Vec<Vec<(String, String)>>,
    definitions_served: usize,
}

#[derive(Debug)]
struct Shared {
    issuer: String,
    token_url: String,
    scope: String,
    state: Mutex<State>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The harness Nuts node.
#[derive(Debug)]
pub struct NutsNode {
    server: Server,
    shared: Arc<Shared>,
}

impl NutsNode {
    /// Starts a device for the tenant `tenant` that grants `scope`, issuing
    /// tokens that live `expires_in` seconds, stating none when `None`.
    ///
    /// Its issuer is `{base}/oauth2/{tenant}`.
    pub async fn start(tenant: &str, scope: &str, expires_in: Option<u64>) -> Self {
        let server = Server::start().await;
        let tenant_path = format!("/oauth2/{tenant}");
        let issuer = format!("{}{tenant_path}", server.uri());
        let token_path = format!("{tenant_path}/token");
        let definition_path = format!("{tenant_path}/presentation_definition");
        let shared = Arc::new(Shared {
            issuer: issuer.clone(),
            token_url: format!("{}{token_path}", server.uri()),
            scope: scope.to_owned(),
            state: Mutex::new(State {
                expires_in,
                ..State::default()
            }),
        });
        let metadata = Metadata {
            issuer: &issuer,
            token_endpoint: &shared.token_url,
            presentation_definition_endpoint: &format!("{}{definition_path}", server.uri()),
            grant_types_supported: [GRANT_TYPE],
            vp_formats: Formats {
                jwt_vp: Algorithms {
                    alg: ["ES256", "ES384"],
                },
                jwt_vc: Algorithms {
                    alg: ["ES256", "ES384"],
                },
            },
            dpop_signing_alg_values_supported: ["ES256", "ES384"],
        };
        Mock::given(method("GET"))
            .and(path(format!(
                "/.well-known/oauth-authorization-server{tenant_path}"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(definition_path))
            .respond_with(Definitions(Arc::clone(&shared)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(token_path))
            .respond_with(Tokens(Arc::clone(&shared)))
            .mount(&server)
            .await;
        Self { server, shared }
    }

    /// The issuer identifier (RFC 8414 §2), the `aud` of every presentation.
    #[must_use]
    pub fn issuer(&self) -> String {
        self.shared.issuer.clone()
    }

    /// The token endpoint.
    #[must_use]
    pub fn token_url(&self) -> String {
        self.shared.token_url.clone()
    }

    /// The base URL of the device's server.
    #[must_use]
    pub fn uri(&self) -> String {
        self.server.uri()
    }

    /// Serves `definition`, a Presentation Definition in JSON, for the scope.
    pub fn define(&self, definition: &str) {
        self.shared.lock().definition = Some(definition.to_owned());
    }

    /// Trusts `jwk` as the key `kid`, a DID URL of `did`, the holder signs
    /// its presentations with.
    pub fn trust_holder(&self, did: &str, kid: &str, jwk: Jwk) {
        self.shared.lock().holder = Some(Trusted {
            did: did.to_owned(),
            kid: kid.to_owned(),
            jwk,
        });
    }

    /// Trusts `jwk` as the key `kid` the credential issuer `did` signs with.
    pub fn trust_issuer(&self, did: &str, kid: &str, jwk: Jwk) {
        self.shared.lock().issuer = Some(Trusted {
            did: did.to_owned(),
            kid: kid.to_owned(),
            jwk,
        });
    }

    /// Refuses every later token request whose `client_id` is not
    /// `client_id`.
    pub fn expect_client_id(&self, client_id: &str) {
        self.shared.lock().client_id = Some(client_id.to_owned());
    }

    /// Requires every later proof to name `nonce`, answering one that does
    /// not with `400` `use_dpop_nonce` and the nonce (RFC 9449 §8).
    pub fn require_nonce(&self, nonce: &str) {
        self.shared.lock().nonce = Some(nonce.to_owned());
    }

    /// Answers every later token request with the RFC 6749 §5.2 error
    /// `error`, `description` and `status`.
    pub fn refuse(&self, status: u16, error: &str, description: &str) {
        self.shared.lock().refusal = Some(Refusal {
            status,
            error: error.to_owned(),
            description: description.to_owned(),
        });
    }

    /// Answers every later token request after `delay`.
    pub fn delay(&self, delay: Duration) {
        self.shared.lock().delay = Some(delay);
    }

    /// Issues every later token typed `Bearer`, as a server that ignored
    /// the proof would.
    pub fn issue_bearer(&self) {
        self.shared.lock().bearer = true;
    }

    /// Stops accepting every token issued so far.
    pub fn revoke_all(&self) {
        self.shared.lock().accepted.clear();
    }

    /// The verdict of every token request, in arrival order.
    #[must_use]
    pub fn verdicts(&self) -> Vec<Verdict> {
        self.shared.lock().verdicts.clone()
    }

    /// The tokens issued so far.
    #[must_use]
    pub fn issued(&self) -> usize {
        self.verdicts()
            .iter()
            .filter(|verdict| **verdict == Verdict::Issued)
            .count()
    }

    /// The Presentation Definitions served so far.
    #[must_use]
    pub fn definitions_served(&self) -> usize {
        self.shared.lock().definitions_served
    }

    /// Every form parameter of every token request, in arrival order.
    #[must_use]
    pub fn forms(&self) -> Vec<Vec<(String, String)>> {
        self.shared.lock().forms.clone()
    }

    /// A matcher for a request carrying `Authorization: DPoP` with a token
    /// this device bound and still accepts, and a proof of the bound key
    /// over the request's method, URL and token (RFC 9449 §7.1).
    #[must_use]
    pub fn dpop_bound(&self) -> DpopBound {
        DpopBound {
            shared: Arc::clone(&self.shared),
        }
    }
}

#[derive(Serialize)]
struct Metadata<'a> {
    issuer: &'a str,
    token_endpoint: &'a str,
    presentation_definition_endpoint: &'a str,
    grant_types_supported: [&'static str; 1],
    vp_formats: Formats,
    dpop_signing_alg_values_supported: [&'static str; 2],
}

#[derive(Serialize)]
struct Formats {
    jwt_vp: Algorithms,
    jwt_vc: Algorithms,
}

#[derive(Serialize)]
struct Algorithms {
    alg: [&'static str; 2],
}

struct Definitions(Arc<Shared>);

impl Respond for Definitions {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let scope = request
            .url
            .query_pairs()
            .find(|(name, _)| name == "scope")
            .map(|(_, value)| value.into_owned());
        let mut state = self.0.lock();
        if scope.as_deref() != Some(self.0.scope.as_str()) {
            return refused(400, "invalid_scope", "unknown scope");
        }
        let Some(definition) = state.definition.clone() else {
            return refused(500, "server_error", "no definition set");
        };
        state.definitions_served = state.definitions_served.saturating_add(1);
        ResponseTemplate::new(200).set_body_raw(definition, "application/json")
    }
}

/// The members of a Presentation Definition the device reads.
#[derive(Deserialize)]
struct Definition {
    id: String,
    input_descriptors: Vec<Descriptor>,
}

#[derive(Deserialize)]
struct Descriptor {
    id: String,
}

/// The members of a Presentation Submission the device reads.
#[derive(Deserialize)]
struct Submission {
    definition_id: String,
    descriptor_map: Vec<Mapping>,
}

#[derive(Deserialize)]
struct Mapping {
    id: String,
    format: String,
    path: String,
}

/// The claims of a presentation the device reads.
#[derive(Deserialize)]
struct PresentationClaims {
    iss: String,
    sub: String,
    aud: String,
    nbf: i64,
    exp: i64,
    nonce: Option<String>,
    vp: Presentation,
}

#[derive(Deserialize)]
struct Presentation {
    #[serde(rename = "type")]
    types: Vec<String>,
    #[serde(rename = "verifiableCredential")]
    credentials: Vec<String>,
}

/// The claims of a credential the device reads.
#[derive(Deserialize)]
struct HeldClaims {
    sub: String,
}

#[derive(Serialize)]
struct TokenBody<'a> {
    access_token: &'a str,
    token_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
    scope: &'a str,
}

#[derive(Serialize)]
struct Refused<'a> {
    error: &'a str,
    error_description: &'a str,
}

struct Tokens(Arc<Shared>);

impl Respond for Tokens {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let form: Vec<(String, String)> = url::form_urlencoded::parse(&request.body)
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let proof = request
            .headers
            .get(dpop::HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut state = self.0.lock();
        state.forms.push(form.clone());
        let delay = state.delay;
        let answer = self.answer(&mut state, &form, proof.as_deref());
        match delay {
            Some(delay) => answer.set_delay(delay),
            None => answer,
        }
    }
}

impl Tokens {
    fn answer(
        &self,
        state: &mut State,
        form: &[(String, String)],
        proof: Option<&str>,
    ) -> ResponseTemplate {
        if let Some(refusal) = state.refusal.clone() {
            state.verdicts.push(Verdict::Refused(refusal.error.clone()));
            return refused(refusal.status, &refusal.error, &refusal.description);
        }
        let jkt = match self.proven(state, proof) {
            Ok(jkt) => jkt,
            Err(answer) => return *answer,
        };
        if let Err(reason) = self.granted(state, form) {
            state.verdicts.push(Verdict::Refused(reason.clone()));
            return refused(400, "invalid_request", &reason);
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        state.accepted.insert(token.clone());
        state.bound.insert(token.clone(), jkt);
        state.verdicts.push(Verdict::Issued);
        ResponseTemplate::new(200).set_body_json(TokenBody {
            access_token: &token,
            token_type: if state.bearer { "Bearer" } else { "DPoP" },
            expires_in: state.expires_in,
            scope: &self.0.scope,
        })
    }

    /// The thumbprint of the proof's key, or the answer to a request whose
    /// proof is refused.
    fn proven(
        &self,
        state: &mut State,
        proof: Option<&str>,
    ) -> Result<String, Box<ResponseTemplate>> {
        let Some(proof) = proof else {
            state
                .verdicts
                .push(Verdict::Refused("no DPoP proof".to_owned()));
            return Err(Box::new(refused(
                400,
                "invalid_dpop_proof",
                "no DPoP proof",
            )));
        };
        let verified = match dpop::verify(proof, "POST", &self.0.token_url, None) {
            Ok(verified) => verified,
            Err(reason) => {
                state.verdicts.push(Verdict::Refused(reason.clone()));
                return Err(Box::new(refused(400, "invalid_dpop_proof", &reason)));
            }
        };
        if let Some(nonce) = state.nonce.clone()
            && verified.proof.nonce.as_ref() != Some(&nonce)
        {
            state
                .verdicts
                .push(Verdict::Refused("use_dpop_nonce".to_owned()));
            return Err(Box::new(
                refused(400, "use_dpop_nonce", "a nonce is required")
                    .insert_header(dpop::NONCE_HEADER, nonce.as_str()),
            ));
        }
        if !state.seen.insert(format!("dpop:{}", verified.proof.jti)) {
            state.verdicts.push(Verdict::Refused(
                "the proof's jti was used before".to_owned(),
            ));
            return Err(Box::new(refused(400, "invalid_dpop_proof", "jti")));
        }
        Ok(verified.jkt)
    }

    /// Checks the grant `form` carries, or says why it is refused.
    fn granted(&self, state: &mut State, form: &[(String, String)]) -> Result<(), String> {
        let field = |name: &str| {
            form.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        if field("grant_type").as_deref() != Some(GRANT_TYPE) {
            return Err("grant_type".to_owned());
        }
        if field("scope").as_deref() != Some(self.0.scope.as_str()) {
            return Err("scope".to_owned());
        }
        if let Some(expected) = &state.client_id
            && field("client_id").as_deref() != Some(expected.as_str())
        {
            return Err("client_id".to_owned());
        }
        let definition: Definition = state
            .definition
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .ok_or("no definition")?;
        let submission: Submission = field("presentation_submission")
            .and_then(|text| serde_json::from_str(&text).ok())
            .ok_or("presentation_submission")?;
        if submission.definition_id != definition.id {
            return Err("the submission answers another definition".to_owned());
        }
        let assertion = field("assertion").ok_or("no assertion")?;
        let holder = state.holder.clone().ok_or("no trusted holder")?;
        let claims = presentation(&assertion, &holder, &self.0.issuer)?;
        let nonce = claims
            .nonce
            .clone()
            .ok_or("the presentation has no nonce")?;
        if !state.seen.insert(format!("vp:{nonce}")) {
            return Err("the presentation nonce was used before".to_owned());
        }
        for descriptor in &definition.input_descriptors {
            if !submission
                .descriptor_map
                .iter()
                .any(|m| m.id == descriptor.id)
            {
                return Err(format!("descriptor {} is not answered", descriptor.id));
            }
        }
        let issuer = state.issuer.clone().ok_or("no trusted credential issuer")?;
        for mapping in &submission.descriptor_map {
            if mapping.format != "jwt_vc" {
                return Err(format!("format {}", mapping.format));
            }
            let index: usize = mapping
                .path
                .strip_prefix("$.verifiableCredential[")
                .and_then(|rest| rest.strip_suffix(']'))
                .and_then(|index| index.parse().ok())
                .ok_or_else(|| format!("path {}", mapping.path))?;
            let held = claims
                .vp
                .credentials
                .get(index)
                .ok_or_else(|| format!("no credential at {}", mapping.path))?;
            credential_of(held, &issuer, &holder.did)?;
        }
        Ok(())
    }
}

/// The verified claims of `assertion`, a presentation `holder` signed for
/// `audience` (Nuts RFC021 §4.2).
fn presentation(
    assertion: &str,
    holder: &Trusted,
    audience: &str,
) -> Result<PresentationClaims, String> {
    let header = jsonwebtoken::decode_header(assertion).map_err(|e| format!("header: {e}"))?;
    if header.kid.as_deref() != Some(holder.kid.as_str()) {
        return Err("the presentation names another key".to_owned());
    }
    let key = DecodingKey::from_jwk(&holder.jwk).map_err(|e| format!("jwk: {e}"))?;
    let mut validation = Validation::new(header.alg);
    validation.validate_exp = false;
    validation.validate_aud = false;
    validation.set_required_spec_claims::<&str>(&[]);
    let claims = jsonwebtoken::decode::<PresentationClaims>(assertion, &key, &validation)
        .map_err(|e| format!("verification: {e}"))?
        .claims;
    if claims.iss != holder.did || claims.sub != holder.did {
        return Err("iss and sub are not the holder".to_owned());
    }
    if claims.aud != audience {
        return Err("aud is not the issuer".to_owned());
    }
    let now = jiff::Timestamp::now().as_second();
    if claims.exp.saturating_sub(claims.nbf) > PRESENTATION_LIFETIME_S
        || claims.nbf > now.saturating_add(SKEW_S)
        || claims.exp < now.saturating_sub(SKEW_S)
    {
        return Err("the presentation is not valid now".to_owned());
    }
    if !claims
        .vp
        .types
        .iter()
        .any(|kind| kind == "VerifiablePresentation")
    {
        return Err("not a VerifiablePresentation".to_owned());
    }
    Ok(claims)
}

/// Verifies `held` as a credential `issuer` signed for `holder`.
fn credential_of(held: &str, issuer: &Trusted, holder: &str) -> Result<(), String> {
    let header =
        jsonwebtoken::decode_header(held).map_err(|e| format!("credential header: {e}"))?;
    if header.kid.as_deref() != Some(issuer.kid.as_str()) {
        return Err("the credential names another issuer key".to_owned());
    }
    let key = DecodingKey::from_jwk(&issuer.jwk).map_err(|e| format!("jwk: {e}"))?;
    let mut validation = Validation::new(header.alg);
    validation.validate_aud = false;
    validation.set_issuer(&[issuer.did.as_str()]);
    validation.set_required_spec_claims(&["exp", "iss", "sub"]);
    let claims = jsonwebtoken::decode::<HeldClaims>(held, &key, &validation)
        .map_err(|e| format!("credential: {e}"))?
        .claims;
    if claims.sub != holder {
        return Err("the credential is not the holder's".to_owned());
    }
    Ok(())
}

/// The wiremock matcher of [`NutsNode::dpop_bound`].
#[derive(Debug)]
pub struct DpopBound {
    shared: Arc<Shared>,
}

impl Match for DpopBound {
    fn matches(&self, request: &Request) -> bool {
        let Some(token) = request
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("DPoP "))
        else {
            return false;
        };
        let Some(proof) = request
            .headers
            .get(dpop::HEADER)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        // The mock server reads every request over plain HTTP and names its
        // URL by `localhost`, so the URL the client sent is its Host and path.
        let Some(host) = request
            .headers
            .get(http::header::HOST)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let htu = format!("http://{host}{}", request.url.path());
        let Ok(verified) = dpop::verify(proof, request.method.as_str(), &htu, Some(token)) else {
            return false;
        };
        let state = self.shared.lock();
        state.accepted.contains(token) && state.bound.get(token) == Some(&verified.jkt)
    }
}

fn refused(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(Refused {
        error,
        error_description: description,
    })
}

/// The base64url claims segment of the compact JWS `jwt`, decoded, for a
/// test that reads what a client sent.
///
/// # Errors
///
/// Returns [`KeyError`] when `jwt` is no compact JWS.
pub fn payload(jwt: &str) -> Result<Vec<u8>, KeyError> {
    let segment = jwt
        .split('.')
        .nth(1)
        .ok_or_else(|| KeyError("not a compact JWS".to_owned()))?;
    URL_SAFE_NO_PAD
        .decode(segment)
        .map_err(|e| KeyError(e.to_string()))
}
