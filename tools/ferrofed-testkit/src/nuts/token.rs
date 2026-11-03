// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The token endpoint of the harness Nuts node: the `vp_token-bearer` grant
//! (Nuts RFC021 §3), its `DPoP` proof (RFC 9449), the presentation (RFC021
//! §4.2) and every credential the submission maps. No specification governs
//! the device: our own design.

use jsonwebtoken::{DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use wiremock::ResponseTemplate;

use crate::issuance::{Grant, Ledger, TokenRequest};
use crate::nuts::{GRANT_TYPE, PRESENTATION_LIFETIME_S, SKEW_S, Shared, State, Trusted};

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

impl Grant for Shared {
    fn token_url(&self) -> &str {
        &self.token_url
    }

    fn requires_proof(&self) -> bool {
        true
    }

    fn answer(
        &self,
        ledger: &mut Ledger,
        request: &TokenRequest,
        jkt: Option<String>,
    ) -> ResponseTemplate {
        if let Err(reason) = self.granted(&self.lock(), ledger, request) {
            return ledger.refuse(400, "invalid_request", &reason);
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        ledger.issue(&token, jkt);
        ResponseTemplate::new(200).set_body_json(TokenBody {
            access_token: &token,
            token_type: "DPoP",
            expires_in: ledger.expires_in,
            scope: &self.scope,
        })
    }
}

impl Shared {
    /// Checks the grant `request` carries, or says why it is refused.
    fn granted(
        &self,
        state: &State,
        ledger: &mut Ledger,
        request: &TokenRequest,
    ) -> Result<(), String> {
        let field = |name: &str| request.field(name);
        if field("grant_type").as_deref() != Some(GRANT_TYPE) {
            return Err("grant_type".to_owned());
        }
        if field("scope").as_deref() != Some(self.scope.as_str()) {
            return Err("scope".to_owned());
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
        let claims = presentation(&assertion, &holder, &self.issuer)?;
        let nonce = claims
            .nonce
            .clone()
            .ok_or("the presentation has no nonce")?;
        if !ledger.spent.insert(format!("vp:{nonce}")) {
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
