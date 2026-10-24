// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The electronic prescription and the electronic dispensation in the HL7
//! Europe Medication Prescription and Dispense form (feature `fhir-r4` with
//! `prescription`, `dispensation` or both).
//!
//! The datasets are the Xt-EHR `EHDSMedicationPrescription` and
//! `EHDSMedicationDispense` 1.0.0 logical models, written as Rust records
//! whose leaves are the FHIR R4 data types the models name, and read and
//! written in the shape of the HL7 Europe MPD 1.0.0 profiles
//! (`MedicationRequest-eu-mpd`, `MedicationDispense-eu-mpd`,
//! `Medication-eu-mpd`):
//!
//! - `prescription` (feature `prescription`): one prescription, a header and
//!   its items, each item one `MedicationRequest`;
//! - `dispensation` (feature `dispensation`): one dispense record, one
//!   `MedicationDispense`.
//!
//! Reading takes a member's resource as FHIR JSON text and refuses, with a
//! typed [`ReadError`], what the R4 model or the dataset cannot hold. Reading
//! keeps only the dataset's elements, so a system that passes a member's
//! resource on passes the text it received. [`Mpd`] holds a resource to the
//! vendored MPD profiles by the rules of
//! [`check_resource`], and names what
//! it cannot evaluate.

#[cfg(feature = "dispensation")]
pub mod dispensation;
#[cfg(feature = "prescription")]
pub mod prescription;

use std::io::Cursor;

use fhir_types::codec::DecodeError;
use fhir_types::codec::EncodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::annotation::Annotation;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::extension::Extension;
use fhir_types::r4::extension::ExtensionValue;
use fhir_types::r4::medication::Medication;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;

use crate::dataset::DatasetError;
use crate::dataset::ResourceProfile;
use crate::receive::conform::CheckError;
use crate::receive::conform::Conformance;
use crate::receive::conform::Unevaluated;
use crate::receive::conform::check_resource;
use crate::receive::strict;
use crate::terminology::bindings::BindingError;
use crate::terminology::bindings::check_required;
use crate::terminology::expansion::Expansions;

/// The canonical URL of the HL7 Europe MPD `Medication` profile.
pub const MPD_MEDICATION: &str = "http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd";

/// The medicinal product of a prescription item or a dispense: the Xt-EHR
/// `EHDSMedication`, as the MPD profiles let a resource carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Medicinal {
    /// A coded product, `medicationCodeableConcept`.
    Coded(CodeableConcept),
    /// A `Medication` resource held elsewhere, `medicationReference`.
    Referenced(Reference),
    /// A `Medication` resource contained in the resource under `id` and
    /// named by the local reference `#{id}`
    /// (<https://hl7.org/fhir/R4/references.html#contained>).
    Contained {
        /// The id the contained resource carries.
        id: String,
        /// The resource, its own `id` left out.
        medication: Box<Medication>,
    },
}

impl Medicinal {
    /// Returns the medicinal product a resource names by `reference` or
    /// `coded`, resolving a local reference against `contained`.
    fn read(
        coded: Option<&CodeableConcept>,
        reference: Option<&Reference>,
        contained: &[Resource],
        element: &'static str,
    ) -> Result<Self, ReadError> {
        if let Some(coded) = coded {
            return Ok(Self::Coded(coded.clone()));
        }
        let reference = reference.ok_or(ReadError::Missing { element })?;
        let Some(local) = reference
            .reference
            .as_ref()
            .and_then(|text| text.value.as_deref())
            .and_then(|text| text.strip_prefix('#'))
        else {
            return Ok(Self::Referenced(reference.clone()));
        };
        let mut found = contained.iter().filter_map(|resource| match resource {
            Resource::Medication(medication) if medication.id.as_deref() == Some(local) => {
                Some(medication)
            }
            _ => None,
        });
        match (found.next(), found.next()) {
            (Some(medication), None) => {
                let mut medication = medication.clone();
                medication.id = None;
                Ok(Self::Contained {
                    id: local.to_owned(),
                    medication,
                })
            }
            _ => Err(ReadError::Dangling { element }),
        }
    }

    /// Returns the reference a resource names the product by, or its code,
    /// with the `Medication` it then contains.
    fn write(&self) -> (Written, Option<Resource>) {
        match self {
            Self::Coded(coded) => (Written::Coded(Box::new(coded.clone())), None),
            Self::Referenced(reference) => (Written::Referenced(Box::new(reference.clone())), None),
            Self::Contained { id, medication } => {
                let mut medication = medication.clone();
                medication.id = Some(id.clone());
                let reference = Reference {
                    reference: Some(format!("#{id}").into()),
                    ..Reference::default()
                };
                (
                    Written::Referenced(Box::new(reference)),
                    Some(Resource::Medication(medication)),
                )
            }
        }
    }
}

