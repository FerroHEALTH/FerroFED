// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR R4 definitions of the registry document in FHIR form: the three
//! `Endpoint` extensions held to FHIR R4 2.5.1 Defining Extensions and the
//! `StructureDefinition` invariants, the five identifier systems held to the
//! `NamingSystem` resource, each rule quoted from the vendored pages under
//! `docs/specs/fhir-r4/`, and each definition tied to the constant and the
//! reader that give it its meaning.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use ferrofed_identity::ihe::mcsd::{
    CONSENT_REFUSAL_CODE_EXTENSION, CREATING_SYSTEM_ID_SYSTEM, EHR_ID_SYSTEM_EXTENSION,
    ENDPOINT_ID_SYSTEM, NODE_ID_SYSTEM, ORGANISATION_ID_SYSTEM, PROFILE_EXTENSION,
    SYSTEM_ID_SYSTEM, definition,
};
use ferrofed_registry::snapshot::ConnectionType;
use fhir_types::codec::{Json, Object, Path, Value};
use fhir_types::r4::naming_system::NamingSystem;
use fhir_types::r4::structure_definition::StructureDefinition;
use ihe_iti::mcsd::directory::Directory;

type TestResult = Result<(), Box<dyn Error>>;

/// One extension the form reads: its URL, its definition, the most times an
/// `Endpoint` carries it, and the one value type its reader takes.
struct Extension {
    url: &'static str,
    definition: &'static str,
    max: &'static str,
    value: &'static str,
}

/// The extensions, with the cardinality and value type the reader holds them
/// to: any number of consent refusal codes and profiles, and one `ehr_id`
/// system, which the reader refuses twice.
const EXTENSIONS: [Extension; 3] = [
    Extension {
        url: CONSENT_REFUSAL_CODE_EXTENSION,
        definition: definition::CONSENT_REFUSAL_CODE,
        max: "*",
        value: "code",
    },
    Extension {
        url: PROFILE_EXTENSION,
        definition: definition::PAYLOAD_PROFILE,
        max: "*",
        value: "canonical",
    },
    Extension {
        url: EHR_ID_SYSTEM_EXTENSION,
        definition: definition::EHR_ID_SYSTEM,
        max: "1",
        value: "uri",
    },
];

/// The identifier systems the form reads, each with its `NamingSystem`.
const NAMING_SYSTEMS: [(&str, &str); 5] = [
    (ORGANISATION_ID_SYSTEM, definition::ORGANISATION_ID),
    (ENDPOINT_ID_SYSTEM, definition::ENDPOINT_ID),
    (NODE_ID_SYSTEM, definition::NODE_ID),
    (SYSTEM_ID_SYSTEM, definition::SYSTEM_ID),
    (CREATING_SYSTEM_ID_SYSTEM, definition::CREATING_SYSTEM_ID),
];

/// The vendored FHIR R4 page `name`.
fn page(name: &str) -> Result<String, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/fhir-r4")
        .join(name);
    Ok(std::fs::read_to_string(path)?)
}

/// `json` as a JSON object.
fn object(json: &str) -> Result<Object, Box<dyn Error>> {
    match serde_json::from_str(json)? {
        Value::Object(object) => Ok(object),
        _ => Err("the definition is not a JSON object".into()),
    }
}

/// The string member `name` of `object`.
fn text<'a>(object: &'a Object, name: &str) -> Option<&'a str> {
    object.get(name).and_then(Value::as_str)
}

/// The non-negative integer `value` holds, when it is one.
fn count(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

/// The members of the array `name` of `object`, each an object.
fn objects<'a>(object: &'a Object, name: &str) -> Result<Vec<&'a Object>, Box<dyn Error>> {
    object
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("no array {name}"))?
        .iter()
        .map(|item| {
            item.as_object()
                .ok_or_else(|| format!("a member of {name} is not an object").into())
        })
        .collect()
}

/// The differential element of `definition` whose id is `id`.
fn element<'a>(definition: &'a Object, id: &str) -> Result<&'a Object, Box<dyn Error>> {
    let differential = definition
        .get("differential")
        .and_then(Value::as_object)
        .ok_or("no differential")?;
    objects(differential, "element")?
        .into_iter()
        .find(|element| text(element, "id") == Some(id))
        .ok_or_else(|| format!("no element {id}").into())
}

