// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The exchange format in FHIR R4, mapped from openEHR by FHIRconnect
//! (feature `openehr`).
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

use fhir_types::codec::{Json, Object, Value};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::resource::Resource;
use fhir_types::r4::schema::SCHEMAS;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::outcome::Warning;
use fhirconnect::engine::seam::Seams;
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
use fhirconnect::resolve::select::select_by_template;
use openehr_mapping_core::diagnostic::Diagnostic;
use openehr_mapping_core::header::metadata::MappingName;
use openehr_mapping_core::header::metadata::MappingNameError;
use openehr_mapping_core::index::WebTemplateIndex;
use openehr_mapping_core::template::PathError;
use openehr_mapping_core::template::TemplateSource;

use crate::transformation::Pin;

/// FHIRconnect context mappings compiled against one operational template.
#[derive(Debug)]
pub struct Mapping {
    programs: ProgramSet,
    files: Vec<String>,
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
        let files = files
            .iter()
            .filter_map(|file| file.as_ref().file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        Ok(Self { programs, files })
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

    /// Returns the file names of the model and context mappings it was
    /// compiled from, in the order given.
    #[must_use]
    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Returns the pin of the context mapping compiled for `template`: its
    /// name, the template, the profile it maps to and the mapping's files, or
    /// `None` when no context maps that template.
    #[must_use]
    pub fn pin(&self, template: &str) -> Option<Pin> {
        self.contexts()
            .find(|context| context.template == template)
            .map(|context| Pin {
                context: Some(context.name.to_owned()),
                template: context.template.to_owned(),
                profile: Some(context.profile.to_owned()),
                files: self.files.clone(),
                package: None,
            })
    }

    /// Returns the id of the archetype the context compiled for `template`
    /// starts at, the root of each resource a run writes, or `None` when no
    /// context maps that template or its start model names no archetype.
    #[must_use]
    pub fn start_archetype(&self, template: &str) -> Option<String> {
        let program = self
            .programs
            .programs()
            .iter()
            .find(|program| program.template().id().as_str() == template)?;
        program
            .models()
            .iter()
            .find(|model| model.name() == program.start())?
            .archetype()
            .map(ToString::to_string)
    }

    /// Returns the compiled programs, for the receive direction.
    pub(crate) const fn programs(&self) -> &ProgramSet {
        &self.programs
    }

    /// Returns the compiled context mappings, each with the template it maps
    /// and the profile it maps to, in compilation order.
    pub fn contexts(&self) -> impl Iterator<Item = Context<'_>> {
        self.programs.programs().iter().map(|program| Context {
            name: program.context().as_str(),
            template: program.template().id().as_str(),
            profile: program.profile().url().as_str(),
        })
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
        self.run(composition, settings, None)
            .map(|(response, _)| response)
    }

    /// Runs the mapping over one canonical-JSON composition under the call
    /// context `context`, as [`Mapping::to_fhir`] does.
    ///
    /// The context's `patient` is written over the subject of every mapped
    /// resource whose type has a `subject` or a `patient` element, and its
    /// `who` and `onBehalfOf` name the agent of the `Provenance`
    /// (FHIRconnect 1.0.0 `$tofhir` context).
    ///
    /// # Errors
    ///
    /// The errors of [`Mapping::to_fhir`].
    pub fn to_fhir_with(
        &self,
        composition: &str,
        settings: &Settings,
        context: CallContext,
    ) -> Result<ToFhirResponse, MappingError> {
        self.run(composition, settings, Some(context))
            .map(|(response, _)| response)
    }

    /// Runs the mapping as [`Mapping::to_fhir_with`] does, and answers beside
    /// the Bundle the losses the run declared, typed, in the order the engine
    /// raised them: the same losses the Bundle's `OperationOutcome` renders.
    ///
    /// # Errors
    ///
    /// The errors of [`Mapping::to_fhir`].
    pub fn to_fhir_with_warnings(
        &self,
        composition: &str,
        settings: &Settings,
        context: CallContext,
    ) -> Result<(ToFhirResponse, Vec<Warning>), MappingError> {
        self.run(composition, settings, Some(context))
    }

    /// Runs the mapping over `composition`, under `context` when given.
    fn run(
        &self,
        composition: &str,
        settings: &Settings,
        context: Option<CallContext>,
    ) -> Result<(ToFhirResponse, Vec<Warning>), MappingError> {
        let payload =
            CompositionPayload::parse(composition).map_err(|source| MappingError::Run {
                source: Box::new(source),
            })?;
        if !matches!(payload, CompositionPayload::Canonical(_)) {
            return Err(MappingError::NotCanonical);
        }
        let template = payload.template_id();
        let request = ToFhirRequest::new(payload);
        let request = match context {
            Some(context) => request.with_context(context),
            None => request,
        };
        let refused = |source: OperationError| MappingError::Run {
            source: Box::new(source),
        };
        let (response, warnings) = run::to_fhir_with_warnings(
            &self.programs,
            &SCHEMAS,
            &Seams::default(),
            settings,
            &request,
        )
        .map_err(refused)?
        .into_parts();
        let template = template.ok_or_else(|| refused(OperationError::CanonicalTemplate))?;
        let program = select_by_template(self.programs.programs(), &template)
            .map_err(|source| refused(OperationError::Select { source }))?;
        let mut bundle = response.into_bundle();
        claim(
            &mut bundle,
            program.resource().as_str(),
            program.profile().url().as_str(),
        )?;
        Ok((ToFhirResponse::new(bundle), warnings))
    }
}

