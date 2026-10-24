// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Documents received in the exchange format, prepared for the one member
//! the deployment declares for their category (Regulation (EU) 2025/327
//! Annex II 2.2 and 2.3, Art 14(1)).
//!
//! A deployment declares, per priority category it receives, the member that
//! stores the category's documents, the HL7 Europe package whose profiles a
//! document is checked against with the packages of its dependency closure,
//! and the FHIRconnect mapping that turns it into one openEHR composition
//! ([`Source`]). [`Receivers::receive`] reads a document, names its category
//! from its `Composition.type`, holds it to every constraint of the
//! category's profiles and of the profiles its entries claim with
//! FerroBRIDGE's validator, and maps it, the original kept in the
//! composition's `FEEDER_AUDIT.original_content`. The result names the
//! member and the patient identifiers the document carries, which the
//! gateway resolves to that member's own `ehr_id` before anything is sent
//! (Federation Tier §5.2, §12.4, N23). Nothing here reaches a member.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use eehrxf::category::Category;
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::receive::category::Uncategorised;
use eehrxf::receive::conform::{Finding, FindingKind, Unevaluated};
use eehrxf::receive::displays::DisplayError;
use eehrxf::receive::openehr::{ReceiveMappingError, ReceivedComposition};
use eehrxf::receive::validation::ValidationError;
use eehrxf::receive::{ReceiveError, ReceivedDocument};
use eehrxf::terminology::Terminology;
use eehrxf::transformation::{Direction, Pin, Report, ReportError};
use ferrobridge_term::client::Client;
use ferrobridge_validator::set::{LoadError, PackageSet, ResolveError};
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;

use crate::transformation::preferred;

/// One category the deployment receives, as it names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The category, by its code ([`Category::code`]), such as
    /// `Patient-Summaries`.
    pub category: String,
    /// The node id of the member that stores the category's documents.
    pub member: String,
    /// The HL7 Europe package, as a `.tgz` archive, whose profiles a
    /// document of the category is checked against.
    pub package: PathBuf,
    /// Every package of `package`'s dependency closure, each a `.tgz`
    /// archive or an unpacked directory, and any package of value sets its
    /// profiles bind.
    pub dependencies: Vec<PathBuf>,
    /// The OPT 1.4 operational template the composition is of, as text.
    pub opt: String,
    /// The model and context mapping files.
    pub files: Vec<PathBuf>,
    /// The `metadata.name` of the context mapping to compile.
    pub context: String,
    /// The language every composition is written in, an ISO 639-1 code.
    pub language: String,
    /// The territory every composition is written in, an ISO 3166-1 code.
    pub territory: String,
}

/// Why the deployment's receiving categories cannot be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReceiversError {
    /// A category is declared more than once.
    #[error("the category {category} is received by more than one member")]
    Twice {
        /// The category's code.
        category: &'static str,
    },
    /// The code names no category this build receives documents of.
    #[error("{category} is no category this build receives documents of")]
    Unreceived {
        /// The code as given.
        category: String,
    },
    /// The packages cannot be read, or one names a dependency none of them
    /// is.
    #[error("the packages of the category {category} do not form a complete package set")]
    Packages {
        /// The category's code.
        category: &'static str,
        /// Why.
        #[source]
        source: Box<LoadError>,
    },
    /// The packages hold no usable profile under a URL the category names.
    #[error("the packages of the category {category} give no profile {url}")]
    Profile {
        /// The category's code.
        category: &'static str,
        /// The profile's canonical URL.
        url: &'static str,
        /// Why.
        #[source]
        source: Box<ResolveError>,
    },
    /// The context mapping does not compile.
    #[error("the context mapping {context} of the category {category} does not compile")]
    Compile {
        /// The category's code.
        category: &'static str,
        /// The context name.
        context: String,
        /// Why.
        #[source]
        source: MappingError,
    },
}

/// One received category, compiled.
#[derive(Debug)]
struct Receiver {
    /// The member's node id.
    member: String,
    /// The canonical URL of the category's `Bundle` profile.
    bundle: &'static str,
    /// The package and its dependency closure, which the validator reads.
    packages: Arc<PackageSet>,
    /// The mapping into openEHR.
    mapping: Mapping,
    /// The file name of the package the profiles are read from.
    package: Option<String>,
    /// The composition language.
    language: String,
    /// The composition territory.
    territory: String,
}

/// The categories a deployment receives, each compiled once.
#[derive(Debug, Default)]
pub struct Receivers {
    by: BTreeMap<Category, Receiver>,
}

