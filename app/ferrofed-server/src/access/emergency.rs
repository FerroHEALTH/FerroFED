// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The limit of the emergency accesses a national contact point relays for
//! one health professional.
//!
//! eHDSI Section II Security Services 10.0.0 §2.5 has the contact point of
//! the patient's country deny an emergency access once the same
//! professional made a number of them in a defined amount of time, "the
//! reason SHALL note that too many attempts for an emergency access have
//! been made", and the event logged; the `NCPeH` Architecture Specifications
//! 10.0.0 §5.1.9 make such an access a deny response. Implementing
//! Regulation (EU) 2026/2083 Art 4(1) binds the national contact points to
//! these specifications, and the gateway behind the contact point applies
//! them on its national side (our own design).
//!
//! A request of a contact point's caller is an emergency access when its
//! token declares a purpose of use `[[access_log.emergency_purpose]]` names
//! (Regulation (EU) 2025/327 Art 11(5)), whether or not an access log
//! records the accesses: the limit is a deny decision of its own, apart from
//! the access record of Annex II 3.2. Every such request that reaches
//! patient data is counted before any member is asked, the patient lookup
//! excepted, which is the identification step and reaches no clinical data.
//! The count is FerroFED's own store's ([`crate::store::emergency`]), so it
//! holds across replicas and restarts. Past the limit the request is denied
//! `emergency-limit-reached`, which the FHIR face answers with
//! `ERROR_SEC_GENERIC` (eHDSI XCA Profile 8.0.1 §2.2.3, "Security
//! validations for the request or the requestor failed"); it is the security
//! event `emergency-limit-reached`, and with an access log its record
//! carries the emergency mark and the anomaly warning
//! `EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS` (`NCPeH` Architecture
//! Specifications 10.0.0 §5.1.9). A request the store cannot count is
//! refused `emergency-uncounted`, so an access is never granted uncounted.

use std::sync::Arc;

use axum::response::Response;
use ehds_logging::classify::Evidence;
use ehds_logging::record::{AccessRecord, Action, DataSubject, PatientIdentifier, Purpose};
use http::Method;

use crate::access::{AccessLog, Accessed, Gate, Reach};
use crate::auth::caller::Caller;
use crate::error::{self, Code};
use crate::face_path::FacePath;
use crate::security::{Event, TARGET};
use crate::state::AppState;
use crate::store::emergency::{Counted, Professional};

/// The anomaly warning the record of a denied emergency access carries as
/// its outcome description (`NCPeH` Architecture Specifications 10.0.0
/// §5.1.9).
pub(crate) const ANOMALY: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS";

/// The outcome description of the record of a denied emergency access.
pub(crate) const TOO_MANY: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS: too many attempts for an emergency access have been made by this health professional in the window (eHDSI Section II Security Services 10.0.0 §2.5)";

/// The request an emergency refusal answers, as the gate read it before the
/// request ran.
#[derive(Debug, Clone, Copy)]
pub(super) struct Attempt<'a> {
    /// The request method.
    pub(super) method: &'a Method,
    /// The request path.
    pub(super) path: &'a str,
    /// The gate the request passes, which names its operation.
    pub(super) gate: &'a Gate,
    /// What the request does with patient data at a node.
    pub(super) reach: Reach,
    /// The client's exchange id, which the refusal echoes.
    pub(super) request_id: &'a str,
    /// The gateway's id of the request, as the log names it.
    pub(super) logged: &'a str,
}

/// The marker of an answer that denies an emergency access past its
/// limit, whose record carries [`TOO_MANY`] as its outcome description.
#[derive(Debug, Clone, Copy)]
pub(super) struct LimitReached;

/// Writes the anomaly warning into `record`, the record of the access
/// `response` answers, when it denies an emergency access past its limit.
pub(super) fn describe(record: &mut AccessRecord, response: &Response) {
    if response.extensions().get::<LimitReached>().is_some() {
        record.outcome_description = Some(String::from(TOO_MANY));
    }
}

