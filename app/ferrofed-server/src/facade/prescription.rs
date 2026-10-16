// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read of one electronic prescription by the id the face names it by
//! (Regulation (EU) 2025/327 Art 14(1)(b)).
//!
//! The id names the member's own endpoint, its `ehr_id` for the patient and
//! its own id of the resource ([`crate::fhir::locator`]), so the read needs
//! no state at the gateway. It is a locator alone: a caller confined to one
//! patient reaches that patient's own `{node, ehr_id}` pairs alone, a
//! resolution binding or the `ehr_id` index that places the `ehr_id` at two
//! members refuses the read with the integrity incident of N42, and one that
//! places it at another member leaves the id with no owner (§12.5.1,
//! §12.5.2). The member is then asked at its own endpoint for the one
//! prescription by its id and by its own `ehr_id` alone, `_id` beside
//! `patient.identifier` (FHIR R4 search §3.1.1.4.1, "functionally
//! equivalent to a simple read"; §5.4.1, N33), so an id pairing one
//! patient's `ehr_id` with another's prescription finds nothing (N26).
//!
//! An id the face did not write, one naming no member that serves
//! prescriptions, and a member that holds no such prescription for the
//! `ehr_id` all have no owner, and answer alike. Asking the member by a
//! search, and the answers of a member that answers two, are our own design:
//! no specification governs them.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientLookup};
use ferrofed_eehrxf::dispensations;
use ferrofed_eehrxf::prescriptions::page::Kept;
use ferrofed_engine::dispatch::DispatchOptions;
use ferrofed_engine::fanout::FanOutError;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_registry::id::EndpointId;
use http::HeaderMap;

use crate::access::{Accessed, Asked};
use crate::facade::answer::Failure;
use crate::facade::native::Chosen;
use crate::facade::prescriptions::{self, Held, Unread};
use crate::facade::request::Arrived;
use crate::facade::{confined, owner};
use crate::federation::Federation;
use crate::fhir::locator::{Locator, Unlocated};

/// What a read by the face's id reads: a prescription or a dispensation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readable {
    /// A `MedicationRequest`, of Art 14(1)(b).
    Prescription,
    /// A `MedicationDispense`, of Art 14(1)(c).
    Dispensation,
}

impl Readable {
    /// The FHIR resource type read.
    pub(crate) fn resource(self) -> &'static str {
        match self {
            Self::Prescription => ferrofed_eehrxf::prescriptions::RESOURCE,
            Self::Dispensation => dispensations::RESOURCE,
        }
    }

    /// The Art 14(1) category of what is read.
    pub(crate) fn category(self) -> Category {
        match self {
            Self::Prescription => Category::ElectronicPrescription,
            Self::Dispensation => Category::ElectronicDispensation,
        }
    }
}

/// One member asked for one resource, and what it answered.
#[derive(Debug)]
pub(crate) struct Read {
    /// What is read.
    pub(crate) readable: Readable,
    /// The member, as the read reached it.
    pub(crate) chosen: Chosen,
    /// The id the read names the prescription by.
    pub(crate) locator: Locator,
    /// What it answered.
    pub(crate) held: Held,
}

impl Read {
    /// The resource the member answered, when it answered exactly one.
    pub(crate) fn resource(&self) -> Option<&Kept> {
        let Held::Found(pages) = &self.held else {
            return None;
        };
        let mut all = pages.iter().flat_map(|page| page.prescriptions.iter());
        match (all.next(), all.next()) {
            (Some(one), None) => Some(&one.kept),
            _ => None,
        }
    }
}

