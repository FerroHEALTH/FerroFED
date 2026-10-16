// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every `https://ferrofed.eu/fhir/` URL the tree writes resolves to a FHIR
//! definition the documentation site publishes, the archetype-local code
//! systems alone excepted: FHIR R4 4.1.2 says an http: address "SHOULD
//! resolve", and the book's page on the registry in FHIR form says why that
//! family does not. The published set is read from the directories
//! `scripts/site/assemble.sh` copies, so a definition the site would not
//! serve counts as missing. No specification governs where a URL is
//! hosted: our own design.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use eehrxf::terminology::system::ARCHETYPE_LOCAL;
use fhir_types::codec::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// The base every FerroFED FHIR URL starts with.
const BASE: &str = "https://ferrofed.eu/fhir/";

/// The resource types the site serves, each as the file name prefix of its
/// definitions.
const TYPES: [&str; 3] = ["StructureDefinition", "CodeSystem", "NamingSystem"];

/// The trees whose text is read for URLs, from the workspace root.
const TREES: [&str; 5] = [
    "app",
    "crates",
    "tools",
    "website/book/src",
    "docs/architecture.md",
];

/// The directories never read: build output and the rendered book.
const SKIPPED: [&str; 3] = ["target", "book", "node_modules"];

/// The workspace root.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The definition directories of the site assembly's `DEFINITIONS` array.
fn definition_directories() -> Result<Vec<String>, Box<dyn Error>> {
    let script = std::fs::read_to_string(root().join("scripts/site/assemble.sh"))?;
    let directories: Vec<String> = script
        .lines()
        .skip_while(|line| line.trim() != "readonly DEFINITIONS=(")
        .skip(1)
        .take_while(|line| line.trim() != ")")
        .map(|line| line.trim().to_owned())
        .collect();
    if directories.is_empty() {
        return Err("the site assembly has no DEFINITIONS array".into());
    }
    Ok(directories)
}

/// The URL a definition file is served at, `None` for a file that is not a
/// definition.
fn served_at(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".json")?;
    let (kind, id) = stem.split_once('-')?;
    if !TYPES.contains(&kind) {
        return None;
    }
    let path = if kind == "NamingSystem" { "sid" } else { kind };
    Some(format!("{BASE}{path}/{id}"))
}

/// The URL a definition names itself by: its `url`, or for a
/// `NamingSystem`, which has none in R4, its preferred `uri`.
fn named(json: &str) -> Result<Option<String>, Box<dyn Error>> {
    let value: Value = serde_json::from_str(json)?;
    if let Some(url) = value.get("url").and_then(Value::as_str) {
        return Ok(Some(url.to_owned()));
    }
    let preferred = value
        .get("uniqueId")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|unique| unique.get("type").and_then(Value::as_str) == Some("uri"))
        .and_then(|unique| unique.get("value"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(preferred)
}

/// Every URL the site serves a definition at, each checked against the
/// URL the definition names itself by.
fn published() -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut urls = BTreeSet::new();
    for directory in definition_directories()? {
        for entry in std::fs::read_dir(root().join(&directory))? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let Some(url) = served_at(name) else {
                continue;
            };
            let named = named(&std::fs::read_to_string(&path)?)?;
            assert_eq!(named.as_deref(), Some(url.as_str()), "{directory}/{name}");
            assert!(urls.insert(url.clone()), "{url} is defined twice");
        }
    }
    Ok(urls)
}

/// The text files under `path`, build output and the rendered book left out.
fn files(path: &Path, found: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if path.is_file() {
        let text = path.extension().is_some_and(|extension| {
            ["rs", "md", "toml", "json"].contains(&extension.to_str().unwrap_or_default())
        });
        if text {
            found.push(path.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let skipped = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| SKIPPED.contains(&name) || name.starts_with('.'));
        if !skipped {
            files(&path, found)?;
        }
    }
    Ok(())
}

/// Every FerroFED FHIR URL `text` writes, past the bare base.
fn urls(text: &str) -> BTreeSet<String> {
    text.match_indices(BASE)
        .filter_map(|(at, _)| text.get(at + BASE.len()..))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'))
                .collect::<String>()
        })
        .map(|path| path.trim_end_matches('.').to_owned())
        .filter(|path| !path.is_empty())
        .map(|path| format!("{BASE}{path}"))
        .collect()
}

#[test]
fn the_vendored_r4_text_makes_resolution_a_recommendation() -> TestResult {
    let page = std::fs::read_to_string(root().join("docs/specs/fhir-r4/terminologies.html"))?;
    assert!(
        page.contains(
            "An http: address SHOULD resolve to some useful description of the code system"
        )
    );
    Ok(())
}

#[test]
fn every_ferrofed_fhir_url_but_the_archetype_local_family_is_published() -> TestResult {
    let published = published()?;
    assert!(
        published
            .iter()
            .all(|url| !url.starts_with(ARCHETYPE_LOCAL)),
        "the site publishes an archetype-local code system"
    );
    let mut found = Vec::new();
    for tree in TREES {
        files(&root().join(tree), &mut found)?;
    }
    let mut written = BTreeSet::new();
    for file in &found {
        written.extend(urls(&std::fs::read_to_string(file)?));
    }
    assert!(
        written.iter().any(|url| url.starts_with(ARCHETYPE_LOCAL)),
        "no archetype-local URL was read, so the tree was not read"
    );
    let unpublished: Vec<&String> = written
        .iter()
        .filter(|url| !url.starts_with(ARCHETYPE_LOCAL))
        .filter(|url| {
            let bare = url.strip_suffix(".json").unwrap_or(url);
            !published.contains(bare)
        })
        .collect();
    assert!(
        unpublished.is_empty(),
        "a ferrofed.eu FHIR URL with no published definition: {unpublished:?}"
    );
    Ok(())
}
