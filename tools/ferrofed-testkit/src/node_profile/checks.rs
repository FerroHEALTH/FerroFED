// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The checks, one per obligation of the Federation-Node profile (§16.2).
//!
//! Every request a check sends names the EHR by its `ehr_id` alone and
//! carries no patient identifier and no `subject`. Each observation is one
//! evidence line with its own verdict, and the finding takes the worst of
//! them ([`Finding::from_observations`]). Statuses are read as the node sent
//! them, so a status ITS-REST does not document for an operation is evidence
//! about the node, never an error of the check.

use http::StatusCode;
use openehr_its::json::from_canonical_json;
use openehr_its::rest::generated::query::ResultSet;
use openehr_rm::v1_2::ehr::ehr::Ehr;
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use uuid::Uuid;

use super::{Answer, Arrangement, Check, CheckError, Credential, Finding, Interface, Verdict};

/// One observation: its verdict and the evidence line.
type Observation = (Verdict, String);

/// The query of the EHR itself, scoped by its `ehr_id` in the `WHERE`
/// clause, which answers one row when the node holds the EHR.
fn ehr_query(ehr_id: Uuid) -> String {
    format!("SELECT e/ehr_id/value FROM EHR e WHERE e/ehr_id/value = '{ehr_id}'")
}

/// The query of the EHR itself, scoped by its `ehr_id` in the `EHR`
/// predicate, which answers one row when the node holds the EHR.
fn ehr_predicate_query(ehr_id: Uuid) -> String {
    format!("SELECT e/ehr_id/value FROM EHR e[ehr_id/value='{ehr_id}']")
}

/// The query of the EHR's compositions, scoped by its `ehr_id` in the `EHR`
/// predicate.
fn composition_query(ehr_id: Uuid) -> String {
    format!("SELECT c/uid/value FROM EHR e[ehr_id/value='{ehr_id}'] CONTAINS COMPOSITION c")
}

/// A query no AQL grammar parses: a `SELECT` with no projection and a `WHERE`
/// with no condition.
const UNPARSABLE_QUERY: &str = "SELECT FROM EHR e WHERE";

/// Returns the number of rows of a `200` query answer, or `None` when the
/// answer is no ITS-REST `RESULT_SET`.
// NOTE: no specification governs the checks: our own design; a body that does not decode is
// itself the observation, which the caller records with the status.
fn rows(answer: &Answer) -> Option<usize> {
    serde_json::from_slice::<ResultSet>(&answer.body)
        .ok()
        .map(|result| result.rows.len())
}

/// Returns the body of `answer` decoded with the strict canonical JSON
/// reader, or `None` when it is not one.
// NOTE: no specification governs the checks: our own design; an decoded_or_reason body is evidence.
fn decoded<T: serde::de::DeserializeOwned>(answer: &Answer) -> Option<T> {
    std::str::from_utf8(&answer.body)
        .ok()
        .and_then(|text| from_canonical_json(text).ok())
}

/// Returns the body of `answer` decoded with the strict canonical JSON
/// reader, or why it is not one, for an evidence line.
fn decoded_or_reason<T: serde::de::DeserializeOwned>(answer: &Answer) -> Result<T, String> {
    let text = std::str::from_utf8(&answer.body).map_err(|error| error.to_string())?;
    from_canonical_json(text).map_err(|error| error.to_string())
}

