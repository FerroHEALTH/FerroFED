// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a conformance run knows of the deployment it scores.
//!
//! It holds the synthetic patient, the members with what each holds of that
//! patient, the spare EHRs a routing scenario needs the gateway not to have
//! seen, and the vendored synthetic content the run commits.
//!
//! The patient is the operator's choice, inside the example arc `2.999`
//! that ITU-T X.660 and ISO/IEC 9834 reserve for examples, so no real
//! identifier can stand in for it. Its value is held as a [`SecretString`]
//! like any patient identifier (§5.4, N33): it travels in the query a client
//! sends the gateway, in the `EHR_STATUS` body of the run's own creates, and
//! to the cross-reference, and the report never prints it.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef, PatientRefError};
use ferrofed_registry::secret::REDACTED;
use openehr_base::v1_3::base_types::identification::generic_id::GenericId;
use openehr_base::v1_3::base_types::identification::object_id::ObjectId;
use openehr_base::v1_3::base_types::identification::party_ref::PartyRef;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use uuid::Uuid;

use crate::conformance::Failure;

/// The example arc every synthetic patient's namespace lies in (ITU-T
/// X.660, ISO/IEC 9834).
pub const EXAMPLE_ARC: &str = "urn:oid:2.999";

/// The prefix of the value of a patient a run mints, which no
/// cross-reference knows.
pub const UNKNOWN_PREFIX: &str = "ffd-conformance-";

/// A patient identifier inside the example arc, the only kind a run uses.
///
/// `Debug` never shows the value.
#[derive(Clone)]
pub struct SyntheticPatient {
    namespace: String,
    value: SecretString,
}

/// A patient identifier a run refuses to use.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PatientError {
    /// The namespace is not an OID inside the example arc.
    #[error(
        "the patient namespace must be {EXAMPLE_ARC} or an OID under it, so no real identifier is used"
    )]
    OutsideArc,
    /// The value is empty, or holds a character other than an ASCII letter,
    /// digit, `.`, `_` or `-`.
    #[error(
        "the patient value must be ASCII letters, digits, '.', '_' or '-', so it is a plain AQL string literal"
    )]
    Value,
    /// The identifier is no patient reference.
    #[error("the patient identifier is no patient reference")]
    Reference(#[source] PatientRefError),
}

impl SyntheticPatient {
    /// Returns the patient `value` in `namespace`.
    ///
    /// # Errors
    ///
    /// Returns [`PatientError::OutsideArc`] for a namespace outside
    /// [`EXAMPLE_ARC`] and [`PatientError::Value`] for a value that is not a
    /// plain token.
    pub fn new(namespace: &str, value: SecretString) -> Result<Self, PatientError> {
        let inside = namespace.strip_prefix(EXAMPLE_ARC).is_some_and(|rest| {
            rest.is_empty()
                || rest.strip_prefix('.').is_some_and(|arcs| {
                    arcs.split('.')
                        .all(|arc| !arc.is_empty() && arc.bytes().all(|byte| byte.is_ascii_digit()))
                })
        });
        if !inside {
            return Err(PatientError::OutsideArc);
        }
        let plain = {
            let text = value.expose_secret();
            !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if !plain {
            return Err(PatientError::Value);
        }
        Ok(Self {
            namespace: namespace.to_owned(),
            value,
        })
    }

    /// Returns a patient in the same namespace that no cross-reference
    /// knows: [`UNKNOWN_PREFIX`] and the 32 hex digits of a random UUID.
    #[must_use]
    pub fn unknown_beside(&self) -> Self {
        Self {
            namespace: self.namespace.clone(),
            value: SecretString::from(format!("{UNKNOWN_PREFIX}{}", Uuid::new_v4().simple())),
        }
    }

    /// Returns the namespace.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the value, which the report never prints.
    #[must_use]
    pub fn value(&self) -> &SecretString {
        &self.value
    }

    /// Returns the patient as a reference, for the cross-reference.
    ///
    /// # Errors
    ///
    /// Returns [`PatientError::Reference`] when the namespace or the value
    /// is empty, which neither ever is.
    pub fn patient_ref(&self) -> Result<PatientRef, PatientError> {
        PatientRef::new(
            IdentifierNamespace::new(self.namespace.as_str()).map_err(PatientError::Reference)?,
            self.value.clone(),
        )
        .map_err(PatientError::Reference)
    }

    /// Returns the `PARTY_REF` an `EHR_STATUS` names the patient with.
    #[must_use]
    pub fn party_ref(&self) -> PartyRef {
        PartyRef {
            namespace: self.namespace.clone(),
            r#type: "PERSON".to_owned(),
            id: ObjectId::GenericId(GenericId {
                value: self.value.expose_secret().to_owned(),
                scheme: "ffd-conformance".to_owned(),
            }),
        }
    }

    /// Returns the patient predicate over `EHR_STATUS.subject.external_ref`
    /// for an `EHR` bound to `e`, as a client writes it (§7.1).
    #[must_use]
    pub fn predicate(&self) -> String {
        format!(
            "e/ehr_status/subject/external_ref/id/value = '{}' \
             AND e/ehr_status/subject/external_ref/namespace = '{}'",
            self.value.expose_secret(),
            self.namespace
        )
    }

    /// Returns `text` with every occurrence of the value replaced, for a
    /// line the report prints.
    #[must_use]
    pub fn redact(&self, text: &str) -> String {
        text.replace(self.value.expose_secret(), REDACTED)
    }
}

impl fmt::Debug for SyntheticPatient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyntheticPatient")
            .field("namespace", &self.namespace)
            .field("value", &REDACTED)
            .finish()
    }
}

