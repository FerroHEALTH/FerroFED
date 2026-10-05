// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The seed data a release attaches for `ferrofed conformance run`: the file
//! `scripts/release/seed-data.sh` writes for the release lane is accepted as
//! `--seed-data`, its SHA-256 beside it is its own, and a seed data file
//! holding anything but the vendored content, or no file of it, is refused.
//! No specification governs the form of the file: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;
use std::process::Command;

use ferrofed_server::conformance::fixture::{
    BUNDLE_FILE, BUNDLE_FORMAT, CLINIC_FILE, HOSPITAL_FILE, SeedData, SeedDataError, TEMPLATE_FILE,
    sha256,
};

type TestResult = Result<(), Box<dyn Error>>;

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The vendored demo data the seed files are read from in a checkout.
const DEMO_DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-ref/docker/demo-data"
);

/// Writes the release's seed data into `out` with the release lane's own
/// script.
fn released(out: &Path) -> TestResult {
    let script = Path::new(ROOT).join("scripts/release/seed-data.sh");
    let output = Command::new("bash").arg(script).arg(out).output()?;
    assert!(
        output.status.success(),
        "the release lane's script writes the seed data: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn the_seed_data_file_a_release_attaches_is_what_a_run_reads() -> TestResult {
    let dir = tempfile::tempdir()?;
    released(dir.path())?;
    let file = dir.path().join(BUNDLE_FILE);

    let bundled = SeedData::read(&file)?;
    let vendored = SeedData::read(Path::new(DEMO_DATA))?;
    assert_eq!(vendored.template(), bundled.template(), "the template");
    assert_eq!(vendored.hospital(), bundled.hospital(), "the hospital's");
    assert_eq!(vendored.clinic(), bundled.clinic(), "the clinic's");

    let digest = std::fs::read_to_string(dir.path().join(format!("{BUNDLE_FILE}.sha256sum")))?;
    assert_eq!(
        format!("{}  {BUNDLE_FILE}\n", sha256(&std::fs::read(&file)?)),
        digest,
        "the SHA-256 beside the file is its own, in sha256sum form"
    );
    Ok(())
}

/// A seed data file of `format` holding `files`, each a name and its text,
/// written into `dir`.
fn bundle(
    dir: &Path,
    format: &str,
    files: &[(&str, &str)],
) -> Result<std::path::PathBuf, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Written<'a> {
        format: &'a str,
        files: std::collections::BTreeMap<&'a str, &'a str>,
    }
    let path = dir.join(BUNDLE_FILE);
    let written = Written {
        format,
        files: files.iter().copied().collect(),
    };
    std::fs::write(&path, serde_json::to_vec(&written)?)?;
    Ok(path)
}

/// The three vendored files as text, by name.
fn vendored_texts() -> Result<Vec<(&'static str, String)>, Box<dyn Error>> {
    let mut texts = Vec::new();
    for name in [TEMPLATE_FILE, HOSPITAL_FILE, CLINIC_FILE] {
        texts.push((
            name,
            std::fs::read_to_string(Path::new(DEMO_DATA).join(name))?,
        ));
    }
    Ok(texts)
}

#[test]
fn a_seed_data_file_holding_other_content_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let texts = vendored_texts()?;
    let mut files: Vec<(&str, &str)> = texts
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    if let Some(clinic) = files.iter_mut().find(|(name, _)| *name == CLINIC_FILE) {
        clinic.1 = "{}";
    }
    let path = bundle(dir.path(), BUNDLE_FORMAT, &files)?;
    let refused = SeedData::read(&path);
    assert!(
        matches!(
            refused,
            Err(SeedDataError::MemberDigest { member, .. }) if member == CLINIC_FILE
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_seed_data_file_lacking_a_file_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let texts = vendored_texts()?;
    let files: Vec<(&str, &str)> = texts
        .iter()
        .filter(|(name, _)| *name != TEMPLATE_FILE)
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let path = bundle(dir.path(), BUNDLE_FORMAT, &files)?;
    let refused = SeedData::read(&path);
    assert!(
        matches!(
            refused,
            Err(SeedDataError::Missing { member, .. }) if member == TEMPLATE_FILE
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_seed_data_file_of_another_format_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let texts = vendored_texts()?;
    let files: Vec<(&str, &str)> = texts
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let path = bundle(dir.path(), "ferrofed-conformance-seed-data/2", &files)?;
    let refused = SeedData::read(&path);
    assert!(
        matches!(refused, Err(SeedDataError::Format { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_file_that_is_no_seed_data_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, "not the seed data")?;
    let refused = SeedData::read(&path);
    assert!(
        matches!(refused, Err(SeedDataError::Bundle { .. })),
        "{refused:?}"
    );
    let absent = SeedData::read(&dir.path().join("absent.json"));
    assert!(
        matches!(absent, Err(SeedDataError::Read { .. })),
        "{absent:?}"
    );
    Ok(())
}