impl Receivers {
    /// Reads the packages and compiles the mapping of every one of
    /// `sources`.
    ///
    /// # Errors
    ///
    /// [`ReceiversError::Twice`] for a category declared twice,
    /// [`ReceiversError::Unreceived`] for one this build does not receive,
    /// [`ReceiversError::Packages`] for packages that cannot be read or lack
    /// a package a manifest names, [`ReceiversError::Profile`] for packages
    /// that give no profile the category names, and
    /// [`ReceiversError::Compile`] for a mapping that does not compile.
    pub fn compile(sources: &[Source]) -> Result<Self, ReceiversError> {
        let mut by = BTreeMap::new();
        for source in sources {
            let (kind, profiles) = Category::of_code(&source.category)
                .and_then(|kind| kind.profiles().map(|profiles| (kind, profiles)))
                .ok_or_else(|| ReceiversError::Unreceived {
                    category: source.category.clone(),
                })?;
            let category = kind.code();
            if by.contains_key(&kind) {
                return Err(ReceiversError::Twice { category });
            }
            let packages = packages(source).map_err(|error| ReceiversError::Packages {
                category,
                source: Box::new(error),
            })?;
            for url in [profiles.bundle, profiles.composition] {
                packages
                    .structure_definition(url)
                    .map_err(|error| ReceiversError::Profile {
                        category,
                        url,
                        source: Box::new(error),
                    })?;
            }
            let mapping = Mapping::compile(&source.opt, &source.files, &[source.context.as_str()])
                .map_err(|error| ReceiversError::Compile {
                    category,
                    context: source.context.clone(),
                    source: error,
                })?;
            by.insert(
                kind,
                Receiver {
                    member: source.member.clone(),
                    bundle: profiles.bundle,
                    packages,
                    mapping,
                    package: source
                        .package
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned()),
                    language: source.language.clone(),
                    territory: source.territory.clone(),
                },
            );
        }
        Ok(Self { by })
    }

    /// Returns every received category with the node id of its member, in
    /// the order of Art 14(1).
    pub fn members(&self) -> impl Iterator<Item = (Category, &str)> {
        self.by
            .iter()
            .map(|(category, receiver)| (*category, receiver.member.as_str()))
    }

    /// Returns every category this build carries that can arrive as a
    /// document and that no member receives, in the order of Art 14(1).
    ///
    /// A category with no document profile, such as an electronic
    /// prescription or dispensation, which HL7 Europe MPD 1.0.0 carries as
    /// single resources, never arrives as a document and is not named.
    pub fn unreceived(&self) -> impl Iterator<Item = Category> {
        Category::ENABLED
            .iter()
            .copied()
            .filter(|category| category.profiles().is_some())
            .filter(|category| !self.by.contains_key(category))
    }

    /// Returns whether no category is received.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }

    /// Reads `text`, a document in the exchange format, and prepares the
    /// composition its category's member stores.
    ///
    /// The document is read under the R4 document rules, its category named
    /// from its `Composition.type`, and validated against the category's
    /// `Bundle` profile by FerroBRIDGE's validator over the category's
    /// packages: every constraint of the profiles and of the profiles its
    /// entries claim or their slices name, invariants and bindings included,
    /// a binding the packages cannot decide asked of `validating`'s
    /// `ValueSet/$validate-code`. Its codings are given the displays
    /// `terminology`'s `$lookup` answers where they carry none, and it is
    /// mapped by the category's mapping with `device` as the engine's own
    /// `Device` reference at the instant `now`. With no terminology server,
    /// a binding the packages cannot decide is listed as not evaluated, and
    /// a coding with no display is refused.
    ///
    /// # Errors
    ///
    /// The [`Unreceived`] that names the first step the document fails.
    pub async fn receive(
        &self,
        text: &str,
        (device, now): (&str, &str),
        (terminology, validating): (&Terminology, Option<&Client>),
    ) -> Result<Prepared, Unreceived> {
        let document = ReceivedDocument::read(text)?;
        let category = document.category()?;
        let receiver = self.by.get(&category).ok_or(Unreceived::NoMember {
            category: category.code(),
        })?;
        let validated = document
            .validate(&receiver.packages, receiver.bundle, validating)
            .await
            .map_err(|error| match error {
                ValidationError::NonConformant { findings } => {
                    Unreceived::NonConformant { findings }
                }
                other => Unreceived::Validation(other),
            })?;
        let unevaluated = validated.unevaluated().to_vec();
        let cautions = validated.cautions().to_vec();
        let document = document
            .displayed(terminology)
            .await
            .map_err(Unreceived::Display)?;
        let settings = Settings::new(device, now).with_defaults(
            Defaults::at(now)
                .with_language(receiver.language.as_str())
                .with_territory(receiver.territory.as_str()),
        );
        let composition = receiver
            .mapping
            .to_openehr(&document, &settings)
            .map_err(Unreceived::Mapping)?;
        let subject = document
            .patient()
            .identifier
            .iter()
            .filter_map(|identifier| {
                // NOTE: FHIR R4 Identifier: a value with no system names no namespace an
                // identity service can resolve it in, so it is legitimately no subject key.
                let system = identifier.system.as_ref()?.value.clone()?;
                let value = identifier.value.as_ref()?.value.clone()?;
                Some(Identifier { system, value })
            })
            .collect();
        let transformation = transformed(receiver, category, (&document, &composition))?;
        Ok(Prepared {
            category,
            member: receiver.member.clone(),
            composition,
            unevaluated,
            cautions,
            subject,
            transformation,
        })
    }
}

