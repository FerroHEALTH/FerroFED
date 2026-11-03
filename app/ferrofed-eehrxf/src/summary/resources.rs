// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR resources of a summary beside its sections.
//!
//! They are the patient the identity binding describes, the gateway as the
//! device that wrote the document, and the organisations of its authors. No
//! specification governs where they are built: our own design.

use ferrofed_identity::role::header::{PatientHeader, PersonName, PostalAddress, Telecom};
use fhir_types::r4::address::Address;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::contact_point::ContactPoint;
use fhir_types::r4::device::{Device, DeviceDeviceName, DeviceVersion};
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::primitives::{Code, Date};
use fhir_types::r4::reference::Reference;

use crate::summary::{Author, DATA_ABSENT_REASON, Organisation, Request};

/// The patient of `request`: the identifier it was asked by, and the
/// header the identity binding holds, each element as the binding gives it.
///
/// An element the binding does not hold is left out, and the birth date,
/// which the EPS `Patient` requires, then carries a `data-absent-reason` of
/// `unknown` (eHN PS Art 10(5)).
pub(super) fn patient(request: &Request) -> Patient {
    header_patient(
        &request.patient_id,
        (&request.system, &request.value),
        &request.header,
    )
}

/// Returns the `Patient` with the id `id` that the identity binding's
/// `header` describes, named by the identifier `system`|`value` it was asked
/// by, each element as the binding gives it.
///
/// An element the binding does not hold is left out, and the birth date
/// then carries a `data-absent-reason` of `unknown` (eHN PS Art 10(5)).
#[must_use]
pub fn header_patient(id: &str, (system, value): (&str, &str), header: &PatientHeader) -> Patient {
    Patient {
        id: Some(id.to_owned()),
        identifier: vec![Identifier {
            system: Some(system.into()),
            value: Some(value.into()),
            ..Identifier::default()
        }],
        name: header.names.iter().map(human_name).collect(),
        birth_date: Some(header.birth_date.as_deref().map_or_else(
            || Date {
                extension: vec![absent()],
                ..Date::default()
            },
            Date::from,
        )),
        gender: header.gender.as_deref().map(Code::from),
        address: header.addresses.iter().map(address).collect(),
        telecom: header.telecoms.iter().map(contact_point).collect(),
        ..Patient::default()
    }
}

/// The FHIR name `name` is written as.
pub(super) fn human_name(name: &PersonName) -> HumanName {
    HumanName {
        r#use: name.purpose.as_deref().map(Code::from),
        text: name
            .text
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        family: name
            .family
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        given: strings(&name.given),
        prefix: strings(&name.prefix),
        suffix: strings(&name.suffix),
        ..HumanName::default()
    }
}

/// The FHIR address `postal` is written as.
pub(super) fn address(postal: &PostalAddress) -> Address {
    Address {
        r#use: postal.purpose.as_deref().map(Code::from),
        text: postal
            .text
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        line: strings(&postal.lines),
        city: postal
            .city
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        district: postal
            .district
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        state: postal
            .state
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        postal_code: postal
            .postal_code
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        country: postal
            .country
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        ..Address::default()
    }
}

/// The FHIR contact point `telecom` is written as.
pub(super) fn contact_point(telecom: &Telecom) -> ContactPoint {
    ContactPoint {
        system: telecom.system.as_deref().map(Code::from),
        value: telecom
            .value
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        r#use: telecom.purpose.as_deref().map(Code::from),
        ..ContactPoint::default()
    }
}

/// The FHIR `string`s `values` are written as.
pub(super) fn strings(values: &[String]) -> Vec<fhir_types::r4::primitives::String> {
    values.iter().map(|value| value.as_str().into()).collect()
}

/// The `data-absent-reason` extension stating `unknown`.
pub(super) fn absent() -> Extension {
    Extension {
        url: DATA_ABSENT_REASON.to_owned(),
        value: Some(ExtensionValue::Code("unknown".into())),
        ..Extension::default()
    }
}

/// The gateway's `Device`, owned by the operator at `operator`.
pub(super) fn device(author: &Author, operator: &str) -> Device {
    Device {
        id: Some(String::from("gateway")),
        device_name: vec![DeviceDeviceName {
            name: author.product.as_str().into(),
            r#type: "manufacturer-name".into(),
            ..DeviceDeviceName::default()
        }],
        version: vec![DeviceVersion {
            value: author.version.as_str().into(),
            ..DeviceVersion::default()
        }],
        // NOTE: eHN PS A.1.4.3: the nature of the summary is carried by its author device.
        r#type: Some(CodeableConcept {
            text: Some("Software that generates the patient summary automatically".into()),
            ..CodeableConcept::default()
        }),
        owner: Some(reference(operator)),
        ..Device::default()
    }
}

/// The `Organization` of `organisation`, under the document id `id`.
pub(super) fn organization(id: &str, organisation: &Organisation) -> Organization {
    Organization {
        id: Some(id.to_owned()),
        name: Some(organisation.shown().into()),
        identifier: organisation
            .identifiers
            .iter()
            .map(|(system, value)| Identifier {
                system: Some(system.as_str().into()),
                value: Some(value.as_str().into()),
                ..Identifier::default()
            })
            .collect(),
        ..Organization::default()
    }
}

/// The literal reference `url`.
pub(super) fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}
