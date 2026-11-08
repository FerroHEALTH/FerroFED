// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's dispensation and its discard held to the
//! permission they need (eHDSI SAML Profile 10.0.0 §2.5: `PPD-046` for a
//! notification on dispensation and on its discard).
//!
//! A dispensation whose token asserts permissions without `PPD-046` is
//! refused `permission-missing` with `ERROR_SEC_GENERIC` and writes
//! nothing; one with it is written. A discard without it is refused
//! `discard-refused` with `ERROR_ED_DISCARD_FAILED` before the member is
//! asked, and with no store to hold it to its dispenser it is refused as an
//! internal error. The window and the dispenser, which need the store, are
//! held in `e2e::discard` (MyHealth@EU Requirements Catalogue 10.0.0
//! 07.04).

use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::dispense::{BY_IDENTIFIER, CREATED, created, creates, dispensation, members, post};
use super::dispensed::{HELD, dispensed};
use super::trc::gateway_with;
use super::withdraw::withdrawal;
use super::{Members, RX_A, RX_B, face_id, page};
use crate::auth::minted;
use crate::contact_point::{HCP_ID, bound, with};
use crate::facade::{EHR_A, NAMESPACE, PATIENT};
use crate::fhir::{FHIR, TestResult};
use crate::support::call;

/// The performer the dispensation fixture names: a display alone.
const PHARMACIST: &str = r#"{"actor":{"display":"Synthetic pharmacist"}}"#;

/// The facility identifier the test dispensations name.
pub(crate) const FACILITY: (&str, &str) = ("urn:oid:2.999.8", "SYNTHETIC-PHARMACY-1");

/// The permission a dispensation needs, as an identity assertion spells
/// it.
pub(crate) const PPD_046: &str = "urn:oasis:names:tc:xspa:1.0:subject:hl7:permission:PPD-046";

/// Another permission, which admits no dispensation.
pub(crate) const PRD_006: &str = "urn:oasis:names:tc:xspa:1.0:subject:hl7:permission:PRD-006";

/// What a contact point asserts in a test: its permissions, none asserted
/// when empty, and its `organization-id`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Asserted<'a> {
    /// The permissions the identity assertion carries.
    pub(crate) permissions: &'a [&'a str],
    /// The `organization-id`.
    pub(crate) organization: &'a str,
}

impl Asserted<'_> {
    /// The test provider, carrying `PPD-046`.
    pub(crate) const PERMITTED: Self = Self {
        permissions: &[PPD_046],
        organization: HCP_ID,
    };
}

/// A dispensing scene: two members, node A holding the test patient's
/// dispensation [`CREATED`] and taking its update, and a gateway over them
/// trusting the test contact point.
pub(crate) struct Dispensing {
    /// The gateway.
    pub(crate) app: Router,
    members: Members,
    _dir: tempfile::TempDir,
}

impl Dispensing {
    /// The scene, its gateway configured with the tables `tables` after its
    /// own, written by `tables` into the directory it is handed.
    pub(crate) async fn start(
        tables: impl FnOnce(&Path) -> Result<String, Box<dyn Error>>,
    ) -> Result<Self, Box<dyn Error>> {
        let members = members((&[RX_A], &[RX_B]), created()).await?;
        Mock::given(method("GET"))
            .and(path("/fhir/MedicationDispense"))
            .respond_with(page(dispensed(HELD)))
            .mount(&members.own_a)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/fhir/MedicationDispense/{CREATED}")))
            .respond_with(ResponseTemplate::new(200).insert_header("etag", "W/\"2\""))
            .mount(&members.own_a)
            .await;
        let dir = tempfile::tempdir()?;
        let tables = tables(dir.path())?;
        let app = gateway_with(&members, dir.path(), &tables)?;
        Ok(Self {
            app,
            members,
            _dir: dir,
        })
    }

    /// The contact point's dispensation, naming the facility `facility`
    /// when it names one, as `asserted`.
    pub(crate) async fn dispense(
        &self,
        asserted: Asserted<'_>,
        facility: Option<(&str, &str)>,
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        let body = performed(&dispensation(BY_IDENTIFIER), facility);
        let token = token(asserted)?;
        call(self.app.clone(), bound(post(&body)?, &token)?).await
    }

    /// The contact point's discard of node A's dispensation, naming the
    /// facility `facility`, as `asserted`.
    pub(crate) async fn discard(
        &self,
        asserted: Asserted<'_>,
        facility: (&str, &str),
    ) -> Result<(StatusCode, String), Box<dyn Error>> {
        let id = face_id("node-a-mpd", EHR_A, CREATED)?;
        let body = performed(&withdrawal(&id), Some(facility));
        let request = Request::put(format!("{FHIR}/MedicationDispense/{id}"))
            .header(header::CONTENT_TYPE, "application/fhir+json")
            .body(Body::from(body))?;
        call(self.app.clone(), bound(request, &token(asserted)?)?).await
    }

