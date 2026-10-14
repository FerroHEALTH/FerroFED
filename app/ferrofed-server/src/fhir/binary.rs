// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {fhir-base}/Binary/{token}`: a member's own document, named in the
//! document list, read through the gateway.
//!
//! The list names each member's document by `Binary/{token}`, relative to
//! the FHIR base, as the HL7 Europe EU Health Data API
//! `EehrxfMhdDocumentReference` requires of `attachment.url`. The token is a
//! compact JWS the gateway signs with its own `[signing]` key, of the JOSE
//! type [`TYPE`] (RFC 8725 §3.11), so it is taken for no other token the
//! gateway reads. Its `aud` and `iss` are the gateway's FHIR base, its
//! lifetime `fhir.document_url_lifetime_secs`, and it carries the member's
//! endpoints, its own `ehr_id`, the category and profile the document was
//! asked in, and the verified caller it was listed for; never a patient
//! identifier (§5.4.1, N33). A read verifies the token, holds the caller to
//! the one it names, and asks the member again by its `ehr_id` alone, as the
//! list did; the bytes are answered as the member sent them and nothing is
//! stored. No specification governs the token: our own design.

use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject};
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_engine::conveyance::signer::Signer;
use ferrofed_engine::fanout::Completion;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::native::Answer;
use ferrofed_registry::id::{EhrId, EndpointId};
use http::{HeaderValue, StatusCode, header};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use super::{attach, outcome};
use crate::access::{Accessed, NoAccess};
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::conveyed;
use crate::error::Code;
use crate::facade::native::{self, Chosen, Native};
use crate::facade::request::Arrived;
use crate::federation::Federation;
use crate::request_id;
use crate::state::AppState;

/// The path prefix of a member's document under `{fhir-base}`.
pub(super) const BINARY: &str = "/Binary/";

/// The JOSE `typ` of the token (RFC 8725 §3.11).
pub(super) const TYPE: &str = "ferrofed-document-url+jwt";

/// The operation an access record names for the read.
const OPERATION: &str = "Binary";

/// The category the list names documents of.
const CATEGORY: &str = "Patient-Summaries";

/// The claims of a token.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// The gateway's FHIR base.
    iss: String,
    /// The gateway's FHIR base.
    aud: String,
    /// When the token was signed, in seconds.
    iat: i64,
    /// When it expires, in seconds.
    exp: i64,
    /// A fresh id.
    jti: String,
    /// The subject of the caller it was listed for.
    sub: String,
    /// The issuer of that caller.
    caller_iss: String,
    /// The member's own exchange-format endpoint.
    endpoint: String,
    /// The member's Query API endpoint, whose client asks it.
    via: String,
    /// The member's own `ehr_id`.
    ehr_id: String,
    /// The category of the document.
    category: String,
    /// The profile it was asked in.
    profile: String,
}