/// The written form of a medicinal product, before it is placed in the
/// choice element of a `MedicationRequest` or a `MedicationDispense`.
enum Written {
    /// The `medicationCodeableConcept` form.
    Coded(Box<CodeableConcept>),
    /// The `medicationReference` form.
    Referenced(Box<Reference>),
}

/// The HL7 Europe MPD profiles, read from the `hl7.fhir.eu.mpd` package.
#[derive(Debug, Clone)]
pub struct Mpd {
    #[cfg(feature = "prescription")]
    request: ResourceProfile,
    #[cfg(feature = "dispensation")]
    dispense: ResourceProfile,
    medication: ResourceProfile,
}

impl Mpd {
    /// Reads the profiles this build checks against from `archive`, the
    /// bytes of the gzip-compressed package tarball a FHIR package registry
    /// serves.
    ///
    /// # Errors
    ///
    /// The errors of [`ResourceProfile::read`] for any profile the package
    /// lacks or holds defective.
    pub fn read(archive: &[u8]) -> Result<Self, DatasetError> {
        let profile = |url| ResourceProfile::read(Cursor::new(archive), url);
        Ok(Self {
            #[cfg(feature = "prescription")]
            request: profile(crate::crosswalk::prescription::MPD_MEDICATION_REQUEST)?,
            #[cfg(feature = "dispensation")]
            dispense: profile(crate::crosswalk::dispensation::MPD_MEDICATION_DISPENSE)?,
            medication: profile(MPD_MEDICATION)?,
        })
    }

    /// Checks `resource`, FHIR JSON of the type `expected`, against
    /// `profile`, and each `Medication` it contains against
    /// `Medication-eu-mpd`.
    fn check(
        &self,
        profile: &ResourceProfile,
        expected: &'static str,
        resource: &Object,
    ) -> Result<Conformance, CheckError> {
        let mut findings = Vec::new();
        let mut passed = Conformance::default();
        let contained = resource
            .get("contained")
            .and_then(Value::as_array)
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_object)
            .filter(|object| {
                object.get("resourceType").and_then(Value::as_str) == Some("Medication")
            });
        let checks = std::iter::once((profile, expected, resource))
            .chain(contained.map(|medication| (&self.medication, "Medication", medication)));
        for (profile, expected, resource) in checks {
            let check = check_resource(profile, expected, resource).and_then(|conformance| {
                Ok(conformance.join(bindings(profile, expected, resource)?))
            });
            match check {
                Ok(conformance) => passed = passed.join(conformance),
                Err(CheckError::NonConformant { findings: found }) => findings.extend(found),
                Err(other) => return Err(other),
            }
        }
        if findings.is_empty() {
            Ok(passed)
        } else {
            Err(CheckError::NonConformant { findings })
        }
    }
}

/// The `required` bindings of `profile` over `resource`, of the type
/// `expected`, each listed as not evaluated: the MPD package carries no
/// expansion of the value sets they name, and no terminology server is asked
/// here.
fn bindings(
    profile: &ResourceProfile,
    expected: &'static str,
    resource: &Object,
) -> Result<Conformance, CheckError> {
    let check =
        check_required(profile, resource, expected, &Expansions::default()).map_err(|error| {
            match error {
                BindingError::Unreadable { location } | BindingError::Decode { location, .. } => {
                    CheckError::Unreadable { location }
                }
            }
        })?;
    Ok(Conformance::listing(
        check.unevaluated().iter().cloned().map(Unevaluated::from),
    ))
}

