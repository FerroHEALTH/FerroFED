// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary header the identity binding holds of a patient.
//!
//! The header is the identification and contact elements of a summary
//! (eHealth Network *Guidelines on Patient Summary* Release 3.4, A.1.1 and
//! A.1.2). The members federate no demographics (Federation Tier §2.3, N32), so the
//! header is asked of the identity binding at the gateway alone and never of
//! a node; nothing here is dispatched (§5.4.1, N33). The header is a
//! patient's personal data: its `Debug` output counts the elements it holds
//! and shows none of them, and nothing here logs a value.

use std::fmt;

use crate::role::demographics::{Ambiguity, DemographicsError};

/// One name of the patient, as the binding holds it (A.1.1.2, A.1.1.3).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PersonName {
    /// What the name is used for, such as `official`, when the binding says.
    pub purpose: Option<String>,
    /// The whole name as text, when the binding holds it.
    pub text: Option<String>,
    /// The family name.
    pub family: Option<String>,
    /// The given names, in order.
    pub given: Vec<String>,
    /// The parts before the name.
    pub prefix: Vec<String>,
    /// The parts after the name.
    pub suffix: Vec<String>,
}

impl PersonName {
    /// Whether the name holds a family name, a given name or a text, the
    /// parts a summary names a patient by.
    #[must_use]
    pub fn names(&self) -> bool {
        self.text.as_deref().is_some_and(|text| !text.is_empty())
            || self
                .family
                .as_deref()
                .is_some_and(|family| !family.is_empty())
            || self.given.iter().any(|given| !given.is_empty())
    }
}

/// One postal address of the patient (A.1.2.1.1 to A.1.2.1.6).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PostalAddress {
    /// What the address is used for, such as `home`, when the binding says.
    pub purpose: Option<String>,
    /// The whole address as text, when the binding holds it.
    pub text: Option<String>,
    /// The street, house number and other delivery lines, in order.
    pub lines: Vec<String>,
    /// The city.
    pub city: Option<String>,
    /// The district.
    pub district: Option<String>,
    /// The state or province.
    pub state: Option<String>,
    /// The post code.
    pub postal_code: Option<String>,
    /// The country.
    pub country: Option<String>,
}

/// One way to reach the patient (A.1.2.1.7, A.1.2.1.8).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Telecom {
    /// The kind, such as `phone` or `email`, when the binding says.
    pub system: Option<String>,
    /// The number or address.
    pub value: Option<String>,
    /// What it is used for, such as `home`, when the binding says.
    pub purpose: Option<String>,
}

/// What the identity binding holds of one patient for a summary header.
///
/// An element the binding does not hold is absent here, and the summary
/// states it absent; nothing is filled in.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct PatientHeader {
    /// The patient's names.
    pub names: Vec<PersonName>,
    /// The date of birth as the binding gives it, an ISO 8601 date
    /// (A.1.1.4).
    pub birth_date: Option<String>,
    /// The administrative gender code (A.1.1.5).
    pub gender: Option<String>,
    /// The postal addresses.
    pub addresses: Vec<PostalAddress>,
    /// The phone numbers and email addresses.
    pub telecoms: Vec<Telecom>,
}

impl PatientHeader {
    /// Whether one of the names holds a part a summary names the patient
    /// by.
    #[must_use]
    pub fn named(&self) -> bool {
        self.names.iter().any(PersonName::names)
    }
}

impl fmt::Debug for PatientHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PatientHeader")
            .field("names", &self.names.len())
            .field("birth_date", &self.birth_date.is_some())
            .field("gender", &self.gender.is_some())
            .field("addresses", &self.addresses.len())
            .field("telecoms", &self.telecoms.len())
            .finish()
    }
}

impl fmt::Debug for PersonName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PersonName").finish_non_exhaustive()
    }
}

impl fmt::Debug for PostalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostalAddress").finish_non_exhaustive()
    }
}

impl fmt::Debug for Telecom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Telecom").finish_non_exhaustive()
    }
}

/// The answer of the identity binding about one patient's header.
#[derive(Debug)]
#[non_exhaustive]
pub enum HeaderAnswer {
    /// The binding holds the patient, with this header.
    Found(Box<PatientHeader>),
    /// The binding knows no patient for the identifier.
    NoMatch,
    /// The binding's answer names no one patient, so no header is taken
    /// from it.
    Ambiguous(Ambiguity),
    /// The binding is not asked about identifiers of this namespace.
    NotHandled,
    /// The binding could not answer.
    Unavailable(DemographicsError),
}