/// One member endpoint of the registry, and what it holds of the patient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The endpoint id.
    pub endpoint: String,
    /// The `system_id` of the endpoint's node.
    pub system_id: String,
    /// The endpoint's managing organisation.
    pub organisation: String,
    /// The path of the endpoint's URL, which a node's own `Location` starts
    /// with.
    pub path: String,
    /// Whether the endpoint is in service, not suspended by the operator.
    pub active: bool,
    /// The patient's EHR at the node, when the run seeded one there.
    pub holding: Option<Holding>,
}

/// The patient's EHR at one member, as the run seeded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holding {
    /// The `ehr_id` the cross-reference names.
    pub ehr_id: String,
    /// The compositions committed to it.
    pub compositions: usize,
}

/// The EHRs the run created with no subject, each for the one scenario
/// that needs an `ehr_id` the gateway has not seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spares {
    /// An EHR the ask-all probe finds: the endpoint and the `ehr_id`
    /// (§12.5.1 step 4).
    pub probe: Option<(String, String)>,
    /// An EHR a write to is never probed for (§12.5.1, N41).
    pub unrouted: Option<String>,
    /// An EHR holding one composition, and that composition's version uid,
    /// for the versioned writes no earlier step routes (§12.4, N41).
    pub versioned: Option<(String, String)>,
}

/// What a run knows of the deployment it scores.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// The synthetic patient.
    pub patient: SyntheticPatient,
    /// Every member endpoint, in registry order.
    pub members: Vec<Member>,
    /// The spare EHRs.
    pub spares: Spares,
    /// Why the patient is held by fewer members than the run seeds, when it
    /// is.
    pub shortfall: Option<String>,
}

impl Fixture {
    /// Returns the members that hold the patient, in registry order.
    pub fn holding(&self) -> impl Iterator<Item = (&Member, &Holding)> {
        self.members
            .iter()
            .filter_map(|member| member.holding.as_ref().map(|holding| (member, holding)))
    }

    /// Returns the members that do not hold the patient, in registry order.
    pub fn not_holding(&self) -> impl Iterator<Item = &Member> {
        self.members
            .iter()
            .filter(|member| member.holding.is_none())
    }

    /// Returns the compositions every member holds of the patient.
    #[must_use]
    pub fn compositions(&self) -> usize {
        self.holding()
            .map(|(_, holding)| holding.compositions)
            .sum()
    }

    /// Returns the member `endpoint` names.
    #[must_use]
    pub fn member(&self, endpoint: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|member| member.endpoint == endpoint)
    }

    /// Returns the first `count` members that hold the patient, or why there
    /// are fewer.
    ///
    /// # Errors
    ///
    /// Returns the reason a scenario needing them cannot run.
    pub fn first_holding(&self, count: usize) -> Result<Vec<(&Member, &Holding)>, String> {
        let holding: Vec<_> = self.holding().take(count).collect();
        if holding.len() < count {
            return Err(match &self.shortfall {
                Some(shortfall) => format!(
                    "needs the patient held by {count} members, and {} hold it: {shortfall}",
                    holding.len()
                ),
                None => format!(
                    "needs the patient held by {count} members, and the cross-reference resolves it at {}",
                    holding.len()
                ),
            });
        }
        Ok(holding)
    }
}