/// Why a member's resource does not read as a dataset.
///
/// No refusal quotes a value of the resource.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReadError {
    /// The text is not JSON.
    #[error("the resource is not JSON")]
    Json {
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// An object of the text repeats a name (RFC 8259 §4).
    #[error("an object of the resource repeats a name")]
    RepeatedName {
        /// The parse failure that names the place.
        #[source]
        source: serde_json::Error,
    },
    /// The text is not a JSON object.
    #[error("the resource is not a JSON object")]
    NotAnObject,
    /// The resource is of another type than the one read.
    #[error("the resource is not a {expected}")]
    WrongType {
        /// The resource type read.
        expected: &'static str,
    },
    /// The resource does not decode against the R4 definition of its type.
    #[error("the resource does not decode against FHIR R4")]
    Decode {
        /// The decode failure, located by element path.
        #[source]
        source: DecodeError,
    },
    /// The decoded resource cannot be encoded again.
    #[error("the decoded resource cannot be encoded as JSON")]
    Encode {
        /// The encode failure.
        #[source]
        source: EncodeError,
    },
    /// The resource reads differently through the R4 model than as JSON.
    #[error("the resource reads differently through the R4 model")]
    Unfaithful,
    /// An element the dataset requires is absent.
    #[error("{element} is absent, and the dataset requires it")]
    Missing {
        /// The resource element, as an element id.
        element: &'static str,
    },
    /// An element repeats more often than the dataset admits.
    #[error("{element} repeats, and the dataset admits one")]
    Repeated {
        /// The resource element, as an element id.
        element: &'static str,
    },
    /// An extension the dataset reads carries no value of its type, or
    /// repeats.
    #[error("the extension {url} carries no single value of its type")]
    Extension {
        /// The extension's canonical URL.
        url: &'static str,
    },
    /// A local reference names no single contained resource of its type.
    #[error("{element} names no single contained resource")]
    Dangling {
        /// The resource element, as an element id.
        element: &'static str,
    },
    /// The resources of one dataset disagree about an element they share.
    #[error("the resources disagree about {element}")]
    Disagree {
        /// The resource element, as an element id.
        element: &'static str,
    },
}

/// Reads `text` as a resource of the type `expected`, strictly: no repeated
/// name, no property the R4 model does not hold, and nothing the model reads
/// differently from the text.
fn decode<T: Json>(text: &str, expected: &'static str) -> Result<T, ReadError> {
    strict::unique_names(text).map_err(|source| {
        if source.is_data() {
            ReadError::RepeatedName { source }
        } else {
            ReadError::Json { source }
        }
    })?;
    let value: Value = serde_json::from_str(text).map_err(|source| ReadError::Json { source })?;
    let Value::Object(read) = value else {
        return Err(ReadError::NotAnObject);
    };
    if read.get("resourceType").and_then(Value::as_str) != Some(expected) {
        return Err(ReadError::WrongType { expected });
    }
    let resource = T::from_json(&read, &mut Path::root(expected))
        .map_err(|source| ReadError::Decode { source })?;
    let again = resource
        .to_json()
        .map_err(|source| ReadError::Encode { source })?;
    if again != read {
        return Err(ReadError::Unfaithful);
    }
    Ok(resource)
}

/// Why a dataset cannot be written in the MPD form.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WriteError {
    /// An element the dataset requires holds nothing.
    #[error("{element} is empty, and the dataset requires it")]
    Empty {
        /// The dataset element, as an Xt-EHR element path.
        element: &'static str,
    },
    /// The dataset holds more than the MPD element that carries it can.
    #[error("{element} holds more than the MPD profile carries")]
    Unrepresentable {
        /// The dataset element, as an Xt-EHR element path.
        element: &'static str,
    },
    /// The written resource cannot be encoded as JSON.
    #[error("the resource cannot be encoded as JSON")]
    Encode {
        /// The encode failure.
        #[source]
        source: EncodeError,
    },
}

/// The value of the one extension at `url` among `extensions`, read by
/// `value`, or `None` when none is present.
fn extension<T>(
    extensions: &[Extension],
    url: &'static str,
    value: impl Fn(&ExtensionValue) -> Option<T>,
) -> Result<Option<T>, ReadError> {
    let mut found = extensions.iter().filter(|extension| extension.url == url);
    match (found.next(), found.next()) {
        (None, _) => Ok(None),
        (Some(only), None) => only
            .value
            .as_ref()
            .and_then(value)
            .map(Some)
            .ok_or(ReadError::Extension { url }),
        (Some(_), Some(_)) => Err(ReadError::Extension { url }),
    }
}

/// The one element of `list`, when it has at most one.
fn single<'a, T>(list: &'a [T], element: &'static str) -> Result<Option<&'a T>, ReadError> {
    match list {
        [] => Ok(None),
        [only] => Ok(Some(only)),
        _ => Err(ReadError::Repeated { element }),
    }
}

/// The text of the one note `notes` carry, when they carry one.
fn note(notes: &[Annotation], element: &'static str) -> Result<Option<String>, ReadError> {
    Ok(single(notes, element)?.and_then(|note| note.text.value.clone()))
}

/// The note `text`, as a resource writes it.
fn annotation(text: Option<&String>) -> Vec<Annotation> {
    text.map(|text| Annotation {
        text: text.as_str().into(),
        ..Annotation::default()
    })
    .into_iter()
    .collect()
}

/// The `meta` that claims `profile`.
fn claiming(profile: &str) -> Meta {
    Meta {
        profile: vec![profile.into()],
        ..Meta::default()
    }
}
