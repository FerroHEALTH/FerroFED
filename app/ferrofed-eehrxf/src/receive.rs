// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Documents received in the exchange format, prepared for the one member
//! the deployment declares for their category (Regulation (EU) 2025/327
//! Annex II 2.2 and 2.3, Art 14(1)).
//!
//! A deployment declares, per priority category it receives, the member that
//! stores the category's documents, the vendored HL7 Europe package whose
//! profiles a document is checked against, and the FHIRconnect mapping that
//! turns it into one openEHR composition ([`Source`]). [`Receivers::receive`]
//! reads a document, names its category from its `Composition.type`, holds
//! it to the category's profiles and maps it, the original kept in the
//! composition's `FEEDER_AUDIT.original_content`. The result names the
//! member and the patient identifiers the document carries, which the
//! gateway resolves to that member's own `ehr_id` before anything is sent
//! (Federation Tier §5.2, §12.4, N23). Nothing here reaches a member.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use eehrxf::category::Category;
use eehrxf::dataset::{DatasetError, ResourceProfile};
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::receive::category::Uncategorised;
use eehrxf::receive::conform::{CheckError, Finding, FindingKind, Unevaluated};
use eehrxf::receive::openehr::{ReceiveMappingError, ReceivedComposition};
use eehrxf::receive::{ReceiveError, ReceivedDocument};
use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;

/// One category the deployment receives, as it names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The category, by its code ([`Category::code`]), such as
    /// `Patient-Summaries`.
    pub category: String,
    /// The node id of the member that stores the category's documents.
    pub member: String,
    /// The vendored HL7 Europe package, as a `.tgz` archive, whose profiles
    /// a document of the category is checked against.
    pub package: PathBuf,
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
    /// The package cannot be opened.
    #[error("the package {} of the category {category} cannot be read", path.display())]
    Package {
        /// The category's code.
        category: &'static str,
        /// The package path.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// The package holds no usable profile under a URL the category names.
    #[error("the package of the category {category} gives no profile {url}")]
    Profile {
        /// The category's code.
        category: &'static str,
        /// The profile's canonical URL.
        url: &'static str,
        /// Why.
        #[source]
        source: DatasetError,
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
    /// The `Bundle` profile.
    bundle: ResourceProfile,
    /// The `Composition` profile.
    composition: ResourceProfile,
    /// The mapping into openEHR.
    mapping: Mapping,
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
    /// Reads the profiles and compiles the mapping of every one of
    /// `sources`.
    ///
    /// # Errors
    ///
    /// [`ReceiversError::Twice`] for a category declared twice,
    /// [`ReceiversError::Unreceived`] for one this build does not receive,
    /// [`ReceiversError::Package`] and [`ReceiversError::Profile`] for a
    /// package that gives no profile the category names, and
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
            let profile = |url: &'static str| {
                let package = std::fs::File::open(&source.package).map_err(|error| {
                    ReceiversError::Package {
                        category,
                        path: source.package.clone(),
                        source: error,
                    }
                })?;
                ResourceProfile::read(package, url).map_err(|error| ReceiversError::Profile {
                    category,
                    url,
                    source: error,
                })
            };
            let bundle = profile(profiles.bundle)?;
            let composition = profile(profiles.composition)?;
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
                    bundle,
                    composition,
                    mapping,
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

    /// Returns whether no category is received.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }

    /// Reads `text`, a document in the exchange format, and prepares the
    /// composition its category's member stores.
    ///
    /// The document is read under the R4 document rules, its category named
    /// from its `Composition.type`, held to the category's profiles, and
    /// mapped by the category's mapping with `device` as the engine's own
    /// `Device` reference at the instant `now`.
    ///
    /// # Errors
    ///
    /// The [`Unreceived`] that names the first step the document fails.
    pub fn receive(&self, text: &str, (device, now): (&str, &str)) -> Result<Prepared, Unreceived> {
        let document = ReceivedDocument::read(text)?;
        let category = document.category()?;
        let receiver = self.by.get(&category).ok_or(Unreceived::NoMember {
            category: category.code(),
        })?;
        let conformance = document
            .check(&receiver.bundle, &receiver.composition)
            .map_err(|error| match error {
                CheckError::NonConformant { findings } => Unreceived::NonConformant { findings },
                other => Unreceived::Check(other),
            })?;
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
        Ok(Prepared {
            category,
            member: receiver.member.clone(),
            composition,
            unevaluated: conformance.unevaluated().to_vec(),
            subject,
        })
    }
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
    /// Every constraint of the profiles the check could not evaluate.
    pub unevaluated: Vec<Unevaluated>,
    /// Every identifier of the document's patient that has a system and a
    /// value, in document order.
    pub subject: Vec<Identifier>,
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
/// constraint of the profiles the check did not evaluate follows as a
/// warning, and so does each loss the mapping declared, so nothing the
/// receipt left unchecked or unmapped is passed silently.
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
    /// The document could not be checked against its profiles.
    #[error("the document could not be checked against its profiles")]
    Check(#[source] CheckError),
    /// The mapping refused the document.
    #[error("the document could not be mapped into openEHR")]
    Mapping(#[source] ReceiveMappingError),
}
