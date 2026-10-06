// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A defective package is refused with the defect named, never read into a
//! partial model.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Root;
use eehrxf::dataset::DatasetError;
use eehrxf::dataset::DatasetModel;

use super::support::MODEL;
use super::support::PROFILE;
use super::support::archive;
use super::support::model;
use super::support::model_with;
use super::support::obligation;
use super::support::profile;
use super::support::profile_element;
use super::support::profile_of;
use super::support::synthetic;

/// Reads a synthetic package with `replace` swapped in at `position`.
fn read_with(
    position: usize,
    replace: String,
) -> Result<Result<DatasetModel, DatasetError>, Box<dyn Error>> {
    let mut members = synthetic();
    let slot = members
        .get_mut(position)
        .ok_or("a member at that position")?;
    slot.1 = replace;
    Ok(DatasetModel::read(archive(&members)?.as_slice()))
}

#[test]
fn a_well_formed_synthetic_package_reads() -> Result<(), Box<dyn Error>> {
    let read = DatasetModel::read(archive(&synthetic())?.as_slice())?;
    let dataset = read.dataset(Root {
        model: MODEL,
        obligations: PROFILE,
    })?;
    let populated: Vec<&str> = dataset
        .producer_elements()
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(populated, ["ExampleModel.item"]);
    Ok(())
}

#[test]
fn bytes_that_are_no_gzip_tarball_are_refused() {
    let read = DatasetModel::read(b"not a package".as_slice());
    assert!(
        matches!(read, Err(DatasetError::Archive { .. })),
        "{read:?}"
    );
}