/// Why the read gives no prescription.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Unfound {
    /// The request failed on the gateway's side before or while it was
    /// sent.
    #[error(transparent)]
    Failure(Failure),
    /// The id has no owner: the face did not write it, it names no member
    /// that serves prescriptions, the gateway places its `ehr_id` at another
    /// member, or the member holds no such prescription for the `ehr_id`,
    /// with what was asked when a member was.
    #[error("no prescription is known by this id")]
    NoOwner(Option<Box<Read>>),
    /// The id names more than one member endpoint.
    #[error("the id names more than one member endpoint")]
    Ambiguous(Vec<EndpointId>),
    /// A resolution binding or the `ehr_id` index places the `ehr_id` at
    /// several members, which claim it (§12.5.2, N42).
    #[error("the ehr_id is claimed by several members (§12.5.2, N42)")]
    Claimed(Vec<EndpointId>),
    /// The caller is confined to a patient whose own `ehr_id` at the member
    /// the id names is another.
    #[error("the id names an EHR beyond the caller's patient")]
    Confined,
    /// The member the id names has no endpoint its requests leave through
    /// (§11.1).
    #[error("the member endpoint {0} has no endpoint in service")]
    Offline(EndpointId),
    /// The member answered more than one prescription for one id (FHIR R4
    /// search §3.1.1.4.1: there can only be one).
    #[error("the member answered more than one prescription for one id")]
    Several(Box<Read>),
    /// The member gave no answer the read can pass through (§11.1).
    #[error("the member gave no answer the read can pass through")]
    Unanswered(Box<Read>),
}

/// Reads the prescription `id` names, as `arrived` asks, at the member of
/// `declared` it names.
///
/// # Errors
///
/// The [`Unfound`] that says why no prescription can be answered.
pub(crate) async fn read(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (readable, id): (Readable, &str),
    declared: &BTreeSet<String>,
) -> Result<Read, Unfound> {
    let logged = arrived.outbound.to_string();
    let snapshot = federation.snapshot();
    let category = readable.category();
    let code = category.code();
    let serving: Vec<EndpointId> = snapshot
        .nodes()
        .filter(|node| declared.contains(node.id().as_str()))
        .filter_map(|node| snapshot.native_for(node.id(), code))
        .map(|native| native.id().clone())
        .collect();
    let locator = Locator::read(id, &serving).map_err(|unlocated| match unlocated {
        Unlocated::Ambiguous(endpoints) => Unfound::Ambiguous(endpoints),
        Unlocated::Malformed | Unlocated::Unknown => Unfound::NoOwner(None),
    })?;
    let native = snapshot
        .native_endpoint(locator.endpoint())
        .ok_or(Unfound::NoOwner(None))?;
    let node = native.node().clone();
    let Some(via) = snapshot.asked_through(&node) else {
        return Err(Unfound::Offline(native.id().clone()));
    };
    let ehr_id = locator.ehr_id();
    // NOTE: §12.5.1: the face reads no targeting header, so the routing steps start at the
    // session's resolution bindings, and a confined grant stands as that step.
    let untargeted = HeaderMap::new();
    let located = if let Some(confinement) = arrived.conveyance.confinement() {
        if !confined::admits(arrived.conveyance, via.id(), ehr_id) {
            confined::stopped("prescription id", &logged);
            return Err(Unfound::Confined);
        }
        let holders = confined::holders(snapshot, confinement, ehr_id);
        owner::located_within(snapshot, &untargeted, &holders)
    } else {
        let held = arrived.session.map(|session| owner::Held {
            bindings: federation.bindings(),
            session,
            now: Instant::now(),
        });
        owner::located(snapshot, &untargeted, held, federation.index(), ehr_id)
    };
    match located {
        Ok(owner::Located::Collision(claimed)) => {
            owner::collided(ehr_id, claimed.detection, &claimed.claimants);
            return Err(Unfound::Claimed(claimed.claimants));
        }
        Ok(owner::Located::At { endpoint, step }) if endpoint.node() != &node => {
            tracing::info!(
                endpoint = %native.id(),
                step = step.as_str(),
                request_id = logged,
                "a prescription id names a member the gateway places its ehr_id elsewhere from"
            );
            return Err(Unfound::NoOwner(None));
        }
        // NOTE: §12.5.1: an index or binding naming the id's member, or none, leaves the member
        // the id names to answer for itself (N26).
        Ok(_) => {}
        // NOTE: §8.4.1: only a targeting header refuses a step, and the face sends none, so
        // this names no owner and never a refusal of the caller's.
        Err(_untargeted) => return Err(Unfound::NoOwner(None)),
    }
    let chosen = Chosen {
        via: via.id().clone(),
        node,
        native: native.id().clone(),
        ehr_id: ehr_id.clone(),
    };
    let held = asked(federation, arrived, (&chosen, readable, locator.resource())).await?;
    let read = Read {
        readable,
        chosen,
        locator,
        held,
    };
    settled(federation, read)
}

