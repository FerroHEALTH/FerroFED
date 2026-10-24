// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIR package set a received patient summary is validated against in
//! full (#808): the committed HL7 Europe Patient Summary package, and the
//! packages of its dependency closure.
//!
//! FerroBRIDGE's validator refuses a package set that lacks a package a
//! manifest names. The closure's committed packages sit under `docs/specs/`;
//! the rest are pinned by URL and sha256 in `scripts/vendor/fhir-packages.sh`
//! and kept in the ignored `.vendor-cache/fhir-packages/`. [`dependencies`]
//! reads those pins from `docs/specs/fhir-packages/PROVENANCE.md`, which the
//! script writes, and fetches a package the cache lacks with `curl`, held to
//! its sha256 before it takes its place, so a test never reads a package
//! other than the pinned one. A test process that finds every package in the
//! cache fetches nothing. No specification governs the harness: our own
//! design.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use aws_lc_rs::digest::{SHA256, digest};

/// The repository root, relative to this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The provenance that pins the cached packages.
const PROVENANCE: &str = "docs/specs/fhir-packages/PROVENANCE.md";

/// The cache directory the packages are fetched into.
const CACHE: &str = ".vendor-cache/fhir-packages";

/// How long a test process waits for another's fetch before it takes the
/// lock over.
const STALE: std::time::Duration = std::time::Duration::from_secs(600);

/// The committed packages of the closure besides the Patient Summary
/// itself: the versions its manifest and its dependencies' manifests name,
/// and the MyHealth@EU value sets its profiles bind.
const COMMITTED: [&str; 6] = [
    "docs/specs/eu-hl7-base/hl7.fhir.eu.base-2.0.1.tgz",
    "docs/specs/eu-hl7-extensions/hl7.fhir.eu.extensions.r4-1.3.0.tgz",
    "docs/specs/eu-hl7-extensions/hl7.fhir.eu.extensions.r4-1.3.1.tgz",
    "docs/specs/hl7-ips/hl7.fhir.uv.ips-2.0.0.tgz",
    "docs/specs/ihe-pharm-mpd/ihe.pharm.mpd.r4-1.0.0-comment-2.tgz",
    "docs/specs/ehdsi-mvc/myhealth.eu.fhir.mvc-package-9.1.0.tgz",
];

/// Why the package set could not be provided.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FhirPackagesError {
    /// A file could not be read or written.
    #[error("{} could not be read or written", path.display())]
    Io {
        /// The path.
        path: PathBuf,
        /// The I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The provenance names an artefact without its source or sha256.
    #[error("{PROVENANCE} names {file} without its source or sha256")]
    Provenance {
        /// The artefact.
        file: String,
    },
    /// `curl` could not fetch a package.
    #[error(
        "{url} could not be fetched; run scripts/vendor/fhir-packages.sh where the network is reachable"
    )]
    Fetch {
        /// The source.
        url: String,
    },
    /// A fetched package does not carry its pinned sha256.
    #[error("{url} has sha256 {got}, pinned {want}")]
    Digest {
        /// The source.
        url: String,
        /// The sha256 of the bytes fetched.
        got: String,
        /// The pinned sha256.
        want: String,
    },
}

/// One pinned package of the cache.
struct Pinned {
    file: String,
    url: String,
    sha256: String,
}

impl fmt::Debug for Pinned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pinned")
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

/// Returns the path of the committed HL7 Europe Patient Summary package, the
/// package whose profiles a patient summary is held to.
#[must_use]
pub fn patient_summary() -> PathBuf {
    Path::new(ROOT).join("docs/specs/eu-hl7-eps/hl7.fhir.eu.eps-1.0.0-ballot.tgz")
}

/// Returns every package of the Patient Summary's dependency closure.
///
/// The closure holds the MyHealth@EU value sets its profiles bind, the
/// committed packages, and the cached ones, each fetched first when the
/// cache lacks it.
///
/// # Errors
///
/// A [`FhirPackagesError`] for a provenance that does not read, a package
/// that cannot be fetched, and one whose bytes are not the pinned ones.
pub fn dependencies() -> Result<Vec<PathBuf>, FhirPackagesError> {
    let root = Path::new(ROOT);
    let mut paths: Vec<PathBuf> = COMMITTED.iter().map(|path| root.join(path)).collect();
    let cache = root.join(CACHE);
    let pins = pins(&root.join(PROVENANCE))?;
    if pins
        .iter()
        .any(|pinned| !cache.join(&pinned.file).is_file())
    {
        fetch_missing(&pins, &cache)?;
    }
    paths.extend(pins.iter().map(|pinned| cache.join(&pinned.file)));
    Ok(paths)
}