/// The vendored file of the operational template the run uploads.
pub const TEMPLATE_FILE: &str = "International Patient Summary.opt";

/// The vendored file of the composition the first holding member receives.
pub const HOSPITAL_FILE: &str = "composition-12345-hospital.json";

/// The vendored file of the composition every other holding member
/// receives.
pub const CLINIC_FILE: &str = "composition-12345-clinic.json";

/// The template id of [`TEMPLATE_FILE`].
pub const TEMPLATE_ID: &str = "International Patient Summary";

/// The SHA-256 digest of each file the run writes, as vendored from the
/// reference implementation's demo data, so a run writes that synthetic
/// content and nothing else.
pub const DIGESTS: [(&str, &str); 3] = [
    (
        TEMPLATE_FILE,
        "fa7148549a2140a12d8aad32a0c8c2e5935660578b657a85cf8576ddea8cac3c",
    ),
    (
        HOSPITAL_FILE,
        "ba705e3ae796f6180d4d70f79b2166b4d00a2bf255d44d22a4d35287d1429c8e",
    ),
    (
        CLINIC_FILE,
        "17275197e5a1bb1202db8496242fa82db6ff50f90ebc8a4eb4b0f25dc12bc3e9",
    ),
];

/// The synthetic content a run writes: the vendored operational template
/// and two vendored compositions whose only party is `PARTY_SELF`.
///
/// `Debug` shows the sizes only.
pub struct SeedData {
    template: Vec<u8>,
    hospital: String,
    clinic: String,
}

/// The seed data could not be read, or is not the vendored content.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SeedDataError {
    /// A file could not be read.
    #[error("{} could not be read", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A file is not the vendored one.
    #[error(
        "{} is not the vendored synthetic file (its SHA-256 differs), and a run writes nothing else",
        path.display()
    )]
    Digest {
        /// The file.
        path: PathBuf,
    },
    /// A composition is not UTF-8 text.
    #[error("{} is not UTF-8 text", path.display())]
    Text {
        /// The file.
        path: PathBuf,
    },
    /// A file that is not a directory is not the seed data file a release
    /// attaches.
    #[error(
        "{} is neither a directory of the seed files nor the seed data file a release attaches",
        path.display()
    )]
    Bundle {
        /// The file.
        path: PathBuf,
        /// What the JSON reader reported.
        #[source]
        source: serde_json::Error,
    },
    /// The seed data file names a format this build does not read.
    #[error("{} is not seed data of the format {BUNDLE_FORMAT}", path.display())]
    Format {
        /// The file.
        path: PathBuf,
    },
    /// The seed data file holds no file of that name.
    #[error("{} holds no {member}", path.display())]
    Missing {
        /// The file.
        path: PathBuf,
        /// The vendored file it lacks.
        member: &'static str,
    },
    /// A file the seed data file holds is not the vendored one.
    #[error(
        "{member} in {} is not the vendored synthetic file (its SHA-256 differs), and a run writes nothing else",
        path.display()
    )]
    MemberDigest {
        /// The seed data file.
        path: PathBuf,
        /// The vendored file it holds.
        member: &'static str,
    },
}

/// The name of the seed data file a release attaches, the vendored files of
/// [`DIGESTS`] in one JSON file.
pub const BUNDLE_FILE: &str = "ferrofed-conformance-seed-data.json";

/// The format the seed data file names, the one this build reads.
pub const BUNDLE_FORMAT: &str = "ferrofed-conformance-seed-data/1";

/// The seed data file a release attaches: its format, and each vendored
/// file's text under its own name. Its other members carry the licence and
/// the source, which the run does not read.
#[derive(Deserialize)]
struct Bundle {
    /// The format, [`BUNDLE_FORMAT`].
    format: String,
    /// Each vendored file's text, by its file name.
    files: BTreeMap<String, String>,
}

/// Returns whether `bytes` are the vendored file `name`, by its SHA-256.
fn vendored(name: &str, bytes: &[u8]) -> bool {
    DIGESTS
        .iter()
        .find(|(file, _)| *file == name)
        .is_some_and(|(_, digest)| *digest == sha256(bytes))
}

