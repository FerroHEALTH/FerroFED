// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the harness token endpoint answers one request: the RFC 6749 §4.4.2
//! client-credentials grant and RFC 8693 token exchange, each authenticated
//! by an RFC 7523 §2.2 client assertion, by the client secret (RFC 6749
//! §2.3.1) or, behind a mutual-TLS front, by the client's certificate (RFC
//! 8705 §2), and a `DPoP` proof where one is required (RFC 9449 §5).

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use wiremock::{Request, Respond, ResponseTemplate};

use crate::dpop;
use crate::oauth::{
    ACCESS_TOKEN_TYPE, ClientAuth, Exchanged, JWT_BEARER, JWT_TOKEN_TYPE, Shared, State,
    TOKEN_EXCHANGE, Verdict, verify_signed, verify_subject,
};

/// The responder the token endpoint's mock answers every request with.
pub(super) struct Responder(pub(super) Arc<Shared>);

#[derive(Serialize)]
struct TokenBody<'a> {
    access_token: &'a str,
    token_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    issued_token_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    authorization_details: Option<Box<RawValue>>,
}

/// One authorization details object, read for its `type` (RFC 9396 §2).
#[derive(Deserialize)]
struct Typed {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
struct Refused<'a> {
    error: &'a str,
    error_description: &'a str,
}

impl Respond for Responder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let form: Vec<(String, String)> = url::form_urlencoded::parse(&request.body)
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        let proof = request
            .headers
            .get(dpop::HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let basic = request
            .headers
            .get(http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Basic "))
            .map(str::to_owned);
        let mut state = self.0.lock();
        state.forms.push(form.clone());
        let delay = state.delay;
        let answer = self.answer(&mut state, &form, (proof.as_deref(), basic.as_deref()));
        match delay {
            Some(delay) => answer.set_delay(delay),
            None => answer,
        }
    }
}

