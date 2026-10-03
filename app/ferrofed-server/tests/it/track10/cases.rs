// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The four positions of track 10, each as the requests that put
//! [`PATIENT`]'s identifier there (§16.3 track 10, §5.4.2), and what each must
//! come to (§5.4.1, §5.4.3, N33).
//!
//! A value the gateway resolves on is consumed and stripped, so the query is
//! answered; a value it cannot consume exactly is a `400` that asks nobody; a
//! header or query parameter the gateway does not forward never reaches a
//! node, answered or refused.

use std::fmt::Write as _;

use http::{Method, StatusCode};
use uuid::Uuid;

use super::{Case, ENDPOINT_A, Expect, PATIENT, Payload, Side};

/// The endpoint header (§8.4).
const ENDPOINT: &str = "openEHR-federation-endpoint";

/// The organisation header (§8.4).
const ORGANISATION: &str = "openEHR-federation-organisation";

/// The containment every query case reads.
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";

/// The patient through `EHR_STATUS.subject.external_ref`, with its
/// namespace.
fn external_ref() -> String {
    format!(
        "e/ehr_status/subject/external_ref/id/value = '{}' AND e/ehr_status/subject/external_ref/namespace = '{}'",
        PATIENT.value(),
        PATIENT.namespace()
    )
}

/// The patient through the `ENTRY`-level `subject`, a `PARTY_IDENTIFIED`
/// whose `DV_IDENTIFIER` carries the value and the issuer.
fn entry_subject() -> String {
    format!(
        "o/subject/identifiers/id = '{}' AND o/subject/identifiers/issuer = '{}'",
        PATIENT.value(),
        PATIENT.namespace()
    )
}

/// The ITS-REST ad hoc query `aql`, with the bindings `parameters`.
fn adhoc(aql: &str, parameters: &[(&str, String)]) -> Payload {
    Payload::Query {
        q: aql.to_owned(),
        parameters: parameters
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    }
}

/// `POST /v1/query/aql` with `aql` and the header lines `headers`.
fn query(name: &str, aql: &str, headers: &[(&str, String)], expect: Expect) -> Case {
    query_at(name, "/v1/query/aql", aql, headers, expect)
}

