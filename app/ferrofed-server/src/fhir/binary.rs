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
//! stored. An answer that is no FHIR document `Bundle` whose identifier has
//! a system and a value (FHIR R4 `bdl-9`) is never served: the member is
//! named in `node-error` with `424` (§11.1, §11.3). A token of a document
//! of another category a member serves itself names its `Binary` by its
//! logical id, and the read asks that member alone for it, answering its
//! bytes as it sent them ([`super::own::read`]). No specification governs
//! the token: our own design.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject};
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_eehrxf::report::Written;
use ferrofed_engine::conveyance::signer::Signer;
use ferrofed_engine::fanout::Completion;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::forward::Forwarded;
use ferrofed_engine::single_node::native::Answer;
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrohealth_smart::client::assertion::AssertionError;
use http::{HeaderValue, StatusCode, header};
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

use super::admission::Asked;
use super::trc::Service;
use super::{UNADMITTED, admission, attach, outcome};
use crate::access::{Accessed, NoAccess};
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::conveyed;
use crate::error::Code;
use crate::facade::native::{self, Chosen, Native};
use crate::facade::summary::own::Kind;
use crate::federation::Federation;

/// The route of a member's document under `{fhir-base}`.
pub(super) const ROUTE: &str = "/Binary/{token}";

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
    /// The version of the member's composition the gateway writes the
    /// document from, for a report it writes; none for a document the
    /// member serves itself.
    composition: Option<String>,
    /// The logical id of the member's `Binary`, for a laboratory,
    /// discharge or imaging document the member serves itself; none
    /// otherwise.
    binary: Option<String>,
}

/// The document a token names, at the member it names.
#[derive(Debug)]
enum Named {
    /// The member's own patient summary.
    Summary,
    /// The report of the category the gateway writes from the composition
    /// of this version.
    Composition(Written, String),
    /// The member's own document of the kind, its `Binary` of this logical
    /// id.
    Own(Kind, String),
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
) -> Result<String, AssertionError> {
    signed(
        (signer, fhir, caller),
        (&chosen.native, chosen),
        (CATEGORY, EPS_COMPOSITION),
        (None, None),
    )
}

/// Returns the token naming, for `caller`, the report of the category
/// `written` the gateway writes from the composition of the version
/// `composition` at the member `chosen`, signed by `signer` for the face
/// `fhir`.
///
/// # Errors
///
/// The [`jsonwebtoken::errors::Error`] of a key that cannot sign.
pub(super) fn mint_written(
    signer: &Signer,
    fhir: &FhirSettings,
    caller: &Caller,
    (chosen, written, composition): (&Chosen, Written, &str),
) -> Result<String, AssertionError> {
    signed(
        (signer, fhir, caller),
        (&chosen.via, chosen),
        (written.category(), written.bundle()),
        (Some(composition), None),
    )
}

/// Returns the token naming, for `caller`, the document of the kind `kind`
/// the member `chosen` serves itself as its `Binary` of the logical id
/// `binary`, signed by `signer` for the face `fhir`.
///
/// # Errors
///
/// The [`jsonwebtoken::errors::Error`] of a key that cannot sign.
pub(super) fn mint_own(
    signer: &Signer,
    fhir: &FhirSettings,
    caller: &Caller,
    (chosen, kind, binary): (&Chosen, Kind, &str),
) -> Result<String, AssertionError> {
    signed(
        (signer, fhir, caller),
        (&chosen.native, chosen),
        (kind.code(), kind.profile()),
        (None, Some(binary)),
    )
}

/// Returns the token, signed by `signer` for the face `fhir` and `caller`,
/// naming the document at `endpoint` of the member `chosen`, of `category`
/// in `profile`, written from the composition `composition` when it is one
/// the gateway writes, or the member's `Binary` of the id `binary`.
fn signed(
    (signer, fhir, caller): (&Signer, &FhirSettings, &Caller),
    (endpoint, chosen): (&EndpointId, &Chosen),
    (category, profile): (&str, &str),
    (composition, binary): (Option<&str>, Option<&str>),
) -> Result<String, AssertionError> {
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
        endpoint: endpoint.as_str().to_owned(),
        via: chosen.via.as_str().to_owned(),
        ehr_id: chosen.ehr_id.as_str().to_owned(),
        category: category.to_owned(),
        profile: profile.to_owned(),
        composition: composition.map(str::to_owned),
        binary: binary.map(str::to_owned),
    };
    signer.keys().current().sign(TYPE, &claims)
}

