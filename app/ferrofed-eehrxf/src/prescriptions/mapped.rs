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
use eehrxf::mappings::{MPD, UnknownContext};
use eehrxf::receive::openehr::{InstructionError, ReceiveMappingError, ReceivedComposition};
use eehrxf::receive::{ReceiveError, ReceivedResources};
use fhir_types::codec::{Json, Value};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::reference::Reference;
use fhirconnect::engine::context::CallContext;
use fhirconnect::engine::traverse::defaults::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_adl::artefact::ArchetypeRepository;
use openehr_adl::error::SyntaxError;
use openehr_adl::opt::OptError;
use openehr_adl::parse::Dialect;
use openehr_base::v1_3::base_types::identification::locatable_ref::LocatableRef;
use openehr_base::v1_3::base_types::identification::object_version_id::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::uid_based_id::UidBasedId;
use openehr_mapping_core::template::{PathError, TemplateSource};

use super::RESOURCE;
use super::page::{Kept, Page, Prescription};

/// The context of the MPD project that maps a medication order to a
/// `MedicationRequest-eu-mpd`.
pub const REQUEST_CONTEXT: &str = "ferrobridge_eu_mpd_request.context";

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
    /// The MPD project compiles no request context of the name the map
    /// mode selects.
    #[error("the MPD project has no request context to compile")]
    Context(#[source] UnknownContext),
    /// The MPD project's request context does not compile against the
    /// template.
    #[error("the MPD project's request context does not compile against the template")]
    Compile(#[source] MappingError),
    /// A composition is not mapped.
    #[error("a composition is not mapped")]
    Run(#[source] MappingError),
    /// No language and territory are named for the compositions of received
    /// dispensations.
    #[error("the map mode names no language and territory for a received dispensation")]
    Undefaulted,
    /// A received dispensation is no resource a collection reads.
    #[error("the received dispensation is no resource a collection reads")]
    Received(#[source] ReceiveError),
    /// A received dispensation does not map into a composition.
    #[error("the received dispensation does not map into a composition")]
    Composed(#[source] ReceiveMappingError),
    /// The composition holds no `ACTION` to name the order on.
    #[error("the composition of the received dispensation names no order")]
    Instructed(#[source] InstructionError),
    /// A mapped resource cannot be read back as JSON.
    #[error("a mapped resource cannot be read as JSON")]
    Resource(#[source] fhir_types::codec::EncodeError),
}

impl<'a> Template<'a> {
    /// Returns the template `text` is: an OPT 1.4 when `archetypes` is
    /// empty, an ADL 2 template over `archetypes` otherwise.
    #[must_use]
    pub fn of(text: &'a str, archetypes: &'a [(String, String)]) -> Self {
        if archetypes.is_empty() {
            Self::Opt14(text)
        } else {
            Self::Adl2 {
                template: text,
                archetypes,
            }
        }
    }

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
    project: Mapping,
    defaults: Option<(String, String)>,
}

impl MapMode {
    /// Compiles the MPD project's request context against `template`.
    ///
    /// # Errors
    ///
    /// The [`MapModeError`] that says why the template does not read or the
    /// context does not compile against it.
    pub fn compile(template: Template<'_>) -> Result<Self, MapModeError> {
        let source = template.source()?;
        // NOTE: no specification governs this: our own design; the project's two contexts map one
        // template, so only the request context is compiled and a run selects it alone.
        let request = MPD
            .with_context(REQUEST_CONTEXT)
            .map_err(MapModeError::Context)?;
        let mapping = Mapping::compile_project(&source, &request).map_err(MapModeError::Compile)?;
        let project = Mapping::compile_project(&source, &MPD).map_err(MapModeError::Compile)?;
        Ok(Self {
            mapping,
            project,
            defaults: None,
        })
    }

    /// Returns this mapping with the language and the territory, an ISO 639-1
    /// and an ISO 3166-1 code, each composition of a received dispensation is
    /// written in.
    #[must_use]
    pub fn with_defaults(mut self, language: &str, territory: &str) -> Self {
        self.defaults = Some((language.to_owned(), territory.to_owned()));
        self
    }

    /// Returns whether a language and a territory are named, so a received
    /// dispensation can be composed.
    #[must_use]
    pub fn composes(&self) -> bool {
        self.defaults.is_some()
    }

    /// Maps the received dispensation `body`, one `MedicationDispense-eu-mpd`
    /// with its contained `Medication-eu-mpd`, into one composition of the
    /// member's template at `now`, a FHIR `instant`, its `ACTION` naming the
    /// medication order `order` and its activity `activity` carries out.
    ///
    /// The dispensation is kept, in a `collection` of it alone, in the
    /// composition's `FEEDER_AUDIT.original_content`
    /// (`eehrxf::receive::openehr`), and the composition is written in the
    /// language and the territory [`MapMode::with_defaults`] names (RM 1.1.0
    /// `COMPOSITION.language`, `territory` 1..1).
    ///
    /// # Errors
    ///
    /// [`MapModeError::Undefaulted`] when no language and territory are named,
    /// [`MapModeError::Received`] when the body is no resource the collection
    /// reads, [`MapModeError::Composed`] when it does not map, and
    /// [`MapModeError::Instructed`] when the composition holds no `ACTION`
    /// to name the order on.
    pub fn dispensation(
        &self,
        body: &str,
        (order, activity): (&LocatableRef, &str),
        now: &str,
    ) -> Result<ReceivedComposition, MapModeError> {
        let (language, territory) = self.defaults.as_ref().ok_or(MapModeError::Undefaulted)?;
        let collection = format!(
            r#"{{"resourceType":"Bundle","type":"collection","entry":[{{"resource":{body}}}]}}"#
        );
        let received = ReceivedResources::read(&collection).map_err(MapModeError::Received)?;
        let settings = Settings::new(DEVICE, now).with_defaults(
            Defaults::at(now)
                .with_language(language.as_str())
                .with_territory(territory.as_str()),
        );
        let composed = self
            .project
            .resources_to_openehr(&received, &settings)
            .map_err(MapModeError::Composed)?;
        composed
            .instructed_by(order, activity)
            .map_err(MapModeError::Instructed)
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
        let answer = self
            .mapping
            .to_fhir_with(
                composition,
                &Settings::new(DEVICE, now),
                CallContext::new().with_patient(patient),
            )
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
    let mut repository = ArchetypeRepository::default();
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
    // NOTE: openEHR AM 2.4 AOM2 §ARCHETYPE_HRID; the template is named by its full id, namespace
    // and version modifier included, the physical id `Display` writes.
    let resolved_id = built.archetype_id.to_string();
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

/// The activity of a medication order a dispensation carries out, the one
/// activity `openEHR-EHR-INSTRUCTION.medication_order.v3` defines, "Order".
pub const ORDER_ACTIVITY: &str = "activities[at0001]";

/// Returns the reference to one medication order of a composition.
///
/// The order is the one at position `entry` among the `INSTRUCTION`s
/// directly under `content` of `composition`, a composition in canonical
/// JSON whose version is `version`; `None` when it holds no such order, its
/// archetype node id names another too, or `version` is no
/// `OBJECT_VERSION_ID`.
///
/// The reference is a `LOCATABLE_REF` to the versioned composition by its
/// `OBJECT_VERSION_ID`, in the `local` namespace, with the archetype path of
/// the instruction (RM 1.1.0 EHR IM `INSTRUCTION_DETAILS.instruction_id`;
/// BASE `LOCATABLE_REF`). An order two `INSTRUCTION`s of one archetype node
/// id share has no path of its own, so it names none: our own design.
#[must_use]
pub fn order_in(composition: &str, version: &str, entry: usize) -> Option<LocatableRef> {
    // NOTE: RM 1.1.0 canonical JSON: a composition that does not read holds no order to name,
    // so it legitimately names none.
    let read: Contents = serde_json::from_str(composition).ok()?;
    let orders: Vec<&str> = read
        .content
        .iter()
        .filter(|item| item.kind.as_deref() == Some("INSTRUCTION"))
        .filter_map(|item| item.archetype_node_id.as_deref())
        .collect();
    let node = *orders.get(entry)?;
    if orders.iter().filter(|other| **other == node).count() != 1 {
        return None;
    }
    let version = ObjectVersionId::new(version).ok()?;
    Some(LocatableRef {
        namespace: String::from("local"),
        r#type: String::from("INSTRUCTION"),
        id: UidBasedId::ObjectVersionId(version),
        path: Some(format!("/content[{node}]")),
    })
}

/// What a composition is read for to name an order: its content items.
#[derive(serde::Deserialize)]
struct Contents {
    /// The `content`.
    #[serde(default)]
    content: Vec<Item>,
}

/// One content item, by its RM type and its archetype node id.
#[derive(serde::Deserialize)]
struct Item {
    /// The `_type`.
    #[serde(rename = "_type")]
    kind: Option<String>,
    /// The `archetype_node_id`.
    archetype_node_id: Option<String>,
}