/// `POST uri` with `aql` and the header lines `headers`.
fn query_at(name: &str, uri: &str, aql: &str, headers: &[(&str, String)], expect: Expect) -> Case {
    let mut lines = vec![("content-type".to_owned(), "application/json".to_owned())];
    lines.extend(headers.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    Case {
        name: name.to_owned(),
        method: Method::POST,
        uri: uri.to_owned(),
        headers: lines,
        body: adhoc(aql, &[]),
        expect,
    }
}

/// The answer of a query both nodes are asked.
fn both() -> Expect {
    Expect::Queried(vec![Side::A, Side::B])
}

/// A `400` that asks nobody (§5.4.1).
fn refused() -> Expect {
    Expect::Unsent(StatusCode::BAD_REQUEST)
}

/// The patient's identifier in an `EHR_STATUS.subject.external_ref`
/// predicate: consumed when it can be consumed exactly, refused when it
/// cannot (§5.4.3, §7.1).
pub(crate) fn in_external_ref() -> Vec<Case> {
    let value = PATIENT.value();
    let namespace = PATIENT.namespace();
    let bound = format!(
        "SELECT c/uid/value {FROM} WHERE e/ehr_status/subject/external_ref/id/value = $id AND e/ehr_status/subject/external_ref/namespace = $ns"
    );
    let mut parameters = query("external_ref through query parameters", &bound, &[], both());
    parameters.body = adhoc(&bound, &[("id", value.clone()), ("ns", namespace)]);
    vec![
        query(
            "external_ref predicate",
            &format!("SELECT c/uid/value {FROM} WHERE {}", external_ref()),
            &[],
            both(),
        ),
        parameters,
        query(
            "external_ref under OR",
            &format!(
                "SELECT c/uid/value {FROM} WHERE ({}) OR c/name/value = 'Synthetic'",
                external_ref()
            ),
            &[],
            refused(),
        ),
        query(
            "external_ref as a LIKE pattern",
            &format!(
                "SELECT c/uid/value {FROM} WHERE e/ehr_status/subject/external_ref/id/value LIKE '{value}*' AND e/ehr_status/subject/external_ref/namespace = '{}'",
                PATIENT.namespace()
            ),
            &[],
            refused(),
        ),
        query(
            "external_ref with a second value for the subject",
            &format!(
                "SELECT c/uid/value {FROM} WHERE {} AND e/ehr_status/subject/external_ref/id/value = '{value}-2'",
                external_ref()
            ),
            &[],
            refused(),
        ),
    ]
}

/// The patient's identifier in a `PARTY_IDENTIFIED`/`DV_IDENTIFIER`
/// predicate: on the `ENTRY`-level `subject` it is resolution input and
/// consumed; on a clinician path, or on any path the gateway did not resolve
/// on, it is a `400` (§5.4.2, §5.4.3).
pub(crate) fn in_party_identified() -> Vec<Case> {
    let value = PATIENT.value();
    vec![
        query(
            "ENTRY subject DV_IDENTIFIER predicate",
            &format!("SELECT c/uid/value {FROM} WHERE {}", entry_subject()),
            &[],
            both(),
        ),
        query(
            "both carriers at once",
            &format!(
                "SELECT c/uid/value {FROM} WHERE {} AND {}",
                external_ref(),
                entry_subject()
            ),
            &[],
            both(),
        ),
        query(
            "the identifier on COMPOSITION.composer",
            &format!(
                "SELECT c/uid/value {FROM} WHERE {} AND c/composer/identifiers/id = '{value}'",
                external_ref()
            ),
            &[],
            refused(),
        ),
        query(
            "the identifier on another path",
            &format!(
                "SELECT c/uid/value {FROM} WHERE {} AND c/name/value = '{value}'",
                entry_subject()
            ),
            &[],
            refused(),
        ),
    ]
}

/// The patient's identifier in a `SELECT` projection: the subject column is
/// re-injected at the gateway and never asked of a node, and the value as a
/// projected literal, whole or rebuilt by a string function, is refused
/// (§5.4.2, N5).
pub(crate) fn in_projection() -> Vec<Case> {
    let value = PATIENT.value();
    let (head, tail) = value.split_at(5);
    vec![
        query(
            "the external_ref subject projected",
            &format!(
                "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value {FROM} WHERE {}",
                external_ref()
            ),
            &[],
            Expect::Reinjected(vec![Side::A, Side::B]),
        ),
        query(
            "the identifier as a projected literal",
            &format!(
                "SELECT '{value}' AS patient, c/uid/value {FROM} WHERE {}",
                external_ref()
            ),
            &[],
            refused(),
        ),
        query(
            "the identifier rebuilt by CONCAT in the projection",
            &format!(
                "SELECT CONCAT('{head}', '{tail}') AS patient, c/uid/value {FROM} WHERE {}",
                external_ref()
            ),
            &[],
            refused(),
        ),
    ]
}

/// The patient's identifier in the client's query string or headers, beside a
/// query that names the patient through `external_ref`: no client header and
/// no client query parameter reaches a node (§5.4.1, N33).
pub(crate) fn in_query_string_or_header() -> Vec<Case> {
    let value = PATIENT.value();
    let namespace = PATIENT.namespace();
    let aql = format!("SELECT c/uid/value {FROM} WHERE {}", external_ref());
    let mut get = query(
        "the AQL and the identifier in a GET query string",
        "",
        &[],
        Expect::Unsent(StatusCode::NOT_IMPLEMENTED),
    );
    get.method = Method::GET;
    get.uri = format!(
        "/v1/query/aql?q={}&patient={value}",
        encoded(&format!(
            "SELECT c/uid/value {FROM} WHERE e/ehr_status/subject/external_ref/id/value = $patient"
        ))
    );
    get.body = Payload::Empty;
    vec![
        query_at(
            "the client's query string",
            &format!(
                "/v1/query/aql?patient={value}&subject_id={value}&subject_namespace={}",
                encoded(&namespace)
            ),
            &aql,
            &[],
            both(),
        ),
        query(
            "X-Request-Id",
            &aql,
            &[("x-request-id", value.clone())],
            both(),
        ),
        query(
            "a free-form header",
            &aql,
            &[("x-patient", value.clone())],
            both(),
        ),
        query(
            "Authorization",
            &aql,
            &[("authorization", format!("Bearer {value}"))],
            both(),
        ),
        query(
            "Prefer",
            &aql,
            &[("prefer", format!("patient={value}"))],
            both(),
        ),
        query(
            "the endpoint header",
            &aql,
            &[(ENDPOINT, value.clone())],
            refused(),
        ),
        query(
            "the organisation header",
            &aql,
            &[(ORGANISATION, value.clone())],
            refused(),
        ),
        query(
            "the completeness header",
            &aql,
            &[("openEHR-federation-completeness", value.clone())],
            refused(),
        ),
        get,
    ]
}

/// `cases` directed at node A by the endpoint header: each answered case asks
/// node A alone, and a case that already targets is left out (§8.4).
pub(crate) fn directed(cases: Vec<Case>) -> Vec<Case> {
    cases
        .into_iter()
        .filter(|case| {
            !case.headers.iter().any(|(name, _)| {
                name.eq_ignore_ascii_case(ENDPOINT) || name.eq_ignore_ascii_case(ORGANISATION)
            })
        })
        .map(|mut case| {
            case.name = format!("{}, directed at node A", case.name);
            case.headers
                .push((ENDPOINT.to_owned(), ENDPOINT_A.to_owned()));
            case.expect = match case.expect {
                Expect::Queried(_) => Expect::Queried(vec![Side::A]),
                Expect::Reinjected(_) => Expect::Reinjected(vec![Side::A]),
                other => other,
            };
            case
        })
        .collect()
}

/// A request of `verb` to `uri` on the single-node route, naming node A in
/// the endpoint header unless `headers` names a target itself.
fn routed(name: &str, uri: String, headers: &[(&str, String)], expect: Expect) -> Case {
    let targets = headers.iter().any(|(header, _)| {
        header.eq_ignore_ascii_case(ENDPOINT) || header.eq_ignore_ascii_case(ORGANISATION)
    });
    let mut lines = vec![("accept".to_owned(), "application/json".to_owned())];
    if !targets {
        lines.push((ENDPOINT.to_owned(), ENDPOINT_A.to_owned()));
    }
    lines.extend(headers.iter().map(|(n, v)| ((*n).to_owned(), v.clone())));
    Case {
        name: format!("{name}, on the single-node route"),
        method: Method::GET,
        uri,
        headers: lines,
        body: Payload::Empty,
        expect,
    }
}

/// The patient's identifier in the query string or the headers of a request
/// the gateway routes to one node: the request travels keyed on the node's
/// `ehr_id` with no client header the ITS-REST operation does not declare,
/// and an undeclared query parameter, or a declared value that does not match
/// its declared kind, refuses it (§7a.1, §5.4.1, N33).
pub(crate) fn on_the_route(ehr_a: Uuid) -> Vec<Case> {
    let value = PATIENT.value();
    let ehr = format!("/v1/ehr/{ehr_a}");
    vec![
        routed(
            "X-Request-Id, a free-form header and Authorization",
            ehr.clone(),
            &[
                ("x-request-id", value.clone()),
                ("x-patient", value.clone()),
                ("authorization", format!("Bearer {value}")),
            ],
            Expect::Routed(StatusCode::OK),
        ),
        routed(
            "an undeclared query parameter",
            format!("{ehr}?patient={value}"),
            &[],
            refused(),
        ),
        routed(
            "subject_id on the ehr_status read",
            format!("{ehr}/ehr_status?subject_id={value}"),
            &[],
            refused(),
        ),
        routed(
            "version_at_time, a date-time parameter",
            format!("{ehr}/ehr_status?version_at_time={value}"),
            &[],
            refused(),
        ),
        routed(
            "Accept, a composed header",
            ehr.clone(),
            &[("accept", format!("application/json; patient={value}"))],
            Expect::Routed(StatusCode::OK),
        ),
        routed(
            "a version_uid in the path",
            format!("{ehr}/ehr_status/{value}"),
            &[],
            refused(),
        ),
        routed(
            "a versioned_object_uid in the path",
            format!("{ehr}/versioned_composition/{value}"),
            &[],
            refused(),
        ),
        routed(
            "the subject addressing form",
            format!(
                "/v1/ehr?subject_id={value}&subject_namespace={}",
                encoded(&PATIENT.namespace())
            ),
            &[],
            Expect::Unsent(StatusCode::NOT_IMPLEMENTED),
        ),
        routed(
            "the endpoint header",
            ehr.clone(),
            &[(ENDPOINT, value.clone())],
            refused(),
        ),
        routed(
            "the organisation header",
            ehr,
            &[(ORGANISATION, value)],
            refused(),
        ),
    ]
}

/// The patient's identifier in the `ehr_id` slot of a read no header, binding
/// or index routes: every form it takes there is a `HIER_OBJECT_ID` that is
/// no UUID, so the ask-all probe never carries it to a member, and the read
/// is a `400` that asks nobody (§5.4.1, N33, §12.5.1).
pub(crate) fn in_the_ehr_id_slot() -> Vec<Case> {
    let value = PATIENT.value();
    let namespace = PATIENT.namespace();
    let oid = namespace.trim_start_matches("urn:oid:");
    let read = |name: &str, uri: String| Case {
        name: format!("{name} in the ehr_id slot, routed by nothing"),
        method: Method::GET,
        uri,
        headers: vec![("accept".to_owned(), "application/json".to_owned())],
        body: Payload::Empty,
        expect: refused(),
    };
    vec![
        read("the identifier", format!("/v1/ehr/{value}")),
        read(
            "the identifier under its namespace",
            format!("/v1/ehr/{oid}::{value}"),
        ),
        read(
            "the identifier, on the ehr_status read",
            format!("/v1/ehr/{value}/ehr_status"),
        ),
        read("the namespace", format!("/v1/ehr/{oid}")),
    ]
}

/// Returns `text` percent-encoded for a query string.
fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            // NOTE: writing to a String cannot fail, so the result is dropped.
            let _written: std::fmt::Result = write!(out, "%{byte:02X}");
        }
    }
    out
}