/// Checks that the node is invocable on `ehr_id` alone: the EHR, its
/// `EHR_STATUS` and its compositions answer to the `ehr_id`, with nothing
/// else in the request (§5.5, §16.2, N7, N34; assists CP-27).
///
/// The node must hold `ehr_id` with at least one `COMPOSITION`, which the
/// caller arranges: a query that finds no composition then shows the node
/// did not serve one.
///
/// # Errors
///
/// Returns [`CheckError`] when a request reaches no answer.
pub async fn invocable_on_ehr_id(
    interface: &Interface,
    ehr_id: Uuid,
) -> Result<Finding, CheckError> {
    let mut seen = Vec::new();

    let ehr = interface.get(&format!("ehr/{ehr_id}"), None).await?;
    seen.push(if ehr.status == StatusCode::OK {
        match decoded_or_reason::<Ehr>(&ehr) {
            Ok(read)
                if read
                    .ehr_id
                    .value()
                    .eq_ignore_ascii_case(&ehr_id.to_string()) =>
            {
                (
                    Verdict::Pass,
                    format!("GET /ehr/{ehr_id} answered 200 with that EHR"),
                )
            }
            Ok(read) => (
                Verdict::Fail,
                format!(
                    "GET /ehr/{ehr_id} answered 200 with the EHR {}",
                    read.ehr_id.value()
                ),
            ),
            Err(reason) => (
                Verdict::Fail,
                format!(
                    "GET /ehr/{ehr_id} answered 200 with a body the canonical JSON reader refuses as an EHR: {reason}"
                ),
            ),
        }
    } else {
        (
            Verdict::Fail,
            format!("GET /ehr/{ehr_id} answered {}", ehr.status),
        )
    });

    let status = interface
        .get(&format!("ehr/{ehr_id}/ehr_status"), None)
        .await?;
    seen.push(if status.status == StatusCode::OK {
        (
            Verdict::Pass,
            format!("GET /ehr/{ehr_id}/ehr_status answered 200"),
        )
    } else {
        (
            Verdict::Fail,
            format!("GET /ehr/{ehr_id}/ehr_status answered {}", status.status),
        )
    });

    let one = interface.query(&ehr_query(ehr_id), None).await?;
    seen.push(rows_observed(
        &one,
        "the query of the EHR scoped by e/ehr_id/value",
        |n| n == 1,
        "one row",
    ));
    let compositions = interface.query(&composition_query(ehr_id), None).await?;
    seen.push(rows_observed(
        &compositions,
        "the query of its compositions scoped by EHR e[ehr_id/value]",
        |n| n > 0,
        "at least one row",
    ));
    Ok(Finding::from_observations(Check::InvocableOnEhrId, seen))
}

/// The observation of a query answer that should be `200` with a row count
/// `expected` accepts, which `wanted` describes.
fn rows_observed(
    answer: &Answer,
    what: &str,
    expected: impl Fn(usize) -> bool,
    wanted: &str,
) -> Observation {
    if answer.status != StatusCode::OK {
        return (Verdict::Fail, format!("{what} answered {}", answer.status));
    }
    match rows(answer) {
        Some(n) if expected(n) => (
            Verdict::Pass,
            format!("{what} answered 200 with {n} row(s)"),
        ),
        Some(n) => (
            Verdict::Fail,
            format!("{what} answered 200 with {n} row(s), where {wanted} was due"),
        ),
        None => (
            Verdict::Fail,
            format!("{what} answered 200 with a body that is no RESULT_SET"),
        ),
    }
}

