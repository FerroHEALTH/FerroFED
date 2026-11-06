// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's requests on the ITS-REST face held to the
//! patient its treatment relationship confirmation binds (eHDSI XCA Profile
//! 8.0.1 §4.2.1, §4.2.2; SAML Profile 10.0.0 §4.3), one refusal and one
//! admission per route a contact point reaches.
//!
//! A query or a read by subject for another patient, an `ehr_id` outside the
//! TRC patient's own pairs, a TRC patient no member holds and one every
//! holder restricts each get the one answer a subject no member may be read
//! for gets under withheld consent exclusions, `404 subject-unavailable`
//! (ITS-REST 1.1.0 EHR API `404`; Regulation (EU) 2025/327 Art 8), with
//! nothing sent ([`query`], [`ehr`]). A request that names no patient or EHR
//! is `403 trc-unbound`, and a token naming no TRC patient `403 trc-missing`.
//! A break of the binding is the `trc-patient-mismatch` security event, with
//! both patients by pseudonym alone ([`event`]).

mod ehr;
mod event;
mod query;

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthSettings;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use tempfile::TempDir;

use super::{TRC_CLAIM, declared, relaying};
use crate::auth::{TestResult, bearing, minted, sent};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, node_answering, registry};
use crate::support::{self, asked, error_body};

/// Another synthetic patient, and its `ehr_id` at node A and at node B.
pub(super) const OTHER: &str = "SENTINEL-OTHER-4f2c";
pub(super) const OTHER_A: &str = "4444dddd-4444-4444-8444-444444444444";
pub(super) const OTHER_B: &str = "5555eeee-5555-4555-8555-555555555555";

/// A synthetic patient both members hold and the consent pre-filter denies
/// asking either about, and its `ehr_id` at each.
pub(super) const RESTRICTED: &str = "SENTINEL-RESTRICTED-8m1v";
const RESTRICTED_A: &str = "6666ffff-6666-4666-8666-666666666666";
const RESTRICTED_B: &str = "7777aaaa-7777-4777-8777-777777777777";

/// A synthetic patient the cross-reference knows at no member.
pub(super) const UNKNOWN: &str = "SENTINEL-UNKNOWN-2h9d";

/// The qualified name the suite's stored query is held under.
pub(super) const NAME: &str = "org.example::trc_compositions";

/// The gateway over node A and node B, its issuer declared a national
/// contact point, holding the suite's stored query.
pub(super) struct Bound {
    /// The application.
    pub(super) app: Router,
    /// Node A.
    pub(super) a: Server,
    /// Node B.
    pub(super) b: Server,
    /// The directory the registry document and the stored queries live in.
    _dir: TempDir,
}

impl Bound {
    /// Asserts that neither node received anything.
    pub(super) async fn nobody_asked(&self) -> TestResult {
        assert!(asked(&self.a).await?.is_empty(), "node A received nothing");
        assert!(asked(&self.b).await?.is_empty(), "node B received nothing");
        Ok(())
    }
}

/// The `[[testkit]]` rows: the TRC patient at both members, another
/// patient at both, and a patient both hold whose pre-filter denies both.
fn rows() -> String {
    let mut text = String::new();
    let rows = [
        (PATIENT, "node-a", EHR_A),
        (PATIENT, "node-b", EHR_B),
        (OTHER, "node-a", OTHER_A),
        (OTHER, "node-b", OTHER_B),
        (RESTRICTED, "node-a", RESTRICTED_A),
        (RESTRICTED, "node-b", RESTRICTED_B),
    ];
    for (value, member, ehr_id) in rows {
        // NOTE: writing to a String cannot fail, so the result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "\n[[testkit.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        );
    }
    for member in ["node-a", "node-b"] {
        let _written: std::fmt::Result = write!(
            text,
            "\n[[testkit.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{RESTRICTED}\"\nmember = \"{member}\"\n"
        );
    }
    text
}

/// The suite's gateway, its issuer declared a national contact point.
pub(super) async fn gateway() -> Result<Bound, Box<dyn Error>> {
    gateway_with(declared()?, &rows()).await
}

/// The suite's gateway with no cross-reference, its issuer declared a
/// national contact point, so no TRC patient can be resolved.
pub(super) async fn unresolving() -> Result<Bound, Box<dyn Error>> {
    gateway_with(declared()?, "").await
}