/// The read `read` settles to, from what its member answered.
fn settled(federation: &Federation, mut read: Read) -> Result<Read, Unfound> {
    let Held::Found(pages) = &read.held else {
        return Err(Unfound::Unanswered(Box::new(read)));
    };
    let asked = read.locator.resource();
    let all = || pages.iter().flat_map(|page| page.prescriptions.iter());
    let matches = all().count();
    let other = all().any(|prescription| prescription.kept.id() != Some(asked));
    if other {
        read.held = Held::Unusable {
            status: http::StatusCode::OK,
            reason: Unread::Other,
        };
        return Err(Unfound::Unanswered(Box::new(read)));
    }
    match matches {
        0 => Err(Unfound::NoOwner(Some(Box::new(read)))),
        1 => {
            owner::learn(federation.index(), read.locator.ehr_id(), &read.chosen.node);
            Ok(read)
        }
        _ => Err(Unfound::Several(Box::new(read))),
    }
}

/// Asks the member `chosen` at its own endpoint for the prescription of
/// the member's own id `resource`, under the budget of the request
/// `arrived`.
async fn asked(
    federation: &Federation,
    arrived: &Arrived<'_>,
    (chosen, readable, resource): (&Chosen, Readable, &str),
) -> Result<Held, Unfound> {
    let budget = federation.budget();
    let overall = arrived
        .started
        .checked_add(budget.overall())
        .ok_or(Unfound::Failure(Failure::FanOut(FanOutError::Clock)))?;
    let per_node = Instant::now()
        .checked_add(budget.per_node())
        .map_or(overall, |at| at.min(overall));
    // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond
    // what it reads of every request, and the member is located by its ehr_id alone.
    let options = DispatchOptions::new(per_node, arrived.conveyance.clone())
        .with_request_id(arrived.outbound)
        .with_withheld(Arc::new(Withheld::none()));
    let until = tokio::time::Instant::from_std(overall);
    prescriptions::paged(
        federation,
        (chosen, readable.resource(), Some(resource)),
        &options,
        until,
    )
    .await
    .map_err(|failure| Unfound::Failure(Failure::FanOut(failure)))
}

/// The access record's facts of the read `read`, or `None` when no request
/// left the gateway or the federation keeps no access log (Annex II 3.2).
///
/// The read serves the electronic prescription category by construction
/// (Art 14(1)(b)), its member is its one origin, contributing when it
/// answered the one prescription, and the record gate names the patient
/// behind the `ehr_id`.
pub(crate) fn accessed(federation: &Federation, read: &Read) -> Option<Accessed> {
    recorded(federation, read, read.resource().is_some())
}

/// The access record's facts of the read `read` whose one resource the
/// face withholds, a version read whose member holds another version, so
/// the member contributed nothing and nothing was delivered.
pub(crate) fn accessed_withheld(federation: &Federation, read: &Read) -> Option<Accessed> {
    recorded(federation, read, false)
}

/// The access record's facts of the read `read`, its member contributing
/// when `contributed`.
fn recorded(federation: &Federation, read: &Read, contributed: bool) -> Option<Accessed> {
    let log = federation.access_log()?;
    if !read.held.left() {
        return None;
    }
    let evidence =
        || Evidence::reached(Basis::Returned, Vec::new()).constructed(read.readable.category());
    let chosen = &read.chosen;
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Read,
        operation: read.readable.resource(),
        resource: Some(format!(
            "{}/{}",
            read.readable.resource(),
            read.locator.resource()
        )),
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: None,
            ehrs: vec![EhrAt {
                endpoint: chosen.native.as_str().to_owned(),
                ehr_id: chosen.ehr_id.as_str().to_owned(),
                patient: PatientLookup::RequestNamed,
            }],
            population: None,
        },
        evidence: evidence(),
        delivered: Some(usize::from(contributed)),
        origins: vec![Asked {
            endpoint: chosen.native.as_str().to_owned(),
            node: Some(chosen.node.as_str().to_owned()),
            system_id: federation
                .snapshot()
                .node(&chosen.node)
                .map(|node| node.system_id().as_str().to_owned()),
            status: read.held.status().as_str().to_owned(),
            rows: Some(read.held.prescriptions()),
            version: None,
            created_ehr: None,
            contributed,
            evidence: contributed.then(evidence),
        }],
        consent_set_aside: BTreeSet::new(),
        inserted: None,
        transformations: Vec::new(),
    })
}
