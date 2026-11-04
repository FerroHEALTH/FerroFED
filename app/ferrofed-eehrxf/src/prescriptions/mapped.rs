// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The map mode of the prescription face.
//!
//! A member's medication orders are mapped into HL7 Europe MPD 1.0.0
//! `MedicationRequest-eu-mpd` resources by the FHIRconnect project the
//! interoperability component ships (`eehrxf::mappings::MPD`).
//! A member in the map mode serves no FHIR endpoint of its own: the face
//! reads its medication order compositions over ITS-REST
//! ([`crate::medication::Medication::Orders`]) and maps each through the
//! project's request context, compiled once against the member's
//! operational template when the configuration is read. The template is an
//! OPT 1.4 or an ADL 2 template, whose operational template is built from it
//! and the archetypes it specialises (openEHR AM 2.4 OPT2 §Operational
//! Template). Each composition gives one [`Page`]: its prescriptions as
//! matches, the resources they reference as includes, and the losses the run
//! declared as outcomes. The run's own `Provenance` is left out, since the
//! searchset names the member each prescription comes from with its own.
//! Which context compiles and what a page keeps are our own design: no
//! specification governs them.

use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::mappings::MPD;
use fhir_types::codec::{Json, Value};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::reference::Reference;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::error::SyntaxError;
use openehr_adl::opt::OptError;
use openehr_adl::parse::Dialect;
use openehr_mapping_core::template::{PathError, TemplateSource};

use super::RESOURCE;
use super::page::{Kept, Page, Prescription};

/// The context of the MPD project that maps a medication order to a
/// `MedicationRequest-eu-mpd`.
pub const REQUEST_CONTEXT: &str = "ferrobridge_eu_mpd_request.context";

/// The stack a template is built and a composition mapped on: the engine
/// walks the template and the composition recursively, deeper than the 2 MiB
/// a runtime worker thread has.
const STACK: usize = 16 * 1024 * 1024;

/// The device a mapping run names as its engine, which no answer carries
/// since the run's `Provenance` is left out.
const DEVICE: &str = "Device/ferrofed-map-mode";

/// The operational template a map-mode member's compositions are of, as a
/// deployment writes it.
#[derive(Debug, Clone, Copy)]
pub enum Template<'a> {
    /// An OPT 1.4 operational template, its XML text.
    Opt14(&'a str),
    /// An ADL 2 template, its text, with every archetype it specialises or
    /// fills a slot with, each a file name and its ADL 1.4 or ADL 2 text.
    Adl2 {
        /// The template's text.
        template: &'a str,
        /// The archetypes, each a file name and its text.
        archetypes: &'a [(String, String)],
    },
}