/// Checks that the node never requires `subject` (§16.2; assists CP-27).
///
/// An EHR created with no `EHR_STATUS`, which the node completes with an
/// anonymous `PARTY_SELF` (ITS-REST `ehr_create`), is read and queried by its
/// `ehr_id`.
///
/// A node that gives the EHR a subject of its own leaves the check not
/// observable, since the EHR then has one.
///
/// # Errors
///
/// Returns [`CheckError`] when a request reaches no answer.
pub async fn subject_not_required(interface: &Interface) -> Result<Finding, CheckError> {
    let mut seen = Vec::new();
    let created = interface.create_ehr().await?;
    if created.status != StatusCode::CREATED {
        seen.push((
            Verdict::Fail,
            format!(
                "POST /ehr with no EHR_STATUS answered {}, where ITS-REST lets the body be omitted and the server supply a PARTY_SELF subject",
                created.status
            ),
        ));
        return Ok(Finding::from_observations(Check::SubjectNotRequired, seen));
    }
    let named = decoded::<Ehr>(&created)
        .map(|ehr| ehr.ehr_id.value().to_owned())
        .or_else(|| {
            created
                .location
                .as_deref()
                .and_then(|location| location.trim_end_matches('/').rsplit('/').next())
                .map(str::to_owned)
        })
        .and_then(|text| Uuid::try_parse(&text).ok());
    let Some(ehr_id) = named else {
        seen.push((
            Verdict::Fail,
            "POST /ehr answered 201 naming no ehr_id in its body or its Location".to_owned(),
        ));
        return Ok(Finding::from_observations(Check::SubjectNotRequired, seen));
    };
    seen.push((
        Verdict::Pass,
        format!("POST /ehr with no EHR_STATUS answered 201 and created {ehr_id}"),
    ));

    let status = interface
        .get(&format!("ehr/{ehr_id}/ehr_status"), None)
        .await?;
    seen.push(if status.status == StatusCode::OK {
        match decoded::<EhrStatus>(&status) {
            Some(read) if read.subject.external_ref.is_none() => (
                Verdict::Pass,
                format!("GET /ehr/{ehr_id}/ehr_status answered a PARTY_SELF subject with no external_ref"),
            ),
            Some(_) => (
                Verdict::NotObservable,
                format!("the node gave EHR {ehr_id} a subject of its own, so the check cannot show it works without one"),
            ),
            None => (
                Verdict::Fail,
                format!("GET /ehr/{ehr_id}/ehr_status answered 200 with no EHR_STATUS"),
            ),
        }
    } else {
        (
            Verdict::Fail,
            format!("GET /ehr/{ehr_id}/ehr_status answered {}", status.status),
        )
    });

    let one = interface.query(&ehr_query(ehr_id), None).await?;
    seen.push(rows_observed(
        &one,
        "the query of the subjectless EHR scoped by e/ehr_id/value",
        |n| n == 1,
        "one row",
    ));
    Ok(Finding::from_observations(Check::SubjectNotRequired, seen))
}

/// Checks that the node passes its own errors through (§16.2; assists CP-18).
///
/// A read of an EHR it does not hold and a query it cannot parse answer the
/// ITS-REST error status, never a success (ITS-REST `ehr_get_by_id`,
/// `ehr_status_get_at_time` and `query_execute_adhoc_query_body`).
///
/// # Errors
///
/// Returns [`CheckError`] when a request reaches no answer.
pub async fn errors_passed_through(interface: &Interface) -> Result<Finding, CheckError> {
    let unknown = Uuid::new_v4();
    let mut seen = Vec::new();
    let ehr = interface.get(&format!("ehr/{unknown}"), None).await?;
    seen.push(error_observed(
        ehr.status,
        &format!("GET /ehr/{unknown}, an ehr_id the node does not hold,"),
        StatusCode::NOT_FOUND,
    ));
    let status = interface
        .get(&format!("ehr/{unknown}/ehr_status"), None)
        .await?;
    seen.push(error_observed(
        status.status,
        &format!("GET /ehr/{unknown}/ehr_status"),
        StatusCode::NOT_FOUND,
    ));
    let query = interface.query(UNPARSABLE_QUERY, None).await?;
    seen.push(error_observed(
        query.status,
        "POST /query/aql of an unparsable query",
        StatusCode::BAD_REQUEST,
    ));
    Ok(Finding::from_observations(Check::ErrorsPassedThrough, seen))
}

/// The observation of an error the node should answer with `due`.
fn error_observed(status: StatusCode, what: &str, due: StatusCode) -> Observation {
    if status == due {
        (Verdict::Pass, format!("{what} answered {status}"))
    } else if status.is_success() {
        (
            Verdict::Fail,
            format!("{what} answered {status}, a success where ITS-REST answers {due}"),
        )
    } else {
        (
            Verdict::Fail,
            format!("{what} answered {status}, where ITS-REST answers {due}"),
        )
    }
}

/// Checks that the node makes and holds its own access decision (§13, N26;
/// assists CP-18).
///
/// An EHR its policy withholds from one principal is refused to that
/// principal on the read and on both `ehr_id`-scoped query forms, the forms a
/// gateway dispatches (§7, N7), while it is served to the principal the
/// policy admits.
///
/// The operator arranges the refusal with the node's own access controls.
///
/// # Errors
///
/// Returns [`CheckError`] when a request reaches no answer.
pub async fn access_decided_at_node(
    interface: &Interface,
    arrangement: &Arrangement,
) -> Result<Finding, CheckError> {
    refusal(Check::AccessDecidedAtNode, interface, arrangement).await
}

