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
//! and `ehds-logging`, both published crates read from the resolved graph,
//! reach neither each other nor the application, only the server links both,
//! and the engine compiles neither, nor any FHIR model (#684, #730). The
//! component's federation half, `ferrofed-eehrxf`, reaches the component, the
//! component reaches nothing of FerroFED, and only the server links the half
//! (#776). That `eehrxf`'s FHIR R4 serialisation compiles no openEHR crate
//! without its `openehr` feature is checked where the crate is built,
//! FerroHEALTH/FerroBRIDGE, because cargo takes no feature flag for a package
//! outside the workspace (#813).
//!
//! The checks read the graph with `cargo tree`, so they hold from the
//! placeholder modules on and turn red the day a dependency edge would break
//! them, before any type exists to hold a store handle. Dev-dependencies are
//! outside them: a test may open a store.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
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

/// The European logging software component, which the server's composition
/// root alone builds records for (Regulation (EU) 2025/327 Art 2(2)(o)). It
/// is the published crate of the family's audit library (FerroAUDIT), so it
/// is read from the dependency graph and is no member.
const LOGGING: &str = "ehds-logging";

/// The European interoperability software component (Regulation (EU)
/// 2025/327 Art 2(2)(n)). It is published from FerroHEALTH/FerroBRIDGE (#813),
/// so it is read from the dependency graph and is no member.
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

/// The operator console, a client of the gateway's public surface.
const CONSOLE: &str = "ferrofed-console";

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
/// as the workspace resolves it, with the features its members turn on: cargo
/// takes no feature flag for a package outside the workspace, such as the
/// published logging component.
fn resolved(package: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    tree(package, &[])
}