/// The refusal of `attempt` by `caller`, when it is an emergency access of
/// a professional a national contact point relays that the limit denies or
/// the store cannot count; `None` for a request that goes on.
pub(super) async fn held(
    state: &AppState,
    caller: Option<&Caller>,
    attempt: Attempt<'_>,
) -> Option<Response> {
    let caller = caller.filter(|caller| caller.is_contact_point())?;
    if attempt.reach == Reach::None || patient_lookup(state, attempt.path) {
        return None;
    }
    let federation = state.federation()?;
    let log = federation.access_log();
    let declared: Vec<Purpose> = caller
        .purposes()
        .iter()
        .map(|purpose| Purpose {
            system: purpose.system.clone(),
            code: purpose.code.clone(),
        })
        .collect();
    federation.emergency_purposes().mark(&declared)?;
    // NOTE: no specification governs this: our own design; client authentication refuses a
    // contact point's token it relays nothing of, so an uncounted one is refused here too.
    let Some(relayed) = caller.relayed() else {
        return Some(refused(
            Code::ContactPointAttributesRequired,
            log,
            caller,
            attempt,
        ));
    };
    let Some(store) = state.store() else {
        tracing::error!(
            request_id = attempt.logged,
            "a national contact point's emergency access cannot be counted: the gateway runs with no store, so it is refused"
        );
        return Some(refused(Code::EmergencyUncounted, log, caller, attempt));
    };
    match store
        .emergency(&Professional::of(relayed), federation.emergency_limit())
        .await
    {
        Ok(Counted::Granted) => None,
        Ok(Counted::Exceeded) => {
            Event::EmergencyLimitReached.record();
            let professional = &relayed.professional;
            tracing::warn!(
                target: TARGET,
                event = Event::EmergencyLimitReached.as_str(),
                warning = ANOMALY,
                contact_point = relayed.contact_point.as_str(),
                country_code = relayed.country_code.as_str(),
                hp_identifier = professional.hp_identifier.as_str(),
                hp_issuing_authority = professional.issuing_authority_name.as_str(),
                request_id = attempt.logged,
                "a health professional a national contact point relays reached the limit of emergency accesses in the window, so the access was denied"
            );
            let mut denied = refused(Code::EmergencyLimitReached, log, caller, attempt);
            denied.extensions_mut().insert(LimitReached);
            Some(denied)
        }
        Err(failure) => {
            tracing::error!(
                error = %crate::chain(&failure),
                request_id = attempt.logged,
                "a national contact point's emergency access could not be counted in the store, so it is refused"
            );
            Some(refused(Code::EmergencyUncounted, log, caller, attempt))
        }
    }
}

/// Whether `path` is the FHIR face's patient lookup, the identification
/// step.
fn patient_lookup(state: &AppState, path: &str) -> bool {
    state
        .fhir()
        .and_then(|fhir| crate::face_path::classify(fhir, path))
        == Some(FacePath::Patients)
}

/// The refusal `code` of `attempt` by `caller`, carrying the access the
/// record layer stores in `log`, when one is configured: no origin asked,
/// nothing delivered.
fn refused(
    code: Code,
    log: Option<&Arc<AccessLog>>,
    caller: &Caller,
    attempt: Attempt<'_>,
) -> Response {
    let mut response = error::fixed(code, attempt.request_id);
    let Some(log) = log else {
        return response;
    };
    response.extensions_mut().insert(Accessed {
        log: Arc::clone(log),
        action: action(attempt.method, attempt.reach),
        operation: attempt.gate.operation(attempt.method, attempt.path),
        resource: None,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: caller.trc_patient().map(|trc| PatientIdentifier {
                namespace: Some(trc.namespace().to_owned()),
                value: trc.value().clone(),
            }),
            ehrs: Vec::new(),
            population: None,
        },
        evidence: Evidence::default(),
        delivered: None,
        origins: Vec::new(),
        consent_set_aside: std::collections::BTreeSet::new(),
        inserted: None,
        transformations: Vec::new(),
    });
    response
}

/// What a request of `method` that `reach` says reaches patient data would
/// have done.
fn action(method: &Method, reach: Reach) -> Action {
    match (reach, method) {
        (Reach::Write, &Method::PUT) => Action::Update,
        (Reach::Write, &Method::DELETE) => Action::Delete,
        (Reach::Write, _) => Action::Create,
        (_, &Method::POST) => Action::Query,
        _ => Action::Read,
    }
}

#[cfg(test)]
mod tests {
    use http::Method;

    use super::{Action, Reach, action};

    #[test]
    fn a_refused_attempt_is_recorded_as_the_access_it_would_have_been() {
        assert_eq!(Action::Read, action(&Method::GET, Reach::Read));
        assert_eq!(Action::Query, action(&Method::POST, Reach::Read));
        assert_eq!(Action::Create, action(&Method::POST, Reach::Write));
        assert_eq!(Action::Update, action(&Method::PUT, Reach::Write));
        assert_eq!(Action::Delete, action(&Method::DELETE, Reach::Write));
    }
}