/// Claims `profile` in the `meta.profile` of every mapped resource of
/// `bundle`, each entry of `resource_type`, unless it claims it already.
///
/// The context mapping that ran maps to `profile`, so the resource it wrote
/// claims conformance to it, appended after any profile it claims.
// NOTE: FHIR R4 Resource.meta.profile is a claim of conformance; the run's context maps to
// `profile`, so the mapped resource claims it, and a receiver selects its mapping by it.
fn claim(bundle: &mut Bundle, resource_type: &str, profile: &str) -> Result<(), MappingError> {
    let mut claimed = false;
    for resource in bundle
        .entry
        .iter_mut()
        .filter_map(|entry| entry.resource.as_mut())
    {
        let mut object = Json::to_json(&*resource).map_err(claim_error)?;
        if !matches!(object.get("resourceType"), Some(Value::String(name)) if name == resource_type)
        {
            continue;
        }
        let meta = object
            .entry(String::from("meta"))
            .or_insert_with(|| Value::Object(Object::new()));
        let Value::Object(meta) = meta else {
            return Err(claim_error(Unclaimable("its meta is no object")));
        };
        let profiles = meta
            .entry(String::from("profile"))
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(profiles) = profiles else {
            return Err(claim_error(Unclaimable("its meta.profile is no array")));
        };
        if !profiles
            .iter()
            .any(|held| matches!(held, Value::String(url) if url == profile))
        {
            profiles.push(Value::String(profile.to_owned()));
        }
        *resource = Resource::from_json(&object, &mut fhir_types::codec::Path::root(resource_type))
            .map_err(claim_error)?;
        claimed = true;
    }
    if claimed {
        Ok(())
    } else {
        Err(claim_error(Unclaimable(
            "the run wrote no resource of the mapped type",
        )))
    }
}

/// The claim failure `source`.
fn claim_error(source: impl std::error::Error + Send + Sync + 'static) -> MappingError {
    MappingError::Claim {
        source: Box::new(source),
    }
}

/// Why the mapped resource cannot claim its profile.
#[derive(Debug, thiserror::Error)]
#[error("the mapped resource cannot claim its profile: {0}")]
struct Unclaimable(&'static str);

/// One compiled context mapping: its name, the template it maps and the
/// profile it maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context<'a> {
    /// The context mapping's `metadata.name`.
    pub name: &'a str,
    /// The id of the operational template it maps.
    pub template: &'a str,
    /// The canonical URL of the profile it maps to, as
    /// `context.profile.url` names it.
    pub profile: &'a str,
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
    /// The mapped resource could not claim the profile its context maps to.
    #[error("the mapped resource cannot claim its profile")]
    Claim {
        /// The encoding failure.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
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

#[cfg(test)]
mod tests {
    use fhir_types::codec::{Json, Path, Value};
    use fhir_types::r4::bundle::Bundle;

    use super::claim;

    const PROFILE: &str = "http://example.org/fhir/StructureDefinition/ferrofed-allergy";

    const PROVENANCE: &str = r#"{ "resource": { "resourceType": "Provenance", "id": "v", "target": [{ "reference": "AllergyIntolerance/a1" }], "recorded": "2026-10-14T10:00:00Z", "agent": [{ "who": { "display": "Synthetic" } }] } }"#;

    fn bundle(entries: &[String]) -> Bundle {
        let text = format!(
            r#"{{ "resourceType": "Bundle", "type": "collection", "entry": [{}] }}"#,
            entries.join(",")
        );
        let Ok(Value::Object(read)) = serde_json::from_str::<Value>(&text) else {
            panic!("the fixture should be a JSON object");
        };
        Bundle::from_json(&read, &mut Path::root("Bundle")).expect("the fixture should decode")
    }

    fn allergy(id: &str, meta: &str) -> String {
        format!(
            r#"{{ "resource": {{ "resourceType": "AllergyIntolerance", "id": "{id}"{meta}, "patient": {{ "reference": "Patient/p" }} }} }}"#
        )
    }

    fn claims(bundle: &Bundle) -> Vec<Vec<String>> {
        bundle
            .entry
            .iter()
            .filter_map(|entry| entry.resource.as_ref())
            .map(|resource| Json::to_json(resource).expect("an entry should encode"))
            .filter(|object| {
                object.get("resourceType").and_then(Value::as_str) == Some("AllergyIntolerance")
            })
            .map(|object| {
                object
                    .get("meta")
                    .and_then(|meta| meta.get("profile"))
                    .and_then(Value::as_array)
                    .map(|profiles| {
                        profiles
                            .iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    #[test]
    fn every_entry_of_the_mapped_type_claims_the_profile_once() {
        let mut mapped = bundle(&[
            allergy("a1", ""),
            String::from(PROVENANCE),
            allergy(
                "a2",
                &format!(r#", "meta": {{ "profile": ["{PROFILE}"] }}"#),
            ),
        ]);
        claim(&mut mapped, "AllergyIntolerance", PROFILE).expect("the run should claim");
        assert_eq!(
            claims(&mapped),
            vec![vec![String::from(PROFILE)], vec![String::from(PROFILE)]],
            "both allergies claim the profile, and the one that held it holds it once"
        );
    }

    #[test]
    fn a_run_with_no_entry_of_the_mapped_type_is_refused() {
        let mut mapped = bundle(&[String::from(PROVENANCE)]);
        assert!(claim(&mut mapped, "AllergyIntolerance", PROFILE).is_err());
    }
}