/// The claims of `token` when the gateway signed it for the face `fhir`
/// with a key `signer` holds, of [`TYPE`], unexpired.
fn verified(signer: &Signer, fhir: &FhirSettings, token: &str) -> Result<Claims, Untaken> {
    let header = jsonwebtoken::decode_header(token).map_err(|e| Untaken::Unverified(Some(e)))?;
    if header.typ.as_deref() != Some(TYPE) {
        return Err(Untaken::Unverified(None));
    }
    let keys = signer.keys();
    let key = [
        Some(keys.current()),
        keys.previous_at(Instant::now()),
        keys.next(),
    ]
    .into_iter()
    .flatten()
    .find(|key| header.kid.as_deref() == Some(key.kid()) && header.alg == key.algorithm())
    .ok_or(Untaken::Unverified(None))?;
    let public: Jwk =
        serde_json::from_value(key.public_jwk()).map_err(|_unread| Untaken::Unverified(None))?;
    let decoding = DecodingKey::from_jwk(&public).map_err(|e| Untaken::Unverified(Some(e)))?;
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
    if claims.composition.is_some() {
        let mapped = Written::of_category(&claims.category)
            .is_some_and(|written| claims.profile == written.bundle());
        if !mapped || claims.binary.is_some() || native != via {
            return Err(Untaken::Unheld);
        }
        let node = snapshot
            .endpoint(&via)
            .map(|held| held.node().clone())
            .ok_or(Untaken::Unheld)?;
        return Ok(Chosen {
            via,
            node,
            native,
            ehr_id,
        });
    }
    let (category, profile) = if claims.binary.is_some() {
        let kind = Kind::of_category(&claims.category).ok_or(Untaken::Unheld)?;
        (kind.code(), kind.profile())
    } else {
        (CATEGORY, EPS_COMPOSITION)
    };
    let node = snapshot
        .native_endpoint(&native)
        .filter(|held| held.serves(category))
        .map(|held| held.node().clone())
        .ok_or(Untaken::Unheld)?;
    let same = snapshot
        .endpoint(&via)
        .is_some_and(|held| held.node() == &node);
    if !same || claims.category != category || claims.profile != profile {
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
pub(super) async fn read(asked: Asked, Path(token): Path<String>) -> Response {
    let started = Instant::now();
    admission::served(
        (&asked, started),
        || outcome(StatusCode::FORBIDDEN, "forbidden", UNADMITTED),
        |admitted| match taken(admitted, &token) {
            Ok(member) => ControlFlow::Continue(member),
            Err(refused) => ControlFlow::Break(refused),
        },
        async |admitted, arrived, (member, named)| {
            let federation = &admitted.federation;
            let (request_id, outbound) = (admitted.request_id, admitted.outbound);
            let service = match named {
                Named::Summary => Service::PatientSummary,
                Named::Composition(..) | Named::Own(..) => Service::OriginalDocument,
            };
            let mut response = async {
                // NOTE: eHDSI XCA Profile 8.0.1 §4.2.2: a contact point reads only documents of its
                // TRC patient, held before the member is asked.
                let pair = (&member.node, &member.ehr_id);
                if let Err(refused) = super::trc::located(admitted, &arrived, pair, service) {
                    return refused;
                }
                match named {
                    Named::Summary => {}
                    Named::Composition(written, composition) => {
                        return super::written::read(
                            admitted,
                            &arrived,
                            (&member, written, &composition),
                        )
                        .await;
                    }
                    Named::Own(kind, binary) => {
                        return super::own::read(admitted, &arrived, (&member, kind, &binary))
                            .await;
                    }
                }
                // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond
                // what it reads of every request, and the member is located by its ehr_id alone.
                let asked = native::asked(
                    federation,
                    &arrived,
                    vec![member],
                    Arc::new(Withheld::default()),
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
                attach(&mut response, accessed(federation, native));
                response
            }
            .await;
            // NOTE: Requirements Catalogue 10.0.0 §1.25: the document's own service names
            // the no-data answer a contact point is given ([`super::errors::outcomes`]).
            response.extensions_mut().insert(service);
            response
        },
    )
    .await
}

/// The member the URL `token` names, for the caller `admitted` names: the
/// gateway's own signature over it, the caller it was signed for, and a
/// member that still holds the document's EHR, or the answer that says why
/// it is not taken.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
fn taken(admitted: &admission::Admitted<'_>, token: &str) -> Result<(Chosen, Named), Response> {
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let (request_id, logged) = (admitted.request_id, &admitted.logged);
    let Some(signer) = federation.signer() else {
        return Err(conveyed::Unconveyed::Unsigned.respond(request_id, logged));
    };
    let claims = verified(signer, fhir, token).map_err(|refused| untaken(&refused, logged))?;
    if claims.sub != admitted.caller.subject() || claims.caller_iss != admitted.caller.issuer() {
        return Err(untaken(&Untaken::OtherCaller, logged));
    }
    let member = chosen(federation, &claims).map_err(|refused| untaken(&refused, logged))?;
    let named = match (claims.composition, claims.binary) {
        (Some(composition), _) => {
            let written = Written::of_category(&claims.category)
                .ok_or_else(|| untaken(&Untaken::Unheld, logged))?;
            Named::Composition(written, composition)
        }
        (None, Some(binary)) => {
            let kind = Kind::of_category(&claims.category)
                .ok_or_else(|| untaken(&Untaken::Unheld, logged))?;
            Named::Own(kind, binary)
        }
        (None, None) => Named::Summary,
    };
    Ok((member, named))
}

/// The answer to a read that asked `native`: the member's bytes as it sent
/// them, or the outcome that says why there are none (§11.3, §11.4).
fn answered(native: &Native) -> Response {
    match &native.answer {
        Answer::Document(document) => passed(document.forwarded()),
        Answer::Unusable { .. } => outcome(
            StatusCode::FAILED_DEPENDENCY,
            "incomplete",
            &format!(
                "the document could not be read: {} (§11.3)",
                ferrofed_eehrxf::list::unidentified(native.chosen.native.as_str())
            ),
        ),
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

/// The `200` that answers a member's bytes `forwarded` carries, as it sent
/// them and of the media type it named, octets when it named none (RFC 9110
/// §8.3).
pub(super) fn passed(forwarded: &Forwarded) -> Response {
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
            population: None,
        },
        evidence: Evidence::reached(Basis::Returned, Vec::new())
            .constructed(Category::PatientSummary),
        delivered: Some(usize::from(delivered)),
        origins: native::origins(federation.snapshot(), natives, true).collect(),
        consent_set_aside: std::collections::BTreeSet::new(),
        inserted: None,
        transformations: Vec::new(),
    })
}
