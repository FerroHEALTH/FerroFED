// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The anomaly warnings of the requests national contact points relay.
//!
//! The `NCPeH` Architecture Specifications 10.0.0 §5.1.9 give the contact
//! point an anomaly detection over a configurable rule, with a warning per
//! rule: too many requests for one patient, counted "even in case of a
//! request for a non existing patient", too many from one point of care,
//! and too many in all. "The triggering event is persisted in the relative
//! audit operation ... as a warning", and the detection "does not block
//! traffic". Counting behind the contact point, as the emergency limit of
//! [`super::emergency`] is, is our own design.
//!
//! Every request of a contact point's caller that reaches patient data is
//! counted, the patient lookup included, against each rule `[anomaly]`
//! declares, whether or not an access log records the accesses: by the
//! patient of the treatment relationship confirmation, by the identity
//! assertion's point of care, and across all requests. The count is
//! FerroFED's own store's ([`crate::store::anomaly`]), so it holds across
//! replicas and restarts. A request past a rule's threshold in its window
//! is served, and is the security event `unexpected-number-of-requests`
//! naming the rule's warning, the patient by its pseudonym alone; with an
//! access log its record carries the warning as its outcome description. A
//! request the store cannot count is served uncounted, the security event
//! `anomaly-uncounted`.

use axum::response::Response;
use ehds_logging::record::AccessRecord;
use ferrohealth_smart::federation::{Attribute, Field};
use secrecy::ExposeSecret;

use crate::access::Reach;
use crate::auth::caller::Caller;
use crate::config::anomaly::AnomalyRules;
use crate::facade::trc::pseudonym;
use crate::security::{Event, TARGET};
use crate::state::AppState;
use crate::store::anomaly::{Counting, Subject};

/// The warning of too many requests for one patient (`NCPeH` Architecture
/// Specifications 10.0.0 §5.1.9).
pub(crate) const FOR_PATIENT: &str =
    "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS_FOR_UNIQUE_PATIENT";

/// The warning of too many requests from one point of care (`NCPeH`
/// Architecture Specifications 10.0.0 §5.1.9).
pub(crate) const FOR_POINT_OF_CARE: &str =
    "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS_FOR_UNIQUE_POINT_OF_CARE";

/// The warning of too many requests in all (`NCPeH` Architecture
/// Specifications 10.0.0 §5.1.9).
pub(crate) const IN_ALL: &str = "EHDSI_WARNING_SEC_UNEXPECTED_NUMBER_OF_REQUESTS";

/// The names the store keys each rule by.
const PATIENT: &str = "patient";
const POINT_OF_CARE: &str = "point-of-care";
const REQUESTS: &str = "requests";

/// The warnings of the rules a served request breached, which its record
/// carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Warnings(Vec<&'static str>);

/// Counts the request `caller` makes, which `reach` says reaches patient
/// data, against the rules in effect, logged as `logged`; the warnings of
/// the rules it breached, or `None` for a request no rule counts or one
/// that breached none.
pub(super) async fn counted(
    state: &AppState,
    caller: Option<&Caller>,
    reach: Reach,
    logged: &str,
) -> Option<Warnings> {
    let caller = caller.filter(|caller| caller.is_contact_point())?;
    if reach == Reach::None {
        return None;
    }
    let rules = state.federation()?.anomaly_rules();
    let subjects = subjects(caller, &rules);
    if subjects.is_empty() {
        return None;
    }
    let Some(store) = state.store() else {
        uncounted(logged, "the gateway runs with no store");
        return None;
    };
    let counts = match store.count_requests(&subjects).await {
        Ok(counts) => counts,
        Err(failure) => {
            uncounted(logged, &crate::chain(&failure));
            return None;
        }
    };
    let warnings: Vec<&'static str> = subjects
        .iter()
        .filter(|counting| {
            counts.iter().any(|(rule, requests)| {
                rule == counting.rule && *requests > u64::from(counting.limits.threshold.get())
            })
        })
        .map(|counting| warning(counting.rule))
        .collect();
    if warnings.is_empty() {
        return None;
    }
    let relayed = caller.relayed();
    let patient = caller
        .trc_patient()
        .map(|trc| pseudonym(trc.token().expose_secret()));
    for warned in &warnings {
        Event::UnexpectedRequests.record();
        tracing::warn!(
            target: TARGET,
            event = Event::UnexpectedRequests.as_str(),
            warning = *warned,
            patient = patient.as_deref(),
            contact_point = relayed.map(|relayed| relayed.contact_point.as_str()),
            country_code = relayed.map(|relayed| relayed.country_code.as_str()),
            request_id = logged,
            "the requests national contact points relay passed an anomaly threshold in its window; the request is served"
        );
    }
    Some(Warnings(warnings))
}

