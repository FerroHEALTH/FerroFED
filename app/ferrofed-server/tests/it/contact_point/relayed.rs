// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a node is told of a contact point's caller (§13.1, N24; §13.4
//! authn-end-user): the Implementing Regulation (EU) 2026/2099 Annex
//! attributes the eHDSI identity assertion carries, under
//! `national_contact_point`, marked as asserted by the contact point, in the
//! conveyance and nowhere else; never a name or an address (GDPR
//! Art 5(1)(c)); and never the IHE IUA `person_id` the token carries
//! (§5.4.1, N33).

use crate::auth::{Gateway, TestResult, assert_admitted, query};
use crate::conveyance::{published, verified};
use crate::facade::wire;
use crate::support::conveyed::{
    ConveyedContactPoint, ConveyedProfessional, ConveyedProvider, ConveyedPurpose,
};

use super::{
    COUNTRY, HCP_AUTHORITY, HCP_ID, HCP_NAME, HP_AUTHORITY, HP_ID, HP_NAME, NCP_ISSUER, PERSON_ID,
    ROLE, ROLE_SYSTEM, declared, presenting, relaying,
};

/// The `national_contact_point` claim the test contact point's caller
/// conveys: each Annex attribute the assertion carries under its data
/// identifier, asserted by the contact point.
fn expected() -> ConveyedContactPoint {
    ConveyedContactPoint {
        asserted_by: NCP_ISSUER.to_owned(),
        health_professional: ConveyedProfessional {
            country_code: COUNTRY.to_owned(),
            hp_identifier: HP_ID.to_owned(),
            issuing_authority_name: HP_AUTHORITY.to_owned(),
            hp_professional_role: vec![ConveyedPurpose {
                system: Some(ROLE_SYSTEM.to_owned()),
                code: ROLE.to_owned(),
            }],
            on_behalf_of: None,
            healthcare_provider_identifier: HCP_ID.to_owned(),
        },
        healthcare_provider: ConveyedProvider {
            healthcare_provider_identifier: HCP_ID.to_owned(),
            issuing_authority_name: HCP_AUTHORITY.to_owned(),
        },
    }
}

/// 2026/2099 Art 7; §13.4 authn-end-user, N24: each node is told the
/// relayed professional and provider, marked as the contact point's
/// assertion, in the conveyance it verifies by the gateway's published key,
/// and no name the assertion carries reaches it anywhere (GDPR
/// Art 5(1)(c)).
// conformance: CP-16
#[tokio::test]
async fn each_node_is_told_the_asserted_attributes_and_no_name() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let keys = published(&gateway.app).await?;
    assert_admitted(&gateway, presenting(query()?, &relaying())?).await?;
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
        let claims = serde_json::to_string(&read)?;
        for name in [HP_NAME, HCP_NAME] {
            assert!(!claims.contains(name), "the conveyance names {name}");
        }
        for (name, value) in &request.headers {
            if name.as_str() != ferrofed_engine::conveyance::HEADER.to_ascii_lowercase() {
                let value = value.to_str().unwrap_or_default();
                for relayed in [HP_ID, HP_AUTHORITY, HCP_AUTHORITY] {
                    assert!(
                        !value.contains(relayed),
                        "{name} carries no relayed attribute: {value}"
                    );
                }
            }
        }
        let sent = wire(node).await?;
        for name in [HP_NAME, HCP_NAME] {
            assert!(!sent.contains(name), "{endpoint} was sent {name}");
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
    let gateway = Gateway::with(declared()).await?;
    assert_admitted(&gateway, presenting(query()?, &relaying())?).await?;
    for node in [&gateway.a, &gateway.b] {
        let sent = wire(node).await?;
        assert!(!sent.is_empty(), "the node was asked");
        assert!(!sent.contains(PERSON_ID), "N33: person_id reached a node");
        assert!(!sent.contains("person_id"), "N33: no claim names it");
    }
    Ok(())
}