/// How one request failed: the RFC 6749 §5.2 code and the reason.
type Failure = (&'static str, String);

impl Responder {
    /// The answer to a request carrying `form`, `proof` and the `basic`
    /// credentials of its `Authorization` header, recorded in `state`.
    fn answer(
        &self,
        state: &mut State,
        form: &[(String, String)],
        (proof, basic): (Option<&str>, Option<&str>),
    ) -> ResponseTemplate {
        if let Some(refusal) = state.refusal.clone() {
            state.verdicts.push(Verdict::Refused(refusal.error.clone()));
            return refused(refusal.status, &refusal.error, &refusal.description);
        }
        let jkt = match self.proven(state, proof) {
            Ok(jkt) => jkt,
            Err(answer) => return *answer,
        };
        let outcome = self.granted(state, form, basic).and_then(|subject| {
            let details = Self::detailed(state, form)?;
            Ok((subject, details))
        });
        match outcome {
            Ok((subject, details)) => {
                let token = issued_token(state.certificate.as_deref());
                state.accepted.insert(token.clone());
                if let Some(subject) = &subject {
                    state.subjects.insert(token.clone(), subject.clone());
                }
                if let Some(jkt) = jkt {
                    state.bound.insert(token.clone(), jkt);
                }
                state.verdicts.push(Verdict::Issued);
                let body = TokenBody {
                    access_token: &token,
                    token_type: if state.dpop { "DPoP" } else { "Bearer" },
                    expires_in: state.expires_in,
                    issued_token_type: (subject.is_some() && !state.untyped)
                        .then_some(ACCESS_TOKEN_TYPE),
                    authorization_details: details.filter(|_| !state.details_omitted),
                };
                ResponseTemplate::new(200).set_body_json(body)
            }
            Err((error, reason)) => {
                state.verdicts.push(Verdict::Refused(reason.clone()));
                refused(400, error, &reason)
            }
        }
    }

    /// The thumbprint of a required proof's key, `None` when no proof is
    /// required, or the answer to a request whose proof is refused.
    fn proven(
        &self,
        state: &mut State,
        proof: Option<&str>,
    ) -> Result<Option<String>, Box<ResponseTemplate>> {
        if !state.dpop {
            return Ok(None);
        }
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
        if !state.jti.insert(verified.proof.jti.clone()) {
            state.verdicts.push(Verdict::Refused(
                "the proof's jti was used before".to_owned(),
            ));
            return Err(Box::new(refused(400, "invalid_dpop_proof", "jti")));
        }
        Ok(Some(verified.jkt))
    }

    /// The caller a request is granted a token for: `None` under the
    /// client-credentials grant, the subject's `sub` under token exchange.
    fn granted(
        &self,
        state: &mut State,
        form: &[(String, String)],
        basic: Option<&str>,
    ) -> Result<Option<String>, Failure> {
        let field = |name: &str| {
            form.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        let exchange = match field("grant_type").as_deref() {
            Some("client_credentials") => false,
            Some(TOKEN_EXCHANGE) if state.callers.is_some() => true,
            _ => return Err(("unsupported_grant_type", "grant_type".to_owned())),
        };
        if state.client_auth == ClientAuth::Secret {
            let method = self.secret(state, form, basic)?;
            state.secret_methods.push(method);
        } else if field("client_secret").is_some() || basic.is_some() {
            return Err(("invalid_request", "a client secret was sent".to_owned()));
        }
        if state.client_auth == ClientAuth::Tls {
            if field("client_id").as_deref() != Some(self.0.client_id.as_str()) {
                return Err(("invalid_client", "client_id".to_owned()));
            }
            if field("client_assertion").is_some() || field("client_assertion_type").is_some() {
                return Err((
                    "invalid_request",
                    "an assertion was sent beside the certificate".to_owned(),
                ));
            }
        } else if state.client_auth == ClientAuth::Assertion
            && field("client_assertion_type").as_deref() != Some(JWT_BEARER)
        {
            return Err(("invalid_client", "client_assertion_type".to_owned()));
        }
        if let Some(expected) = &state.scope
            && field("scope").as_deref() != Some(expected.as_str())
        {
            return Err(("invalid_scope", "scope".to_owned()));
        }
        if state.client_auth == ClientAuth::Assertion {
            let assertion = field("client_assertion")
                .ok_or(("invalid_client", "no client_assertion".to_owned()))?;
            state.assertions.push(assertion.clone());
            self.assertion(state, &assertion, "invalid_client")?;
        }
        if !exchange {
            return Ok(None);
        }
        if field("subject_token_type").as_deref() != Some(ACCESS_TOKEN_TYPE) {
            return Err(("invalid_request", "subject_token_type".to_owned()));
        }
        if field("actor_token_type").as_deref() != Some(JWT_TOKEN_TYPE) {
            return Err(("invalid_request", "actor_token_type".to_owned()));
        }
        if let Some(expected) = &state.resource
            && field("resource").as_deref() != Some(expected.as_str())
        {
            return Err(("invalid_target", "resource".to_owned()));
        }
        let actor = field("actor_token").ok_or(("invalid_request", "no actor_token".to_owned()))?;
        self.assertion(state, &actor, "invalid_request")?;
        let subject_token =
            field("subject_token").ok_or(("invalid_request", "no subject_token".to_owned()))?;
        let callers = state
            .callers
            .clone()
            .ok_or(("unsupported_grant_type", "grant_type".to_owned()))?;
        let subject = verify_subject(&subject_token, &callers.jwks, &callers.issuer)
            .map_err(|reason| ("invalid_grant", reason))?;
        state.exchanges.push(Exchanged {
            subject: subject.clone(),
            scope: field("scope"),
            resource: field("resource"),
        });
        Ok(Some(subject))
    }

    /// The method a request authenticated by the client secret used, the
    /// `basic` credentials of its `Authorization` header or the
    /// `client_id` and `client_secret` of its body, never both and never
    /// beside an assertion (RFC 6749 §2.3.1).
    fn secret(
        &self,
        state: &State,
        form: &[(String, String)],
        basic: Option<&str>,
    ) -> Result<&'static str, Failure> {
        let field = |name: &str| {
            form.iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        };
        if field("client_assertion").is_some() || field("client_assertion_type").is_some() {
            return Err((
                "invalid_request",
                "an assertion was sent beside the secret".to_owned(),
            ));
        }
        let expected = state.secret.as_deref().unwrap_or_default();
        let (method, client_id, secret) = match (basic, field("client_secret")) {
            (Some(_), Some(_)) => {
                return Err((
                    "invalid_request",
                    "the secret was sent twice, in the header and the body".to_owned(),
                ));
            }
            (Some(basic), None) => {
                let decoded = STANDARD
                    .decode(basic)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .ok_or(("invalid_client", "the Basic credentials".to_owned()))?;
                let (user, password) = decoded
                    .split_once(':')
                    .ok_or(("invalid_client", "the Basic credentials".to_owned()))?;
                let decode = |text: &str| -> String {
                    url::form_urlencoded::parse(format!("v={text}").as_bytes())
                        .map(|(_, value)| value.into_owned())
                        .next()
                        .unwrap_or_default()
                };
                ("client_secret_basic", decode(user), decode(password))
            }
            (None, Some(secret)) => (
                "client_secret_post",
                field("client_id").unwrap_or_default(),
                secret,
            ),
            (None, None) => return Err(("invalid_client", "no client secret".to_owned())),
        };
        if client_id != self.0.client_id || secret != expected {
            return Err(("invalid_client", "the client id or secret".to_owned()));
        }
        Ok(method)
    }

    /// The `authorization_details` a request asks for, read and held to the
    /// types the endpoint supports when a test set them, to be stated back
    /// in the token response (RFC 9396 §5, §6, §7).
    fn detailed(
        state: &State,
        form: &[(String, String)],
    ) -> Result<Option<Box<RawValue>>, Failure> {
        let Some(supported) = &state.details else {
            return Ok(None);
        };
        let refused = |reason: &str| ("invalid_authorization_details", reason.to_owned());
        let text = form
            .iter()
            .find(|(key, _)| key == "authorization_details")
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| refused("no authorization_details"))?;
        let details = serde_json::from_str::<Vec<Typed>>(text)
            .map_err(|_shape| refused("not an array of typed objects"))?;
        if details.is_empty() {
            return Err(refused("no authorization details object"));
        }
        if let Some(unknown) = details
            .iter()
            .find(|detail| !supported.contains(&detail.kind))
        {
            return Err(refused(&format!("unknown type {}", unknown.kind)));
        }
        serde_json::from_str::<Box<RawValue>>(text)
            .map(Some)
            .map_err(|_shape| refused("not JSON"))
    }

    /// Verifies `assertion` as the gateway's, refusing with `error`, and
    /// spends its `jti`.
    fn assertion(
        &self,
        state: &mut State,
        assertion: &str,
        error: &'static str,
    ) -> Result<(), Failure> {
        let audience = state.audience.as_deref().unwrap_or(&self.0.token_url);
        let claims = verify_signed(
            assertion,
            &state.jwks,
            (&self.0.client_id, audience),
            state.algorithm,
        )
        .map_err(|reason| (error, reason))?;
        if !state.jti.insert(claims.jti) {
            return Err((error, "the jti was used before".to_owned()));
        }
        Ok(())
    }
}

/// A fresh access token: a random one, or, bound to the certificate of
/// `thumbprint`, a JWT whose `cnf` claim names it (RFC 8705 §3.1). The
/// harness signs nothing; the client reads the claim and the node matches
/// the token whole.
fn issued_token(thumbprint: Option<&str>) -> String {
    let jti = uuid::Uuid::new_v4().simple().to_string();
    let Some(thumbprint) = thumbprint else {
        return jti;
    };
    // The jti is hex and a thumbprint base64url, so neither needs escaping.
    let claims = format!(r#"{{"jti":"{jti}","cnf":{{"x5t#S256":"{thumbprint}"}}}}"#);
    format!(
        "{}.{}.{}",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"at+jwt"}"#),
        URL_SAFE_NO_PAD.encode(claims),
        URL_SAFE_NO_PAD.encode(&jti)
    )
}

fn refused(status: u16, error: &str, description: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(Refused {
        error,
        error_description: description,
    })
}