    /// How many creates reached node A.
    pub(crate) async fn created(&self) -> Result<usize, Box<dyn Error>> {
        Ok(creates(&self.members.own_a).await?.len())
    }

    /// How many updates reached node A.
    pub(crate) async fn updated(&self) -> Result<usize, Box<dyn Error>> {
        updates(&self.members.own_a).await
    }
}

/// The updates that reached `server`.
async fn updates(server: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .filter(|request| request.method == http::Method::PUT)
        .count())
}

/// `body`, its performer the organisation `facility` names, contained, when
/// it names one.
fn performed(body: &str, facility: Option<(&str, &str)>) -> String {
    let Some((system, value)) = facility else {
        return body.to_owned();
    };
    body.replacen(
        r#""contained":[{"resourceType":"Patient","#,
        &format!(
            r#""contained":[{{"resourceType":"Organization","id":"pharmacy","identifier":[{{"system":"{system}","value":"{value}"}}]}},{{"resourceType":"Patient","#
        ),
        1,
    )
    .replace(
        PHARMACIST,
        &format!(r##"{PHARMACIST},{{"actor":{{"reference":"#pharmacy"}}}}"##),
    )
}

/// The contact point's token, its TRC binding the test patient, as
/// `asserted`.
fn token(asserted: Asserted<'_>) -> Result<String, Box<dyn Error>> {
    let mut claims: Claims = with(
        "patient.id",
        Value::String(format!("{NAMESPACE}|{PATIENT}")),
    );
    claims.ehdsi.insert(
        "provider.healthcare_provider_identifier".to_owned(),
        json!(asserted.organization),
    );
    if !asserted.permissions.is_empty() {
        claims.ehdsi.insert(
            "professional.permissions".to_owned(),
            json!(asserted.permissions),
        );
    }
    minted(&claims)
}

/// The codes of the first issue's `details` of the outcome `text`.
pub(crate) fn codes(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    Ok(outcome
        .pointer("/issue/0/details/coding")
        .and_then(Value::as_array)
        .map(|codings| {
            codings
                .iter()
                .filter_map(|coding| coding.get("code").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

/// Asserts that `answer` refuses a discard with `ERROR_ED_DISCARD_FAILED`.
pub(crate) fn assert_discard_refused(
    what: &str,
    (status, text): &(StatusCode, String),
) -> TestResult {
    assert_eq!(StatusCode::FORBIDDEN, *status, "{what}: {text}");
    assert_eq!(
        vec!["discard-refused", "ERROR_ED_DISCARD_FAILED"],
        codes(text)?,
        "{what}: {text}"
    );
    Ok(())
}

/// The scene with no store.
async fn unstored() -> Result<Dispensing, Box<dyn Error>> {
    Dispensing::start(|_dir: &Path| Ok(String::new())).await
}

#[tokio::test]
async fn a_dispensation_without_ppd_046_is_refused_and_writes_nothing() -> TestResult {
    let scene = unstored().await?;
    let asserted = Asserted {
        permissions: &[PRD_006],
        ..Asserted::PERMITTED
    };
    let (status, text) = scene.dispense(asserted, Some(FACILITY)).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!(
        vec!["permission-missing", "ERROR_SEC_GENERIC"],
        codes(&text)?,
        "SAML Profile 10.0.0 §2.5: {text}"
    );
    assert_eq!(0, scene.created().await?, "nothing is written");
    Ok(())
}

#[tokio::test]
async fn a_dispensation_with_ppd_046_or_no_permissions_asserted_is_written() -> TestResult {
    for permissions in [&[PRD_006, PPD_046][..], &[]] {
        let scene = unstored().await?;
        let asserted = Asserted {
            permissions,
            ..Asserted::PERMITTED
        };
        let (status, text) = scene.dispense(asserted, Some(FACILITY)).await?;
        assert_eq!(StatusCode::CREATED, status, "{permissions:?}: {text}");
        assert_eq!(1, scene.created().await?, "{permissions:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_discard_without_ppd_046_is_refused_before_the_member() -> TestResult {
    let scene = unstored().await?;
    let asserted = Asserted {
        permissions: &[PRD_006],
        ..Asserted::PERMITTED
    };
    let answer = scene.discard(asserted, FACILITY).await?;
    assert_discard_refused("without PPD-046", &answer)?;
    assert_eq!(0, scene.updated().await?, "the member is never asked");
    Ok(())
}

#[tokio::test]
async fn a_discard_no_store_can_hold_to_its_dispenser_is_refused() -> TestResult {
    let scene = unstored().await?;
    let (status, text) = scene.discard(Asserted::PERMITTED, FACILITY).await?;
    assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{text}");
    assert_eq!(0, scene.updated().await?, "the member is never asked");
    Ok(())
}
