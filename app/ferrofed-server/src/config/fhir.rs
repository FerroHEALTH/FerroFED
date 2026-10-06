// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[fhir]` table: the FHIR R4 face of the European exchange format
//! (Regulation (EU) 2025/327 Annex II 2.1).
//!
//! The face sits on a base of its own, `{fhir-base}`, beside `{base}`, so
//! no ITS-REST path changes (Federation Tier §4.1, N28). It names its
//! document's entries under the absolute URL of that base, which it takes
//! from `server.public_url`, and writes the document as the operator the
//! table names. Each `[[fhir.mapping]]` is one FHIRconnect context mapping
//! that feeds one patient summary section, compiled when the configuration
//! is read, so a mapping that does not compile, or maps to a profile the
//! section does not take, refuses the start and `config check`. The section
//! queries the face runs are the gateway's own, compiled into it, so the
//! face does not need `[stored_queries]`. No specification governs the
//! configuration: our own design.

use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_eehrxf::summary::mappings::{Mappings, MappingsError, Source};
use serde::Deserialize;

use crate::base_path::{BasePath, BasePathError};
use crate::config::Config;
use crate::config::public_url::PublicUrl;

/// The path segments directly under `{base}` the gateway serves itself,
/// which `{fhir-base}` may not start with when `{base}` is `/`.
const OWN_SEGMENTS: [&str; 4] = ["v1", "health", "operator", ".well-known"];

/// The `[fhir]` table as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fhir {
    /// The path the face is served under, `{fhir-base}`.
    pub base: Option<String>,
    /// The organisation that operates the gateway, which authors every
    /// summary beside the gateway itself.
    pub operator: Operator,
    /// The FHIRconnect context mappings, each feeding one section.
    pub mapping: Vec<MappingEntry>,
}

/// The `[fhir.operator]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Operator {
    /// Its display name.
    pub name: Option<String>,
    /// The system of its identifier, with `identifier_value`.
    pub identifier_system: Option<String>,
    /// The value of its identifier, with `identifier_system`.
    pub identifier_value: Option<String>,
}

/// One `[[fhir.mapping]]` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MappingEntry {
    /// The section it feeds, by the slug of its section query, such as
    /// `allergies-and-intolerances`.
    pub section: String,
    /// The OPT 1.4 operational template it compiles against.
    pub template: PathBuf,
    /// The model and context mapping files.
    pub files: Vec<PathBuf>,
    /// The `metadata.name` of the context mapping.
    pub context: String,
}

/// The face, resolved: its base, the operator and the compiled mappings.
#[derive(Debug, Clone)]
pub struct FhirSettings {
    /// `{fhir-base}`.
    pub base: BasePath,
    /// The absolute URL of `{fhir-base}`, the base of every `fullUrl`.
    pub absolute: String,
    /// The operator.
    pub operator: Organisation,
    /// The compiled mappings.
    pub mappings: Arc<Mappings>,
    /// The table as written, which a reload compares.
    pub written: Fhir,
}