/// The package sets read so far, by the files they were read from.
type Read = BTreeMap<Vec<(PathBuf, Option<(u64, std::time::SystemTime)>)>, Arc<PackageSet>>;

/// Every package set this process read, so a reload or a second gateway
/// over unchanged files reads them once (no specification governs this: our
/// own design).
static READ: LazyLock<Mutex<Read>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// The package set of `source`'s package and dependencies: the one read
/// before from the same files, of the same length and modification time,
/// or a new read.
fn packages(source: &Source) -> Result<Arc<PackageSet>, LoadError> {
    let paths: Vec<&PathBuf> = std::iter::once(&source.package)
        .chain(source.dependencies.iter())
        .collect();
    // NOTE: no specification governs this: our own design; a file whose metadata cannot be
    // read is keyed without it, and the read that follows reports why it cannot be used.
    let key: Vec<_> = paths
        .iter()
        .map(|path| {
            let stamp = std::fs::metadata(path)
                .and_then(|metadata| Ok((metadata.len(), metadata.modified()?)))
                .ok();
            ((*path).clone(), stamp)
        })
        .collect();
    if let Some(read) = READ
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return Ok(Arc::clone(read));
    }
    let read = Arc::new(PackageSet::load(paths)?);
    READ.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, Arc::clone(&read));
    Ok(read)
}

/// What `receiver`'s mapping did to `document`, of `category`, which it
/// mapped to `composition`: every coded element of every entry of the
/// document as received, and every loss the run declared.
fn transformed(
    receiver: &Receiver,
    category: Category,
    (document, composition): (&ReceivedDocument, &ReceivedComposition),
) -> Result<Report, Unreceived> {
    let template = composition.template_id();
    // NOTE: the run selected the context that wrote the composition's template, so its pin
    // exists; the template alone is named should it not.
    let mut pin = receiver.mapping.pin(template).unwrap_or_else(|| Pin {
        template: template.to_owned(),
        ..Pin::default()
    });
    pin.package.clone_from(&receiver.package);
    let mut report = Report::new(Direction::Received, category, pin);
    for resource in document
        .bundle()
        .entry
        .iter()
        .filter_map(|entry| entry.resource.as_ref())
    {
        report
            .count_coded(resource, preferred)
            .map_err(Unreceived::Transformation)?;
    }
    if let Some(outcome) = composition.outcome() {
        report.count_losses(outcome);
    }
    Ok(report)
}

/// One identifier of the document's patient, with its system.
///
/// `Debug` shows the system and never the value.
#[derive(Clone, PartialEq, Eq)]
pub struct Identifier {
    /// The identifier's system, the namespace it is issued in.
    pub system: String,
    /// The identifier's value.
    pub value: String,
}

impl fmt::Debug for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identifier")
            .field("system", &self.system)
            .finish_non_exhaustive()
    }
}

/// A received document prepared for its member.
#[derive(Debug, Clone)]
pub struct Prepared {
    /// The document's category.
    pub category: Category,
    /// The node id of the member that stores it.
    pub member: String,
    /// The composition, the original document kept in it.
    pub composition: ReceivedComposition,
    /// Every constraint of the profiles the validator met and did not
    /// evaluate.
    pub unevaluated: Vec<Unevaluated>,
    /// Every issue of severity `warning` or `information` the validator
    /// found.
    pub cautions: Vec<Finding>,
    /// Every identifier of the document's patient that has a system and a
    /// value, in document order.
    pub subject: Vec<Identifier>,
    /// What the transformation into openEHR did (eHealth Network General
    /// Guidelines Release 3.1 Art 6(3)).
    pub transformation: Report,
}

