// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `OperationOutcome` answers of the receive path that refuse a
//! document: one the component did not prepare, one whose patient did not
//! resolve, and one whose write did not succeed.

use axum::response::Response;
use eehrxf::receive::displays::DisplayError;
use eehrxf::receive::openehr::SubjectInsertionError;
use eehrxf::receive::validation::ValidationError;
use ehds_logging::record::WrittenVersion;
use ferrofed_eehrxf::face;
use ferrofed_eehrxf::receive::{self, Prepared, Unreceived};
use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::single_node::ehr::EhrCallError;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;

use super::{Origin, accessed};
use crate::access::{Accessed, NoAccess};
use crate::facade::{confined, security};
use crate::federation::Federation;

/// Why the patient of a prepared document names no one `ehr_id` at its
/// member, with nothing sent to any member.
#[derive(Debug)]
pub(super) enum Unresolved {
    /// No identifier of the patient is in a namespace the face resolves.
    NoIdentifier,
    /// The member holds no EHR for the patient.
    NoEhr,
    /// The identifiers name different patients, or one names none.
    Disagree,
    /// No resolver is configured.
    NoResolver,
    /// The demographics step or the resolver could not answer.
    Unavailable,
    /// The caller's grant is confined to another patient.
    OtherPatient,
    /// The patient of the caller's confined grant could not be resolved.
    Unconfined(confined::Unconfined),
}

impl Unresolved {
    /// The answer to a document `prepared` whose patient did not resolve,
    /// under the client's `request_id` and the gateway's `logged` id.
    pub(super) fn respond(
        &self,
        prepared: &Prepared,
        (request_id, logged): (&str, &str),
    ) -> Response {
        let member = &prepared.member;
        match self {
            Self::NoIdentifier => refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                "required",
                "the document's Patient carries no identifier in a namespace the gateway resolves",
            ),
            Self::NoEhr => refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                "not-found",
                &format!("member {member} holds no EHR for the document's patient"),
            ),
            Self::Disagree => refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                "business-rule",
                &format!(
                    "the identifiers of the document's Patient do not name one patient at member {member}"
                ),
            ),
            Self::NoResolver => refused(
                StatusCode::NOT_IMPLEMENTED,
                "not-supported",
                "no cross-reference resolver is configured to resolve the document's patient (§5.2)",
            ),
            Self::Unavailable => refused(
                StatusCode::FAILED_DEPENDENCY,
                "transient",
                &format!("the document's patient could not be resolved at member {member}"),
            ),
            Self::OtherPatient => confined::refused("receive", request_id, logged),
            Self::Unconfined(unconfined) => unconfined.respond(request_id, logged),
        }
    }
}

/// The answer to a write `error` ended, with its access record when the
/// request left the gateway.
pub(super) fn failed(
    federation: &Federation,
    prepared: &Prepared,
    at: &Origin<'_>,
    error: &EhrCallError,
    logged: &str,
) -> Response {
    let endpoint = at.endpoint.id();
    let member = &prepared.member;
    let (status, code, diagnostics) = match error {
        EhrCallError::Withheld { endpoint, part } => {
            security::gate_stopped(endpoint, part, logged);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "security",
                String::from(
                    "the write would have carried a patient identifier outside its body, so it was not sent (§5.4.1, N33)",
                ),
            )
        }
        EhrCallError::Rejected { status, .. } => (
            StatusCode::BAD_GATEWAY,
            "processing",
            format!(
                "member {member} refused the composition at endpoint {endpoint} with {}",
                status.as_u16()
            ),
        ),
        EhrCallError::TimeOut { .. } => (
            StatusCode::GATEWAY_TIMEOUT,
            "timeout",
            format!("member {member} did not answer at endpoint {endpoint} before the deadline"),
        ),
        EhrCallError::Capped(_) | EhrCallError::Expired { .. } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "transient",
            format!("the write to member {member} could not be sent before the deadline"),
        ),
        other => {
            tracing::warn!(
                error = %crate::chain(other),
                request_id = logged,
                "a received document's composition was not written"
            );
            (
                StatusCode::BAD_GATEWAY,
                "exception",
                format!("the write to member {member} at endpoint {endpoint} failed"),
            )
        }
    };
    let mut response = refused(status, code, &diagnostics);
    let accessed = reached(error)
        .and_then(|reached| accessed(federation, prepared, at, (reached.0.as_ref(), reached.1)));
    attach(&mut response, accessed);
    response
}

