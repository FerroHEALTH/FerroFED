// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Who may dispense through a national contact point, and who may discard
//! what was dispensed.
//!
//! eHDSI SAML Profile 10.0.0 §2.5 has an identity assertion that carries
//! permissions carry at least `PPD-046` for a notification on dispensation
//! and on its discard; a token that asserts none is not held to it. A
//! contact point's dispensation the owner created is recorded with its
//! dispenser ([`crate::store::dispensed`]): the identity assertion's
//! `organization-id` and the facility the dispensation names on
//! `performer.actor` (HL7 Europe MPD 1.0.0). A contact point's discard is
//! then admitted only within 24 hours of the dispensation and from that
//! dispenser, matched on both identifiers (MyHealth@EU Requirements
//! Catalogue 10.0.0 07.04; `NCPeH` Components Specification 10.0.0
//! §3.1.6.4.2, "issued by the same HCPO"). A dispensation with no facility
//! identifier, or one the face could name no id of, is recorded nowhere, so
//! no discard of it is admitted. Every other caller is held to none of
//! this; the member decides. A dispensation without `PPD-046` is refused
//! `ERROR_SEC_GENERIC` and a refused discard `ERROR_ED_DISCARD_FAILED`
//! (Requirements Catalogue 10.0.0 §1.25), each `403` before the member is
//! asked; that both are `403` is our own design.

use axum::response::Response;
use ferrofed_eehrxf::dispensations::Received;
use http::StatusCode;

use crate::auth::caller::Caller;
use crate::auth::contact_point;
use crate::error::Code;
use crate::error::ncp::NcpError;
use crate::facade::dispensation::Dispensed;
use crate::facade::prescription::Read;
use crate::fhir::trc::refused;
use crate::state::AppState;
use crate::store::dispensed::{Dispensation, Dispenser};

/// The permission a dispensation and its discard need (eHDSI SAML Profile
/// 10.0.0 §2.5: "Notification on Dispensation", "Notification on
/// Dispensation Discard").
pub(crate) const PERMISSION: &str = "PPD-046";

/// The refusal code of a contact point's write whose token asserts
/// permissions without [`PERMISSION`].
pub(crate) const UNPERMITTED: &str = "permission-missing";

/// The refusal code of a contact point's discard that is not admitted.
pub(crate) const DISCARD_REFUSED: &str = "discard-refused";

/// Holds a contact point's dispensation to [`PERMISSION`]; every other
/// caller passes.
///
/// # Errors
///
/// `403` [`UNPERMITTED`] with `ERROR_SEC_GENERIC`.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn may_dispense(caller: &Caller) -> Result<(), Response> {
    let Some(external) = caller.asserted() else {
        return Ok(());
    };
    if contact_point::permits(external, PERMISSION) == Some(false) {
        return Err(refused(
            StatusCode::FORBIDDEN,
            (UNPERMITTED, NcpError::of(Code::OperationRefused)),
            &format!(
                "{UNPERMITTED}: the identity assertion carries permissions without {PERMISSION}, which a dispensation needs (eHDSI SAML Profile 10.0.0 §2.5)"
            ),
        ));
    }
    Ok(())
}

/// Records who dispensed `received`, when `caller` is a contact point and
/// the owner created it as `dispensed` names, so its discard can be held to
/// its dispenser.
///
/// A dispensation that names no facility, that the face names no id of, or
/// that the store fails to record is logged and answered as made, since
/// the owner holds it; no discard of it is then admitted.
pub(super) async fn record(
    state: &AppState,
    caller: &Caller,
    (received, dispensed): (&Received, Option<&Dispensed>),
    logged: &str,
) {
    let (Some(relayed), Some(Dispensed(id))) = (caller.relayed(), dispensed) else {
        return;
    };
    let Some(facility) = received.dispenser.clone() else {
        tracing::warn!(
            request_id = logged,
            "a national contact point's dispensation names no dispensing facility, so no discard of it will be admitted"
        );
        return;
    };
    let Some(store) = state.store() else {
        tracing::error!(
            request_id = logged,
            "a national contact point's dispensation was made, but the gateway runs with no store, so no discard of it will be admitted"
        );
        return;
    };
    let who = Dispenser {
        organization: relayed.provider.identifier.clone(),
        facility,
    };
    if let Err(failure) = store.record_dispenser(&Dispensation::of(id), &who).await {
        tracing::error!(
            request_id = logged,
            error = %crate::chain(&failure),
            "a national contact point's dispensation was made, but its dispenser could not be recorded, so no discard of it will be admitted"
        );
    }
}

/// Holds a contact point's discard `received` of the dispensation `read`
/// found to its permission, its window and its dispenser; every other
/// caller passes.
///
/// # Errors
///
/// `403` [`DISCARD_REFUSED`] with `ERROR_ED_DISCARD_FAILED`, and the
/// internal error when the store cannot be asked.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) async fn may_discard(
    state: &AppState,
    caller: &Caller,
    (received, read): (&Received, &Read),
    (request_id, logged): (&str, &str),
) -> Result<(), Response> {
    let (Some(relayed), Some(external)) = (caller.relayed(), caller.asserted()) else {
        return Ok(());
    };
    if contact_point::permits(external, PERMISSION) == Some(false) {
        return Err(discard_refused(&format!(
            "the identity assertion carries permissions without {PERMISSION}, which a discard needs (eHDSI SAML Profile 10.0.0 §2.5)"
        )));
    }
    let Some(store) = state.store() else {
        tracing::error!(
            request_id = logged,
            "a national contact point's discard cannot be checked: the gateway runs with no store, so it is refused"
        );
        return Err(crate::error::fixed(Code::Internal, request_id));
    };
    let Some(id) = read.locator.id() else {
        return Err(discard_refused(NOT_RECORDED));
    };
    let recorded = match store.dispenser(&Dispensation::of(&id)).await {
        Ok(recorded) => recorded,
        Err(failure) => {
            tracing::error!(
                request_id = logged,
                error = %crate::chain(&failure),
                "a national contact point's discard cannot be checked: the store did not answer, so it is refused"
            );
            return Err(crate::error::fixed(Code::Internal, request_id));
        }
    };
    let Some(recorded) = recorded else {
        return Err(discard_refused(NOT_RECORDED));
    };
    // NOTE: MyHealth@EU RC 10.0.0 07.04: a discard comes from the point of care that dispensed,
    // matched by the assertion's organization-id and the facility on performer.actor.
    let same = recorded.organization == relayed.provider.identifier
        && received.dispenser.as_ref() == Some(&recorded.facility);
    if !same {
        return Err(discard_refused(
            "the discard comes from another healthcare provider organisation or facility than the dispensation (MyHealth@EU Requirements Catalogue 10.0.0 07.04)",
        ));
    }
    Ok(())
}

/// Why a discard of a dispensation with no live record is refused: none
/// was written by a contact point, or its 24 hours have passed.
const NOT_RECORDED: &str = "the dispensation was not written through a national contact point within the last 24 hours, so its discard is not admitted (MyHealth@EU Requirements Catalogue 10.0.0 07.04)";

/// The `403` of a refused discard, for the reason `why`.
fn discard_refused(why: &str) -> Response {
    refused(
        StatusCode::FORBIDDEN,
        (DISCARD_REFUSED, Some(NcpError::ED_DISCARD_FAILED)),
        &format!("{DISCARD_REFUSED}: {why}"),
    )
}
