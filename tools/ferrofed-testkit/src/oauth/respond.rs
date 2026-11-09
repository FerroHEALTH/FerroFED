// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How the harness token endpoint answers one request: the RFC 6749 §4.4.2
//! client-credentials grant and RFC 8693 token exchange, each authenticated
//! by an RFC 7523 §2.2 client assertion, by the client secret (RFC 6749
//! §2.3.1) or, behind a mutual-TLS front, by the client's certificate (RFC
//! 8705 §2), and a `DPoP` proof where one is required (RFC 9449 §5).

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use wiremock::ResponseTemplate;

use crate::issuance::{Grant, Ledger, TokenRequest};
use crate::oauth::{
    ACCESS_TOKEN_TYPE, ClientAuth, Exchanged, JWT_BEARER, JWT_TOKEN_TYPE, Shared, State,
    TOKEN_EXCHANGE, verify_signed, verify_subject,
};

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

/// How one request failed: the RFC 6749 §5.2 code and the reason.
type Failure = (&'static str, String);

impl Grant for Shared {
    fn token_url(&self) -> &str {
        &self.token_url
    }

    fn requires_proof(&self) -> bool {
        self.lock().dpop
    }

    fn answer(
        &self,
        ledger: &mut Ledger,
        request: &TokenRequest,
        jkt: Option<String>,
    ) -> ResponseTemplate {
        let mut state = self.lock();
        let outcome = self
            .granted(&mut state, ledger, request)
            .and_then(|subject| Ok((subject, Self::detailed(&state, request)?)));
        match outcome {
            Ok((subject, details)) => {
                let onward =
                    subject
                        .as_deref()
                        .zip(state.onward_issuer.as_deref())
                        .map(|(sub, issuer)| OnwardClaims {
                            iss: issuer,
                            aud: request.field("resource").unwrap_or_default(),
                            sub,
                            client_id: &self.client_id,
                            lifetime: ledger.expires_in.unwrap_or(300),
                        });
                let token = match onward {
                    Some(claims) => onward_token(&claims),
                    None => issued_token(state.certificate.as_deref()),
                };
                ledger.issue(&token, jkt);
                if let Some(subject) = &subject {
                    state.subjects.insert(token.clone(), subject.clone());
                }
                let body = TokenBody {
                    access_token: &token,
                    token_type: if state.dpop { "DPoP" } else { "Bearer" },
                    expires_in: ledger.expires_in,
                    issued_token_type: (subject.is_some() && !state.untyped)
                        .then_some(ACCESS_TOKEN_TYPE),
                    authorization_details: details.filter(|_| !state.details_omitted),
                };
                ResponseTemplate::new(200).set_body_json(body)
            }
            Err((error, reason)) => ledger.refuse(400, error, &reason),
        }
    }
}

impl Shared {
    /// The caller a request is granted a token for: `None` under the
    /// client-credentials grant, the subject's `sub` under token exchange.
    fn granted(
        &self,
        state: &mut State,
        ledger: &mut Ledger,
        request: &TokenRequest,
    ) -> Result<Option<String>, Failure> {
        let field = |name: &str| request.field(name);
        let basic = request.basic.as_deref();
        let exchange = match field("grant_type").as_deref() {
            Some("client_credentials") => false,
            Some(TOKEN_EXCHANGE) if state.callers.is_some() => true,
            _ => return Err(("unsupported_grant_type", "grant_type".to_owned())),
        };
        if state.client_auth == ClientAuth::Secret {
            let method = self.secret(state, request)?;
            state.secret_methods.push(method);
        } else if field("client_secret").is_some() || basic.is_some() {
            return Err(("invalid_request", "a client secret was sent".to_owned()));
        }
        if state.client_auth == ClientAuth::Tls {
            if field("client_id").as_deref() != Some(self.client_id.as_str()) {
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
            self.assertion(state, ledger, &assertion, "invalid_client")?;
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
        self.assertion(state, ledger, &actor, "invalid_request")?;
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
    fn secret(&self, state: &State, request: &TokenRequest) -> Result<&'static str, Failure> {
        let field = |name: &str| request.field(name);
        if field("client_assertion").is_some() || field("client_assertion_type").is_some() {
            return Err((
                "invalid_request",
                "an assertion was sent beside the secret".to_owned(),
            ));
        }
        let expected = state.secret.as_deref().unwrap_or_default();
        let (method, client_id, secret) = match (request.basic.as_deref(), field("client_secret")) {
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
        if client_id != self.client_id || secret != expected {
            return Err(("invalid_client", "the client id or secret".to_owned()));
        }
        Ok(method)
    }

    /// The `authorization_details` a request asks for, read and held to the
    /// types the endpoint supports when a test set them, to be stated back
    /// in the token response (RFC 9396 §5, §6, §7).
    fn detailed(state: &State, request: &TokenRequest) -> Result<Option<Box<RawValue>>, Failure> {
        let Some(supported) = &state.details else {
            return Ok(None);
        };
        let refused = |reason: &str| ("invalid_authorization_details", reason.to_owned());
        let text = request
            .field("authorization_details")
            .ok_or_else(|| refused("no authorization_details"))?;
        let details = serde_json::from_str::<Vec<Typed>>(&text)
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
        serde_json::from_str::<Box<RawValue>>(&text)
            .map(Some)
            .map_err(|_shape| refused("not JSON"))
    }

    /// Verifies `assertion` as the gateway's, refusing with `error`, and
    /// spends its `jti`.
    fn assertion(
        &self,
        state: &mut State,
        ledger: &mut Ledger,
        assertion: &str,
        error: &'static str,
    ) -> Result<(), Failure> {
        let audience = state.audience.as_deref().unwrap_or(&self.token_url);
        let claims = verify_signed(
            assertion,
            &state.jwks,
            (&self.client_id, audience),
            state.algorithm,
        )
        .map_err(|reason| (error, reason))?;
        if !ledger.spent.insert(format!("assertion:{}", claims.jti)) {
            return Err((error, "the jti was used before".to_owned()));
        }
        Ok(())
    }
}

/// What an exchanged JWT access token states (RFC 9068 §2.2, RFC 8693 §4.1).
struct OnwardClaims<'a> {
    iss: &'a str,
    aud: String,
    sub: &'a str,
    client_id: &'a str,
    lifetime: u64,
}

/// A fresh exchanged access token: an unsigned JWT stating `claims`, the
/// client named as the actor, issued now.
fn onward_token(claims: &OnwardClaims<'_>) -> String {
    let iat = jiff::Timestamp::now().as_second();
    let exp = iat.saturating_add(i64::try_from(claims.lifetime).unwrap_or(i64::MAX));
    let payload = serde_json::json!({
        "iss": claims.iss,
        "aud": claims.aud,
        "sub": claims.sub,
        "client_id": claims.client_id,
        "act": {"sub": claims.client_id},
        "iat": iat,
        "exp": exp,
        "jti": uuid::Uuid::new_v4().simple().to_string(),
    });
    format!(
        "{}.{}.c2ln",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"at+jwt"}"#),
        URL_SAFE_NO_PAD.encode(payload.to_string())
    )
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