/// What each rule in effect counts `caller`'s request by: the rules that
/// have a subject for it.
fn subjects<'a>(caller: &Caller, rules: &'a AnomalyRules) -> Vec<Counting<'a>> {
    let mut subjects = Vec::new();
    if let (Some(limits), Some(trc)) = (rules.patient.as_ref(), caller.trc_patient()) {
        subjects.push(Counting {
            rule: PATIENT,
            subject: Subject::of(&[trc.namespace(), trc.value().expose_secret()]),
            limits,
        });
    }
    let point_of_care = caller
        .asserted()
        .and_then(|external| external.field(Field::PointOfCare))
        .and_then(Attribute::text)
        .filter(|text| !text.is_empty());
    if let (Some(limits), Some(point_of_care)) = (rules.point_of_care.as_ref(), point_of_care) {
        let contact_point = caller
            .relayed()
            .map_or("", |relayed| relayed.contact_point.as_str());
        subjects.push(Counting {
            rule: POINT_OF_CARE,
            subject: Subject::of(&[contact_point, point_of_care]),
            limits,
        });
    }
    if let Some(limits) = rules.requests.as_ref() {
        subjects.push(Counting {
            rule: REQUESTS,
            subject: Subject::of(&[]),
            limits,
        });
    }
    subjects
}

/// The warning of the rule the store keys as `rule`.
fn warning(rule: &str) -> &'static str {
    match rule {
        PATIENT => FOR_PATIENT,
        POINT_OF_CARE => FOR_POINT_OF_CARE,
        _ => IN_ALL,
    }
}

/// Logs that the request logged as `logged` was served uncounted, for
/// `cause`.
fn uncounted(logged: &str, cause: &str) {
    Event::AnomalyUncounted.record();
    tracing::error!(
        target: TARGET,
        event = Event::AnomalyUncounted.as_str(),
        cause,
        request_id = logged,
        "a request a national contact point relays could not be counted against the anomaly rules, so it is served uncounted"
    );
}

/// Writes the warnings `response` carries into `record`, the record of the
/// access it answers, after any description the record already holds.
pub(super) fn describe(record: &mut AccessRecord, response: &Response) {
    let Some(Warnings(warnings)) = response.extensions().get::<Warnings>() else {
        return;
    };
    let mut described: Vec<String> = record.outcome_description.take().into_iter().collect();
    described.extend(warnings.iter().map(|warned| (*warned).to_owned()));
    record.outcome_description = Some(described.join("; "));
}

#[cfg(test)]
mod tests {
    use super::{
        FOR_PATIENT, FOR_POINT_OF_CARE, IN_ALL, PATIENT, POINT_OF_CARE, REQUESTS, warning,
    };

    #[test]
    fn each_rule_names_its_ncpeh_warning() {
        assert_eq!(FOR_PATIENT, warning(PATIENT));
        assert_eq!(FOR_POINT_OF_CARE, warning(POINT_OF_CARE));
        assert_eq!(IN_ALL, warning(REQUESTS));
    }
}
