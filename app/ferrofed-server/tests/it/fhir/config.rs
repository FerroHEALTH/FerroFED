// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[fhir]` table refused at configuration load: a face with no base of
//! its own, no public URL to name its entries under, no registry to read
//! from, no operator, no mapping, or a mapping that names no section, does
//! not compile, or maps to a profile its section does not take.

use std::collections::BTreeMap;
use std::path::Path;

use ferrofed_eehrxf::summary::mappings::MappingsError;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::fhir::FhirError;
use ferrofed_testkit::eps;

use super::{FHIR, PUBLIC, TestResult, UNREACHED_SUPPLIER, fhir_tables};

/// The root of the workspace, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The configuration of a gateway over a registry document in `dir`, with
/// `server` as its `[server]` keys and `fhir` as its `[fhir]` tables.
fn resolved(dir: &Path, server: &str, fhir: &str) -> Result<(), Error> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        crate::facade::registry("http://a.example.org", "http://b.example.org", ""),
    )
    .map_err(|source| Error::Read {
        path: document.clone(),
        source,
    })?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[server]\n{server}\n\n[registry]\ndocument = {document}\n\n\
         [federation]\nid = \"example-federation\"\n{fhir}"
    );
    Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
        .resolve()
        .map(|_| ())
}

/// The public URL key the face needs.
fn public() -> String {
    format!("public_url = \"{PUBLIC}\"")
}

#[test]
fn the_face_resolves_with_its_base_its_operator_and_its_mapping() -> TestResult {
    let dir = tempfile::tempdir()?;
    resolved(dir.path(), &public(), &fhir_tables(UNREACHED_SUPPLIER))?;
    Ok(())
}

#[test]
fn a_face_with_no_demographics_binding_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER);
    let without = tables.split("\n[pdqm]").next().ok_or("the [fhir] tables")?;
    assert!(!without.contains("[pdqm"), "{without}");
    let refused = resolved(dir.path(), &public(), without);
    assert!(
        matches!(refused, Err(Error::Fhir(FhirError::NoDemographics))),
        "EPS ips-pat-1: the header names the patient, and only the binding can: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_face_with_no_public_url_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let refused = resolved(dir.path(), "", &fhir_tables(UNREACHED_SUPPLIER));
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Missing {
                key: "server.public_url"
            }))
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_face_on_a_path_of_the_its_rest_face_is_refused() -> TestResult {
    for base in ["/", "/v1", "/v1/fhir", "/health", "/operator"] {
        let dir = tempfile::tempdir()?;
        let tables = fhir_tables(UNREACHED_SUPPLIER)
            .replace(&format!("base = \"{FHIR}\""), &format!("base = \"{base}\""));
        let refused = resolved(dir.path(), &public(), &tables);
        assert!(
            matches!(refused, Err(Error::Fhir(FhirError::Overlaps { .. }))),
            "{base}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn a_face_with_no_operator_or_no_mapping_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER).replace("name = \"Synthetic Operator\"\n", "");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Missing {
                key: "fhir.operator.name"
            }))
        ),
        "{refused:?}"
    );
    let unmapped =
        format!("\n[fhir]\nbase = \"{FHIR}\"\n\n[fhir.operator]\nname = \"Synthetic Operator\"\n");
    let refused = resolved(dir.path(), &public(), &unmapped);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Missing {
                key: "fhir.mapping"
            }))
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_mapping_that_names_no_section_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER).replace("allergies-and-intolerances", "alerts");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Section { position: 0 }))
        ),
        "the alerts have no section query: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_mapping_to_a_profile_its_section_does_not_take_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER).replace("allergies-and-intolerances", "problems");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Mapping(
                MappingsError::Profile { .. }
            )))
        ),
        "{refused:?}"
    );
    let example =
        Path::new(ROOT).join("crates/eehrxf/tests/fixtures/mapping/ferrofed_allergy.context.yml");
    let [_, context] = eps::mapping_files();
    let tables = fhir_tables(UNREACHED_SUPPLIER)
        .replace(
            &context.display().to_string(),
            &example.display().to_string(),
        )
        .replace(eps::CONTEXT, "ferrofed_allergy.context");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Mapping(
                MappingsError::Profile { .. }
            )))
        ),
        "an example.org profile is no EPS entry profile: {refused:?}"
    );
    Ok(())
}

#[test]
fn a_mapping_that_does_not_compile_or_read_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER).replace(eps::CONTEXT, "ferrofed_absent.context");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Mapping(
                MappingsError::Compile { .. }
            )))
        ),
        "{refused:?}"
    );
    let tables = fhir_tables(UNREACHED_SUPPLIER)
        .replace(&eps::opt().display().to_string(), "/absent/template.opt");
    let refused = resolved(dir.path(), &public(), &tables);
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Template { position: 0, .. }))
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_face_with_no_registry_is_refused() -> TestResult {
    let text = format!(
        "profile = \"development\"\n\n[server]\n{}\n{}",
        public(),
        fhir_tables(UNREACHED_SUPPLIER)
    );
    let refused =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve();
    assert!(
        matches!(refused, Err(Error::Fhir(FhirError::NoRegistry))),
        "{refused:?}"
    );
    Ok(())
}

/// The `[pmir]` table of a feed served at `path` under `{base}`.
#[cfg(feature = "binding-ihe")]
fn pmir(path: &str) -> String {
    format!(
        "\n[pmir]\nurl = \"https://pmir.example.org/fhir/\"\nfeed_token = \"Qz7feedtoken\"\npath = \"{path}\"\n"
    )
}

#[cfg(feature = "binding-ihe")]
#[test]
fn a_pmir_feed_route_on_the_face_is_refused_naming_both_keys() -> TestResult {
    for path in ["/fhir", "/fhir/feed", "/FHIR/Patient/feed"] {
        let dir = tempfile::tempdir()?;
        let tables = format!("{}{}", fhir_tables(UNREACHED_SUPPLIER), pmir(path));
        let refused = resolved(dir.path(), &public(), &tables);
        assert!(
            matches!(
                refused,
                Err(Error::Fhir(FhirError::Clash { key: "pmir.path" }))
            ),
            "{path}: {refused:?}"
        );
        let shown = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            shown.contains("fhir.base") && shown.contains("pmir.path"),
            "{shown}"
        );
    }
    let dir = tempfile::tempdir()?;
    let tables = fhir_tables(UNREACHED_SUPPLIER)
        .replace(&format!("base = \"{FHIR}\""), "base = \"/pmir/eu\"");
    let refused = resolved(dir.path(), &public(), &format!("{tables}{}", pmir("/pmir")));
    assert!(
        matches!(
            refused,
            Err(Error::Fhir(FhirError::Clash { key: "pmir.path" }))
        ),
        "the face under the feed: {refused:?}"
    );
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[test]
fn a_pmir_feed_route_beside_the_face_is_admitted() -> TestResult {
    for path in ["/pmir/feed", "/fhirfeed", "/fhir-feed"] {
        let dir = tempfile::tempdir()?;
        let tables = format!("{}{}", fhir_tables(UNREACHED_SUPPLIER), pmir(path));
        resolved(dir.path(), &public(), &tables)?;
    }
    Ok(())
}