/// Why a token is not taken; every reason answers alike.
#[derive(Debug, thiserror::Error)]
enum Untaken {
    /// The token is no JWS of [`TYPE`] signed by a key of the gateway's.
    #[error("the token is no document URL the gateway signed")]
    Unverified(#[source] Option<jsonwebtoken::errors::Error>),
    /// The token names another caller.
    #[error("the token names another caller")]
    OtherCaller,
    /// The token names an endpoint or an `ehr_id` that is no longer held.
    #[error("the token names no member the registry holds")]
    Unheld,
}

/// Returns the token naming `chosen`'s document for `caller`, signed by
/// `signer` for the face `fhir`.
///
/// # Errors
///
/// The [`jsonwebtoken::errors::Error`] of a key that cannot sign.
pub(super) fn mint(
    signer: &Signer,
    fhir: &FhirSettings,
    caller: &Caller,
    chosen: &Chosen,
) -> Result<String, jsonwebtoken::errors::Error> {
    let iat = jiff::Timestamp::now().as_second();
    let lifetime = i64::try_from(fhir.document_url_lifetime.as_secs()).unwrap_or(i64::MAX);
    let claims = Claims {
        iss: fhir.absolute.clone(),
        aud: fhir.absolute.clone(),
        iat,
        exp: iat.saturating_add(lifetime),
        jti: uuid::Uuid::new_v4().to_string(),
        sub: caller.subject().to_owned(),
        caller_iss: caller.issuer().to_owned(),
        endpoint: chosen.native.as_str().to_owned(),
        via: chosen.via.as_str().to_owned(),
        ehr_id: chosen.ehr_id.as_str().to_owned(),
        category: CATEGORY.to_owned(),
        profile: EPS_COMPOSITION.to_owned(),
    };
    signer.sign(TYPE, &claims)
}

/// The claims of `token` when the gateway signed it for the face `fhir`
/// with a key `signer` holds, of [`TYPE`], unexpired.
fn verified(signer: &Signer, fhir: &FhirSettings, token: &str) -> Result<Claims, Untaken> {
    let header = jsonwebtoken::decode_header(token).map_err(|e| Untaken::Unverified(Some(e)))?;
    if header.typ.as_deref() != Some(TYPE) {
        return Err(Untaken::Unverified(None));
    }
    let keys = signer.keys();
    let key = [Some(keys.current()), keys.previous(), keys.next()]
        .into_iter()
        .flatten()
        .find(|key| header.kid.as_deref() == Some(key.kid()) && header.alg == key.algorithm())
        .ok_or(Untaken::Unverified(None))?;
    let decoding = DecodingKey::from_jwk(key.public()).map_err(|e| Untaken::Unverified(Some(e)))?;
    let algorithm: Algorithm = key.algorithm();
    let mut validation = Validation::new(algorithm);
    validation.leeway = 0;
    validation.set_audience(&[fhir.absolute.as_str()]);
    validation.set_issuer(&[fhir.absolute.as_str()]);
    validation.set_required_spec_claims(&["exp", "aud", "iss", "sub"]);
    jsonwebtoken::decode::<Claims>(token, &decoding, &validation)
        .map(|data| data.claims)
        .map_err(|e| Untaken::Unverified(Some(e)))
}

/// The member a verified token `claims` names, as the registry holds it.
fn chosen(federation: &Federation, claims: &Claims) -> Result<Chosen, Untaken> {
    let snapshot = federation.snapshot();
    let native = EndpointId::new(claims.endpoint.as_str()).map_err(|_bad| Untaken::Unheld)?;
    let via = EndpointId::new(claims.via.as_str()).map_err(|_bad| Untaken::Unheld)?;
    let ehr_id = EhrId::new(claims.ehr_id.as_str()).map_err(|_bad| Untaken::Unheld)?;
    let node = snapshot
        .native_endpoint(&native)
        .filter(|held| held.serves(CATEGORY))
        .map(|held| held.node().clone())
        .ok_or(Untaken::Unheld)?;
    let same = snapshot
        .endpoint(&via)
        .is_some_and(|held| held.node() == &node);
    if !same || claims.category != CATEGORY || claims.profile != EPS_COMPOSITION {
        return Err(Untaken::Unheld);
    }
    Ok(Chosen {
        via,
        node,
        native,
        ehr_id,
    })
}

/// The answer to a token that is not taken: a `404` naming nothing, so a
/// token of another caller, an expired one and a forged one answer alike.
fn untaken(untaken: &Untaken, logged: &str) -> Response {
    tracing::warn!(
        error = %crate::chain(untaken),
        request_id = logged,
        "a document URL was not taken"
    );
    let mut response = outcome(
        StatusCode::NOT_FOUND,
        "not-found",
        "no document at this URL; list the documents again",
    );
    response.extensions_mut().insert(NoAccess);
    response
}

/// `GET {fhir-base}/Binary/{token}`.
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    Path(token): Path<String>,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    let outbound = outbound.map_or_else(OutboundId::mint, |Extension(id)| id);
    let request_id = request_id::of(&headers).unwrap_or_default();
    let logged = outbound.to_string();
    let (Some(federation), Some(fhir)) = (state.federation(), state.fhir()) else {
        return crate::error::fixed(Code::NotImplemented, request_id);
    };
    let caller = caller.as_deref();
    // NOTE: §13.1, N25: a document is read only for a caller the gate admitted with the
    // scopes that cover it, whatever path reached this handler.
    let Some(admitted) = caller.filter(|caller| !caller.covering().is_empty()) else {
        return outcome(
            StatusCode::FORBIDDEN,
            "forbidden",
            "the caller was not admitted to the section queries",
        );
    };
    let Some(signer) = federation.signer() else {
        return conveyed::Unconveyed::Unsigned.respond(request_id, &logged);
    };
    let claims = match verified(signer, fhir, &token) {
        Ok(claims) => claims,
        Err(refused) => return untaken(&refused, &logged),
    };
    if claims.sub != admitted.subject() || claims.caller_iss != admitted.issuer() {
        return untaken(&Untaken::OtherCaller, &logged);
    }
    let member = match chosen(&federation, &claims) {
        Ok(member) => member,
        Err(refused) => return untaken(&refused, &logged),
    };
    let conveyance = match conveyed::of(&federation, caller) {
        Ok(conveyance) => conveyance,
        Err(unconveyed) => return unconveyed.respond(request_id, &logged),
    };
    let on_behalf = admitted.on_behalf();
    let conveyance =
        match crate::facade::confined_by(&federation, caller, started, conveyance).await {
            Ok(conveyance) => conveyance,
            Err(unconfined) => return unconfined.respond(request_id, &logged),
        };
    let session = admitted.session();
    let arrived = Arrived {
        headers: &headers,
        request_id,
        outbound,
        conveyance: &conveyance,
        started,
        session: Some(&session),
        requester: admitted.requester(),
        on_behalf: &on_behalf,
    };
    // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond
    // what it reads of every request, and the member is located by its ehr_id alone.
    let asked = native::asked(
        &federation,
        &arrived,
        vec![member],
        Arc::new(Withheld::none()),
    )
    .await;
    let natives = match asked {
        Ok(natives) => natives,
        Err(failure) => return failure.respond(request_id, outbound),
    };
    let Some(native) = natives.first() else {
        return crate::error::fixed(Code::Internal, request_id);
    };
    let mut response = answered(native);
    attach(&mut response, accessed(&federation, native));
    response
}

