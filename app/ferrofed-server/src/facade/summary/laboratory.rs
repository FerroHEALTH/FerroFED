// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory reports a document list gathers beside the patient
//! summary (Regulation (EU) 2025/327 Art 14(1)(e)), and how their answers
//! settle the list's completeness (§11.3, §11.4).
//!
//! A member declared `native` for `Laboratory-Reports` is asked at its own
//! endpoint, by its own `ehr_id` alone (§5.4.1, N33), with the interactions
//! the HL7 Europe EU Health Data API 1.0.0-ballot
//! `CapabilityStatement-EEHRxF-DocumentAccessProvider` SHALL support:
//! `Patient?identifier=` for its patient's logical id, then
//! `DocumentReference?patient=&type=` for its laboratory reports, each
//! searchset followed to its last page. It is left out of the laboratory
//! report query, which every other member is asked when the gateway maps
//! the category ([`Laboratory::Mapped`]).

use std::collections::BTreeSet;
use std::sync::Arc;

use ehds_logging::category::Category;
use ferrofed_eehrxf::list::{LABORATORY_REPORT, LOINC, RESOURCE, attachment_urls};
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::fanout::{Completion, FanOutError};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_search::{By, Next};
use ferrofed_registry::id::EndpointId;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use tokio::task::JoinSet;
use url::Url;

use super::Unsummarised;
use crate::facade::answer::Failure;
use crate::facade::located::Located;
use crate::facade::mapped::{self, Answered};
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions::{Held, Unprescribed, Unread, paged};
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// The resource type a member's patient is found by.
const PATIENT: &str = "Patient";

/// The laboratory reports a list gathers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Laboratory {
    /// None.
    Unlisted,
    /// Each member's own, from the members that serve them themselves.
    Own,
    /// Those, and the gateway's, from every other member's compositions.
    Mapped,
}

/// A member that serves laboratory reports itself, and what its own
/// endpoint answered.
#[derive(Debug)]
pub(crate) struct Own {
    /// The member, as the list reached it.
    pub(crate) chosen: Chosen,
    /// Every page of its `DocumentReference` search, or why there is none.
    pub(crate) held: Held,
}

/// The members `located` dispatches to that serve laboratory reports at
/// their own endpoint, in endpoint order, and, when `laboratory` is
/// [`Laboratory::Mapped`], what every other member answered the laboratory
/// report query for the patient `system`|`value`.
///
/// # Errors
///
/// The [`Unsummarised`] of a query that could not be run.
pub(super) async fn queried(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, laboratory): (&Located, Laboratory),
    patient: (&str, &str),
) -> Result<(Vec<Chosen>, Vec<(EndpointId, Answered)>), Unsummarised> {
    if laboratory == Laboratory::Unlisted {
        return Ok((Vec::new(), Vec::new()));
    }
    let own = native::chosen_for(
        federation.snapshot(),
        &located.targets,
        Category::MedicalTestResult.code(),
    );
    let answered = if laboratory == Laboratory::Mapped {
        laboratory_reports(federation, arrived, (located, &own), patient).await?
    } else {
        Vec::new()
    };
    Ok((own, answered))
}

/// Keeps in `gathered` what the members answered the laboratory report
/// query, `laboratory`, and what those that serve the reports themselves
/// answered, `own`, with every endpoint a request left for.
pub(super) fn kept(
    gathered: &mut super::Gathered,
    laboratory: Vec<(EndpointId, Answered)>,
    own: Vec<Own>,
) {
    for (endpoint, answered) in &laboratory {
        if matches!(
            answered,
            Answered::Rows(_) | Answered::Silent { left: true, .. }
        ) {
            gathered.sent.insert(endpoint.clone());
        }
    }
    for member in &own {
        if member.held.left() {
            gathered.sent.insert(member.chosen.native.clone());
        }
    }
    gathered.laboratory = laboratory;
    gathered.own_laboratory = own;
}

