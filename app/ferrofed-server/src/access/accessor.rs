// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The accessor an access record names: the verified caller, and for a
//! caller a national contact point vouched for, the professional and the
//! provider it relays (Regulation (EU) 2025/327 Annex II 3.1, 3.2(a), (b);
//! Implementing Regulation (EU) 2026/2099 Art 7).

use ehds_logging::record::{Accessor, Professional, Purpose};
use ferrofed_engine::conveyance::caller::{Acting, AssuranceLevel};

use crate::auth::caller::Caller;
use crate::auth::contact_point;

/// The accessor `caller` is: the person and the client the verified token
/// names, the provider it acts for, who acts, the professional and the
/// assurance level as authentication verified them, and its purposes of use
/// (Annex II 3.1, 3.2(a), (b)).
///
/// For a caller a national contact point vouched for, the client is the
/// contact point's connector, and the record also names the professional
/// and the provider of another Member State it relays, with their
/// country and issuing authorities and what the identity assertion states
/// of them, marked as asserted by the contact point and with no name or
/// address (Implementing Regulation (EU) 2026/2099 Art 7; GDPR
/// Art 5(1)(c)).
pub(super) fn accessor(caller: &Caller) -> Accessor {
    let requester = caller.requester();
    Accessor {
        issuer: Some(caller.issuer().to_owned()),
        subject: caller.subject().to_owned(),
        client_id: Some(caller.client_id().to_owned()),
        audience: caller.audience().map(str::to_owned),
        provider: requester
            .map(|requester| requester.organisation().to_owned())
            .or_else(|| caller.organisation().map(str::to_owned)),
        // NOTE: Annex II 3.2(b), no specification governs the mapping: our own design; only
        // a caller authentication verified as a natural person is recorded as one.
        acting: Some(if caller.acting() == Acting::Person {
            ehds_logging::record::Acting::Person
        } else {
            ehds_logging::record::Acting::Client
        }),
        roles: Vec::new(),
        token_id: None,
        // NOTE: Annex II 3.2(a); a verified caller carries no data permit or other policy
        // beside its token, so none is recorded.
        policies: Vec::new(),
        assurance: caller.assurance().map(|level| match level {
            AssuranceLevel::Low => ehds_logging::record::AssuranceLevel::Low,
            AssuranceLevel::Substantial => ehds_logging::record::AssuranceLevel::Substantial,
            AssuranceLevel::High => ehds_logging::record::AssuranceLevel::High,
        }),
        professional: Professional {
            name: caller.professional().name.clone(),
            identifier: caller.professional().identifier.clone(),
        },
        alt_id: requester.map(|requester| requester.professional().to_owned()),
        purposes: caller
            .purposes()
            .iter()
            .map(|purpose| Purpose {
                system: purpose.system.clone(),
                code: purpose.code.clone(),
            })
            .collect(),
        relayed: relayed(caller),
        // NOTE: no specification governs this: our own design; the gateway binds no
        // caller to a party and reads no actor chain or representation into the record.
        binding: None,
        actors: Vec::new(),
        represents: None,
    }
}

/// What a contact point relays for `caller`, as the access record names
/// it.
#[expect(
    clippy::expect_used,
    reason = "the gate admits a contact point's caller only with what contact_point::record accepts, so the record holds the same relayed values"
)]
fn relayed(caller: &Caller) -> Option<ehds_logging::record::Relayed> {
    let (relayed, asserted) = caller.relayed().zip(caller.asserted())?;
    Some(
        contact_point::record(relayed, asserted)
            .expect("the gate should have refused what the access record refuses"),
    )
}