/// The suite's gateway, its issuer no national contact point.
pub(super) async fn plain() -> Result<Bound, Box<dyn Error>> {
    gateway_with(support::caller::auth(), &rows()).await
}

/// The suite's gateway authenticating its callers as `auth` says, with the
/// `[[testkit]]` rows `rows`, its stored query held at version `1.0.0`.
async fn gateway_with(auth: AuthSettings, rows: &str) -> Result<Bound, Box<dyn Error>> {
    let a = node_answering("8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1").await;
    let b = node_answering("6cb19121-4307-4a29-9c1c-b6d6a2ab3b77::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(&a.uri(), &b.uri(), ""))?;
    let document = toml::Value::String(document.display().to_string());
    let store = toml::Value::String(dir.path().join("definitions.redb").display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\nnode_selection = \"ask-all\"\n\
         per_node_timeout_ms = 2000\noverall_timeout_ms = 3000\n\
         demographic_endpoint = \"node-a-pub\"\n\n\
         [stored_queries]\npath = {store}\n{rows}"
    );
    let settings =
        Config::from_sources(Some(&support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&settings)?;
    let mut server = crate::facade::settings_with_room();
    server.auth = auth;
    let app = ferrofed_server::router(Arc::new(state), &server);
    let stored = Request::put(format!("/v1/definition/query/{NAME}/1.0.0"))
        .header(http::header::CONTENT_TYPE, "text/plain")
        .body(Body::from(stored_aql()))?;
    let (status, _, text) = sent(&app, bearing(stored, &minted(&relaying())?)?).await?;
    assert_eq!(StatusCode::OK, status, "§12.7: stored: {text}");
    Ok(Bound {
        app,
        a,
        b,
        _dir: dir,
    })
}

/// The suite's stored query, naming its patient through `$patient`.
fn stored_aql() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = $patient \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// The contact point's claims, its TRC binding `value` in the suite's
/// namespace.
pub(super) fn treating(value: &str) -> Claims {
    let mut claims = relaying();
    claims
        .other
        .insert(TRC_CLAIM.to_owned(), format!("{NAMESPACE}|{value}"));
    claims
}

/// The contact point's claims with no TRC patient.
pub(super) fn untreated() -> Claims {
    let mut claims = relaying();
    claims.other.remove(TRC_CLAIM);
    claims
}

/// `request` bearing the contact point's token, its TRC binding `value`.
pub(super) fn as_treating(
    request: Request<Body>,
    value: &str,
) -> Result<Request<Body>, Box<dyn Error>> {
    bearing(request, &minted(&treating(value))?)
}

/// `request` sent to `bound`, with the status and the body of the answer.
pub(super) async fn answered(
    bound: &Bound,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let (status, _, text) = sent(&bound.app, request).await?;
    Ok((status, text))
}

/// The status of `request`'s answer from `bound` and its error body,
/// without the members that name the exchange (`instance`, `request_id`),
/// which differ for every request.
pub(super) async fn outcome(
    bound: &Bound,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let (status, text) = answered(bound, request).await?;
    let problem = error_body(&text)?;
    Ok((status, format!("{} {}", problem.code, problem.message)))
}

/// Sends `request` to `bound` and asserts the ITS-REST no-data answer:
/// `404 subject-unavailable`, naming no patient and no `ehr_id`, with
/// nothing sent to either node.
pub(super) async fn assert_no_data(bound: &Bound, request: Request<Body>) -> TestResult {
    let (status, text) = answered(bound, request).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "XCA §4.2.2: {text}");
    assert_eq!("subject-unavailable", error_body(&text)?.code, "{text}");
    for value in [PATIENT, OTHER, UNKNOWN, OTHER_A, OTHER_B, EHR_A, EHR_B] {
        assert!(!text.contains(value), "the answer names no patient: {text}");
    }
    bound.nobody_asked().await
}

/// Sends `request` to `bound` and asserts it is refused `403` with `code`,
/// with nothing sent to either node.
pub(super) async fn assert_refused_as(
    bound: &Bound,
    request: Request<Body>,
    code: &str,
) -> TestResult {
    let (status, text) = answered(bound, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!(code, error_body(&text)?.code, "{text}");
    bound.nobody_asked().await
}