/// The answer to a read that asked `native`: the member's bytes as it sent
/// them, or the outcome that says why there are none (§11.3, §11.4).
fn answered(native: &Native) -> Response {
    match &native.answer {
        Answer::Document(forwarded) => {
            let media = forwarded
                .headers()
                .get(header::CONTENT_TYPE)
                .cloned()
                .unwrap_or(HeaderValue::from_static("application/octet-stream"));
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, media)],
                Body::from(forwarded.body().to_vec()),
            )
                .into_response()
        }
        Answer::Absent => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "the member holds this document no longer; list the documents again",
        ),
        silent => {
            let status = native::settled(
                std::slice::from_ref(native),
                StatusCode::OK,
                Completion::AllOrNothing,
            );
            let code = if status == StatusCode::GATEWAY_TIMEOUT {
                "timeout"
            } else {
                "incomplete"
            };
            outcome(
                status,
                code,
                &format!(
                    "the document could not be read: no answer from {} ({}) (§11.3)",
                    native.chosen.native.as_str(),
                    silent.status().as_str()
                ),
            )
        }
    }
}

/// The access record's facts of a read that asked `native`, or `None` when
/// no request left the gateway or the federation keeps no access log
/// (Annex II 3.2); the record gate names the patient behind the `ehr_id`.
fn accessed(federation: &Federation, native: &Native) -> Option<Accessed> {
    let log = federation.access_log()?;
    if !native.answer.contact().left() {
        return None;
    }
    let natives = std::slice::from_ref(native);
    let delivered = matches!(native.answer, Answer::Document(_));
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Read,
        operation: OPERATION,
        resource: None,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: None,
            ehrs: native::ehrs(natives).collect(),
        },
        evidence: Evidence::reached(Basis::Returned, Vec::new())
            .constructed(Category::PatientSummary),
        delivered: Some(usize::from(delivered)),
        origins: native::origins(federation.snapshot(), natives, true).collect(),
        consent_set_aside: std::collections::BTreeSet::new(),
    })
}