impl SeedData {
    /// Reads the three files of [`DIGESTS`] from `path` and checks each
    /// digest.
    ///
    /// `path` is the directory holding them, as a checkout vendors them, or
    /// the seed data file a release attaches ([`BUNDLE_FILE`]), which holds
    /// each of them as text under its own name.
    ///
    /// # Errors
    ///
    /// Returns [`SeedDataError::Read`] when a file cannot be read,
    /// [`SeedDataError::Digest`] or [`SeedDataError::MemberDigest`] when one
    /// is not the vendored file, [`SeedDataError::Text`] when a composition
    /// is not text, and [`SeedDataError::Bundle`],
    /// [`SeedDataError::Format`] or [`SeedDataError::Missing`] when a file
    /// that is no directory is not a seed data file holding all three.
    pub fn read(path: &Path) -> Result<Self, SeedDataError> {
        if path.is_dir() {
            Self::read_dir(path)
        } else {
            Self::read_bundle(path)
        }
    }

    /// Reads the three files of [`DIGESTS`] from the directory `dir`.
    fn read_dir(dir: &Path) -> Result<Self, SeedDataError> {
        let read = |name: &str| -> Result<(PathBuf, Vec<u8>), SeedDataError> {
            let path = dir.join(name);
            let bytes = std::fs::read(&path).map_err(|source| SeedDataError::Read {
                path: path.clone(),
                source,
            })?;
            if !vendored(name, &bytes) {
                return Err(SeedDataError::Digest { path });
            }
            Ok((path, bytes))
        };
        let text = |(path, bytes): (PathBuf, Vec<u8>)| {
            String::from_utf8(bytes).map_err(|_not_text| SeedDataError::Text { path })
        };
        Ok(Self {
            template: read(TEMPLATE_FILE)?.1,
            hospital: text(read(HOSPITAL_FILE)?)?,
            clinic: text(read(CLINIC_FILE)?)?,
        })
    }

    /// Reads the three files of [`DIGESTS`] from the seed data file `path`.
    fn read_bundle(path: &Path) -> Result<Self, SeedDataError> {
        let bytes = std::fs::read(path).map_err(|source| SeedDataError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let mut bundle: Bundle =
            serde_json::from_slice(&bytes).map_err(|source| SeedDataError::Bundle {
                path: path.to_path_buf(),
                source,
            })?;
        if bundle.format != BUNDLE_FORMAT {
            return Err(SeedDataError::Format {
                path: path.to_path_buf(),
            });
        }
        let mut take = |member: &'static str| -> Result<String, SeedDataError> {
            let text = bundle
                .files
                .remove(member)
                .ok_or_else(|| SeedDataError::Missing {
                    path: path.to_path_buf(),
                    member,
                })?;
            if !vendored(member, text.as_bytes()) {
                return Err(SeedDataError::MemberDigest {
                    path: path.to_path_buf(),
                    member,
                });
            }
            Ok(text)
        };
        Ok(Self {
            template: take(TEMPLATE_FILE)?.into_bytes(),
            hospital: take(HOSPITAL_FILE)?,
            clinic: take(CLINIC_FILE)?,
        })
    }

    /// Returns the operational template.
    #[must_use]
    pub fn template(&self) -> &[u8] {
        &self.template
    }

    /// Returns the composition the first holding member receives.
    #[must_use]
    pub fn hospital(&self) -> &str {
        &self.hospital
    }

    /// Returns the composition every other holding member receives.
    #[must_use]
    pub fn clinic(&self) -> &str {
        &self.clinic
    }
}

impl fmt::Debug for SeedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeedData")
            .field("template_bytes", &self.template.len())
            .field("hospital_bytes", &self.hospital.len())
            .field("clinic_bytes", &self.clinic.len())
            .finish()
    }
}

/// Returns the hexadecimal SHA-256 digest of `bytes` (FIPS 180-4).
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _hex_written = write!(hex, "{byte:02x}");
            hex
        })
}

/// The composer line of the vendored hospital composition.
const COMPOSER: &str = "\"name\": \"Dr. Mark Antonio\"";