/// Asks every member of `chosen` for its laboratory reports at its own
/// endpoint, at once, under the budget of the request `arrived`, refusing to
/// send anything carrying one of `withheld`, in the order of `chosen`.
///
/// # Errors
///
/// [`Failure::FanOut`] when the clock cannot hold the budget, when a member
/// has no client or no native endpoint, or when a task failed.
pub(super) async fn own(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    chosen: Vec<Chosen>,
    withheld: Arc<Withheld>,
) -> Result<Vec<Own>, Failure> {
    let (options, until) =
        native::options(federation, arrived, withheld).map_err(Failure::FanOut)?;
    let mut tasks = JoinSet::new();
    for (position, member) in chosen.into_iter().enumerate() {
        let federation = Arc::clone(federation);
        let options = options.clone();
        tasks.spawn(async move {
            let held = reports(&federation, &member, &options, until).await;
            held.map(|held| {
                (
                    position,
                    Own {
                        chosen: member,
                        held,
                    },
                )
            })
        });
    }
    let mut answered = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let own = joined
            .map_err(|joined| Failure::FanOut(FanOutError::Task(joined)))?
            .map_err(Failure::FanOut)?;
        if let Held::Unusable { status, reason } = &own.1.held {
            tracing::warn!(
                endpoint = own.1.chosen.native.as_str(),
                status = status.as_u16(),
                reason = %reason,
                request_id = %arrived.outbound,
                "a member answered laboratory reports the list cannot pass through"
            );
        }
        answered.push(own);
    }
    answered.sort_by_key(|(position, _)| *position);
    Ok(answered.into_iter().map(|(_, own)| own).collect())
}

/// What `member` answered for its laboratory reports, under `options`,
/// abandoned when the overall budget `until` runs out: no page when it
/// holds no `Patient` for its `ehr_id`.
async fn reports(
    federation: &Federation,
    member: &Chosen,
    options: &DispatchOptions,
    until: tokio::time::Instant,
) -> Result<Held, FanOutError> {
    let patients = paged(
        federation,
        (member, PATIENT, By::Identifier),
        options,
        until,
    )
    .await?;
    let Held::Found(pages) = patients else {
        return Ok(patients);
    };
    let ids: Vec<Option<&str>> = pages
        .iter()
        .flat_map(|page| &page.prescriptions)
        .map(|found| found.kept.id())
        .collect();
    let patient = match ids.as_slice() {
        // NOTE: FHIR R4 search: an empty searchset says the member holds no Patient for its ehr_id,
        // so it holds no laboratory report of the patient either.
        [] => return Ok(Held::Found(Vec::new())),
        [Some(id)] if logical(id) => (*id).to_owned(),
        _ => {
            return Ok(Held::Unusable {
                status: StatusCode::OK,
                reason: Unread::Patient,
            });
        }
    };
    let kind = format!("{LOINC}|{LABORATORY_REPORT}");
    let by = By::Patient {
        patient: &patient,
        kind: &kind,
    };
    let documents = paged(federation, (member, RESOURCE, by), options, until).await?;
    let Held::Found(pages) = &documents else {
        return Ok(documents);
    };
    let Some(base) = federation
        .snapshot()
        .native_endpoint(&member.native)
        .map(|native| native.url().clone())
    else {
        return Err(FanOutError::UnknownEndpoint {
            endpoint: member.via.clone(),
        });
    };
    let readable = pages
        .iter()
        .flat_map(|page| &page.prescriptions)
        .flat_map(|document| attachment_urls(&document.kept.resource))
        .all(|url| binary(&base, url).is_some());
    if !readable {
        return Ok(Held::Unusable {
            status: StatusCode::OK,
            reason: Unread::Attachment,
        });
    }
    Ok(documents)
}

/// The logical id of the member's `Binary` that `url`, an attachment URL of
/// one of its `DocumentReference`s, names under its base `base`: the read
/// `Binary/{id}`, relative to the base or absolute under it, with no query
/// or fragment, or `None` for any other URL.
pub(crate) fn binary(base: &Url, url: &str) -> Option<String> {
    let mut root = base.clone();
    if !root.path().ends_with('/') {
        root.set_path(&format!("{}/", root.path()));
    }
    let named = root.join(url).ok()?;
    if named.query().is_some() {
        return None;
    }
    let under = Next::under(base, &named).ok()?;
    let id = under.path().strip_prefix("/Binary/")?;
    logical(id).then(|| id.to_owned())
}