/// The package names in the normal and build dependency closure of `package`,
/// with the features `selection` names.
fn tree(package: &str, selection: &[&str]) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut arguments = vec!["--prefix", "none", "--format", "{p}"];
    arguments.extend_from_slice(selection);
    Ok(cargo_tree(package, &arguments)?
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

/// The package names `package` depends on directly through its normal and
/// build dependencies, with every feature on.
fn direct(package: &str) -> Result<BTreeSet<String>, Box<dyn Error>> {
    Ok(cargo_tree(
        package,
        &[
            "--depth",
            "1",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--all-features",
        ],
    )?
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
    Ok(cargo_tree(
        package,
        &[
            "--invert",
            target,
            "--depth",
            "1",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--all-features",
        ],
    )?
    .lines()
    .filter_map(|line| line.split_whitespace().next())
    .filter(|name| *name != target)
    .map(str::to_owned)
    .collect())
}

/// Runs `cargo tree` over the normal and build edges of `package` with
/// `arguments` added, and returns what it printed.
///
/// The child reads the lockfile and the registry already on disk
/// (`--frozen`), so it never reaches the network. Cargo still takes the
/// package cache lock of its home to resolve, offline too, and a cargo
/// building elsewhere on the machine holds that lock for its whole build. So
/// the child runs with a home of its own in a temporary directory, whose lock
/// no other cargo takes, and whose `registry` and `git` link to the real
/// home's, which an offline, frozen resolution only reads (the Cargo book,
/// "Cargo Home"). It never cleans the cache it borrows
/// (`cache.auto-clean-frequency`).
fn cargo_tree(package: &str, arguments: &[&str]) -> Result<String, Box<dyn Error>> {
    let home = tempfile::tempdir()?;
    let shared = match std::env::var_os("CARGO_HOME") {
        Some(shared) => PathBuf::from(shared),
        None => std::env::home_dir()
            .ok_or("neither CARGO_HOME nor a home directory names the cargo home")?
            .join(".cargo"),
    };
    for cache in ["registry", "git"] {
        let source = shared.join(cache);
        if source.exists() {
            std::os::unix::fs::symlink(&source, home.path().join(cache))?;
        }
    }
    let output = Command::new(env!("CARGO"))
        .env("CARGO_HOME", home.path())
        .env("CARGO_CACHE_AUTO_CLEAN_FREQUENCY", "never")
        .arg("tree")
        .arg("--manifest-path")
        .arg(Path::new(ROOT).join("Cargo.toml"))
        .args(["--package", package])
        .args(["--edges", "normal,build"])
        .args(arguments)
        .arg("--frozen")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo tree {arguments:?} failed for {package}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
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
fn the_interoperability_component_depends_on_nothing_in_ferrofed() -> Result<(), Box<dyn Error>> {
    let mut internal = members("crates")?;
    internal.extend(members("app")?);
    internal.extend(members("tools")?);
    let reached = resolved(INTEROPERABILITY)?;
    assert!(
        reached.contains(INTEROPERABILITY),
        "cargo tree for {INTEROPERABILITY} did not list the crate itself, so its output was not read"
    );
    let breaches: Vec<String> = internal
        .iter()
        .filter(|name| reached.contains(name.as_str()))
        .map(|name| format!("{INTEROPERABILITY} depends on {name}"))
        .collect();
    assert!(
        breaches.is_empty(),
        "the interoperability component depends on FerroFED (#106): {breaches:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_operator_console_reaches_the_gateway_over_http_alone() -> Result<(), Box<dyn Error>> {
    let reached = closure(CONSOLE)?;
    assert!(
        reached.contains(CONSOLE),
        "cargo tree for {CONSOLE} did not list the crate itself, so its output was not read"
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

/// The crates of a page drawn in the browser, which the operator console
/// links no more: every page is one document its server draws with FerroUI
/// 0.6, and FerroUI's script modules are files the console serves (#1457).
const BROWSER_RENDERING: &[&str] = &[
    "leptos",
    "leptos_axum",
    "leptos_meta",
    "leptos_router",
    "server_fn",
    "tachys",
    "wasm-bindgen",
    "web-sys",
    "js-sys",
    "console_error_panic_hook",
];

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn the_operator_console_draws_every_page_on_its_server() -> Result<(), Box<dyn Error>> {
    let reached = closure(CONSOLE)?;
    assert!(
        reached.contains("ferrohealth-ui"),
        "cargo tree for {CONSOLE} did not list FerroUI, so its output was not read"
    );
    let breaches: Vec<&str> = BROWSER_RENDERING
        .iter()
        .copied()
        .filter(|name| reached.contains(*name))
        .collect();
    assert!(
        breaches.is_empty(),
        "the operator console links a crate that draws a page in the browser, which FerroUI 0.6 has no place for (#1457): {breaches:?}"
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
    let interoperability = resolved(INTEROPERABILITY)?;
    let logging = resolved(LOGGING)?;
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
    let application = closure(APPLICATION)?;
    assert!(
        application.contains(INTEROPERABILITY) && application.contains(LOGGING),
        "the server no longer links both harmonised components"
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
fn the_interoperability_glue_sits_between_the_server_and_the_component()
-> Result<(), Box<dyn Error>> {
    let mut checked = members("crates")?;
    checked.extend(members("app")?);
    checked.extend(members("tools")?);
    assert!(
        checked.contains(INTEROPERABILITY_GLUE),
        "the crate map lost {INTEROPERABILITY_GLUE}: {checked:?}"
    );
    let glue = closure(INTEROPERABILITY_GLUE)?;
    assert!(
        glue.contains(INTEROPERABILITY_GLUE) && glue.contains(INTEROPERABILITY),
        "cargo tree for {INTEROPERABILITY_GLUE} did not list it or the component, so its output was not read"
    );
    let component = resolved(INTEROPERABILITY)?;
    let mut breaches: Vec<String> = checked
        .iter()
        .filter(|name| component.contains(name.as_str()))
        .map(|name| format!("{INTEROPERABILITY} reaches {name}"))
        .collect();
    for member in &checked {
        if member == APPLICATION || member == INTEROPERABILITY_GLUE {
            continue;
        }
        if closure(member)?.contains(INTEROPERABILITY_GLUE) {
            breaches.push(format!("{member} reaches {INTEROPERABILITY_GLUE}"));
        }
    }
    assert!(
        breaches.is_empty(),
        "the component links FerroFED, or a crate other than the server links its federation half (Art 2(2)(n)): {breaches:?}"
    );
    Ok(())
}