/// Whether `name` is a name csd-0 and nsd-0 admit, `[A-Z]([A-Za-z0-9_]){0,254}`.
fn computable(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && name.len() <= 255
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[test]
fn the_rules_the_tests_hold_are_the_vendored_r4_text() -> TestResult {
    let defining = page("defining-extensions.html")?;
    for rule in [
        "Extensions SHALL only be used on a target that appears in their context list",
        "Cardinality = 1...1 (fixed)",
        "value = canonical URL (fixed)",
        "Use 0..0 if a simple (non-nested extension)",
    ] {
        assert!(defining.contains(rule), "2.5.1 does not say: {rule}");
    }
    let structure = page("structuredefinition.html")?;
    for rule in [
        "If the structure defines an extension then the structure must have context information",
        "All element definitions must have unique ids (diff)",
        "In any differential, all the elements must start with the StructureDefinition's specified type",
    ] {
        assert!(
            structure.contains(rule),
            "StructureDefinition does not say: {rule}"
        );
    }
    let naming = page("namingsystem.html")?;
    for rule in [
        "Root systems cannot have uuid identifiers",
        "Can't have more than one preferred identifier for a type",
        "codesystem | identifier | root",
        "oid | uuid | uri | other",
        "name.matches('[A-Z]([A-Za-z0-9_]){0,254}')",
    ] {
        assert!(naming.contains(rule), "NamingSystem does not say: {rule}");
    }
    Ok(())
}

#[test]
fn every_ferrofed_url_the_reader_names_has_a_definition() -> TestResult {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/ihe/mcsd/mod.rs"),
    )?;
    let named: BTreeSet<&str> = source
        .split('"')
        .filter(|piece| piece.starts_with("https://ferrofed.eu/fhir/"))
        .collect();
    let defined: BTreeSet<&str> = EXTENSIONS
        .iter()
        .map(|extension| extension.url)
        .chain(NAMING_SYSTEMS.iter().map(|(system, _)| *system))
        .chain([ConnectionType::SYSTEM])
        .collect();
    assert_eq!(named, defined);
    Ok(())
}

#[test]
fn each_extension_is_a_simple_extension_of_endpoint_at_its_url() -> TestResult {
    for extension in &EXTENSIONS {
        let object = object(extension.definition)?;
        let read = StructureDefinition::from_json(&object, &mut Path::root("StructureDefinition"))?;
        assert_eq!(read.to_json()?, object, "{}", extension.url);
        assert_eq!(text(&object, "fhirVersion"), Some("4.0.1"));
        assert_eq!(text(&object, "url"), Some(extension.url));
        assert_eq!(extension.url.rsplit('/').next(), text(&object, "id"));
        assert!(computable(text(&object, "name").unwrap_or_default()));
        assert_eq!(text(&object, "type"), Some("Extension"));
        assert_eq!(text(&object, "kind"), Some("complex-type"));
        assert_eq!(object.get("abstract"), Some(&Value::Bool(false)));
        assert_eq!(
            text(&object, "baseDefinition"),
            Some("http://hl7.org/fhir/StructureDefinition/Extension")
        );
        assert_eq!(text(&object, "derivation"), Some("constraint"));
        let contexts = objects(&object, "context")?;
        let [context] = contexts.as_slice() else {
            return Err(format!("{}: one context", extension.url).into());
        };
        assert_eq!(text(context, "type"), Some("element"));
        assert_eq!(text(context, "expression"), Some("Endpoint"));
    }
    Ok(())
}

