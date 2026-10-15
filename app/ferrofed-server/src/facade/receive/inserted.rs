// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mark of a document a person, or their representative, inserts in the
//! person's own EHR (Regulation (EU) 2025/327 Art 4(2), Art 5).
//!
//! Art 5 asks that what a natural person or their representative inserts
//! "be clearly distinguishable as having been inserted by the natural person
//! or by his or her representative". A caller whose grant is confined to
//! their own record and who acts in person inserts as the record's subject,
//! a `PARTY_SELF`. When the caller's token names a representative, the
//! current actor of its RFC 8693 §4.1 `act` claim, which its issuer declares
//! one, each entry names that representative as a `PARTY_IDENTIFIED` with
//! the one identifier the token states for them: the `act` `sub`, issued by
//! the `act` `iss`, or the token's own `iss` when the claim names none. The
//! gateway reads who acts only from the token, and a representative named by
//! the patient's own identifier, or by the token's own `sub`, is refused, so
//! the provider names nothing of the patient.

use axum::response::Response;
use eehrxf::receive::openehr::Representative;
use ehds_logging::record::Insertion;
use ferrofed_eehrxf::receive::Prepared;
use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::{Acting, Principal};
use http::StatusCode;
use openehr_rm::v1_2::data_types::basic::dv_identifier::DvIdentifier;
use secrecy::{ExposeSecret, SecretString};

use crate::auth::caller::{Actor, Caller};
use crate::facade::receive::refusal::{self, refused};

/// Marks the composition of `prepared` as inserted by who acts behind
/// `caller`, when the caller under `conveyance` is a natural person whose
/// grant is confined to their own record, and returns it with who inserted
/// it; `withheld` holds every identifier of the document's patient.
///
/// # Errors
///
/// The [`Response`] of a composition that cannot carry the mark, or of a
/// representative the token names by nothing, by the patient's identifier
/// or by its own `sub`, with nothing sent to any member.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
pub(super) fn mark(
    mut prepared: Prepared,
    (conveyance, caller): (&Conveyance, &Caller),
    withheld: &[SecretString],
    logged: &str,
) -> Result<(Prepared, Option<Insertion>), Response> {
    if !in_person(conveyance) {
        return Ok((prepared, None));
    }
    let (marked, inserted) = match caller.representative() {
        None => (
            prepared.composition.inserted_by_subject(),
            Insertion::Subject,
        ),
        Some(actor) => {
            let representative = representative(actor, caller, withheld, logged)?;
            (
                prepared
                    .composition
                    .inserted_by_representative(&representative),
                Insertion::Representative,
            )
        }
    };
    prepared.composition = marked.map_err(|unmarked| refusal::unmarked(&unmarked, logged))?;
    Ok((prepared, Some(inserted)))
}

/// Whether the caller under `conveyance` is a natural person whose grant is
/// confined to their own record: the grant a person, or their
/// representative, inserts information in the person's EHR with
/// (Regulation (EU) 2025/327 Art 5).
fn in_person(conveyance: &Conveyance) -> bool {
    conveyance.confinement().is_some()
        && matches!(
            conveyance.principal(),
            Principal::Caller(caller) if caller.acting == Acting::Person
        )
}

/// The representative `actor` names, identified as `caller`'s token states
/// them.
///
/// # Errors
///
/// The [`Response`] of an actor named by nothing, or by a value that names
/// the patient: one of `withheld`, or the token's own `sub`.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
fn representative(
    actor: &Actor,
    caller: &Caller,
    withheld: &[SecretString],
    logged: &str,
) -> Result<Representative, Response> {
    let names_patient = actor.subject.as_deref().is_some_and(|subject| {
        subject == caller.subject()
            || withheld
                .iter()
                .any(|identifier| identifier.expose_secret() == subject)
    });
    // NOTE: Federation Tier §5.4.1, N33, RM Release 1.1.0 Common IM PARTY_IDENTIFIED: a
    // party other than the subject never carries the patient's identifier.
    if names_patient {
        tracing::warn!(
            request_id = logged,
            "a token named its representative by an identifier of the patient, so the document was refused"
        );
        return Err(refused(
            StatusCode::FORBIDDEN,
            "forbidden",
            "the token names the person's representative by an identifier of the person",
        ));
    }
    // NOTE: RFC 7519 §4.1.2, a `sub` is unique "in the context of the issuer"; an `act` with no
    // `iss` names its actor in the context of the token's own issuer.
    let identifier = actor.subject.clone().map(|id| DvIdentifier {
        issuer: Some(
            actor
                .issuer
                .clone()
                .unwrap_or_else(|| caller.issuer().to_owned()),
        ),
        assigner: None,
        id,
        r#type: None,
    });
    Representative::new(None, identifier).ok_or_else(|| {
        refused(
            StatusCode::FORBIDDEN,
            "forbidden",
            "the token states that a representative acts and names them by nothing",
        )
    })
}
