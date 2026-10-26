// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient of a request on the FHIR face, located once over the
//! members.
//!
//! The patient is named by an identifier with its system, bound into the
//! patient summary's section queries through `$patient` and `$namespace`
//! alone ([`ferrofed_eehrxf::patient_summary`]), and the first of them
//! carries it through confinement, localization, the consent pre-filter and
//! resolution, as a stored query's patient is carried (§7.1). Every request
//! of the face that reaches a member for one patient starts here: the
//! summary, the document list and the prescription search. No
//! specification governs the use of a section query as the carrier: our own
//! design.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::section_query::{ISSUER, PATIENT};
use ferrofed_engine::fanout::{Completion, FanOutError};
use http::HeaderMap;
use openehr_federation::aql::Analysis;
use openehr_federation::dedup::DedupMode;
use openehr_its::rest::generated::query::AdhocQueryExecute;

use crate::facade::answer::{self, Failure};
use crate::facade::request::{self, Arrived, Submitted};
use crate::facade::summary::Unsummarised;
use crate::facade::{completeness, intake, plan};
use crate::federation::Federation;

/// The patient of a request on the FHIR face, resolved once over the
/// members.
///
/// It holds the completeness the request asks, the plan over the members
/// the patient resolved at, the section queries bound to the patient, and
/// whether a consent decision may be disclosed to the caller.
pub(crate) struct Located {
    /// The completeness the request asks (§11.3).
    pub(crate) completion: Completion,
    /// The plan over the members the patient resolved at.
    pub(crate) targets: plan::Targets,
    /// Every section query, bound to the patient.
    pub(crate) analyses: Vec<(Section, Analysis)>,
    /// Whether a consent decision may be disclosed to the caller.
    pub(crate) disclosed: bool,
}

/// Localizes, checks against the consent pre-filter and resolves the
/// patient `system`|`value` once, as `arrived` asks.
///
/// The request is confined to the caller's grant before anything is asked,
/// and held within it after (§13.1, N24).
///
/// # Errors
///
/// The [`Unsummarised`] that says why the patient cannot be located,
/// [`Failure::NoDestination`] among them when no member holds it or none
/// may say so (§11.2, §11.3).
pub(crate) async fn located(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (system, value): (&str, &str),
) -> Result<Located, Unsummarised> {
    let logged = arrived.outbound.to_string();
    let completion =
        completeness::of(arrived.headers, federation.best_effort()).map_err(Failure::from)?;
    let deadline = arrived
        .started
        .checked_add(federation.budget().overall())
        .ok_or(Failure::FanOut(FanOutError::Clock))?;
    let analyses = analysed(federation, completion, (system, value), &logged)?;
    let (_, first) = analyses
        .first()
        .ok_or(Unsummarised::Internal("no section query"))?;
    answer::confinement::confine(
        federation,
        (arrived.conveyance, arrived.on_behalf),
        first,
        (deadline, &logged),
    )
    .await?;
    let disclosed = federation.discloses_consent_to(arrived.conveyance);
    let (targets, _) = answer::planning::targeted(
        federation,
        (first, plan::Selection::Undirected),
        (arrived.requester, arrived.on_behalf),
        (
            deadline,
            disclosed,
            federation.sets_aside_consent_for(arrived.conveyance),
        ),
    )
    .await?;
    answer::confinement::held_within(federation, arrived.conveyance, (first, &targets), &logged)?;
    if targets.plan.has_no_destination() {
        return Err(Failure::NoDestination.into());
    }
    answer::planning::remember(federation, arrived.session, &targets);
    Ok(Located {
        completion,
        targets,
        analyses,
        disclosed,
    })
}

/// Every section query, read and analysed as a stored query bound to the
/// patient `system`|`value`, under `completion`.
fn analysed(
    federation: &Federation,
    completion: Completion,
    (system, value): (&str, &str),
    logged: &str,
) -> Result<Vec<(Section, Analysis)>, Unsummarised> {
    // NOTE: §8.4; the FHIR face names no endpoint, so no targeting header is read.
    let undirected = HeaderMap::new();
    let mut analyses = Vec::with_capacity(Section::ALL.len());
    for section in Section::ALL {
        let name = section
            .name()
            .map_err(|_unnamed| Unsummarised::Internal("a section query has no qualified name"))?;
        let request = AdhocQueryExecute {
            q: section.aql(),
            offset: None,
            fetch: None,
            query_parameters: Some(intake::strings([(PATIENT, value), (ISSUER, system)])),
            additional_properties: BTreeMap::new(),
        };
        let submitted = Submitted::Stored {
            request,
            name: name.as_str(),
        };
        let (_, _, parsed) = request::read(
            federation,
            (submitted, &undirected),
            (completion, DedupMode::None),
            logged,
        )?;
        analyses.push((section, parsed));
    }
    Ok(analyses)
}