/// Whether `id` is a FHIR R4 `id`, `[A-Za-z0-9\-\.]{1,64}`, other than a dot
/// segment, so it names one resource in a path.
fn logical(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
        && id != "."
        && id != ".."
}

/// What every member the plan of `located` dispatches to, but those of
/// `own`, answered the laboratory report query for the patient
/// `system`|`value` ([`ferrofed_eehrxf::laboratory`]), run as the
/// medication queries are.
async fn laboratory_reports(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (located, own): (&Located, &[Chosen]),
    (system, value): (&str, &str),
) -> Result<Vec<(EndpointId, Answered)>, Unsummarised> {
    let name = ferrofed_eehrxf::laboratory::name()
        .map_err(|_unnamed| Unsummarised::Internal("the laboratory query has no qualified name"))?;
    // NOTE: Federation Tier § The four identifiers, N33: a member asked at its own endpoint for
    // its laboratory reports is asked no laboratory query, so no report is listed twice.
    let members: BTreeSet<EndpointId> = located
        .targets
        .plan
        .dispatched()
        .filter(|endpoint| !own.iter().any(|member| &member.via == *endpoint))
        .cloned()
        .collect();
    if members.is_empty() {
        return Ok(Vec::new());
    }
    mapped::queried(
        federation,
        arrived,
        located,
        (
            (system, value),
            (name.as_str(), ferrofed_eehrxf::laboratory::aql()),
        ),
        &members,
    )
    .await
    .map_err(|unprescribed| match unprescribed {
        Unprescribed::Failure(failure) => Unsummarised::Failure(failure),
        _ => Unsummarised::Internal("the laboratory query could not be run"),
    })
}

/// The status `status` becomes once the laboratory report query's answers
/// `laboratory` and the members' own answers `own` are settled under
/// `completion`: a silent member fails it under all-or-nothing, `504` for
/// one in `time-out` or `offline` and `424` otherwise (§11.3, §11.4).
pub(super) fn laboratory_settled(
    (laboratory, own): (&[(EndpointId, Answered)], &[Own]),
    mut status: StatusCode,
    completion: Completion,
) -> StatusCode {
    if completion != Completion::AllOrNothing {
        return status;
    }
    let queried = laboratory.iter().map(|(_, answered)| match answered {
        Answered::Rows(_) => EndpointStatus::Active,
        Answered::Silent { status, .. } => *status,
    });
    let asked = own.iter().map(|own| own.held.status());
    for member in queried.chain(asked) {
        match member {
            EndpointStatus::Active => {}
            EndpointStatus::TimeOut | EndpointStatus::Offline => {
                status = StatusCode::GATEWAY_TIMEOUT;
            }
            _ if status == StatusCode::OK => status = StatusCode::FAILED_DEPENDENCY,
            _ => {}
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::binary;
    use url::Url;

    #[test]
    fn a_binary_under_the_members_base_is_read_by_its_id() {
        let base = Url::parse("https://member.example.org/fhir").unwrap();
        for url in [
            "Binary/lab-1",
            "https://member.example.org/fhir/Binary/lab-1",
            "./Binary/lab-1",
        ] {
            assert_eq!(Some("lab-1".to_owned()), binary(&base, url), "{url}");
        }
    }

    #[test]
    fn every_other_attachment_url_names_no_binary() {
        let base = Url::parse("https://member.example.org/fhir/").unwrap();
        for url in [
            "https://other.example.org/fhir/Binary/lab-1",
            "https://member.example.org/Binary/lab-1",
            "Binary/lab-1?_format=json",
            "Binary/lab-1#part",
            "Binary/",
            "Binary/a/b",
            "Binary/..",
            "DocumentReference/lab-1",
            "Binary/lab%2F1",
            "urn:uuid:5a1e0000-0000-4000-8000-00000000000a",
        ] {
            assert_eq!(None, binary(&base, url), "{url}");
        }
    }
}
