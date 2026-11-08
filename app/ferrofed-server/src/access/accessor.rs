// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The accessor an access record names: the verified caller, and for a
//! caller a national contact point vouched for, the professional and the
//! provider it relays (Regulation (EU) 2025/327 Annex II 3.1, 3.2(a), (b);
//! Implementing Regulation (EU) 2026/2099 Art 7).

use ehds_logging::record::{Accessor, IssuedId, Professional, Purpose};
use ferrofed_engine::conveyance::caller::Acting;
use ferrohealth_smart::assurance::AssuranceLevel;

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
/// of them, marked as asserted by the contact point (Implementing
/// Regulation (EU) 2026/2099 Art 7), and the provider it acts for by its
/// identifier under its issuing authority.
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
        acts_for: acts_for(caller),
        // NOTE: Annex II 3.1; the gate refuses patient data to a client acting with no
        // person (`natural-person-required`), so no record is of a system client's commit.
        facilities: Vec::new(),
    }
}

/// The healthcare provider `caller` acts for, by its identifier under its
/// issuer (Annex II 3.2(a)), when the token names both: the provider a
/// contact point relays, or the URA of a Dutch requester.
///
/// The IHE IUA `subject_organization_id` names the organisation by one
/// identifier with no issuer apart, so the record holds it at `provider`
/// alone.
fn acts_for(caller: &Caller) -> Option<IssuedId> {
    if let Some(relayed) = caller.relayed() {
        return Some(IssuedId {
            issuer: relayed.provider.issuing_authority_name.clone(),
            id: relayed.provider.identifier.clone(),
        });
    }
    ura(caller)
}

/// The URA of a Dutch requester, under the root OID of the URA register,
/// the one `nl-generic-functions` names a care provider by to Mitz.
#[cfg(feature = "binding-nl")]
fn ura(caller: &Caller) -> Option<IssuedId> {
    caller.requester().map(|requester| IssuedId {
        issuer: format!("urn:oid:{}", nl_generic_functions::mitz::URA_ROOT),
        id: requester.organisation().to_owned(),
    })
}

/// A build without the Dutch binding reads no URA register.
#[cfg(not(feature = "binding-nl"))]
fn ura(_caller: &Caller) -> Option<IssuedId> {
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::caller::{Stated, VerifiedBy};

    /// A verified caller whose IUA `subject_organization_id` is `organisation`.
    fn caller(organisation: Option<&str>) -> Caller {
        Caller::new(
            Stated {
                issuer: "https://as.example.test".to_owned(),
                subject: "synthetic-person".to_owned(),
                client_id: "synthetic-client".to_owned(),
                organisation: organisation.map(str::to_owned),
                granted: String::new(),
                purposes: Vec::new(),
            },
            VerifiedBy::Signature,
        )
    }

    /// Annex II 3.2(a): a Dutch requester's provider is recorded as acted for
    /// by its URA under the URA register's root.
    #[cfg(feature = "binding-nl")]
    #[test]
    fn a_dutch_requester_acts_for_its_ura() {
        let requester = ferrofed_identity::role::consent::Requester::new(
            "synthetic-uzi".to_owned(),
            "01.015".to_owned(),
            "00000001".to_owned(),
            "V4".to_owned(),
        );
        let accessor = accessor(&caller(None).with_requester(requester));
        let acts_for = accessor.acts_for.as_ref();
        assert_eq!(
            acts_for.map(|id| (id.issuer.as_str(), id.id.as_str())),
            Some(("urn:oid:2.16.528.1.1007.3.3", "00000001"))
        );
    }

    /// An IUA organisation names no issuer apart, so it is recorded as the
    /// provider and acted for by no issued identifier.
    #[test]
    fn an_iua_organisation_is_the_provider_alone() {
        let accessor = accessor(&caller(Some("urn:oid:2.999.7")));
        assert_eq!(accessor.provider.as_deref(), Some("urn:oid:2.999.7"));
        assert!(accessor.acts_for.is_none());
        assert_eq!(0, accessor.facilities.len());
    }
}