/// Checks that the node checks consent before it releases data (§13.2, N27,
/// N27a; assists CP-19).
///
/// An EHR whose patient refused consent to one principal is refused to that
/// principal on the read and on both `ehr_id`-scoped query forms, however the
/// request reached the node.
///
/// The operator arranges the consent refusal at the node; where nothing can
/// arrange one, the finding is [`Finding::not_arranged`].
///
/// # Errors
///
/// Returns [`CheckError`] when a request reaches no answer.
pub async fn consent_before_release(
    interface: &Interface,
    arrangement: &Arrangement,
) -> Result<Finding, CheckError> {
    refusal(Check::ConsentBeforeRelease, interface, arrangement).await
}

/// The refusal both release checks observe, recorded under `check`.
async fn refusal(
    check: Check,
    interface: &Interface,
    arrangement: &Arrangement,
) -> Result<Finding, CheckError> {
    let ehr_id = arrangement.ehr_id;
    let permitted = Some(&arrangement.permitted);
    let refused = Some(&arrangement.refused);
    let mut seen = Vec::new();

    let read = interface.get(&format!("ehr/{ehr_id}"), permitted).await?;
    seen.push(if read.status == StatusCode::OK {
        (
            Verdict::Pass,
            format!(
                "GET /ehr/{ehr_id} as the permitted principal {} answered 200",
                user(permitted)
            ),
        )
    } else {
        (
            Verdict::NotObservable,
            format!(
                "GET /ehr/{ehr_id} as the permitted principal {} answered {}, so a refusal of another shows nothing",
                user(permitted),
                read.status
            ),
        )
    });
    let read = interface.get(&format!("ehr/{ehr_id}"), refused).await?;
    seen.push(withheld(
        &read,
        &format!(
            "GET /ehr/{ehr_id} as the refused principal {}",
            user(refused)
        ),
        false,
    ));

    let forms = [
        (ehr_query(ehr_id), "the query scoped by e/ehr_id/value"),
        (
            ehr_predicate_query(ehr_id),
            "the query scoped by EHR e[ehr_id/value]",
        ),
    ];
    for (aql, form) in &forms {
        let served = interface.query(aql, permitted).await?;
        seen.push(match (served.status, rows(&served)) {
            (StatusCode::OK, Some(n)) if n > 0 => (
                Verdict::Pass,
                format!("{form} as the permitted principal answered {n} row(s)"),
            ),
            (status, _) => (
                Verdict::NotObservable,
                format!(
                    "{form} as the permitted principal answered {status} with no row, so a refusal of another shows nothing"
                ),
            ),
        });
        let answer = interface.query(aql, refused).await?;
        seen.push(withheld(
            &answer,
            &format!("{form} as the refused principal"),
            true,
        ));
    }
    Ok(Finding::from_observations(check, seen))
}

/// The user name of `credential`, for an evidence line.
fn user(credential: Option<&Credential>) -> &str {
    credential.map_or("(none)", Credential::user)
}

/// The observation of what a refused principal was answered: a refusal
/// status withholds, and so does a query answer with no row when `query` is
/// set; a success that carries data releases it.
fn withheld(answer: &Answer, what: &str, query: bool) -> Observation {
    let status = answer.status;
    if matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return (
            Verdict::Pass,
            format!("{what} answered {status} and released nothing"),
        );
    }
    if !status.is_success() {
        return (
            Verdict::NotObservable,
            format!("{what} answered {status}: nothing released, and no refusal either"),
        );
    }
    match (query, rows(answer)) {
        (true, Some(0)) => (
            Verdict::Pass,
            format!("{what} answered {status} with no row"),
        ),
        (true, Some(n)) => (
            Verdict::Fail,
            format!("{what} answered {status} and released {n} row(s) of the withheld EHR"),
        ),
        (true, None) => (
            Verdict::NotObservable,
            format!("{what} answered {status} with a body that is no RESULT_SET"),
        ),
        _ => (
            Verdict::Fail,
            format!("{what} answered {status} and released the withheld EHR"),
        ),
    }
}