/// What a write that ended in `error` left at its member and the §11.1
/// status of the member's answer, or `None` when the request never left the
/// gateway.
///
/// A member that took the write but named no version of it left one the
/// record cannot tell; every other failure left none.
// NOTE: §11.1, N16: a member reached that answered with a failure is `node-error`, and one
// that gave no answer in time is `time-out`, as for a routed write.
fn reached(error: &EhrCallError) -> Option<(Option<WrittenVersion>, EndpointStatus)> {
    let status = match error {
        EhrCallError::Unversioned { .. } => {
            return Some((Some(WrittenVersion::Unnamed), EndpointStatus::Active));
        }
        EhrCallError::TimeOut { .. } => EndpointStatus::TimeOut,
        EhrCallError::Unreachable { .. } => EndpointStatus::Offline,
        other => match Contact::of_ehr_call_error(other) {
            Contact::Answered(_) => EndpointStatus::NodeError,
            Contact::Silent => EndpointStatus::TimeOut,
            Contact::Unsent | Contact::Capped => return None,
        },
    };
    Some((None, status))
}

/// The answer to a document the component did not prepare, with nothing
/// sent to any member.
pub(super) fn unprepared(unreceived: &Unreceived, logged: &str) -> Response {
    match unreceived {
        Unreceived::Read(_) => refused(
            StatusCode::BAD_REQUEST,
            "invalid",
            &crate::chain(unreceived),
        ),
        Unreceived::Category(_) | Unreceived::NoMember { .. } => refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "not-supported",
            &unreceived.to_string(),
        ),
        Unreceived::NonConformant { findings } => {
            let mut response = crate::fhir::fhir_json(
                StatusCode::UNPROCESSABLE_ENTITY,
                &receive::nonconformant(findings),
            );
            response.extensions_mut().insert(NoAccess);
            response
        }
        Unreceived::Mapping(_)
        | Unreceived::Display(DisplayError::Missing { .. } | DisplayError::Uncoded { .. }) => {
            refused(
                StatusCode::UNPROCESSABLE_ENTITY,
                "processing",
                &crate::chain(unreceived),
            )
        }
        // NOTE: no specification governs this: our own design; the terminology server is
        // the gateway's upstream, so its refusal or silence is a 502 naming its status.
        Unreceived::Display(DisplayError::Lookup { .. })
        | Unreceived::Validation(ValidationError::Terminology { .. }) => refused(
            StatusCode::BAD_GATEWAY,
            "transient",
            &crate::chain(unreceived),
        ),
        _ => {
            tracing::error!(
                error = %crate::chain(unreceived),
                request_id = logged,
                "a received document could not be prepared"
            );
            refused(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the document could not be checked against its profiles",
            )
        }
    }
}

/// The answer to a person's document whose composition cannot be marked as
/// inserted by them (Regulation (EU) 2025/327 Art 5), with nothing sent to
/// any member.
pub(super) fn unmarked(unmarked: &SubjectInsertionError, logged: &str) -> Response {
    match unmarked {
        SubjectInsertionError::NoEntry | SubjectInsertionError::GenericEntry => refused(
            StatusCode::UNPROCESSABLE_ENTITY,
            "processing",
            &format!("the document cannot be marked as inserted by the person: {unmarked}"),
        ),
        _ => {
            tracing::error!(
                error = %crate::chain(unmarked),
                request_id = logged,
                "a person's composition could not be marked as inserted by them"
            );
            refused(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the composition could not be marked as inserted by the person",
            )
        }
    }
}

/// An `OperationOutcome` of one error, answered with `status`, stating that
/// no member was sent anything.
pub(super) fn refused(status: StatusCode, code: &str, diagnostics: &str) -> Response {
    let mut response = crate::fhir::fhir_json(status, &face::outcome(code, diagnostics));
    response.extensions_mut().insert(NoAccess);
    response
}

/// Attaches `accessed` to `response` for the access log, or the statement
/// that no member was sent anything.
pub(super) fn attach(response: &mut Response, accessed: Option<Accessed>) {
    response.extensions_mut().remove::<NoAccess>();
    match accessed {
        Some(accessed) => {
            response.extensions_mut().insert(accessed);
        }
        None => {
            response.extensions_mut().insert(NoAccess);
        }
    }
}

