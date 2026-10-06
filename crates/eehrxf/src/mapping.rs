// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The exchange format in FHIR R4, mapped from openEHR by FHIRconnect.
//!
//! FHIRconnect 1.0.0 maps an openEHR composition to FHIR resources through a
//! context mapping per (profile, template) and the model mappings it starts
//! from (<https://sevkohler.github.io/FHIRconnect-spec/build/site/FHIRconnect/v1.0.0/basics/main.html>).
//! This module runs FerroBRIDGE's `fhirconnect` engine in process: a
//! [`Mapping`] compiles the context mappings once against the operational
//! template, and [`Mapping::to_fhir`] runs the compiled program over each
//! canonical-JSON composition of that template, answering a FHIR R4 `Bundle`
//! of the mapped resources with one `Provenance` covering them. The crate
//! authors no mapping language and no per-category mapping code: the mapping
//! files are the input.

use std::fmt;
use std::path::Path;

use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::traverse::functions::NoMappingFunctions;
use fhirconnect::model::load::load_set;
use fhirconnect::model::semantic::StaticMappingCodes;
use fhirconnect::operations::contract::CompositionPayload;
use fhirconnect::operations::contract::ToFhirRequest;
use fhirconnect::operations::contract::ToFhirResponse;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::programs::ProgramSet;
use fhirconnect::operations::programs::ProgramSetError;
use fhirconnect::operations::run;
use fhirconnect::operations::run::Settings;
use fhirconnect::resolve::compile::compile;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::header::metadata::MappingNameError;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::PathError;
use openehr_mapping_core::template::TemplateSource;

/// FHIRconnect context mappings compiled against one operational template.
#[derive(Debug)]
pub struct Mapping {
    programs: ProgramSet,
}

impl Mapping {
    /// Compiles the context mappings `contexts` names from the mapping
    /// `files`, against the OPT 1.4 operational template `opt`.
    ///
    /// Every file is loaded, checked against the FHIRconnect schemas and the
    /// semantic rules, and every context compiled, before the mapping exists;
    /// no mapping function is registered, so a file that calls one is
    /// refused.
    ///
    /// # Errors
    ///
    /// Returns [`MappingError::Template`] when `opt` is not an OPT 1.4
    /// template, [`MappingError::ContextName`] when a context name is not a
    /// mapping name, [`MappingError::Refused`] with every diagnostic when a
    /// file does not load or a context does not compile, and
    /// [`MappingError::Program`] when a context maps another template.
    pub fn compile<P: AsRef<Path>>(
        opt: &str,
        files: &[P],
        contexts: &[&str],
    ) -> Result<Self, MappingError> {
        let source = TemplateSource::opt14(opt).map_err(|source| MappingError::Template {
            source: Box::new(source),
        })?;
        let index = WebTemplateIndex::build(&source).map_err(|source| MappingError::Template {
            source: Box::new(source),
        })?;
        let codes = StaticMappingCodes::default();
        let set = load_set(files.iter().map(AsRef::as_ref), &codes)
            .map_err(|diagnostics| MappingError::Refused { diagnostics })?;
        let mut compiled = Vec::with_capacity(contexts.len());
        for context in contexts {
            let name = MappingName::new(*context)
                .map_err(|source| MappingError::ContextName { source })?;
            compiled.push(
                compile(&set, &name, &index, &SCHEMAS, &codes)
                    .map_err(|diagnostics| MappingError::Refused { diagnostics })?,
            );
        }
        let mut programs = ProgramSet::new();
        programs.insert_template(index);
        for program in compiled {
            programs
                .insert_program(program)
                .map_err(|source| MappingError::Program { source })?;
        }
        Ok(Self { programs })
    }

    /// Returns how many compiled context mappings the mapping holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.programs.len()
    }

    /// Returns whether the mapping holds no compiled context mapping.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.programs.is_empty()
    }

    /// Runs the mapping over one canonical-JSON composition.
    ///
    /// The composition names its template at `archetype_details.template_id`,
    /// and the program compiled for that template maps it. The answer is a
    /// FHIR R4 `collection` Bundle of the mapped resources, one `Provenance`
    /// covering them, and an `OperationOutcome` entry when the run declared a
    /// loss.
    ///
    /// # Errors
    ///
    /// Returns [`MappingError::NotCanonical`] when `composition` is a JSON
    /// object without `_type` (a flat composition), and
    /// [`MappingError::Run`] when it does not parse, names a template the
    /// mapping was not compiled against, or carries an element the mapping
    /// cannot carry.
    pub fn to_fhir(
        &self,
        composition: &str,
        settings: &Settings,
    ) -> Result<ToFhirResponse, MappingError> {
        let payload =
            CompositionPayload::parse(composition).map_err(|source| MappingError::Run {
                source: Box::new(source),
            })?;
        if !matches!(payload, CompositionPayload::Canonical(_)) {
            return Err(MappingError::NotCanonical);
        }
        run::to_fhir(
            &self.programs,
            &SCHEMAS,
            &NoMappingFunctions,
            settings,
            &ToFhirRequest::new(payload),
        )
        .map_err(|source| MappingError::Run {
            source: Box::new(source),
        })
    }
}

/// Why a mapping cannot be compiled or run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MappingError {
    /// The operational template is not an OPT 1.4 template the index can be
    /// built over.
    #[error("the operational template cannot be read")]
    Template {
        /// The template failure.
        #[source]
        source: Box<PathError>,
    },
    /// A context name is not a FHIRconnect mapping name.
    #[error("a context name is not a mapping name")]
    ContextName {
        /// The name failure.
        #[source]
        source: MappingNameError,
    },
    /// A mapping file did not load or a context did not compile.
    #[error("the mapping files were refused: {}", Refusals(diagnostics))]
    Refused {
        /// Every diagnostic, in the engine's order.
        diagnostics: Vec<Diagnostic>,
    },
    /// A compiled context maps a template other than the one supplied.
    #[error("a context maps another template")]
    Program {
        /// The program-set failure.
        #[source]
        source: ProgramSetError,
    },
    /// The composition is a flat composition, which this entry point does
    /// not take.
    #[error("the composition is not canonical JSON: it carries no _type")]
    NotCanonical,
    /// The engine refused the run.
    #[error("the mapping run was refused")]
    Run {
        /// The engine's refusal.
        #[source]
        source: Box<OperationError>,
    },
}

/// Renders a diagnostic list on one line.
struct Refusals<'a>(&'a [Diagnostic]);

impl fmt::Display for Refusals<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, diagnostic) in self.0.iter().enumerate() {
            if position > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{diagnostic}")?;
        }
        Ok(())
    }
}