#[test]
fn each_extension_has_the_cardinality_and_value_type_the_reader_takes() -> TestResult {
    for extension in &EXTENSIONS {
        let object = object(extension.definition)?;
        let differential = object
            .get("differential")
            .and_then(Value::as_object)
            .ok_or("no differential")?;
        let ids: Vec<_> = objects(differential, "element")?
            .into_iter()
            .map(|element| {
                assert_eq!(text(element, "path"), text(element, "id"));
                text(element, "id")
            })
            .collect();
        assert_eq!(
            ids,
            [
                Some("Extension"),
                Some("Extension.extension"),
                Some("Extension.url"),
                Some("Extension.value[x]"),
            ]
        );
        let root = element(&object, "Extension")?;
        assert_eq!(root.get("min").and_then(count), Some(0));
        assert_eq!(text(root, "max"), Some(extension.max), "{}", extension.url);
        assert_eq!(
            text(element(&object, "Extension.extension")?, "max"),
            Some("0")
        );
        assert_eq!(
            text(element(&object, "Extension.url")?, "fixedUri"),
            Some(extension.url)
        );
        let value = element(&object, "Extension.value[x]")?;
        assert_eq!(value.get("min").and_then(count), Some(1));
        let types: Vec<_> = objects(value, "type")?
            .into_iter()
            .map(|kind| text(kind, "code"))
            .collect();
        assert_eq!(types, [Some(extension.value)], "{}", extension.url);
    }
    Ok(())
}

/// A directory of one `Endpoint` carrying one extension `url` with the
/// member `value` set to `sample`.
fn directory(url: &str, value: &str, sample: &str) -> Result<Directory, Box<dyn Error>> {
    let body = format!(
        r#"{{"resourceType": "Bundle", "type": "collection", "entry": [{{
          "fullUrl": "https://registry.example.org/fhir/Endpoint/node-a-pub",
          "resource": {{
            "resourceType": "Endpoint", "id": "node-a-pub",
            "extension": [{{"url": "{url}", "{value}": "{sample}"}}],
            "status": "active",
            "connectionType": {{"system": "{system}", "code": "openehr-rest-query"}},
            "payloadType": [{{"text": "openEHR"}}],
            "address": "https://cdr-a.example.org/openehr"
          }}
        }}]}}"#,
        system = ConnectionType::SYSTEM,
    );
    Ok(Directory::from_json(body.as_bytes())?)
}

/// What the reader of `url` reads from `directory`'s one endpoint.
fn read(directory: &Directory, url: &str) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    let [endpoint] = directory.endpoints() else {
        return Err("one endpoint".into());
    };
    let values: Vec<Option<&str>> = match url {
        CONSENT_REFUSAL_CODE_EXTENSION => endpoint.extension_codes(url).collect(),
        PROFILE_EXTENSION => endpoint.extension_canonicals(url).collect(),
        EHR_ID_SYSTEM_EXTENSION => endpoint.extension_uris(url).collect(),
        _ => return Err(format!("no reader of {url}").into()),
    };
    Ok(values
        .into_iter()
        .map(|value| value.map(str::to_owned))
        .collect())
}

#[test]
fn the_reader_takes_the_defined_value_type_and_no_other() -> TestResult {
    let sample = "urn:oid:2.999.4.1";
    let every = ["code", "canonical", "uri"];
    for extension in &EXTENSIONS {
        for kind in every {
            let mut chars = kind.chars();
            let member = format!(
                "value{}{}",
                chars
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or_default(),
                chars.as_str()
            );
            let read = read(&directory(extension.url, &member, sample)?, extension.url)?;
            let expected = (kind == extension.value).then(|| sample.to_owned());
            assert_eq!(read, [expected], "{} with {member}", extension.url);
        }
    }
    Ok(())
}

#[test]
fn each_identifier_system_is_a_naming_system_with_its_uri_preferred() -> TestResult {
    for (system, definition) in NAMING_SYSTEMS {
        let object = object(definition)?;
        let read = NamingSystem::from_json(&object, &mut Path::root("NamingSystem"))?;
        assert_eq!(read.to_json()?, object, "{system}");
        assert_eq!(text(&object, "resourceType"), Some("NamingSystem"));
        assert_eq!(system.rsplit('/').next(), text(&object, "id"));
        assert!(computable(text(&object, "name").unwrap_or_default()));
        assert_eq!(text(&object, "status"), Some("active"));
        assert_eq!(text(&object, "kind"), Some("identifier"));
        let unique = objects(&object, "uniqueId")?;
        let [only] = unique.as_slice() else {
            return Err(format!("{system}: one uniqueId").into());
        };
        assert_eq!(text(only, "type"), Some("uri"));
        assert_eq!(text(only, "value"), Some(system));
        assert_eq!(only.get("preferred"), Some(&Value::Bool(true)));
    }
    Ok(())
}