#[test]
fn a_package_without_a_manifest_is_refused() -> Result<(), Box<dyn Error>> {
    let members = vec![("package/StructureDefinition-ExampleModel.json", model())];
    let read = DatasetModel::read(archive(&members)?.as_slice());
    assert!(
        matches!(read, Err(DatasetError::MissingManifest)),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_definition_that_does_not_parse_is_refused_with_its_file() -> Result<(), Box<dyn Error>> {
    let read = read_with(1, String::from("{\"resourceType\":\"StructureDefinition\""))?;
    let Err(DatasetError::Json { file, .. }) = read else {
        return Err(format!("{read:?}").into());
    };
    assert_eq!(file, "package/StructureDefinition-ExampleModel.json");
    Ok(())
}

#[test]
fn an_element_without_a_cardinality_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        1,
        model_with(r#"{"id":"ExampleModel.note","path":"ExampleModel.note","min":0}"#),
    )?;
    assert!(
        matches!(read, Err(DatasetError::Element { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn an_element_with_an_unreadable_upper_bound_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        1,
        model_with(r#"{"id":"ExampleModel.note","path":"ExampleModel.note","min":0,"max":"many"}"#),
    )?;
    assert!(
        matches!(read, Err(DatasetError::Element { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn an_element_given_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        1,
        model_with(r#"{"id":"ExampleModel.item","path":"ExampleModel.item","min":0,"max":"1"}"#),
    )?;
    assert!(
        matches!(read, Err(DatasetError::DuplicateElement { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_model_given_twice_is_refused() -> Result<(), Box<dyn Error>> {
    let mut members = synthetic();
    members.push(("package/StructureDefinition-ExampleModelCopy.json", model()));
    let read = DatasetModel::read(archive(&members)?.as_slice());
    assert!(
        matches!(read, Err(DatasetError::DuplicateUrl { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_model_without_a_snapshot_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        1,
        format!(
            r#"{{"resourceType":"StructureDefinition","url":"{MODEL}","name":"ExampleModel","kind":"logical","derivation":"specialization"}}"#
        ),
    )?;
    assert!(
        matches!(read, Err(DatasetError::MissingSnapshot { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_logical_definition_without_a_derivation_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        1,
        format!(
            r#"{{"resourceType":"StructureDefinition","url":"{MODEL}","name":"ExampleModel","kind":"logical"}}"#
        ),
    )?;
    assert!(
        matches!(
            read,
            Err(DatasetError::Derivation {
                derivation: None,
                ..
            })
        ),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_profile_over_a_model_the_package_lacks_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        2,
        profile(
            "http://example.org/fhir/StructureDefinition/Elsewhere",
            "ExampleModel.item",
            &obligation("SHALL:able-to-populate"),
        ),
    )?;
    assert!(
        matches!(read, Err(DatasetError::UnknownBase { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_profile_whose_snapshot_drops_an_element_of_its_model_is_refused() -> Result<(), Box<dyn Error>>
{
    let elements = [
        profile_element("ExampleModel", ""),
        profile_element("ExampleModel.item", &obligation("SHALL:able-to-populate")),
    ]
    .join(",");
    let read = read_with(2, profile_of(MODEL, &elements))?;
    let Err(DatasetError::Uncovered { path, .. }) = read else {
        return Err(format!("{read:?}").into());
    };
    assert_eq!(path, "ExampleModel.note");
    Ok(())
}

#[test]
fn an_obligation_may_reach_below_the_model_into_the_profile_snapshot() -> Result<(), Box<dyn Error>>
{
    // A profile's snapshot expands the elements of a logical type an element
    // names, as EHDSHealthProfessionalObligations does for `name.family`.
    let read = read_with(
        2,
        profile(
            MODEL,
            "ExampleModel.item.detail",
            &obligation("SHALL:able-to-populate"),
        ),
    )?;
    let model = read?;
    let dataset = model.dataset(Root {
        model: MODEL,
        obligations: PROFILE,
    })?;
    let populated: Vec<&str> = dataset
        .producer_elements()
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(populated, ["ExampleModel.item.detail"]);
    assert!(
        dataset
            .model()
            .element("ExampleModel.item.detail")
            .is_none()
    );
    Ok(())
}

#[test]
fn an_obligation_with_two_codes_is_refused() -> Result<(), Box<dyn Error>> {
    let read = read_with(
        2,
        profile(
            MODEL,
            "ExampleModel.item",
            r#"{"url":"http://hl7.org/fhir/StructureDefinition/obligation","extension":[{"url":"code","valueCode":"SHALL:able-to-populate"},{"url":"code","valueCode":"SHOULD:display"}]}"#,
        ),
    )?;
    assert!(
        matches!(read, Err(DatasetError::Obligation { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn an_obligation_with_an_empty_actor_is_refused_rather_than_read_as_binding_every_actor()
-> Result<(), Box<dyn Error>> {
    let read = read_with(
        2,
        profile(
            MODEL,
            "ExampleModel.item",
            r#"{"url":"http://hl7.org/fhir/StructureDefinition/obligation","extension":[{"url":"code","valueCode":"SHALL:able-to-populate"},{"url":"actor"}]}"#,
        ),
    )?;
    assert!(
        matches!(read, Err(DatasetError::Obligation { .. })),
        "{read:?}"
    );
    Ok(())
}

#[test]
fn a_dataset_whose_profile_constrains_another_model_is_missing() -> Result<(), Box<dyn Error>> {
    let read = DatasetModel::read(archive(&synthetic())?.as_slice())?;
    let missing = read.dataset(Root {
        model: "http://example.org/fhir/StructureDefinition/Elsewhere",
        obligations: PROFILE,
    });
    assert!(
        matches!(missing, Err(DatasetError::MissingRoot { .. })),
        "{missing:?}"
    );
    Ok(())
}

#[test]
fn members_outside_the_package_folder_and_the_index_are_not_read() -> Result<(), Box<dyn Error>> {
    let mut members = synthetic();
    members.push(("package/.index.json", String::from("{\"index-version\":1}")));
    members.push(("package/example/broken.json", String::from("{")));
    members.push(("other/broken.json", String::from("{")));
    let read = DatasetModel::read(archive(&members)?.as_slice())?;
    assert_eq!(read.models().count(), 1);
    Ok(())
}
