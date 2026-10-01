// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The clinical path does no synchronous storage work (#40,
//! `docs/architecture.md` sections 8 and 11). The storage implementations
//! live in `app/ferrofed-server`; no library crate, the engine and the merge
//! included, may reach one through its normal or build dependencies.
//!
//! The check reads the crate graph with `cargo tree`, so it holds from the
//! placeholder crates on and turns red the day a dependency edge would break
//! it, before any type exists to hold a store handle. Dev-dependencies are
//! outside it: a test may open a store.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;
use std::process::Command;

/// Crates that implement storage, by crates.io name. A new storage engine is
/// added here in the change that first considers it.
const STORAGE: &[&str] = &[
    "bb8-postgres",
    "deadpool-postgres",
    "diesel",
    "fjall",
    "heed",
    "libsqlite3-sys",
    "lmdb",
    "mongodb",
    "postgres",
    "redb",
    "redis",
    "rocksdb",
    "rusqlite",
    "sea-orm",
    "sled",
    "sqlx",
    "sqlx-core",
    "sqlx-mysql",
    "sqlx-postgres",
    "sqlx-sqlite",
    "tokio-postgres",
];

/// The application holds the storage implementations, so no library may
/// depend on it either.
const APPLICATION: &str = "ferrofed-server";

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The package names in the normal and build dependency closure of `package`,
/// with every feature on, so a storage dependency behind a feature is seen too.
fn closure(package: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let manifest = Path::new(ROOT).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .arg("tree")
        .arg("--manifest-path")
        .arg(&manifest)
        .args(["--package", package])
        .args(["--edges", "normal,build"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(["--all-features", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree failed for {package}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

/// Every library member, read from the `crates/` directory, so a new crate is
/// covered the day it lands.
fn library_members() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut members = BTreeSet::new();
    for entry in std::fs::read_dir(Path::new(ROOT).join("crates"))? {
        let entry = entry?;
        if entry.path().join("Cargo.toml").is_file() {
            members.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(members)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn no_library_crate_reaches_a_storage_implementation() -> Result<(), Box<dyn Error>> {
    let members = library_members()?;
    assert!(
        members.contains("ferrofed-engine") && members.contains("ferrofed-merge"),
        "the crate map lost the engine or the merge: {members:?}"
    );
    let mut breaches = Vec::new();
    for member in &members {
        let reached = closure(member)?;
        assert!(
            reached.contains(member.as_str()),
            "cargo tree for {member} did not list the crate itself, so its output was not read"
        );
        for name in STORAGE.iter().chain(std::iter::once(&APPLICATION)) {
            if reached.contains(*name) {
                breaches.push(format!("{member} depends on {name}"));
            }
        }
    }
    assert!(
        breaches.is_empty(),
        "storage reaches a library crate (#40): {breaches:?}"
    );
    Ok(())
}
