// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the suites share: the vendored Xt-EHR package, and a writer for
//! synthetic package archives.

use std::error::Error;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;

use eehrxf::dataset::DatasetModel;
use eehrxf::dataset::ResourceProfile;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

/// The vendored `xtehr.eu.ehds.models` 1.0.0 package, as the registry serves
/// it.
pub(crate) fn xtehr_package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/eu-xtehr-models/xtehr.eu.ehds.models-1.0.0.tgz")
}

/// Reads the vendored package into a dataset model.
pub(crate) fn xtehr() -> Result<DatasetModel, Box<dyn Error>> {
    Ok(DatasetModel::read(std::fs::File::open(xtehr_package())?)?)
}

/// The vendored `hl7.fhir.eu.eps` 1.0.0-ballot package, as the registry
/// serves it.
pub(crate) fn eps_package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz")
}

/// Reads the profile at `url` from the vendored EPS package.
pub(crate) fn eps(url: &str) -> Result<ResourceProfile, Box<dyn Error>> {
    Ok(ResourceProfile::read(
        std::fs::File::open(eps_package())?,
        url,
    )?)
}

/// The vendored `hl7.fhir.eu.base` 2.0.1 package, which holds the EU core
/// profiles the EPS profiles build on and the entries of a section claim.
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
pub(crate) fn eu_base_package() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/eu-hl7-base/hl7.fhir.eu.base-2.0.1.tgz")
}

/// Reads the profile at `url` from whichever vendored package holds it, the
/// EPS package or the EU base package, or answers that neither does.
#[cfg(all(feature = "openehr", feature = "patient-summary"))]
pub(crate) fn vendored(url: &str) -> Result<ResourceProfile, Box<dyn Error>> {
    for package in [eps_package(), eu_base_package()] {
        // NOTE: FHIR NPM packages; a profile one package lacks is legitimately absent there.
        if let Ok(profile) = ResourceProfile::read(std::fs::File::open(package)?, url) {
            return Ok(profile);
        }
    }
    Err(format!("no vendored package holds the profile {url}").into())
}

/// One archive member: its path and its bytes.
pub(crate) type Member = (String, Vec<u8>);

/// Returns every regular member of the vendored package, in archive order.
pub(crate) fn xtehr_members() -> Result<Vec<Member>, Box<dyn Error>> {
    let mut archive = tar::Archive::new(GzDecoder::new(std::fs::File::open(xtehr_package())?));
    let mut members = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        members.push((path, bytes));
    }
    Ok(members)
}

/// Writes `members` as a gzip-compressed tarball, in the order given.
pub(crate) fn archive<P: AsRef<str>, B: AsRef<[u8]>>(
    members: &[(P, B)],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (path, bytes) in members {
        let bytes = bytes.as_ref();
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(bytes.len())?);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path.as_ref(), bytes)?;
    }
    let mut encoder = builder.into_inner()?;
    encoder.flush()?;
    Ok(encoder.finish()?)
}

/// The manifest of a synthetic package.
pub(crate) const MANIFEST: &str = r#"{"name":"example.synthetic.models","version":"0.0.1"}"#;

/// The canonical URL of the synthetic logical model.
pub(crate) const MODEL: &str = "http://example.org/fhir/StructureDefinition/ExampleModel";

/// The canonical URL of the synthetic obligations profile.
pub(crate) const PROFILE: &str =
    "http://example.org/fhir/StructureDefinition/ExampleModelObligations";

/// A synthetic logical model with one required item and one optional note.
pub(crate) fn model() -> String {
    model_with(
        r#"{"id":"ExampleModel.note","path":"ExampleModel.note","min":0,"max":"1","type":[{"code":"string"}]}"#,
    )
}

/// The synthetic logical model with `last` as its final element.
pub(crate) fn model_with(last: &str) -> String {
    format!(
        r#"{{"resourceType":"StructureDefinition","url":"{MODEL}","name":"ExampleModel","kind":"logical","derivation":"specialization","snapshot":{{"element":[{{"id":"ExampleModel","path":"ExampleModel","min":0,"max":"*"}},{{"id":"ExampleModel.item","path":"ExampleModel.item","min":1,"max":"1","type":[{{"code":"string"}}],"short":"A synthetic item"}},{last}]}}}}"#
    )
}

/// A producer obligation with `code`.
pub(crate) fn obligation(code: &str) -> String {
    format!(
        r#"{{"url":"http://hl7.org/fhir/StructureDefinition/obligation","extension":[{{"url":"code","valueCode":"{code}"}},{{"url":"actor","valueCanonical":"https://www.xt-ehr.eu/specifications/fhir/actor-producer"}}]}}"#
    )
}

/// A synthetic obligations profile over `base` that puts `extensions` on the
/// element `path`.
///
/// Its snapshot holds the three elements of the synthetic model and, when
/// `path` is none of them, `path` as a fourth.
pub(crate) fn profile(base: &str, path: &str, extensions: &str) -> String {
    let mut elements = Vec::new();
    for id in ["ExampleModel", "ExampleModel.item", "ExampleModel.note"] {
        elements.push(profile_element(
            id,
            if id == path { extensions } else { "" },
        ));
    }
    if !["ExampleModel", "ExampleModel.item", "ExampleModel.note"].contains(&path) {
        elements.push(profile_element(path, extensions));
    }
    profile_of(base, &elements.join(","))
}

/// A synthetic obligations profile over `base` with the snapshot `elements`.
pub(crate) fn profile_of(base: &str, elements: &str) -> String {
    format!(
        r#"{{"resourceType":"StructureDefinition","url":"{PROFILE}","name":"ExampleModelObligations","kind":"logical","derivation":"constraint","baseDefinition":"{base}","snapshot":{{"element":[{elements}]}}}}"#
    )
}

/// One element of a synthetic profile snapshot, with `extensions`.
pub(crate) fn profile_element(id: &str, extensions: &str) -> String {
    format!(r#"{{"id":"{id}","path":"{id}","min":0,"max":"1","extension":[{extensions}]}}"#)
}

/// A well-formed synthetic package: the manifest, the model and a profile
/// with a producer obligation on the item.
pub(crate) fn synthetic() -> Vec<(&'static str, String)> {
    vec![
        ("package/package.json", MANIFEST.to_owned()),
        ("package/StructureDefinition-ExampleModel.json", model()),
        (
            "package/StructureDefinition-ExampleModelObligations.json",
            profile(
                MODEL,
                "ExampleModel.item",
                &obligation("SHALL:able-to-populate"),
            ),
        ),
    ]
}