/// Why the `[fhir]` table is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FhirError {
    /// A required key is not set.
    #[error("[fhir] is set and {key} is not")]
    Missing {
        /// The key.
        key: &'static str,
    },
    /// `fhir.base` is no path.
    #[error("fhir.base is no path")]
    Base(#[source] BasePathError),
    /// `fhir.base` is `/`, or lies on a path of the ITS-REST face.
    #[error(
        "fhir.base {base} is {{base}} or lies on a path the gateway serves under it; give the face a base of its own"
    )]
    Overlaps {
        /// The base as written.
        base: String,
    },
    /// `server.public_url` does not take `fhir.base` as a path.
    #[error("server.public_url does not take fhir.base as a path")]
    Absolute(#[source] url::ParseError),
    /// The gateway federates nothing, so the face would serve nothing.
    #[error("[fhir] serves the patient summary from the members, and no [registry] is set")]
    NoRegistry,
    /// No demographics binding is set, so no summary could name its patient.
    #[error(
        "[fhir] writes the patient summary header from the demographics binding, and no [pdqm] is set"
    )]
    NoDemographics,
    /// One of the operator's identifier keys is set without the other.
    #[error("fhir.operator.identifier_system and fhir.operator.identifier_value are set together")]
    HalfIdentifier,
    /// A mapping names no section the face fills.
    #[error("fhir.mapping[{position}].section names no section of the patient summary")]
    Section {
        /// The position of the entry.
        position: usize,
    },
    /// A mapping's template cannot be read.
    #[error("fhir.mapping[{position}].template {} cannot be read", path.display())]
    Template {
        /// The position of the entry.
        position: usize,
        /// The path.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// A mapping does not compile, or maps to a profile its section does
    /// not take.
    #[error("a [[fhir.mapping]] is refused")]
    Mapping(#[from] MappingsError),
    /// A route a binding serves lies on the face's base, or the base on it.
    #[error("fhir.base and {key} name overlapping routes; give each a path of its own")]
    Clash {
        /// The key of the binding's route, such as `pmir.path`.
        key: &'static str,
    },
}

impl Config {
    /// Resolves `[fhir]`, when it is set, for the gateway served under
    /// `server_base` at `public`.
    ///
    /// # Errors
    ///
    /// The [`FhirError`] of [`Fhir::resolve`].
    pub(crate) fn resolve_fhir(
        &self,
        server_base: &BasePath,
        public: Option<&PublicUrl>,
    ) -> Result<Option<FhirSettings>, FhirError> {
        #[cfg(feature = "binding-ihe")]
        let demographics = self.pdqm.is_some();
        #[cfg(not(feature = "binding-ihe"))]
        let demographics = false;
        self.fhir
            .as_ref()
            .map(|fhir| {
                fhir.resolve(
                    server_base,
                    public,
                    (self.registry.configured(), demographics),
                )
            })
            .transpose()
    }
}

impl Fhir {
    /// Resolves the table, the gateway served under `server_base` at
    /// `public`, federating when `federates`, and with a demographics
    /// binding to fill the summary header from when `demographics`.
    ///
    /// # Errors
    ///
    /// The [`FhirError`] that names the fault.
    pub fn resolve(
        &self,
        server_base: &BasePath,
        public: Option<&PublicUrl>,
        (federates, demographics): (bool, bool),
    ) -> Result<FhirSettings, FhirError> {
        let written = self
            .base
            .as_deref()
            .ok_or(FhirError::Missing { key: "fhir.base" })?;
        let base = written.parse::<BasePath>().map_err(FhirError::Base)?;
        if overlaps(&base, server_base) {
            return Err(FhirError::Overlaps {
                base: written.to_owned(),
            });
        }
        let public = public.ok_or(FhirError::Missing {
            key: "server.public_url",
        })?;
        let absolute = url::Url::parse(public.as_str())
            .and_then(|url| url.join(base.as_str()))
            .map_err(FhirError::Absolute)?;
        if !federates {
            return Err(FhirError::NoRegistry);
        }
        let name = self.operator.name.clone().ok_or(FhirError::Missing {
            key: "fhir.operator.name",
        })?;
        let identifiers = match (
            &self.operator.identifier_system,
            &self.operator.identifier_value,
        ) {
            (Some(system), Some(value)) => vec![(system.clone(), value.clone())],
            (None, None) => Vec::new(),
            _ => return Err(FhirError::HalfIdentifier),
        };
        if self.mapping.is_empty() {
            return Err(FhirError::Missing {
                key: "fhir.mapping",
            });
        }
        let mut sources = Vec::with_capacity(self.mapping.len());
        for (position, entry) in self.mapping.iter().enumerate() {
            let section =
                Section::from_slug(&entry.section).ok_or(FhirError::Section { position })?;
            let opt =
                std::fs::read_to_string(&entry.template).map_err(|source| FhirError::Template {
                    position,
                    path: entry.template.clone(),
                    source,
                })?;
            sources.push(Source {
                section,
                opt,
                files: entry.files.clone(),
                context: entry.context.clone(),
            });
        }
        let mappings = Arc::new(Mappings::compile(&sources)?);
        // NOTE: HL7 Europe EPS 1.0.0-ballot `patient-eu-eps` ips-pat-1 requires a name, and
        // the members federate none (Federation Tier §2.3, N32), so the binding gives it.
        if !demographics {
            return Err(FhirError::NoDemographics);
        }
        Ok(FhirSettings {
            base,
            absolute: absolute.as_str().trim_end_matches('/').to_owned(),
            operator: Organisation {
                id: String::from("operator"),
                name: Some(name),
                identifiers,
            },
            mappings,
            written: self.clone(),
        })
    }
}

/// Refuses `route`, the absolute path a binding serves under the key `key`,
/// when it is `fhir`'s base, lies under it, or holds it, ASCII case ignored,
/// so the face and the binding never share a path.
///
/// # Errors
///
/// [`FhirError::Clash`] naming `key`.
pub fn apart(fhir: &FhirSettings, route: &str, key: &'static str) -> Result<(), FhirError> {
    let base = fhir.base.as_str().to_ascii_lowercase();
    let route = route.to_ascii_lowercase();
    let holds = |outer: &str, inner: &str| {
        inner == outer
            || inner
                .strip_prefix(outer)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    if holds(&base, &route) || holds(&route, &base) {
        return Err(FhirError::Clash { key });
    }
    Ok(())
}

/// Whether `base` is the ITS-REST face's `server_base`, lies under it, or,
/// under the root, starts with a segment the gateway serves itself.
fn overlaps(base: &BasePath, server_base: &BasePath) -> bool {
    if base.is_root() || base == server_base {
        return true;
    }
    if server_base.is_root() {
        let first = base
            .as_str()
            .trim_start_matches('/')
            .split('/')
            .next()
            .unwrap_or_default();
        return OWN_SEGMENTS.contains(&first);
    }
    base.as_str()
        .strip_prefix(server_base.as_str())
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::overlaps;
    use crate::base_path::BasePath;

    fn path(text: &str) -> BasePath {
        text.parse().unwrap()
    }

    #[test]
    fn the_face_never_shares_a_path_with_the_its_rest_face() {
        for (fhir, base) in [
            ("/", "/"),
            ("/v1", "/"),
            ("/v1/fhir", "/"),
            ("/health", "/"),
            ("/operator", "/"),
            ("/.well-known", "/"),
            ("/ehr", "/ehr"),
            ("/ehr/fhir", "/ehr"),
        ] {
            assert!(overlaps(&path(fhir), &path(base)), "{fhir} under {base}");
        }
        for (fhir, base) in [
            ("/fhir", "/"),
            ("/fhir", "/ehr"),
            ("/ehrx", "/ehr"),
            ("/eu/fhir", "/rest/openehr"),
        ] {
            assert!(!overlaps(&path(fhir), &path(base)), "{fhir} beside {base}");
        }
    }
}
