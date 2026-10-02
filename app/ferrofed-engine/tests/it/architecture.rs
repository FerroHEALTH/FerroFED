// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crate boundaries, read from the crate graph (no specification governs
//! them: our own design).
//!
//! The clinical path does no synchronous storage work (#40): the
//! storage implementations live in `app/ferrofed-server`, so no published
//! `crates/*` library and no other `app/*` crate (the engine included) may
//! reach one, or the server, through its normal or build dependencies. The
//! IHE and Dutch binding crates depend on nothing in FerroFED (#106), so a
//! patient index or another gateway can use them as they are. The engine names
//! no HTTP engine directly, so every request to a node is built by
//! `openehr-its`'s client runtime (#34).
//!
//! The checks read the graph with `cargo tree`, so they hold from the
//! placeholder modules on and turn red the day a dependency edge would break
//! them, before any type exists to hold a store handle. Dev-dependencies are
//! outside them: a test may open a store.

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

/// HTTP engines and clients, by crates.io name. Every request to a node is
/// built by `openehr-its`'s client runtime (#34), so the engine names none of
/// them directly; the runtime's own engine reaches it transitively.
const HTTP_ENGINES: &[&str] = &[
    "attohttpc",
    "curl",
    "h2",
    "hyper",
    "hyper-util",
    "isahc",
    "reqwest",
    "surf",
    "ureq",
];

/// The application holds the storage implementations, so no other member may
/// depend on it either.
const APPLICATION: &str = "ferrofed-server";

/// The binding crates that carry no FerroFED dependency at all.
const STANDALONE: &[&str] = &["ihe-iti", "nl-generic-functions"];

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The package names in the normal and build dependency closure of `package`,
/// with every feature on, so a dependency behind a feature is seen too.
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

/// The package names `package` depends on directly through its normal and
/// build dependencies, with every feature on.
fn direct(package: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let manifest = Path::new(ROOT).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .arg("tree")
        .arg("--manifest-path")
        .arg(&manifest)
        .args(["--package", package])
        .args(["--edges", "normal,build"])
        .args(["--depth", "1"])
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

/// The package names of the members under one workspace directory, read from
/// the tree, so a new member is covered the day it lands.
fn members(directory: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut members = BTreeSet::new();
    for entry in std::fs::read_dir(Path::new(ROOT).join(directory))? {
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
fn no_crate_but_the_server_reaches_a_storage_implementation() -> Result<(), Box<dyn Error>> {
    let mut checked = members("crates")?;
    checked.extend(members("app")?);
    assert!(
        checked.remove(APPLICATION),
        "the crate map lost the server: {checked:?}"
    );
    assert!(
        checked.contains("openehr-federation") && checked.contains("ferrofed-engine"),
        "the crate map lost the federation crate or the engine: {checked:?}"
    );
    let mut breaches = Vec::new();
    for member in &checked {
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
        "storage reaches a crate other than the server (#40): {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_binding_crates_depend_on_nothing_in_ferrofed() -> Result<(), Box<dyn Error>> {
    let mut internal = members("crates")?;
    internal.extend(members("app")?);
    internal.extend(members("tools")?);
    let mut breaches = Vec::new();
    for binding in STANDALONE {
        assert!(
            internal.contains(*binding),
            "the crate map lost the binding crate {binding}: {internal:?}"
        );
        let reached = closure(binding)?;
        assert!(
            reached.contains(*binding),
            "cargo tree for {binding} did not list the crate itself, so its output was not read"
        );
        for name in &internal {
            if name != binding && reached.contains(name.as_str()) {
                breaches.push(format!("{binding} depends on {name}"));
            }
        }
    }
    assert!(
        breaches.is_empty(),
        "a binding crate depends on FerroFED (#106): {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_engine_builds_no_http_request_of_its_own() -> Result<(), Box<dyn Error>> {
    let named = direct("ferrofed-engine")?;
    assert!(
        named.contains("openehr-its"),
        "the engine lost its node client runtime: {named:?}"
    );
    let engines: Vec<&str> = HTTP_ENGINES
        .iter()
        .copied()
        .filter(|engine| named.contains(*engine))
        .collect();
    assert!(
        engines.is_empty(),
        "the engine depends on an HTTP engine directly, so it could build a request outside openehr-its (#34): {engines:?}"
    );
    Ok(())
}