/// Where a prepared document was written: the member, the endpoint it was
/// reached through, and the version the member created.
#[derive(Debug, Clone, Copy)]
pub struct Written<'a> {
    /// The member's node id.
    pub member: &'a str,
    /// The endpoint.
    pub endpoint: &'a str,
    /// The `OBJECT_VERSION_ID` of the new composition version.
    pub version: &'a str,
}

/// Returns the `OperationOutcome` of `prepared`, written as `written` says.
///
/// The first issue names the member, the endpoint and the new version; each
/// constraint of the profiles the validator did not evaluate follows as a
/// warning, then each warning the validator found, and each loss the
/// mapping declared, so nothing the receipt left unchecked or unmapped is
/// passed silently.
#[must_use]
pub fn written(prepared: &Prepared, written: Written<'_>) -> OperationOutcome {
    let mut issue = vec![issue(
        ("information", "informational"),
        &format!(
            "the {} document was written to member {} through endpoint {} as version {}",
            prepared.category.code(),
            written.member,
            written.endpoint,
            written.version
        ),
        None,
    )];
    issue.extend(prepared.unevaluated.iter().map(|unevaluated| {
        self::issue(
            ("warning", "not-supported"),
            &format!(
                "{} of {} was not evaluated",
                unevaluated.reason, unevaluated.profile
            ),
            Some(&unevaluated.element),
        )
    }));
    issue.extend(prepared.cautions.iter().map(|caution| {
        let (severity, code) = match &caution.kind {
            FindingKind::Issue { severity, code, .. } => (*severity, *code),
            _ => ("warning", "informational"),
        };
        self::issue(
            (severity, code),
            &format!("{} ({})", caution, caution.profile),
            Some(&caution.location),
        )
    }));
    if let Some(declared) = prepared.composition.outcome() {
        issue.extend(declared.issue.iter().cloned());
    }
    OperationOutcome {
        issue,
        ..OperationOutcome::default()
    }
}

/// Returns the `OperationOutcome` of a document that breaks its profiles,
/// one error issue per finding, located by its element path.
#[must_use]
pub fn nonconformant(findings: &[Finding]) -> OperationOutcome {
    OperationOutcome {
        issue: findings
            .iter()
            .map(|finding| {
                let kind = match finding.kind {
                    FindingKind::Issue { code, .. } => code,
                    FindingKind::TooFew { .. } => "required",
                    FindingKind::Pattern => "value",
                    FindingKind::Invariant { .. } => "invariant",
                    _ => "structure",
                };
                issue(
                    ("error", kind),
                    &format!("{} ({})", finding, finding.profile),
                    Some(&finding.location),
                )
            })
            .collect(),
        ..OperationOutcome::default()
    }
}

/// One issue of `severity` and the FHIR R4 `issue-type` `kind`, with the
/// text `diagnostics`, at the element path `at` when one is named.
fn issue(
    (severity, kind): (&str, &str),
    diagnostics: &str,
    at: Option<&str>,
) -> OperationOutcomeIssue {
    OperationOutcomeIssue {
        severity: severity.into(),
        code: kind.into(),
        diagnostics: Some(diagnostics.into()),
        expression: at.map(Into::into).into_iter().collect(),
        ..OperationOutcomeIssue::default()
    }
}

/// Why a received document is not prepared.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Unreceived {
    /// The text is no document the component reads.
    #[error(transparent)]
    Read(#[from] ReceiveError),
    /// The document is of no one category this build receives.
    #[error(transparent)]
    Category(#[from] Uncategorised),
    /// The deployment declares no member for the document's category.
    #[error("no member receives documents of the category {category}")]
    NoMember {
        /// The category's code.
        category: &'static str,
    },
    /// The document breaks its category's profiles.
    #[error("the document breaks its profiles in {} places", findings.len())]
    NonConformant {
        /// Every finding.
        findings: Vec<Finding>,
    },
    /// The document could not be validated against its profiles, or a
    /// `required` binding of them could not be checked.
    #[error("the document could not be validated against its profiles")]
    Validation(#[source] ValidationError),
    /// The mapping refused the document.
    #[error("the document could not be mapped into openEHR")]
    Mapping(#[source] ReceiveMappingError),
    /// A coding of the document carries no display, and none could be
    /// looked up.
    #[error("a coding of the document has no display")]
    Display(#[source] DisplayError),
    /// The transformation of the document could not be counted.
    #[error("the transformation of the document could not be counted")]
    Transformation(#[source] ReportError),
}
