// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `report` job: one archive that says which FerroFED a deployment runs,
//! built how and configured how, and what the running gateway reports, for
//! a complaint or a serious-incident report.
//!
//! Regulation (EU) 2025/327 Art 44(7) has the manufacturer report a serious
//! incident within three days of becoming aware of it, and Art 44(5) has an
//! authority pass on "the data necessary for the identification of the EHR
//! system concerned". The archive carries that data in one step: the build
//! and its pins ([`provenance::Build`]), the release artefacts and the
//! commands that verify their attestations ([`provenance::Release`]), the
//! effective configuration with every credential, URL credential and
//! development cross-reference redacted ([`redact`]), and, from a gateway
//! running on this host, readiness, the dependency states, the integrity
//! incidents and the metrics ([`source`]). A part that cannot be read is
//! named in the manifest with the reason, never written as an empty file.
//!
//! No part carries a patient identifier or a clinical payload (§5.4.1,
//! N33): the live parts are the routing ids, counts and states the
//! gateway's own surfaces answer with, and the configuration keeps no value
//! of `[dev]`. The archive is an uncompressed POSIX tar under one directory,
//! [`ROOT`], with `manifest.json` first. No specification governs the
//! report's form: our own design.

pub mod provenance;
pub mod redact;
pub mod source;

use std::fs::File;
use std::io::Write as _;
use std::path::Path;

use ferrofed_registry::health::DependencyReport;
use ferrofed_registry::manufacturer::{MANUFACTURER, Manufacturer};
use ferrofed_registry::operator::IncidentReport;
use http::StatusCode;
use jiff::Timestamp;
use secrecy::SecretString;
use serde::Serialize;

use crate::config::settings::Settings;
use crate::healthcheck::{READINESS, TIMEOUT};
use crate::report::provenance::{Build, Release};
use crate::report::source::{Listener, SourceError};

/// The directory every file of the archive sits under.
pub const ROOT: &str = "ferrofed-report";

/// The name of the archive format, as the manifest states it.
pub const FORMAT: &str = "ferrofed-report";

/// The version of the archive format, raised when a file changes meaning.
pub const FORMAT_VERSION: u32 = 1;

/// The path of the dependency states, below `{base}`.
const DEPENDENCIES: &str = "/health/dependencies";

/// A report, collected and not yet written.
#[derive(Debug)]
pub struct Report {
    created: Timestamp,
    parts: Vec<Part>,
    missing: Vec<Missing>,
}

/// One file of the archive.
#[derive(Debug)]
struct Part {
    path: &'static str,
    content: &'static str,
    source: Option<String>,
    bytes: Vec<u8>,
}

/// A file the archive should hold and does not, with the reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Missing {
    /// The file's path in the archive, below [`ROOT`].
    pub path: &'static str,
    /// Why it is missing.
    pub reason: String,
}

/// `manifest.json`: what the archive is and what it holds.
#[derive(Debug, Serialize)]
struct Manifest<'r> {
    format: &'static str,
    format_version: u32,
    created: String,
    product: &'static str,
    version: &'static str,
    manufacturer: Manufacturer,
    entries: Vec<ManifestEntry<'r>>,
    missing: &'r [Missing],
}

/// One file in the manifest.
#[derive(Debug, Serialize)]
struct ManifestEntry<'r> {
    path: &'static str,
    content: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'r str>,
    bytes: usize,
    sha256: String,
}