// NOTE: PDQm ITI-78; a received document's patient is looked up through the IHE binding,
// so these refusals are reached only in a build that carries it.
#[cfg(all(test, feature = "binding-ihe"))]
mod tests {
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::body::Body;
    use axum::response::{IntoResponse, Response};
    use axum::routing::post;
    use eehrxf::receive::openehr::ReceiveMappingError;
    use eehrxf::receive::validation::ValidationError;
    use ehds_logging::record::{PatientIdentifier, WrittenVersion};
    use ferrobridge_validator::validate::ValidateError;
    use ferrofed_eehrxf::receive::{Prepared, Unreceived};
    use ferrofed_engine::dispatch::cap::Capped;
    use ferrofed_engine::hygiene::Part;
    use ferrofed_engine::single_node::ehr::EhrCallError;
    use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
    use ferrofed_testkit::{eps, fhir_packages};
    use fhir_types::r4::operation_outcome::OperationOutcome;
    use http::{Method, Request, StatusCode};
    use openehr_federation::status::EndpointStatus;
    use openehr_its::rest::client::{ClientError, ErrorBody};
    use secrecy::SecretString;
    use tower::ServiceExt as _;

    use super::{Unresolved, failed, reached, unprepared};
    use crate::access::{Accessed, NoAccess};
    use crate::config::Config;
    use crate::facade::confined::Unconfined;
    use crate::facade::receive::Origin;
    use crate::state::AppState;

    type TestResult = Result<(), Box<dyn Error>>;

    /// The synthetic patient's namespace, under the example OID arc.
    const NAMESPACE: &str = "urn:oid:2.999.1";

    /// The synthetic patient.
    const PATIENT: &str = "SENTINEL-RECEIVE-61rx";

    /// The synthetic patient's `ehr_id` at node A.
    const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";

    /// The face's base path.
    const FHIR: &str = "/fhir";

    /// A registry of node A alone, whose endpoint is never dialled.
    const REGISTRY: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "http://127.0.0.1:9"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

    /// The state of a development gateway receiving patient summaries at
    /// node A, its registry written under `dir`.
    fn state(dir: &Path) -> Result<Arc<AppState>, Box<dyn Error>> {
        let path = |value: &Path| toml::Value::String(value.display().to_string());
        let registry = dir.join("registry.toml");
        std::fs::write(&registry, REGISTRY)?;
        let key = dir.join("signing-key.pem");
        std::fs::write(&key, ferrofed_testkit::oauth::es384_pem()?)?;
        let [model, context] = eps::mapping_files();
        let dependencies = fhir_packages::dependencies()?
            .iter()
            .map(|dependency| path(dependency).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "profile = \"development\"\n\n[server]\npublic_url = \"https://gw.example.org\"\n\n\
             [registry]\ndocument = {}\n\n[federation]\nid = \"example-federation\"\n\
             node_selection = \"ask-all\"\n\n\
             [[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\n\
             member = \"node-a\"\nehr_id = \"{EHR_A}\"\n\n[audit]\ndestination = \"log\"\n\n\
             [signing]\nkey_file = {}\njwks_uri = \"https://gw.example.org/.well-known/jwks.json\"\n\n\
             [fhir]\nbase = \"{FHIR}\"\nreceive_namespaces = [\"{NAMESPACE}\"]\n\n\
             [fhir.operator]\nname = \"Synthetic Operator\"\n\
             identifier_system = \"urn:oid:2.999.9\"\nidentifier_value = \"operator-1\"\n\n\
             [[fhir.mapping]]\nsection = \"allergies-and-intolerances\"\ntemplate = {}\n\
             files = [{}, {}]\ncontext = \"{}\"\n\n\
             [pdqm]\nurl = \"http://127.0.0.1:9/fhir/\"\ntransaction = \"iti-78\"\n\
             master = \"{NAMESPACE}\"\ntimeout_ms = 1000\n\n\
             [pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n\n\
             [[fhir.receive]]\ncategory = \"Patient-Summaries\"\nmember = \"node-a\"\n\
             package = {}\ndependencies = [{dependencies}]\ntemplate = {}\n\
             files = [{}, {}]\ncontext = \"{}\"\nlanguage = \"en\"\nterritory = \"NL\"\n",
            path(&registry),
            path(&key),
            path(&eps::opt()),
            path(&model),
            path(&context),
            eps::CONTEXT,
            path(&eps::package()),
            path(&eps::opt()),
            path(&model),
            path(&context),
            eps::CONTEXT
        );
        let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
        Ok(Arc::new(AppState::build(&settings)?))
    }

