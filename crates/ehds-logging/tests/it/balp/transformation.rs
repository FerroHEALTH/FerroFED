// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A semantic transformation, written as an entity of its own on the access
//! record of the access that applied it (eHealth Network General Guidelines
//! Release 3.1 Art 6(3)).

use std::collections::BTreeMap;

use ehds_logging::balp::transformation::{TRANSFORMATION_DESCRIPTION, TRANSFORMATION_ENTITY};
use ehds_logging::category::Category;
use ehds_logging::record::{AccessRecord, Action, Outcome};
use ehds_logging::transformation::{Direction, Elements, Mapping, Transformation};

use super::{details, entity, record, written};

/// A transformation of a received patient summary that transcoded two
/// section codes, left one allergy code as text and dropped one element no
/// path names.
fn received() -> Transformation {
    let mut transcoded = Elements::default();
    transcoded.add(Some("Composition.section.code"));
    transcoded.add(Some("Composition.section.code"));
    let mut untranscoded = Elements::default();
    untranscoded.add(Some("AllergyIntolerance.code"));
    let mut dropped = Elements::default();
    dropped.add(None);
    Transformation {
        direction: Direction::Received,
        category: Some(Category::PatientSummary),
        mapping: Mapping {
            context: Some("example.context".to_owned()),
            template: "example.template.v1".to_owned(),
            profile: Some("https://example.org/fhir/StructureDefinition/allergy".to_owned()),
            files: vec!["model.yml".to_owned(), "context.yml".to_owned()],
            package: Some("example.package-1.0.0.tgz".to_owned()),
        },
        terminology: vec!["example-release|1.0".to_owned()],
        transcoded,
        untranscoded,
        dropped,
    }
}

/// A record of a creation that applied `transformations`.
fn applying(transformations: Vec<Transformation>) -> AccessRecord {
    let mut record = record(Action::Create, Outcome::Success);
    record.transformations = transformations;
    record
}

#[test]
fn a_transformation_is_written_in_its_own_entity_by_path() {
    let written = written(&applying(vec![received()]));
    let entities = entity(&written, TRANSFORMATION_ENTITY);
    let [entity] = entities.as_slice() else {
        panic!("one transformation entity: {written}");
    };
    assert_eq!(entity["type"]["code"], "4");
    assert_eq!(entity["description"], TRANSFORMATION_DESCRIPTION);
    assert_eq!(
        details(entity, "ehds-transformation-direction"),
        ["received"]
    );
    assert_eq!(
        details(entity, "ehds-transformation-category"),
        [
            "http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs|Patient-Summaries"
        ]
    );
    assert_eq!(details(entity, "mapping-context"), ["example.context"]);
    assert_eq!(details(entity, "mapping-template"), ["example.template.v1"]);
    assert_eq!(
        details(entity, "mapping-profile"),
        ["https://example.org/fhir/StructureDefinition/allergy"]
    );
    assert_eq!(
        details(entity, "mapping-file"),
        ["model.yml", "context.yml"]
    );
    assert_eq!(
        details(entity, "mapping-package"),
        ["example.package-1.0.0.tgz"]
    );
    assert_eq!(
        details(entity, "terminology-version"),
        ["example-release|1.0"]
    );
    assert_eq!(details(entity, "ehds-transcoded"), ["2"]);
    assert_eq!(
        details(entity, "ehds-transcoded-path"),
        ["Composition.section.code=2"]
    );
    assert_eq!(details(entity, "ehds-untranscoded"), ["1"]);
    assert_eq!(
        details(entity, "ehds-untranscoded-path"),
        ["AllergyIntolerance.code=1"]
    );
    assert_eq!(
        details(entity, "ehds-dropped"),
        ["1"],
        "an element no path names is counted all the same"
    );
    assert!(details(entity, "ehds-dropped-path").is_empty());
}

#[test]
fn a_transformation_that_dropped_nothing_writes_zeros() {
    let mut nothing = received();
    nothing.transcoded = Elements::default();
    nothing.untranscoded = Elements::default();
    nothing.dropped = Elements::default();
    let written = written(&applying(vec![nothing]));
    let entities = entity(&written, TRANSFORMATION_ENTITY);
    let [entity] = entities.as_slice() else {
        panic!("one transformation entity: {written}");
    };
    for count in ["ehds-transcoded", "ehds-untranscoded", "ehds-dropped"] {
        assert_eq!(details(entity, count), ["0"], "{count}");
    }
    for path in [
        "ehds-transcoded-path",
        "ehds-untranscoded-path",
        "ehds-dropped-path",
    ] {
        assert!(details(entity, path).is_empty(), "{path}");
    }
}

#[test]
fn each_transformation_is_one_entity_and_an_access_with_none_writes_none() {
    let mut provided = received();
    provided.direction = Direction::Provided;
    provided.mapping.package = None;
    let written = written(&applying(vec![received(), provided]));
    let entities = entity(&written, TRANSFORMATION_ENTITY);
    let directions: Vec<&str> = entities
        .iter()
        .flat_map(|entity| details(entity, "ehds-transformation-direction"))
        .collect();
    assert_eq!(directions, ["received", "provided"], "in the order applied");
    let plain = super::written(&applying(Vec::new()));
    assert!(entity(&plain, TRANSFORMATION_ENTITY).is_empty(), "{plain}");
}

#[test]
fn elements_count_at_least_what_their_paths_count() {
    let elements = Elements::new(1, BTreeMap::from([("Observation.code".to_owned(), 3)]));
    assert_eq!(elements.count(), 3, "a count below its paths' is raised");
    let elements = Elements::new(5, BTreeMap::from([("Observation.code".to_owned(), 3)]));
    assert_eq!(elements.count(), 5, "two elements no path names");
}
