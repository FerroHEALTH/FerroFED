// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crate boundaries, read from the crate graph (no specification governs
//! them: our own design).
//!
//! The clinical path does no synchronous storage work (#40): the
//! storage implementations live in `app/ferrofed-server`, so no published
//! `crates/*` library and no other `app/*` crate (the engine included) may
//! reach one, or the server, through its normal or build dependencies. The
//! IHE and Dutch binding crates, and the RFC 8414 crate they share with the
//! core, depend on nothing in FerroFED (#106), so a
//! patient index or another gateway can use them as they are. The engine names
//! no HTTP engine directly, so every request to a node is built by
//! `openehr-its`'s client runtime (#34). The operator console links no part
//! of the gateway, so it reaches it over HTTP as any client does (#275). The
//! two harmonised software components of Regulation (EU) 2025/327, `eehrxf`
//! and `ehds-logging`, reach neither each other nor the application, only the
//! server links both, and the engine compiles neither, nor any FHIR model
//! (#684, #730). `eehrxf`'s FHIR R4 serialisation compiles no openEHR crate
//! without its `openehr` mapping feature (#730).
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

/// The binding crates, the specification crates they share, and the two EHDS
/// harmonised components, that carry no FerroFED dependency at all; a binding
/// crate may depend on another, and the components are held apart by
/// `the_two_harmonised_components_are_independent_of_each_other`.
const STANDALONE: &[&str] = &[
    "ihe-iti",
    "nl-generic-functions",
    "oauth-server-metadata",
    "ehds-logging",
    "eehrxf",
];

/// The European logging software component, which the server's composition
/// root alone builds records for (Regulation (EU) 2025/327 Art 2(2)(o)).
const LOGGING: &str = "ehds-logging";

/// The European interoperability software component (Regulation (EU)
/// 2025/327 Art 2(2)(n)).
const INTEROPERABILITY: &str = "eehrxf";

/// The crates of the interoperability component's mapping engine, which the
/// logging component must not reach either.
const MAPPING_ENGINE: &[&str] = &["fhirconnect", "openehr-mapping-core"];

/// The interoperability component's federation half, which must not reach
/// the logging component.
const INTEROPERABILITY_GLUE: &str = "ferrofed-eehrxf";

/// The engine, the gateway core.
const ENGINE: &str = "ferrofed-engine";

/// The FHIR model crate, which the core never compiles.
const FHIR_MODEL: &str = "fhir-types";

/// The interoperability component's FHIR R4 serialisation, which compiles no
/// openEHR crate without the mapping from openEHR.
const SERIALISATION: &str = "fhir-r4";

/// The prefix of every crate of the openEHR family, by crates.io name.
const OPENEHR: &str = "openehr-";

/// The operator console, a client of the gateway's public surface.
const VIEWER: &str = "ferrofed-viewer";

/// The parts of the gateway the operator console must not link: it reaches
/// the gateway over HTTP as any other client does.
const GATEWAY: &[&str] = &["ferrofed-engine", "ferrofed-identity", "ferrofed-server"];

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The package names in the normal and build dependency closure of `package`,
/// with every feature on, so a dependency behind a feature is seen too.
fn closure(package: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    tree(package, &["--all-features"])
}

/// The package names in the normal and build dependency closure of `package`
/// with `feature` alone on.
fn closure_of(package: &str, feature: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    tree(package, &["--no-default-features", "--features", feature])
}