/// Returns the vendored hospital composition with `patient` on its composer.
///
/// The patient's own identifier is added to the composer as a
/// `DV_IDENTIFIER`: a commit whose content carries the identifier as
/// modelled data, which reaches the node byte-identical (§5.4 scope note,
/// N22).
///
/// # Errors
///
/// Returns [`Failure::Check`] when `hospital` names no composer to add it
/// to.
pub fn composition_carrying(hospital: &str, patient: &SyntheticPatient) -> Result<String, Failure> {
    if !hospital.contains(COMPOSER) {
        return Err(Failure::Check(
            "the vendored composition names its composer".to_owned(),
        ));
    }
    let identifier = format!(
        "{COMPOSER},\n  \"identifiers\": [{{\"_type\": \"DV_IDENTIFIER\", \"issuer\": \"{ns}\", \"assigner\": \"{ns}\", \"id\": \"{id}\", \"type\": \"MR\"}}]",
        ns = patient.namespace(),
        id = patient.value().expose_secret()
    );
    Ok(hospital.replacen(COMPOSER, &identifier, 1))
}

#[cfg(test)]
mod tests {
    use super::{PatientError, SyntheticPatient, UNKNOWN_PREFIX};
    use secrecy::{ExposeSecret, SecretString};

    fn patient(namespace: &str, value: &str) -> Result<SyntheticPatient, PatientError> {
        SyntheticPatient::new(namespace, SecretString::from(value.to_owned()))
    }

    #[test]
    fn only_a_namespace_in_the_example_arc_is_admitted() {
        for admitted in ["urn:oid:2.999", "urn:oid:2.999.1", "urn:oid:2.999.1.38"] {
            assert!(patient(admitted, "ffd-test-0038").is_ok(), "{admitted}");
        }
        for refused in [
            "urn:oid:2.16.840.1.113883.2.4.6.3",
            "urn:oid:2.9990",
            "urn:oid:2.999.",
            "urn:oid:2.999..1",
            "urn:oid:2.999.x",
            "2.999.1",
            "",
        ] {
            assert!(
                matches!(
                    patient(refused, "ffd-test-0038"),
                    Err(PatientError::OutsideArc)
                ),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_value_that_is_no_plain_token_is_refused() {
        for refused in ["", "o'brien", "a b", "x\n", "ü"] {
            assert!(
                matches!(
                    patient("urn:oid:2.999.1", refused),
                    Err(PatientError::Value)
                ),
                "{refused:?}"
            );
        }
    }

    /// The vendored demo data the seed files are read from in a checkout.
    const DEMO_DATA: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/federation-ref/docker/demo-data"
    );

    #[test]
    fn the_recorded_digests_are_the_vendored_files() {
        let data = super::SeedData::read(std::path::Path::new(DEMO_DATA))
            .expect("the vendored files match their recorded SHA-256");
        assert!(data.hospital().contains("\"_type\": \"COMPOSITION\""));
        assert!(data.clinic().contains("\"_type\": \"COMPOSITION\""));
        assert!(!data.template().is_empty());
        let carrying = super::composition_carrying(
            data.hospital(),
            &patient("urn:oid:2.999.1.1", "ffd-test-0038").expect("admitted"),
        )
        .expect("the vendored composition names its composer");
        assert!(carrying.contains("\"id\": \"ffd-test-0038\""), "{carrying}");
    }

    #[test]
    fn a_file_that_is_not_the_vendored_one_is_refused() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        for (file, _) in super::DIGESTS {
            let source = std::path::Path::new(DEMO_DATA).join(file);
            std::fs::copy(source, dir.path().join(file)).expect("copied");
        }
        std::fs::write(dir.path().join(super::CLINIC_FILE), "{}").expect("written");
        assert!(matches!(
            super::SeedData::read(dir.path()),
            Err(super::SeedDataError::Digest { .. })
        ));
    }

    #[test]
    fn the_unknown_patient_is_fresh_and_debug_never_shows_a_value() {
        let known = patient("urn:oid:2.999.1.1", "ffd-test-0038").expect("admitted");
        let unknown = known.unknown_beside();
        assert_eq!(known.namespace(), unknown.namespace());
        assert!(unknown.value().expose_secret().starts_with(UNKNOWN_PREFIX));
        assert_ne!(
            unknown.value().expose_secret(),
            known.unknown_beside().value().expose_secret()
        );
        let shown = format!("{known:?}");
        assert!(!shown.contains("ffd-test-0038"), "{shown}");
        assert_eq!("x *** y", known.redact("x ffd-test-0038 y"));
    }
}