/// What stops a report from being written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReportError {
    /// A part could not be serialized.
    #[error("{path} could not be written")]
    Serialize {
        /// The part.
        path: &'static str,
        /// Why.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// The archive could not be written.
    #[error("the archive {} could not be written", path.display())]
    Write {
        /// The archive.
        path: std::path::PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
}

impl Report {
    /// Collects the report of the deployment `settings` configure, with
    /// `tree`, the merged configuration they were resolved from, and
    /// `operator`, a token with the operator scope that reads the
    /// integrity incidents.
    ///
    /// # Errors
    /// [`ReportError::Serialize`] when a part the binary writes itself
    /// cannot be serialized. A live part that cannot be read is
    /// [`missing`](Self::missing), never an error.
    pub async fn collect(
        settings: &Settings,
        tree: &toml::Table,
        operator: Option<&SecretString>,
    ) -> Result<Self, ReportError> {
        let mut report = Self {
            created: Timestamp::now(),
            parts: Vec::new(),
            missing: Vec::new(),
        };
        let build = Build::current();
        let release = Release::of(&build);
        report.own("build.json", "the build and its pins", &build)?;
        report.own(
            "release.json",
            "the release artefacts and how their attestations are verified",
            &release,
        )?;
        let configuration =
            toml::to_string(&redact::redact(tree)).map_err(|source| ReportError::Serialize {
                path: "configuration.toml",
                source: Box::new(source),
            })?;
        report.parts.push(Part {
            path: "configuration.toml",
            content: "the effective configuration, every value that could be a credential or a patient identifier replaced by ***",
            source: None,
            bytes: configuration.into_bytes(),
        });
        report.live(settings, operator).await;
        Ok(report)
    }

    /// The files the report should hold and does not.
    #[must_use]
    pub fn missing(&self) -> &[Missing] {
        &self.missing
    }

    /// The name the archive takes when none is given.
    #[must_use]
    pub fn default_name(&self) -> String {
        format!("{ROOT}-{}.tar", self.created.strftime("%Y%m%dT%H%M%SZ"))
    }

    /// Writes the archive to `path`, which must not exist yet.
    ///
    /// # Errors
    /// [`ReportError::Write`] when the file exists or cannot be written,
    /// and [`ReportError::Serialize`] when the manifest cannot be.
    pub fn write(&self, path: &Path) -> Result<(), ReportError> {
        let failed = |source| ReportError::Write {
            path: path.to_path_buf(),
            source,
        };
        let manifest = self.manifest()?;
        let file = File::create_new(path).map_err(failed)?;
        let mut archive = tar::Builder::new(file);
        // NOTE: no specification governs this: our own design; a clock before
        // 1970 stamps the files at the epoch, which tar cannot go below.
        let mtime = self.created.as_second().max(0).unsigned_abs();
        let files = std::iter::once(("manifest.json", manifest.as_slice())).chain(
            self.parts
                .iter()
                .map(|part| (part.path, part.bytes.as_slice())),
        );
        for (name, bytes) in files {
            let mut header = tar::Header::new_ustar();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o644);
            header.set_mtime(mtime);
            header.set_uid(0);
            header.set_gid(0);
            let size =
                u64::try_from(bytes.len()).map_err(|error| failed(std::io::Error::other(error)))?;
            header.set_size(size);
            archive
                .append_data(&mut header, format!("{ROOT}/{name}"), bytes)
                .map_err(failed)?;
        }
        let mut file = archive.into_inner().map_err(failed)?;
        file.flush().map_err(failed)?;
        file.sync_all().map_err(failed)
    }

    /// Returns `manifest.json`.
    fn manifest(&self) -> Result<Vec<u8>, ReportError> {
        let manifest = Manifest {
            format: FORMAT,
            format_version: FORMAT_VERSION,
            created: self.created.to_string(),
            product: crate::body::PRODUCT,
            version: crate::body::VERSION,
            manufacturer: MANUFACTURER,
            entries: self
                .parts
                .iter()
                .map(|part| ManifestEntry {
                    path: part.path,
                    content: part.content,
                    source: part.source.as_deref(),
                    bytes: part.bytes.len(),
                    sha256: crate::conformance::fixture::sha256(&part.bytes),
                })
                .collect(),
            missing: &self.missing,
        };
        pretty("manifest.json", &manifest)
    }

    /// Adds `value`, which the binary knows itself, as `path`.
    fn own<T: Serialize>(
        &mut self,
        path: &'static str,
        content: &'static str,
        value: &T,
    ) -> Result<(), ReportError> {
        let bytes = pretty(path, value)?;
        self.parts.push(Part {
            path,
            content,
            source: None,
            bytes,
        });
        Ok(())
    }

    /// Adds the parts read from the gateway running on this host.
    async fn live(&mut self, settings: &Settings, operator: Option<&SecretString>) {
        let base = &settings.server.base_path;
        match Listener::new(
            settings.server.listen,
            settings.server.tls.as_ref(),
            TIMEOUT,
        ) {
            Err(error) => {
                for path in [
                    "health/readiness.json",
                    "health/dependencies.json",
                    "incidents.json",
                ] {
                    self.miss(path, &error);
                }
            }
            Ok(gateway) => {
                let readiness = base.join(READINESS);
                let outcome = gateway
                    .get(&readiness, None)
                    .await
                    .and_then(|(status, body)| {
                        if status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE {
                            source::json(&readiness, body).map(|body| (status, body))
                        } else {
                            Err(refused(&readiness, status))
                        }
                    });
                self.add(
                    "health/readiness.json",
                    "readiness: the phase and every indicator",
                    &readiness,
                    outcome,
                );
                let dependencies = base.join(DEPENDENCIES);
                let outcome =
                    Self::document::<DependencyReport>(&gateway, &dependencies, None).await;
                self.add(
                    "health/dependencies.json",
                    "the last observed state of every dependency",
                    &dependencies,
                    outcome,
                );
                let incidents = base.join(crate::operator::INCIDENTS);
                let outcome =
                    Self::document::<IncidentReport>(&gateway, &incidents, operator).await;
                self.add(
                    "incidents.json",
                    "the integrity incidents: every kind's count and the most recent of each",
                    &incidents,
                    outcome,
                );
            }
        }
        let metrics = match settings.metrics.listen {
            None => Err(SourceError::NotServed(
                "metrics.listen is not set, so no admin listener serves the metrics",
            )),
            Some(listen) => match Listener::new(listen, settings.metrics.tls.as_ref(), TIMEOUT) {
                Err(error) => Err(error),
                Ok(admin) => {
                    admin
                        .get(crate::metrics::PATH, None)
                        .await
                        .and_then(|(status, body)| {
                            if status == StatusCode::OK {
                                source::text(crate::metrics::PATH, body).map(|body| (status, body))
                            } else {
                                Err(refused(crate::metrics::PATH, status))
                            }
                        })
                }
            },
        };
        self.add(
            "metrics.txt",
            "the metrics in the Prometheus text format",
            crate::metrics::PATH,
            metrics,
        );
    }

    /// Asks `GET path` of `gateway` for the JSON document `T`.
    async fn document<T>(
        gateway: &Listener,
        path: &str,
        bearer: Option<&SecretString>,
    ) -> Result<(StatusCode, Vec<u8>), SourceError>
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        let (status, body) = gateway.get(path, bearer).await?;
        if status != StatusCode::OK {
            return Err(refused(path, status));
        }
        source::document::<T>(path, &body).map(|body| (status, body))
    }

    /// Adds the part read from `GET asked`, or names it missing.
    fn add(
        &mut self,
        path: &'static str,
        content: &'static str,
        asked: &str,
        outcome: Result<(StatusCode, Vec<u8>), SourceError>,
    ) {
        match outcome {
            Ok((status, bytes)) => self.parts.push(Part {
                path,
                content,
                source: Some(format!("GET {asked} answered {status}")),
                bytes,
            }),
            Err(error) => self.miss(path, &error),
        }
    }

    /// Names `path` missing for `error`.
    fn miss(&mut self, path: &'static str, error: &SourceError) {
        self.missing.push(Missing {
            path,
            reason: crate::chain(error),
        });
    }
}

/// The refusal of `GET path` with `status`, with a hint where one helps.
fn refused(path: &str, status: StatusCode) -> SourceError {
    let hint = if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        "; give --operator-token-file a token that carries the operator scope"
    } else {
        ""
    };
    SourceError::Status {
        path: path.to_owned(),
        status,
        hint,
    }
}

/// Returns `value` as pretty JSON with a final newline.
fn pretty<T: Serialize>(path: &'static str, value: &T) -> Result<Vec<u8>, ReportError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| ReportError::Serialize {
        path,
        source: Box::new(source),
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}