/// The package names in the normal and build dependency closure of `package`,
/// with the features `selection` names.
fn tree(package: &str, selection: &[&str]) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let manifest = Path::new(ROOT).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .arg("tree")
        .arg("--manifest-path")
        .arg(&manifest)
        .args(["--package", package])
        .args(["--edges", "normal,build"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(selection)
        .arg("--locked")
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

/// The package names in `package`'s normal and build closure, with every
/// feature on, that depend on `target` directly; empty when the closure does
/// not hold `target`.
fn dependents(package: &str, target: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    if !closure(package)?.contains(target) {
        return Ok(BTreeSet::new());
    }
    let manifest = Path::new(ROOT).join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .arg("tree")
        .arg("--manifest-path")
        .arg(&manifest)
        .args(["--package", package])
        .args(["--invert", target])
        .args(["--edges", "normal,build"])
        .args(["--depth", "1"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(["--all-features", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree --invert {target} failed for {package}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != target)
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
            if !STANDALONE.contains(&name.as_str()) && reached.contains(name.as_str()) {
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
fn the_operator_console_reaches_the_gateway_over_http_alone() -> Result<(), Box<dyn Error>> {
    let reached = closure(VIEWER)?;
    assert!(
        reached.contains(VIEWER),
        "cargo tree for {VIEWER} did not list the crate itself, so its output was not read"
    );
    let breaches: Vec<&str> = GATEWAY
        .iter()
        .copied()
        .filter(|name| reached.contains(*name))
        .collect();
    assert!(
        breaches.is_empty(),
        "the operator console links a part of the gateway, so it could do what no client can (#275): {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn no_crate_but_the_server_builds_an_access_record() -> Result<(), Box<dyn Error>> {
    let mut breaches = Vec::new();
    for member in members("app")?.iter().chain(&members("crates")?) {
        if member == APPLICATION || member == LOGGING {
            continue;
        }
        if closure(member)?.contains(LOGGING) {
            breaches.push(member.clone());
        }
    }
    assert!(
        breaches.is_empty(),
        "a crate other than the server reaches the logging component, so it could build or emit a record (#623): {breaches:?}"
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

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_two_harmonised_components_are_independent_of_each_other() -> Result<(), Box<dyn Error>> {
    // NOTE: Regulation (EU) 2025/327 Art 2(2)(n), (o) define each component as
    // "independent of" the other; reading that as no dependency edge either
    // way is our own reading, which the crate graph enforces.
    let application = members("app")?;
    let interoperability = closure(INTEROPERABILITY)?;
    let logging = closure(LOGGING)?;
    assert!(
        interoperability.contains(INTEROPERABILITY) && logging.contains(LOGGING),
        "cargo tree did not list a component itself, so its output was not read"
    );
    let mut breaches = Vec::new();
    if interoperability.contains(LOGGING) {
        breaches.push(format!("{INTEROPERABILITY} reaches {LOGGING}"));
    }
    for name in std::iter::once(&INTEROPERABILITY)
        .chain(MAPPING_ENGINE)
        .chain(std::iter::once(&INTEROPERABILITY_GLUE))
    {
        if logging.contains(*name) {
            breaches.push(format!("{LOGGING} reaches {name}"));
        }
    }
    for (component, reached) in [(INTEROPERABILITY, &interoperability), (LOGGING, &logging)] {
        for name in &application {
            if reached.contains(name.as_str()) {
                breaches.push(format!("{component} reaches {name}"));
            }
        }
    }
    assert!(
        breaches.is_empty(),
        "a harmonised component depends on the other or on the application (Art 2(2)(n), (o)): {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn no_crate_but_the_composition_root_links_both_components() -> Result<(), Box<dyn Error>> {
    let mut checked = members("crates")?;
    checked.extend(members("app")?);
    checked.extend(members("tools")?);
    assert!(
        checked.contains(INTEROPERABILITY) && checked.contains(LOGGING),
        "the crate map lost a harmonised component: {checked:?}"
    );
    let mut breaches = Vec::new();
    for member in &checked {
        if member == APPLICATION {
            continue;
        }
        let reached = closure(member)?;
        if reached.contains(INTEROPERABILITY) && reached.contains(LOGGING) {
            breaches.push(member.clone());
        }
    }
    assert!(
        breaches.is_empty(),
        "a crate other than the server links both harmonised components: {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_engine_compiles_neither_component_and_no_fhir() -> Result<(), Box<dyn Error>> {
    let reached = closure(ENGINE)?;
    assert!(
        reached.contains(ENGINE),
        "cargo tree for {ENGINE} did not list the crate itself, so its output was not read"
    );
    let mut breaches: Vec<String> = [INTEROPERABILITY, LOGGING]
        .iter()
        .chain(MAPPING_ENGINE)
        .filter(|name| reached.contains(**name))
        .map(|name| format!("{ENGINE} reaches {name}"))
        .collect();
    breaches.extend(
        dependents(ENGINE, FHIR_MODEL)?
            .into_iter()
            .map(|dependent| format!("{dependent} brings {FHIR_MODEL} into {ENGINE}")),
    );
    if reached.contains(FHIR_MODEL) && breaches.is_empty() {
        breaches.push(format!("{ENGINE} reaches {FHIR_MODEL}"));
    }
    assert!(
        breaches.is_empty(),
        "the core compiles a harmonised component or a FHIR model: {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_fhir_serialisation_alone_compiles_no_openehr() -> Result<(), Box<dyn Error>> {
    let reached = closure_of(INTEROPERABILITY, SERIALISATION)?;
    assert!(
        reached.contains(INTEROPERABILITY) && reached.contains(FHIR_MODEL),
        "cargo tree for {INTEROPERABILITY} with {SERIALISATION} did not list the crate or its FHIR model, so its output was not read"
    );
    let breaches: Vec<&String> = reached
        .iter()
        .filter(|name| name.starts_with(OPENEHR) || MAPPING_ENGINE.contains(&name.as_str()))
        .collect();
    assert!(
        breaches.is_empty(),
        "{INTEROPERABILITY} with {SERIALISATION} alone compiles openEHR, which only its mapping feature may: {breaches:?}"
    );
    Ok(())
}
