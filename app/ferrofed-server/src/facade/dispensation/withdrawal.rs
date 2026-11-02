// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The withdrawal of a dispensation the face received, written as an
//! update at the one member that holds it (MyHealth@EU Requirements
//! Catalogue v10.0.0 07.04, §1.15.4; FHIR R4 `http.html` §3.1.0.4).
//!
//! The dispensation is first read by the id the face named it by, as
//! [`crate::facade::prescription::read`] reads it, so the update reaches
//! the member that id names only when that member holds the dispensation
//! for the `ehr_id` the id names (N26). It is then sent to that member's own
//! endpoint once, `PUT {fhir}/MedicationDispense/{id}` by the member's own
//! id, its body as the client sent it but for its top-level `id`, which
//! names the member's own resource there (§3.1.0.4, the inverse of the
//! read's rename of §3.1.0.2). The member decides whether its state still
//! admits the withdrawal; the gateway never sends it again. How the
//! national side carries a discard is our own design: no specification
//! governs it.

use std::sync::Arc;

use axum::response::Response;
use ferrofed_eehrxf::dispensations::{self, Received};
use ferrofed_engine::hygiene::Withheld;
use http::StatusCode;
use secrecy::SecretString;

use super::answer::{Deed, Ended, Written, attach, from_member, unmade_outcome};
use super::{Sent, write};
use crate::error::Code;
use crate::facade::prescription::{self, Read};
use crate::facade::request::Arrived;
use crate::federation::Federation;

/// The operation an access record names for a withdrawal: the FHIR R4
/// `update` interaction on `MedicationDispense`.
pub(crate) const OPERATION: &str = "update MedicationDispense";

/// The status a withdrawal carries (FHIR R4 `medicationdispense-status`).
pub(crate) const WITHDRAWN: &str = "entered-in-error";

/// A withdrawal, as the answers to it name it.
const WITHDRAWAL: Deed = Deed {
    object: "the withdrawal of the dispensation",
    did: "withdrew the dispensation",
    done: "made at",
};

/// Writes the withdrawal `received`, whose text is `text`, of the one
/// dispensation `read` found, as `arrived` asks.
pub(crate) async fn withdraw(
    federation: &Arc<Federation>,
    arrived: &Arrived<'_>,
    (received, text): (&Received, &str),
    read: &Read,
) -> Response {
    let logged = arrived.outbound.to_string();
    let own = read.locator.resource();
    let Some(body) = dispensations::with_id(text, own) else {
        tracing::error!(
            request_id = logged,
            "a withdrawal checked to carry its id had none to replace"
        );
        return crate::error::fixed(Code::Internal, arrived.request_id);
    };
    // NOTE: §5.4.1, N33: the member is located by its ehr_id alone, and the patient the body
    // names is withheld from every part of the request the gateway composes.
    let withheld = Arc::new(Withheld::new(
        received
            .patient
            .iter()
            .map(|(_, value)| SecretString::from(value.clone())),
    ));
    let sent = Sent::Update {
        id: own,
        body: body.as_bytes(),
    };
    let written = write(federation, arrived, (&read.chosen, sent), withheld).await;
    answered(federation, (received, read), &written, &logged)
}

/// The answer to the withdrawal `received` of the dispensation `read`
/// found, whose update ended as `written`.
fn answered(
    federation: &Federation,
    (received, read): (&Received, &Read),
    written: &Written,
    logged: &str,
) -> Response {
    let writing = &read.chosen;
    let accessed = match written {
        Written::Sent(created) => {
            prescription::accessed_written(federation, read, (created, OPERATION))
        }
        Written::Unsent => prescription::accessed(federation, read),
    };
    let unmade = match super::answer::ended((writing, written), WITHDRAWAL, logged) {
        Ended::Made(made) => {
            let outcome = dispensations::withdrawn(
                received,
                (writing.node.as_str(), writing.native.as_str()),
            );
            let response = crate::fhir::fhir_json(StatusCode::OK, &outcome);
            let mut response = from_member(federation, writing, made, response);
            attach(&mut response, accessed);
            return response;
        }
        Ended::Unmade(unmade) => unmade,
    };
    let (status, outcome) = unmade_outcome(writing, WITHDRAWAL, &unmade);
    let mut response = crate::fhir::fhir_json(status, &outcome);
    attach(&mut response, accessed);
    response
}
