// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a caller's token states about its professional, as
//! `ferrohealth-smart` read it, in the shapes the gateway conveys.
//!
//! A `persons` token states it under its issuer's `[[auth.attributes]]`
//! table, and a `federation` token under its issuer's `[[auth.federation]]`
//! table. With `mapping = "iua"` the crate reads the IHE IUA extension (ITI
//! TF-2 3.71.4.2.2.1.1: `subject_name`, `national_provider_identifier`,
//! `subject_role`, `subject_organization_id`, `purpose_of_use`), and a
//! `persons` table may add the `system|code` `purpose_of_use` of each RFC 9396
//! `authorization_details` entry (the Federation Tier's Annex B §B.4a.3). The
//! crate refuses a token whose claim has another shape. The agency that
//! issued the professional's identifier is the table's
//! `professional.issuing_authority_name` source, which IUA defines no claim
//! for. The IUA `person_id`, a patient identifier, is never read (§5.4.1,
//! N33). A caller whose issuer has no such table states nothing about its
//! professional.

use ferrofed_engine::conveyance::relayed::Role;
use ferrohealth_smart::federation::person::Attributes;
use ferrohealth_smart::federation::{Attribute, Code, External, Field};
use ferrohealth_smart::resource::Caller as SmartCaller;

use crate::auth::caller::{Professional, PurposeOfUse};

/// Where a caller's statement about its professional was read.
#[derive(Clone, Copy)]
pub(super) enum Statement<'caller> {
    /// A `persons` token, under its issuer's `[[auth.attributes]]` table.
    Persons(&'caller Attributes),
    /// A `federation` token, under its issuer's `[[auth.federation]]` table.
    Federation(&'caller External),
    /// A caller whose issuer states nothing about its professional.
    None,
}

impl<'caller> Statement<'caller> {
    /// Returns the statement `caller`'s token carries.
    pub(super) fn of(caller: &'caller SmartCaller) -> Self {
        if let Some(attributes) = caller.attributes() {
            return Self::Persons(attributes);
        }
        caller
            .kind()
            .external()
            .map_or(Self::None, Self::Federation)
    }

    /// The value of `field`, when the token states it.
    fn field(self, field: Field) -> Option<&'caller Attribute> {
        match self {
            Self::Persons(attributes) => attributes.field(field),
            Self::Federation(external) => external.field(field),
            Self::None => None,
        }
    }

    /// The non-empty text value of `field`, when the token states one.
    pub(super) fn text(self, field: Field) -> Option<String> {
        match self.field(field)? {
            Attribute::Text(value) if !value.is_empty() => Some(value.clone()),
            Attribute::Text(_) | Attribute::Codes(_) => None,
        }
    }

    /// The codings of `field`, when the token states them.
    fn codes(self, field: Field) -> &'caller [Code] {
        match self.field(field) {
            Some(Attribute::Codes(codes)) => codes,
            _ => &[],
        }
    }

    /// Returns every purpose of use the token declares, sorted and without
    /// duplicates.
    pub(super) fn purposes(self) -> Vec<PurposeOfUse> {
        let codes = match self {
            Self::Persons(attributes) => attributes.purposes(),
            Self::Federation(_) | Self::None => self.codes(Field::PurposeOfUse),
        };
        let mut purposes: Vec<PurposeOfUse> = codes
            .iter()
            .map(|purpose| PurposeOfUse {
                system: purpose.system.clone(),
                code: purpose.code.clone(),
            })
            .collect();
        purposes.sort();
        purposes.dedup();
        purposes
    }

    /// Returns the professional's identification the token states.
    pub(super) fn professional(self) -> Professional {
        Professional {
            identifier: self.text(Field::ProfessionalHpIdentifier),
            name: self.text(Field::ProfessionalName),
            issuing_authority: self.text(Field::ProfessionalIssuingAuthorityName),
            roles: self
                .codes(Field::ProfessionalHpProfessionalRole)
                .iter()
                .map(|role| Role {
                    system: role.system.clone(),
                    code: role.code.clone(),
                })
                .collect(),
        }
    }
}
