// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch binding's consent pre-filter, `[nl_gf.mitz]`: the closed
//! authorization question asked of the stub Mitz for each candidate's data
//! holder before resolution (Annex B §B.6, N27a, §13.2.1).
//!
//! A member whose holder Mitz denies is `consent-denied`, never asked, and
//! clears `complete` while the query succeeds (§11.1, §11.3, N37). A member
//! Mitz permits is asked, and its node checks consent itself (N27, §14.3).
//! A Mitz that cannot answer leaves the candidates to their nodes, the
//! pre-filter's declared policy. In process: two mock CDRs, the development
//! cross-reference, and the stub Mitz; every value is synthetic, the patient
//! in the `urn:oid:2.999` example arc, which `[nl_gf.mitz] namespaces` lists
//! as standing for the BSN.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

mod config;
mod decision;
mod requester;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::RequesterClaims;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use http::StatusCode;
use serde::Deserialize;

use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, body, crossref, patient_query, post, registry, schema,
};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The Prometheus name of the pre-filter call counter.
const PREFILTER_CALLS: &str = "ferrofed_consent_prefilter_requests_total";

/// The URA of node A's and node B's care provider.
const URA_A: &str = "ura-test-0001";
const URA_B: &str = "ura-test-0002";

/// The names of the token claims the test issuer's
/// `[auth.issuer.requester]` maps; configured, since no bound specification
/// names them.
const PROFESSIONAL_CLAIM: &str = "test_uzi_number";
const ROLE_CLAIM: &str = "test_uzi_role";
const ORGANISATION_CLAIM: &str = "test_ura";
const ORGANISATION_TYPE_CLAIM: &str = "test_organisation_type";

/// The default caller: a synthetic professional number and role.
const CALLER: (&str, &str) = ("professional0001", "01.015");

/// The `[nl_gf.mitz]` table asking the Mitz at `endpoint`, with `holders`
/// as its holders table body.
fn mitz_table(endpoint: &str, holders: &str) -> String {
    format!(
        "\n[nl_gf.mitz]\nurl = \"{endpoint}\"\nnamespaces = [\"{NAMESPACE}\"]\npurpose = \"TREAT\"\ndata_categories = [\"GGC002\"]\ntimeout_ms = 1000\n\n[nl_gf.mitz.holders]\n{holders}"
    )
}

/// The holders of node A and node B, each with its URA.
fn holders() -> String {
    format!(
        "\"node-a\" = {{ type = \"V6\", ura = \"{URA_A}\" }}\n\"node-b\" = {{ type = \"V6\", ura = \"{URA_B}\" }}\n"
    )
}

/// The requester claims the test issuer's tokens carry.
fn requester_claims() -> RequesterClaims {
    RequesterClaims {
        professional: PROFESSIONAL_CLAIM.to_owned(),
        role: ROLE_CLAIM.to_owned(),
        organisation: ORGANISATION_CLAIM.to_owned(),
        organisation_type: ORGANISATION_TYPE_CLAIM.to_owned(),
    }
}

/// The development gateway over node A and node B, resolving the patient at
/// both, with the Mitz pre-filter asking `mitz`, and the test issuer's
/// tokens naming their requester in [`requester_claims`].
fn gateway_over(dir: &Path, nodes: (&str, &str), mitz: &str) -> Result<Router, Box<dyn Error>> {
    Ok(metered_over(dir, nodes, mitz)?.0)
}

/// The gateway of [`gateway_over`], with its state for the metrics.
fn metered_over(
    dir: &Path,
    (a, b): (&str, &str),
    mitz: &str,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{}{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        mitz_table(mitz, &holders())
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let mut server = crate::facade::settings_with_room();
    for issuer in &mut server.auth.issuers {
        issuer.requester = Some(requester_claims());
    }
    let state = Arc::new(AppState::with_federation(federation));
    Ok((ferrofed_server::router(Arc::clone(&state), &server), state))
}

/// The `Authorization` value of a caller whose token names the professional
/// and role `asking`, or no requester at all.
fn bearer_for(asking: Option<(&str, &str)>) -> Result<String, Box<dyn Error>> {
    let mut claims = crate::support::claims();
    if let Some((professional, role)) = asking {
        for (name, value) in [
            (PROFESSIONAL_CLAIM, professional),
            (ROLE_CLAIM, role),
            (ORGANISATION_CLAIM, "ura-test-0100"),
            (ORGANISATION_TYPE_CLAIM, "V6"),
        ] {
            claims.other.insert(name.to_owned(), value.to_owned());
        }
    }
    Ok(format!(
        "Bearer {}",
        crate::support::issuer().mint(&claims)?
    ))
}

/// Runs the patient query as `asking`, checks the answer against the
/// schema, and returns its status, its text and its typed read.
async fn ask_as(
    app: Router,
    asking: Option<(&str, &str)>,
) -> Result<(StatusCode, String, Answer), Box<dyn Error>> {
    let mut request = post(body(&patient_query())?)?;
    request
        .headers_mut()
        .insert(http::header::AUTHORIZATION, bearer_for(asking)?.parse()?);
    let (status, text) = call(app, request).await?;
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    Ok((status, text, answer))
}

/// Runs the patient query as the default caller.
async fn ask(app: Router) -> Result<(StatusCode, String, Answer), Box<dyn Error>> {
    ask_as(app, Some(CALLER)).await
}

/// The `meta.federation.consent.error` of an answer, when present.
fn consent_error(text: &str) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct WithConsent {
        meta: ConsentMeta,
    }
    #[derive(Deserialize)]
    struct ConsentMeta {
        federation: ConsentFederation,
    }
    #[derive(Deserialize)]
    struct ConsentFederation {
        consent: Option<ConsentReport>,
    }
    #[derive(Deserialize)]
    struct ConsentReport {
        error: String,
    }
    let read: WithConsent = serde_json::from_str(text)?;
    Ok(read.meta.federation.consent.map(|consent| consent.error))
}
