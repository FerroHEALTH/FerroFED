// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a node is told of a contact point's caller (§13.1, N24; §13.4
//! authn-end-user): every attribute of Implementing Regulation (EU)
//! 2026/2099 Annex Tables 1 and 2 under `national_contact_point`, marked as
//! asserted by the contact point, in the conveyance and nowhere else; and
//! never the IHE IUA `person_id` the token carries (§5.4.1, N33).

use crate::auth::{Gateway, TestResult, assert_admitted, bearing, minted, query};
use crate::conveyance::{published, verified};
use crate::facade::wire;
use crate::support::ISSUER;
use crate::support::conveyed::{
    ConveyedContactPoint, ConveyedProfessional, ConveyedProvider, ConveyedPurpose,
};

use super::{
    COUNTRY, FAMILY, GIVEN, HCP_ADDRESS, HCP_AUTHORITY, HCP_ID, HCP_NAME, HP_AUTHORITY, HP_ID,
    PERSON_ID, ROLE, ROLE_SYSTEM, declared, relaying,
};

/// The `national_contact_point` claim the test contact point's caller
/// conveys: every Annex attribute under its data identifier, asserted by
/// the contact point.
fn expected() -> ConveyedContactPoint {
    ConveyedContactPoint {
        asserted_by: ISSUER.to_owned(),
        health_professional: ConveyedProfessional {
            family_name: FAMILY.to_owned(),
            given_name: GIVEN.to_owned(),
            country_code: COUNTRY.to_owned(),
            hp_identifier: HP_ID.to_owned(),
            issuing_authority_name: HP_AUTHORITY.to_owned(),
            hp_professional_role: vec![ConveyedPurpose {
                system: Some(ROLE_SYSTEM.to_owned()),
                code: ROLE.to_owned(),
            }],
            healthcare_provider_identifier: HCP_ID.to_owned(),
        },
        healthcare_provider: ConveyedProvider {
            healthcare_provider_identifier: HCP_ID.to_owned(),
            issuing_authority_name: HCP_AUTHORITY.to_owned(),
            healthcare_provider_name: HCP_NAME.to_owned(),
            healthcare_provider_address: HCP_ADDRESS.to_owned(),
        },
    }
}

/// 2026/2099 Art 7; §13.4 authn-end-user, N24: each node is told the
/// relayed professional and provider, marked as the contact point's
/// assertion, in the conveyance it verifies by the gateway's published key.
// conformance: CP-16
#[tokio::test]
async fn each_node_is_told_every_annex_attribute_marked_as_asserted() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let keys = published(&gateway.app).await?;
    assert_admitted(&gateway, bearing(query()?, &minted(&relaying())?)?).await?;
    for (node, endpoint) in [(&gateway.a, "node-a-pub"), (&gateway.b, "node-b-pub")] {
        let requests = node.received_requests().await.ok_or("recording is on")?;
        let [request] = requests.as_slice() else {
            return Err(format!("one query at {endpoint}").into());
        };
        let token = request
            .headers
            .get(ferrofed_engine::conveyance::HEADER)
            .ok_or("the conveyance")?
            .to_str()?;
        let read = verified(token, &keys, endpoint)?;
        assert_eq!(Some(expected()), read.national_contact_point, "{endpoint}");
        for (name, value) in &request.headers {
            if name.as_str() != ferrofed_engine::conveyance::HEADER.to_ascii_lowercase() {
                let value = value.to_str().unwrap_or_default();
                for relayed in [FAMILY, HP_AUTHORITY, HCP_ADDRESS] {
                    assert!(
                        !value.contains(relayed),
                        "{name} carries no relayed attribute: {value}"
                    );
                }
            }
        }
    }
    Ok(())
}

/// N33, §5.4.1: the IUA `person_id` a contact point's token carries is a
/// patient identifier, never read, so no byte of it reaches any node, in the
/// conveyance or any other part of the request.
// conformance: CP-26 track-10
#[tokio::test]
async fn no_iua_person_id_reaches_a_node() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    assert_admitted(&gateway, bearing(query()?, &minted(&relaying())?)?).await?;
    for node in [&gateway.a, &gateway.b] {
        let sent = wire(node).await?;
        assert!(!sent.is_empty(), "the node was asked");
        assert!(!sent.contains(PERSON_ID), "N33: person_id reached a node");
        assert!(!sent.contains("person_id"), "N33: no claim names it");
    }
    Ok(())
}
