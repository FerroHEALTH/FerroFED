// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PMIR ITI-93 Mobile Patient Identity Feed message each member's feed
//! application sends SanteMPI (PMIR 1.6.0 §2:3.93.4.1).

use fhir_types::r4::bundle::{Bundle, BundleEntry, BundleEntryRequest};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::message_header::{
    MessageHeader, MessageHeaderDestination, MessageHeaderEvent, MessageHeaderSource,
};
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use uuid::Uuid;

use super::source;
use crate::seed::{EhrDomain, PatientId};

/// The destination every feed message names, a synthetic endpoint.
const FEED_DESTINATION: &str = "urn:oid:2.999.3.1";

/// Returns the ITI-93 message that registers `patient` under `ehr_id` in
/// `domain`: a message Bundle whose focus is a history Bundle holding the
/// Patient (PMIR 1.6.0 §2:3.93.4.1.2).
pub(super) fn feed_message(domain: EhrDomain, patient: PatientId, ehr_id: Uuid) -> Bundle {
    let key =
        format!("{}-{}", source(domain).name.to_lowercase(), patient.value()).replace('_', "-");
    let identifier = |system: String, value: String, usage: &str| Identifier {
        r#use: Some(usage.into()),
        system: Some(system.into()),
        value: Some(value.into()),
        ..Identifier::default()
    };
    let record = Patient {
        id: Some(key.clone()),
        active: Some(true.into()),
        identifier: vec![
            identifier(domain.system(), ehr_id.to_string(), "official"),
            identifier(patient.namespace(), patient.value(), "usual"),
        ],
        name: vec![HumanName {
            family: Some("Synthetic".into()),
            given: vec![patient.value().as_str().into()],
            ..HumanName::default()
        }],
        gender: Some("unknown".into()),
        birth_date: Some("1970-01-01".into()),
        ..Patient::default()
    };
    let history = Bundle {
        id: Some(key.clone()),
        r#type: "history".into(),
        entry: vec![BundleEntry {
            full_url: Some(format!("Patient/{key}").into()),
            resource: Some(Resource::Patient(Box::new(record))),
            request: Some(BundleEntryRequest {
                method: "POST".into(),
                url: format!("Patient/{key}").into(),
                ..BundleEntryRequest::default()
            }),
            ..BundleEntry::default()
        }],
        ..Bundle::default()
    };
    let header = MessageHeader {
        id: Some(key.clone()),
        meta: None,
        implicit_rules: None,
        language: None,
        text: None,
        contained: Vec::new(),
        extension: Vec::new(),
        modifier_extension: Vec::new(),
        event: MessageHeaderEvent::Uri("urn:ihe:iti:pmir:2019:patient-feed".into()),
        destination: vec![MessageHeaderDestination {
            endpoint: FEED_DESTINATION.into(),
            ..MessageHeaderDestination::default()
        }],
        sender: None,
        enterer: None,
        author: None,
        source: MessageHeaderSource {
            endpoint: format!("{}.feed", domain.system()).into(),
            ..MessageHeaderSource::default()
        },
        responsible: None,
        reason: None,
        response: None,
        focus: vec![Reference {
            reference: Some(format!("Bundle/{key}").into()),
            ..Reference::default()
        }],
        definition: None,
    };
    Bundle {
        r#type: "message".into(),
        entry: vec![
            BundleEntry {
                full_url: Some(format!("MessageHeader/{key}").into()),
                resource: Some(Resource::MessageHeader(Box::new(header))),
                ..BundleEntry::default()
            },
            BundleEntry {
                full_url: Some(format!("Bundle/{key}").into()),
                resource: Some(Resource::Bundle(Box::new(history))),
                ..BundleEntry::default()
            },
        ],
        ..Bundle::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{FEED_DESTINATION, feed_message};
    use crate::seed::{EhrDomain, PatientId};
    use uuid::Uuid;

    #[test]
    fn the_feed_message_carries_the_patient_and_the_ehr_id() {
        let ehr_id = Uuid::from_u128(0x3333_3333_3333_4333_8333_3333_3333_3333);
        let message = feed_message(EhrDomain::new(1), PatientId::new(1, 38), ehr_id);
        let json = serde_json::to_string(&message).expect("a message");
        for needle in [
            "\"type\":\"message\"",
            "urn:ihe:iti:pmir:2019:patient-feed",
            "\"type\":\"history\"",
            "urn:oid:2.999.2.1",
            "33333333-3333-4333-8333-333333333333",
            "urn:oid:2.999.1.1",
            "ffd-test-0038",
            FEED_DESTINATION,
        ] {
            assert!(json.contains(needle), "{needle} in {json}");
        }
    }
}