/// Why a map-mode member's mapping is refused or a composition is not
/// mapped; no variant's text names a value a composition carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MapModeError {
    /// The OPT 1.4 template does not read.
    #[error("the OPT 1.4 template does not read")]
    Opt14(#[source] PathError),
    /// The ADL 2 template, or one of its archetypes, does not parse.
    #[error("{file} does not parse as ADL")]
    Syntax {
        /// The file, or `the template`.
        file: String,
        /// The first syntax error the parser named.
        #[source]
        source: Option<SyntaxError>,
    },
    /// The operational template cannot be built from the template and its
    /// archetypes.
    #[error("the operational template cannot be built from the template and its archetypes")]
    Operational(#[source] OptError),
    /// The project's files could not be written where they are compiled
    /// from.
    #[error("the MPD project's files could not be written to compile them")]
    Files(#[source] std::io::Error),
    /// The MPD project's request context does not compile against the
    /// template.
    #[error("the MPD project's request context does not compile against the template")]
    Compile(#[source] MappingError),
    /// A composition is not mapped.
    #[error("a composition is not mapped")]
    Run(#[source] MappingError),
    /// The thread the work runs on could not be started.
    #[error("the thread a mapping runs on could not be started")]
    Thread(#[source] std::io::Error),
    /// The work panicked on its thread, a defect of the mapping engine.
    #[error("a mapping panicked on its thread")]
    Panicked,
    /// A mapped resource cannot be read back as JSON.
    #[error("a mapped resource cannot be read as JSON")]
    Resource(#[source] fhir_types::codec::EncodeError),
}

impl Template<'_> {
    /// Returns the operational template, read from an OPT 1.4 or built from an
    /// ADL 2 template and its archetypes.
    ///
    /// # Errors
    ///
    /// [`MapModeError::Opt14`] for an OPT 1.4 that does not read,
    /// [`MapModeError::Syntax`] for an ADL file that does not parse, and
    /// [`MapModeError::Operational`] when the operational template cannot be
    /// built.
    pub fn source(self) -> Result<TemplateSource, MapModeError> {
        match self {
            Self::Opt14(xml) => TemplateSource::opt14(xml).map_err(MapModeError::Opt14),
            Self::Adl2 {
                template,
                archetypes,
            } => operational(template, archetypes),
        }
    }
}

/// A map-mode member's mapping: the MPD project's request context compiled
/// against the member's operational template.
#[derive(Debug)]
pub struct MapMode {
    mapping: Mapping,
}

impl MapMode {
    /// Compiles the MPD project's request context against `template`.
    ///
    /// # Errors
    ///
    /// The [`MapModeError`] that says why the template does not read or the
    /// context does not compile against it.
    pub fn compile(template: Template<'_>) -> Result<Self, MapModeError> {
        deep(|| Self::compiled(template))?
    }

    /// Compiles the MPD project's request context against `template` on the
    /// calling thread.
    fn compiled(template: Template<'_>) -> Result<Self, MapModeError> {
        let source = template.source()?;
        // NOTE: no specification governs this: our own design; the project's two contexts map one
        // template, so only the request context is compiled and a run selects it alone.
        let directory = tempfile::tempdir().map_err(MapModeError::Files)?;
        let files = MPD
            .write_to(directory.path())
            .map_err(MapModeError::Files)?;
        let mapping = Mapping::compile_template(&source, &files, &[REQUEST_CONTEXT])
            .map_err(MapModeError::Compile)?;
        Ok(Self { mapping })
    }

    /// Maps `composition`, a medication order composition in canonical
    /// JSON, into one page of prescriptions, each naming the patient by the
    /// identifier `system`|`value` the search names, mapped at `now`, a FHIR
    /// `instant`.
    ///
    /// The patient is named by its identifier alone (FHIR R4
    /// `Reference.identifier`), since the gateway holds no `Patient` for it.
    ///
    /// # Errors
    ///
    /// [`MapModeError::Run`] when the composition is not mapped, and
    /// [`MapModeError::Resource`] when a mapped resource does not read back.
    pub fn page(
        &self,
        composition: &str,
        (system, value): (&str, &str),
        now: &str,
    ) -> Result<Page, MapModeError> {
        let patient = Reference {
            identifier: Some(Box::new(Identifier {
                system: Some(system.into()),
                value: Some(value.into()),
                ..Identifier::default()
            })),
            ..Reference::default()
        };
        let answer = deep(|| {
            self.mapping.to_fhir_with(
                composition,
                &Settings::new(DEVICE, now),
                CallContext::new().with_patient(patient),
            )
        })?
        .map_err(MapModeError::Run)?;
        let mut page = Page::default();
        for resource in answer
            .bundle()
            .entry
            .iter()
            .filter_map(|entry| entry.resource.as_ref())
        {
            let object = Json::to_json(resource).map_err(MapModeError::Resource)?;
            let resource_type = object
                .get("resourceType")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let kept = Kept {
                full_url: None,
                resource: Value::Object(object),
                resource_type,
            };
            match kept.resource_type.as_str() {
                RESOURCE => page.prescriptions.push(Prescription {
                    identifiers: identifiers(&kept.resource),
                    kept,
                    unreadable: false,
                }),
                "OperationOutcome" => page.outcomes.push(kept),
                "Provenance" => {}
                _ => page.included.push(kept),
            }
        }
        Ok(page)
    }
}

/// The operational template of the ADL 2 `template` and its `archetypes`.
fn operational(
    template: &str,
    archetypes: &[(String, String)],
) -> Result<TemplateSource, MapModeError> {
    let mut repository = ArchetypeRepository::new();
    for (file, text) in archetypes {
        let extension = std::path::Path::new(file).extension();
        let dialect = if extension.is_some_and(|extension| {
            extension.eq_ignore_ascii_case("adls") || extension.eq_ignore_ascii_case("adlt")
        }) {
            Dialect::Adl2
        } else {
            Dialect::Adl14
        };
        repository
            .insert(openehr_adl::assemble::parse_artefact(text, dialect).map_err(syntax(file))?);
    }
    let root = openehr_adl::assemble::parse_artefact(template, Dialect::Adl2)
        .map_err(syntax("the template"))?;
    let built =
        openehr_adl::opt::create_opt(&root, &repository).map_err(MapModeError::Operational)?;
    let id = &built.archetype_id;
    // NOTE: openEHR AM 2.4 AOM2 §ARCHETYPE_HRID; the template is named by its interface id,
    // `rm_publisher-rm_package-rm_class.concept.vmajor.minor.patch`.
    let resolved_id = format!(
        "{}-{}-{}.{}.v{}",
        id.rm_publisher, id.rm_package, id.rm_class, id.concept_id, id.release_version
    );
    Ok(TemplateSource::Opt2 {
        template: Box::new(built),
        resolved_id,
    })
}

/// The business identifiers of `resource` that have both a system and a
/// value (`MedicationRequest.identifier`).
fn identifiers(resource: &Value) -> Vec<(String, String)> {
    let Some(Value::Array(held)) = resource.get("identifier") else {
        return Vec::new();
    };
    held.iter()
        .filter_map(|identifier| {
            let system = identifier.get("system").and_then(Value::as_str)?;
            let value = identifier.get("value").and_then(Value::as_str)?;
            (!system.is_empty() && !value.is_empty()).then(|| (system.to_owned(), value.to_owned()))
        })
        .collect()
}

/// The refusal of the artefact `file` names, from the syntax errors its
/// parse gave.
fn syntax(file: &str) -> impl FnOnce(Vec<SyntaxError>) -> MapModeError + use<'_> {
    move |errors| MapModeError::Syntax {
        file: file.to_owned(),
        source: errors.into_iter().next(),
    }
}

/// Runs `work` on a thread of its own with [`STACK`] of stack, and waits for
/// its answer.
///
/// No specification governs this: our own design. The caller's thread,
/// a runtime worker among them, keeps its own stack size.
///
/// # Errors
///
/// [`MapModeError::Thread`] when the thread cannot be started, and
/// [`MapModeError::Panicked`] when `work` panicked.
fn deep<T: Send>(work: impl FnOnce() -> T + Send) -> Result<T, MapModeError> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name(String::from("ferrofed-map-mode"))
            .stack_size(STACK)
            .spawn_scoped(scope, work)
            .map_err(MapModeError::Thread)?
            .join()
            .map_err(|_panicked| MapModeError::Panicked)
    })
}