    /// The synthetic patient's summary, prepared by the face of `state`.
    async fn prepared(state: &AppState) -> Result<Prepared, Box<dyn Error>> {
        let fhir = state.fhir().ok_or("the face is configured")?;
        let document = eps::received_document(&[(NAMESPACE, PATIENT)], "Synthetic substance one");
        Ok(fhir
            .receivers
            .receive(
                &document,
                (&fhir.absolute, "2026-10-15T00:00:00Z"),
                (&fhir.terminology, None),
            )
            .await?)
    }

    /// What the client reads of `response`, answered under the face of
    /// `state`: the status, the issue type and the diagnostics of its first
    /// issue, after the face's own outcome layer.
    async fn wire(
        state: &Arc<AppState>,
        response: Response,
    ) -> Result<(StatusCode, String, String), Box<dyn Error>> {
        let held = Arc::new(Mutex::new(Some(response)));
        let answer = move || {
            let held = Arc::clone(&held);
            async move {
                held.lock()
                    .ok()
                    .and_then(|mut held| held.take())
                    .unwrap_or_else(|| StatusCode::GONE.into_response())
            }
        };
        let app = Router::new()
            .route(&format!("{FHIR}/Bundle"), post(answer))
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(state),
                crate::fhir::errors::outcomes,
            ));
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("{FHIR}/Bundle"))
            .body(Body::empty())?;
        let answered = app.oneshot(request).await?;
        let status = answered.status();
        let bytes = axum::body::to_bytes(answered.into_body(), usize::MAX).await?;
        let outcome: OperationOutcome = serde_json::from_slice(&bytes)?;
        let issue = outcome.issue.first().ok_or("the outcome has an issue")?;
        let code = issue.code.value.clone().unwrap_or_default();
        let diagnostics = issue
            .diagnostics
            .as_ref()
            .and_then(|diagnostics| diagnostics.value.clone())
            .unwrap_or_default();
        Ok((status, code, diagnostics))
    }

    /// Whether `response` states that no member was sent anything, and
    /// carries no access record.
    fn nothing_sent(response: &Response) -> bool {
        response.extensions().get::<NoAccess>().is_some()
            && response.extensions().get::<Accessed>().is_none()
    }

    /// The answer to a write of the synthetic summary that ended `error`.
    async fn written(
        state: &Arc<AppState>,
        error: &EhrCallError,
    ) -> Result<Response, Box<dyn Error>> {
        let prepared = prepared(state).await?;
        let federation = state.federation().ok_or("the gateway federates")?;
        let snapshot = federation.snapshot();
        let endpoint = snapshot
            .asked_through(&NodeId::new("node-a")?)
            .ok_or("node A has an endpoint")?;
        let ehr_id = EhrId::new(EHR_A)?;
        let at = Origin {
            endpoint,
            ehr_id: &ehr_id,
            named: PatientIdentifier {
                namespace: Some(NAMESPACE.to_owned()),
                value: SecretString::from(PATIENT),
            },
            inserted: None,
        };
        Ok(failed(&federation, &prepared, &at, error, "outbound-1"))
    }

    /// The deadline error the client runtime reports for a write never sent.
    fn elapsed() -> ClientError {
        ClientError::DeadlineElapsed {
            method: Method::POST,
            path: format!("/v1/ehr/{EHR_A}/composition"),
            sent: false,
        }
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_write_the_outbound_gate_stops_is_a_security_outcome_with_nothing_sent() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let response = written(
            &state,
            &EhrCallError::Withheld {
                endpoint: EndpointId::new("node-a-pub")?,
                part: Part::Url,
            },
        )
        .await?;
        assert!(
            nothing_sent(&response),
            "§5.4.1, N33: the gate sent nothing"
        );
        let (status, code, diagnostics) = wire(&state, response).await?;
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{diagnostics}");
        assert_eq!("security", code);
        assert!(diagnostics.contains("(§5.4.1, N33)"), "{diagnostics}");
        assert!(!diagnostics.contains(PATIENT), "{diagnostics}");
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_write_the_in_flight_cap_held_back_is_transient_with_nothing_sent() -> TestResult {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let capped = Capped {
            endpoint: EndpointId::new("node-a-pub")?,
        };
        let response = written(&state, &EhrCallError::Capped(capped)).await?;
        assert!(nothing_sent(&response), "§11.5: the member was never asked");
        let (status, code, diagnostics) = wire(&state, response).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{diagnostics}");
        assert_eq!("transient", code);
        assert_eq!(
            "the write to member node-a could not be sent before the deadline",
            diagnostics
        );
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_write_whose_deadline_passed_before_it_left_is_transient_with_nothing_sent()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let expired = EhrCallError::Expired {
            endpoint: EndpointId::new("node-a-pub")?,
            source: Box::new(elapsed()),
        };
        let response = written(&state, &expired).await?;
        assert!(nothing_sent(&response), "§11.5: the member was never asked");
        let (status, code, diagnostics) = wire(&state, response).await?;
        assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{diagnostics}");
        assert_eq!("transient", code);
        assert_eq!(
            "the write to member node-a could not be sent before the deadline",
            diagnostics
        );
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_document_the_mapping_refuses_is_a_processing_outcome_with_nothing_sent() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let response = unprepared(
            &Unreceived::Mapping(ReceiveMappingError::Untemplated),
            "outbound-1",
        );
        assert!(nothing_sent(&response));
        let (status, code, diagnostics) = wire(&state, response).await?;
        assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{diagnostics}");
        assert_eq!("processing", code);
        assert_eq!(
            "the document could not be mapped into openEHR: the mapped composition names no template",
            diagnostics
        );
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_document_its_profiles_cannot_check_is_an_exception_with_nothing_sent() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let unchecked = ValidationError::Validator {
            profile: String::from("https://example.org/fhir/StructureDefinition/misplaced"),
            source: Box::new(ValidateError::NoSnapshot {
                url: String::from("https://example.org/fhir/StructureDefinition/misplaced"),
            }),
        };
        let response = unprepared(&Unreceived::Validation(unchecked), "outbound-1");
        assert!(nothing_sent(&response));
        let (status, code, diagnostics) = wire(&state, response).await?;
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, status, "{diagnostics}");
        assert_eq!("exception", code);
        assert_eq!(
            "the document could not be checked against its profiles",
            diagnostics
        );
        Ok(())
    }

    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_confined_grant_whose_patient_cannot_be_resolved_is_an_incomplete_outcome()
    -> TestResult {
        let dir = tempfile::tempdir()?;
        let state = state(dir.path())?;
        let prepared = prepared(&state).await?;
        for unconfined in [
            Unconfined::Departed(EndpointId::new("node-a-pub")?),
            Unconfined::Unanswered(NodeId::new("node-a")?),
        ] {
            let expected = unconfined.to_string();
            let response =
                Unresolved::Unconfined(unconfined).respond(&prepared, ("req-1", "outbound-1"));
            let (status, code, diagnostics) = wire(&state, response).await?;
            assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{diagnostics}");
            assert_eq!("incomplete", code);
            assert_eq!(
                format!("patient-context-unavailable: {expected}"),
                diagnostics
            );
        }
        Ok(())
    }

    #[test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    fn a_failed_write_names_its_member_in_the_status_set_of_a_routed_write() -> TestResult {
        let endpoint = || EndpointId::new("node-a-pub");
        let cases = [
            (
                EhrCallError::Rejected {
                    endpoint: endpoint()?,
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    body: ErrorBody::from_bytes(Vec::new()),
                },
                Some((None, EndpointStatus::NodeError)),
            ),
            (
                EhrCallError::TimeOut {
                    endpoint: endpoint()?,
                    source: Box::new(elapsed()),
                },
                Some((None, EndpointStatus::TimeOut)),
            ),
            (
                EhrCallError::Unversioned {
                    endpoint: endpoint()?,
                    status: StatusCode::CREATED,
                },
                Some((Some(WrittenVersion::Unnamed), EndpointStatus::Active)),
            ),
            (
                EhrCallError::Expired {
                    endpoint: endpoint()?,
                    source: Box::new(elapsed()),
                },
                None,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(expected, reached(&error), "§11.1, N16: {error}");
        }
        Ok(())
    }
}
