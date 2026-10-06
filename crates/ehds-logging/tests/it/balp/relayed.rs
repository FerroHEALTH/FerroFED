// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a national contact point relayed (Implementing Regulation (EU)
//! 2026/2099 Annex Tables 1 and 2), written in an entity of its own and
//! marked asserted, and the correlation identifier the client sent.

use ehds_logging::balp::exchange;
use ehds_logging::record::{
    AccessRecord, Action, Coded, Outcome, Relayed, RelayedProfessional, RelayedProvider,
};

use super::{details, entity, gateway, record, written};

/// The synthetic values of a relayed professional and provider.
const CONTACT_POINT: &str = "https://ncp.example.org";
const FAMILY: &str = "Qz7-family-51";
const GIVEN: &str = "Qz7-given-52";
const HP_ID: &str = "Qz7-hp-53";
const HP_AUTHORITY: &str = "Qz7-hp-authority-54";
const ROLE: &str = "Qz7-role-55";
const HCP_ID: &str = "Qz7-hcp-56";
const HCP_AUTHORITY: &str = "Qz7-hcp-authority-57";
const HCP_NAME: &str = "Qz7-hcp-name-58";
const HCP_ADDRESS: &str = "Qz7-hcp-address-59";
const CORRELATION: &str = "Qz7-correlation-60";

/// A record of an access a national contact point relayed, with the
/// correlation identifier it sent.
fn relayed_record() -> AccessRecord {
    let mut record = record(Action::Query, Outcome::Success);
    record.accessor.relayed = Some(Relayed {
        contact_point: CONTACT_POINT.to_owned(),
        country_code: "XA".to_owned(),
        professional: RelayedProfessional {
            family_name: FAMILY.to_owned(),
            given_name: GIVEN.to_owned(),
            identifier: HP_ID.to_owned(),
            issuing_authority: HP_AUTHORITY.to_owned(),
            roles: vec![Coded {
                system: Some("urn:oid:2.999.9".to_owned()),
                code: ROLE.to_owned(),
            }],
        },
        provider: RelayedProvider {
            identifier: HCP_ID.to_owned(),
            issuing_authority: HCP_AUTHORITY.to_owned(),
            name: HCP_NAME.to_owned(),
            address: HCP_ADDRESS.to_owned(),
        },
    });
    record.request.correlation = Some(CORRELATION.to_owned());
    record
}

/// Implementing Regulation (EU) 2026/2099 Art 7, Annex Tables 1 and 2: every
/// relayed attribute is written, beside the contact point that asserted it
/// and the mark that it is asserted.
#[test]
fn a_relayed_professional_and_provider_ride_in_their_own_entity_marked_asserted() {
    let written = written(&relayed_record());
    let relayed = entity(&written, "ehds-relayed");
    let [relayed] = relayed.as_slice() else {
        panic!("one relayed entity, got {relayed:?}");
    };
    assert_eq!(relayed["type"]["code"], "4");
    for (kind, value) in [
        ("contact-point", CONTACT_POINT),
        ("asserted", "true"),
        ("country-code", "XA"),
        ("hp-family-name", FAMILY),
        ("hp-given-name", GIVEN),
        ("hp-identifier", HP_ID),
        ("hp-issuing-authority", HP_AUTHORITY),
        ("provider-identifier", HCP_ID),
        ("provider-issuing-authority", HCP_AUTHORITY),
        ("provider-name", HCP_NAME),
        ("provider-address", HCP_ADDRESS),
    ] {
        assert_eq!(details(relayed, kind), [value], "{kind}");
    }
    assert_eq!(
        details(relayed, "hp-professional-role"),
        [format!("urn:oid:2.999.9|{ROLE}")]
    );
}

/// The correlation identifier the client sent is a detail of the request
/// id's entity, so the record joins the client's own log.
#[test]
fn the_correlation_identifier_rides_with_the_request_id() {
    let written = written(&relayed_record());
    let transaction = written["entity"]
        .as_array()
        .expect("entities")
        .iter()
        .find(|entity| entity["type"]["code"] == "XrequestId")
        .expect("entity:transaction");
    assert_eq!(details(transaction, "correlation-id"), [CORRELATION]);
}

#[test]
fn a_record_no_contact_point_relayed_has_no_relayed_entity() {
    let written = written(&record(Action::Query, Outcome::Success));
    assert!(entity(&written, "ehds-relayed").is_empty());
    assert!(!written.to_string().contains("correlation-id"));
}

#[test]
fn no_debug_shows_a_relayed_value() {
    let record = relayed_record();
    let shown = format!("{record:?} {:?}", exchange(&record, &gateway()));
    for value in [
        FAMILY,
        GIVEN,
        HP_ID,
        HP_AUTHORITY,
        ROLE,
        HCP_ID,
        HCP_AUTHORITY,
        HCP_NAME,
        HCP_ADDRESS,
        CORRELATION,
    ] {
        assert!(!shown.contains(value), "{value} in {shown}");
    }
}
