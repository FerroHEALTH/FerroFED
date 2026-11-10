// SPDX-FileCopyrightText: Cadasto B.V.
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
mod grant;
mod requester;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::config::auth::AuthSettings;
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
pub(crate) const URA_B: &str = "ura-test-0002";

/// The names of the token claims the test issuer's `[[auth.federation]]`
/// table reads the requester from; configured, since no bound specification
/// names them.
const PROFESSIONAL_CLAIM: &str = "test_uzi_number";
const ROLE_CLAIM: &str = "test_uzi_role";
const ORGANISATION_CLAIM: &str = "test_ura";
const ORGANISATION_TYPE_CLAIM: &str = "test_organisation_type";

/// The authentication context class every federation test token states,
/// which the issuer's assurance table reads as level substantial.
const ACR: &str = "urn:example:loa:substantial";

/// The default caller: a synthetic professional number and role.
pub(crate) const CALLER: (&str, &str) = ("professional0001", "01.015");

/// The `[nl_gf.mitz]` table asking the Mitz at `endpoint`, with `holders`
/// as its holders table body.
pub(crate) fn mitz_table(endpoint: &str, holders: &str) -> String {
    format!(
        "\n[nl_gf.mitz]\nurl = \"{endpoint}\"\nnamespaces = [\"{NAMESPACE}\"]\npurpose = \"TREAT\"\ndata_categories = [\"GGC002\"]\ntimeout = '1s'\n\n[nl_gf.mitz.holders]\n{holders}"
    )
}

/// The holders of node A and node B, each with its URA.
pub(crate) fn holders() -> String {
    format!(
        "\"node-a\" = {{ type = \"V6\", ura = \"{URA_A}\" }}\n\"node-b\" = {{ type = \"V6\", ura = \"{URA_B}\" }}\n"
    )
}

/// The domestic `federation` issuer the requester's tokens come from, which
/// signs with the test issuer's key.
pub(crate) const FEDERATION: &str = "https://federation.issuer.example.test";

/// The suite's `[auth]` with a second issuer, [`FEDERATION`], a domestic
/// `federation` issuer: an IUA mapping with the professional's country and
/// issuing authority as constants, an assurance table, and the requester's
/// four NL fields read from the claims named above.
pub(crate) fn federation_toml() -> String {
    let jwks = toml::Value::String(crate::support::caller::jwks_file().display().to_string());
    format!(
        "{persons}\n[[auth.issuer]]\nissuer = \"{issuer}\"\nkind = \"federation\"\naudience = [\"{audience}\"]\njwks_file = {jwks}\n\n[auth.issuer.assurance]\nlevels = {{ \"{ACR}\" = \"substantial\" }}\n\n[[auth.federation]]\nissuer = \"{issuer}\"\ncontact_point = false\nmapping = \"iua\"\ncountries = [\"NL\"]\nissuing_authorities = [\"CIBG\"]\nprovider_identifier_prefixes = [\"ura-test-\"]\n\n[auth.federation.fields]\n\"professional.country_code\" = [{{ constant = \"NL\" }}]\n\"professional.issuing_authority_name\" = [{{ constant = \"CIBG\" }}]\n\"nl.professional_number\" = [{{ claim = \"{PROFESSIONAL_CLAIM}\" }}]\n\"nl.role\" = [{{ claim = \"{ROLE_CLAIM}\" }}]\n\"nl.ura\" = [{{ claim = \"{ORGANISATION_CLAIM}\" }}]\n\"nl.organisation_type\" = [{{ claim = \"{ORGANISATION_TYPE_CLAIM}\" }}]\n",
        persons = crate::support::caller::auth_toml(),
        issuer = FEDERATION,
        audience = crate::support::caller::AUDIENCE,
    )
}

/// The `[auth]` [`federation_toml`] resolves to.
pub(crate) fn federation_auth() -> Result<AuthSettings, Box<dyn Error>> {
    let settings = Config::from_sources(Some(&federation_toml()), &BTreeMap::new())?.resolve()?;
    Ok(settings.server.auth)
}

/// `claims`, a token of [`FEDERATION`] that the `iua` mapping reads as a
/// professional of another domestic party: a name, a role, an identifier
/// and an organisation, at level substantial.
pub(crate) fn as_external_professional(claims: &mut ferrofed_testkit::issuer::Claims) {
    FEDERATION.clone_into(&mut claims.iss);
    let mut extensions = claims
        .extensions
        .take()
        .unwrap_or_else(ferrofed_testkit::issuer::Extensions::treatment);
    extensions.ihe_iua.subject_name = Some(String::from("Synthetic Professional"));
    extensions.ihe_iua.national_provider_identifier = Some(String::from("hp-test-0001"));
    extensions.ihe_iua.subject_organization_id = Some(String::from("ura-test-0100"));
    extensions.ihe_iua.subject_role = vec![ferrofed_testkit::issuer::Coding {
        system: String::from("urn:oid:2.999.4.15.111"),
        code: String::from("01.015"),
    }];
    claims.extensions = Some(extensions);
    claims.other.insert(String::from("acr"), ACR.to_owned());
}

/// The development gateway over node A and node B, resolving the patient at
/// both, with the Mitz pre-filter asking `mitz`, and the test issuer a
/// domestic `federation` issuer ([`federation_toml`]).
fn gateway_over(dir: &Path, nodes: (&str, &str), mitz: &str) -> Result<Router, Box<dyn Error>> {
    Ok(metered_over(dir, nodes, mitz)?.0)
}

/// The gateway of [`gateway_over`], with its state for the metrics.
fn metered_over(
    dir: &Path,
    nodes: (&str, &str),
    mitz: &str,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    metered_with(dir, nodes, &mitz_table(mitz, &holders()))
}

/// The gateway of [`metered_over`], with `table` as its `[nl_gf.mitz]`
/// tables.
fn metered_with(
    dir: &Path,
    (a, b): (&str, &str),
    table: &str,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout = '2s'\noverall_timeout = '3s'\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n{}{}",
        crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        table
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let mut server = crate::facade::settings_with_room();
    server.auth = federation_auth()?;
    let state = Arc::new(AppState::with_federation(federation));
    Ok((ferrofed_server::router(Arc::clone(&state), &server), state))
}

/// `claims`, an external professional's ([`as_external_professional`])
/// naming the professional and role `(professional, role)` of a synthetic
/// organisation in the claims [`federation_toml`] reads the requester from.
pub(crate) fn naming_requester(
    claims: &mut ferrofed_testkit::issuer::Claims,
    (professional, role): (&str, &str),
) {
    as_external_professional(claims);
    for (name, value) in [
        (PROFESSIONAL_CLAIM, professional),
        (ROLE_CLAIM, role),
        (ORGANISATION_CLAIM, "ura-test-0100"),
        (ORGANISATION_TYPE_CLAIM, "V6"),
    ] {
        claims.other.insert(name.to_owned(), value.to_owned());
    }
}

/// The `Authorization` value of a caller whose token names the professional
/// and role `asking`, or no requester at all.
fn bearer_for(asking: Option<(&str, &str)>) -> Result<String, Box<dyn Error>> {
    let mut claims = crate::support::caller::claims();
    as_external_professional(&mut claims);
    if let Some(asking) = asking {
        naming_requester(&mut claims, asking);
    }
    Ok(format!(
        "Bearer {}",
        crate::support::caller::issuer().mint(&claims)?
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