/// Fetches every one of `pins` that `cache` lacks, one test process at a
/// time: the others wait for the lock directory to go, so a run of many
/// test processes downloads each package once.
#[expect(
    clippy::create_dir,
    reason = "the lock is a directory whose creation fails when another process holds it, which create_dir_all would not report"
)]
fn fetch_missing(pins: &[Pinned], cache: &Path) -> Result<(), FhirPackagesError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| FhirPackagesError::Io { path, source }
    };
    std::fs::create_dir_all(cache).map_err(io(cache))?;
    let lock = cache.join(".fetching");
    let started = std::time::Instant::now();
    loop {
        match std::fs::create_dir(&lock) {
            Ok(()) => break,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // NOTE: no specification governs this: our own design; a lock older than
                // the longest fetch is left by a process that died, and is taken over.
                if started.elapsed() > STALE {
                    let _removed = std::fs::remove_dir(&lock);
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(error) => return Err(io(&lock)(error)),
        }
    }
    let fetched = pins.iter().try_for_each(|pinned| {
        let path = cache.join(&pinned.file);
        if path.is_file() {
            Ok(())
        } else {
            fetch(pinned, cache, &path)
        }
    });
    let _released = std::fs::remove_dir(&lock);
    fetched
}

/// The cached packages `provenance` pins, in its order.
fn pins(provenance: &Path) -> Result<Vec<Pinned>, FhirPackagesError> {
    let text = std::fs::read_to_string(provenance).map_err(|source| FhirPackagesError::Io {
        path: provenance.to_owned(),
        source,
    })?;
    let mut pins = Vec::new();
    let mut current: Option<(String, Option<String>, Option<String>)> = None;
    let mut finish = |entry: Option<(String, Option<String>, Option<String>)>| match entry {
        None => Ok(()),
        Some((file, Some(url), Some(sha256))) => {
            pins.push(Pinned { file, url, sha256 });
            Ok(())
        }
        Some((file, _, _)) => Err(FhirPackagesError::Provenance { file }),
    };
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("### `") {
            finish(current.take())?;
            let file = heading
                .split_once('`')
                .map_or(heading, |(file, _)| file)
                .to_owned();
            current = Some((file, None, None));
        } else if let Some((_, url, sha256)) = current.as_mut() {
            if let Some(source) = line.strip_prefix("- Source: <") {
                *url = Some(source.trim_end_matches('>').to_owned());
            } else if let Some(hash) = line.strip_prefix("- sha256: `") {
                *sha256 = Some(hash.trim_end_matches('`').to_owned());
            }
        }
    }
    finish(current)?;
    Ok(pins)
}

/// Fetches `pinned` into `cache` as `path`, held to its sha256 before it
/// takes its place, so a concurrent reader never sees a partial file.
fn fetch(pinned: &Pinned, cache: &Path, path: &Path) -> Result<(), FhirPackagesError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |source| FhirPackagesError::Io { path, source }
    };
    std::fs::create_dir_all(cache).map_err(io(cache))?;
    let partial = cache.join(format!("{}.{}.part", pinned.file, std::process::id()));
    let status = Command::new("curl")
        .args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-fsSL",
            "--retry",
            "2",
            "-m",
            "300",
        ])
        .args(["-A", "ferrofed-testkit", "-o"])
        .arg(&partial)
        .arg(&pinned.url)
        .status()
        .map_err(io(&partial))?;
    if !status.success() {
        let _removed = std::fs::remove_file(&partial);
        return Err(FhirPackagesError::Fetch {
            url: pinned.url.clone(),
        });
    }
    let bytes = std::fs::read(&partial).map_err(io(&partial))?;
    let got: String = digest(&SHA256, &bytes)
        .as_ref()
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .collect();
    if got != pinned.sha256 {
        let _removed = std::fs::remove_file(&partial);
        return Err(FhirPackagesError::Digest {
            url: pinned.url.clone(),
            got,
            want: pinned.sha256.clone(),
        });
    }
    std::fs::rename(&partial, path).map_err(io(path))
}
