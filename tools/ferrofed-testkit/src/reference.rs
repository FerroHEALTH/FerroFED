// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Federation Tier reference implementation as a second gateway in the
//! end-to-end harness, behind the `FERROFED_E2E` gate.
//!
//! The differential run sends the same requests to FerroFED and to the
//! reference implementation over the same two FerroEHR nodes and compares
//! what each answers. The reference implementation is evidence, never an
//! oracle: a difference is adjudicated against the specification.
//!
//! [`Reference::start`] builds the reference implementation from its pinned
//! source and runs it beside a PostgreSQL database of its own. The source is
//! the vendored tree at `docs/specs/federation-ref/`, which is the upstream
//! commit [`REFERENCE_COMMIT`] less its Maven manifest; the manifest is
//! fetched from the same commit and held to [`POM_SHA256`], the digest the
//! vendored provenance records. Maven runs in a container of the
//! digest-pinned [`MAVEN`] image, and the jar it packages is copied into an
//! image on the digest-pinned [`TEMURIN_JRE`] image, so no published image of
//! the upstream is needed. A failed build returns the end of Maven's own
//! output as [`ReferenceError::Build`].
//!
//! Maven fetches the build's dependencies into a local repository. When
//! [`MAVEN_REPOSITORY_VARIABLE`] names a host directory, that directory is
//! the repository, so a run whose directory a CI cache restored needs no
//! download, and Maven retries a download that fails
//! ([`MAVEN_RETRIES`] times).
//!
//! The reference container reaches the nodes and its database through the
//! Docker host gateway, as [`CONTAINER_HOST`]: each node sits behind a
//! [`CapturingProxy`](crate::proxy::CapturingProxy) started with
//! [`CapturingProxy::start_reachable`](crate::proxy::CapturingProxy::start_reachable),
//! so its journal shows what the reference implementation dispatched.
//!
//! No specification governs the harness; it is FerroFED's own design.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use testcontainers::core::wait::ExitWaitStrategy;
use testcontainers::core::{Host, IntoContainerPort, Mount, WaitFor};
use testcontainers::{ContainerAsync, GenericBuildableImage, GenericImage, ImageExt};

use crate::containers::database::{self, Postgres};
use crate::containers::images::{MAVEN, TEMURIN_JRE};
use crate::containers::{self, HarnessError, retry};

/// The upstream commit the vendored reference implementation is.
pub const REFERENCE_COMMIT: &str = "92aff3cb1d8738ea0ce0e013b5a8fc2942438fd5";

/// The sha256 of the Maven manifest at [`REFERENCE_COMMIT`], as the
/// vendored provenance records it.
pub const POM_SHA256: &str = "4447a33ca0d3321e812252e0a2bef4b454aa66ab83bb1837e88e63cd88d72dd1";

/// The host name a container reaches the Docker host by.
pub const CONTAINER_HOST: &str = "host.docker.internal";

/// The vendored source tree the image is built from.
const SOURCE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-ref/src"
);

/// The repository of the image the harness builds.
const IMAGE: &str = "ferrofed-e2e/openehr-federation-ref";

/// The port the reference implementation listens on inside its container.
const PORT: u16 = 8080;

/// The path that answers `200` once the reference implementation serves.
const READINESS_PATH: &str = "/actuator/health";

/// The database the reference implementation keeps its registry in, which is
/// also its login role.
const DATABASE: &str = "federation";

/// Where the registry document is copied inside the container.
const REGISTRY_TARGET: &str = "/app/registry.json";

/// Where the configuration is copied inside the container.
const SETTINGS_TARGET: &str = "/app/differential.yml";

/// How many bytes of the container's output a startup failure carries.
const LOG_TAIL: usize = 6_000;

/// The environment variable naming the host directory Maven keeps its local
/// repository in; unset, the repository lives and dies with the build
/// container.
pub const MAVEN_REPOSITORY_VARIABLE: &str = "FERROFED_E2E_MAVEN_REPOSITORY";

/// How many times Maven retries a download that fails, the Maven Resolver
/// setting `aether.transport.http.retryHandler.count`
/// (<https://maven.apache.org/resolver/configuration.html>).
pub const MAVEN_RETRIES: u32 = 6;

/// Where the build container holds the sources and Maven writes the jar.
const BUILD_DIRECTORY: &str = "/build";

/// Where the build container holds Maven's local repository.
const REPOSITORY_TARGET: &str = "/maven-repository";

/// The jar Maven packages, under [`BUILD_DIRECTORY`].
const JAR: &str = "target/openehr-federation-ref-0.9.0-SNAPSHOT.jar";

/// How long the Maven build may run, downloads included.
const BUILD_BUDGET: Duration = Duration::from_mins(15);

/// The reference implementation could not be built or started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReferenceError {
    /// The Maven manifest could not be fetched from the pinned commit.
    #[error("the reference implementation's manifest could not be fetched")]
    Fetch(#[source] reqwest::Error),
    /// The fetched manifest is not the one the provenance records.
    #[error("the reference implementation's manifest has sha256 {found}, expected {POM_SHA256}")]
    Digest {
        /// The digest of what was fetched.
        found: String,
    },
    /// The host directory [`MAVEN_REPOSITORY_VARIABLE`] names could not be
    /// created.
    #[error("the Maven repository directory {path} could not be created")]
    Repository {
        /// The directory.
        path: String,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// Maven ran and did not package the reference implementation.
    #[error(
        "the reference implementation's Maven build exited with {}; its output ends:\n{log}",
        code.map_or_else(|| "no exit code".to_owned(), |code| format!("code {code}"))
    )]
    Build {
        /// Maven's exit code, when Docker reports one.
        code: Option<i64>,
        /// The tail of Maven's standard output and error.
        log: String,
    },
    /// Docker could not build the image or start the container.
    #[error("the reference implementation's {stage} failed")]
    Container {
        /// What was being done.
        stage: &'static str,
        /// What Docker reported.
        #[source]
        source: testcontainers::TestcontainersError,
    },
    /// The database or the readiness probe failed.
    #[error("the reference implementation's harness failed")]
    Harness(#[source] Box<HarnessError>),
    /// The reference implementation started and never became ready.
    #[error("the reference implementation did not become ready; its output ends:\n{log}")]
    NotReady {
        /// The tail of the container's standard output and error.
        log: String,
        /// The failed readiness probe.
        #[source]
        source: Box<HarnessError>,
    },
}

/// A running reference implementation with its database, torn down when it
/// is dropped.
#[derive(Debug)]
pub struct Reference {
    /// The gateway.
    server: ContainerAsync<GenericImage>,
    /// Its database server.
    database: Postgres,
    /// The origin the gateway is reachable at from the host.
    origin: String,
}

impl Reference {
    /// Builds the reference implementation from its pinned source and starts
    /// it with the registry document `registry` and the Spring configuration
    /// `settings`, then waits for its health endpoint.
    ///
    /// `registry` is the JSON document the reference implementation applies
    /// at startup, and `settings` a YAML file under `federation.*` that the
    /// harness adds to the packaged configuration. The harness sets the
    /// database connection and the registry location itself. A node the
    /// documents name is reached at [`container_origin`].
    ///
    /// # Errors
    ///
    /// Returns [`ReferenceError::Fetch`] or [`ReferenceError::Digest`] when
    /// the manifest cannot be had at its pin, [`ReferenceError::Repository`]
    /// when Maven's repository directory cannot be created,
    /// [`ReferenceError::Build`] when Maven fails,
    /// [`ReferenceError::Container`] when Docker refuses the build or the
    /// container,
    /// [`ReferenceError::Harness`] when the database cannot be started, and
    /// [`ReferenceError::NotReady`] when the gateway never answers.
    pub async fn start(registry: &str, settings: &str) -> Result<Self, ReferenceError> {
        let image = build().await?;
        let database = database::postgres(DATABASE, &[])
            .await
            .map_err(|error| ReferenceError::Harness(Box::new(error)))?;
        let server = retry::start(|| {
            image
                .clone()
                .with_exposed_port(PORT.tcp())
                .with_host(CONTAINER_HOST, Host::HostGateway)
                .with_env_var(
                    "SPRING_DATASOURCE_URL",
                    format!(
                        "jdbc:postgresql://{CONTAINER_HOST}:{}/{DATABASE}",
                        database.port()
                    ),
                )
                .with_env_var("SPRING_DATASOURCE_USERNAME", DATABASE)
                .with_env_var(
                    "SPRING_DATASOURCE_PASSWORD",
                    containers::role_password(DATABASE),
                )
                .with_env_var(
                    "FEDERATION_REGISTRY_BOOTSTRAP_FILE",
                    format!("file:{REGISTRY_TARGET}"),
                )
                .with_env_var(
                    "SPRING_CONFIG_ADDITIONAL_LOCATION",
                    format!("file:{SETTINGS_TARGET}"),
                )
                .with_copy_to(REGISTRY_TARGET, registry.as_bytes().to_vec())
                .with_copy_to(SETTINGS_TARGET, settings.as_bytes().to_vec())
        })
        .await
        .map_err(|source| ReferenceError::Container {
            stage: "container",
            source,
        })?;
        let origin = origin(&server).await?;
        if let Err(source) = containers::await_readiness(&format!("{origin}{READINESS_PATH}")).await
        {
            return Err(ReferenceError::NotReady {
                log: tail(&server).await,
                source: Box::new(source),
            });
        }
        Ok(Self {
            server,
            database,
            origin,
        })
    }

    /// Returns the origin the reference implementation is reachable at from
    /// the host, with no path; its ITS-REST API is under `/v1`.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the gateway's container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns the gateway's database server.
    #[must_use]
    pub fn database(&self) -> &Postgres {
        &self.database
    }

    /// Returns the tail of the gateway's output, for a failure message.
    pub async fn log(&self) -> String {
        tail(&self.server).await
    }
}

/// Returns the origin a container reaches a host listener on `port` at.
#[must_use]
pub fn container_origin(port: u16) -> String {
    format!("http://{CONTAINER_HOST}:{port}")
}

/// Packages the vendored source with the pinned manifest and builds the
/// gateway's image around the jar.
///
/// The image build is cached by the Docker builder, so a call whose jar is
/// the same as the last one's reuses its layers.
async fn build() -> Result<GenericImage, ReferenceError> {
    let jar = package(manifest().await?).await?;
    let dockerfile = format!(
        "FROM {jre}\n\
         COPY gateway.jar /app/gateway.jar\n\
         EXPOSE {PORT}\n\
         ENTRYPOINT [\"java\", \"-jar\", \"/app/gateway.jar\"]\n",
        jre = TEMURIN_JRE.reference(),
    );
    retry::build(|| {
        GenericBuildableImage::new(IMAGE, REFERENCE_COMMIT)
            .with_dockerfile_string(dockerfile.clone())
            .with_data(jar.clone(), "./gateway.jar")
    })
    .await
    .map_err(|source| ReferenceError::Container {
        stage: "image build",
        source,
    })
}

/// Runs Maven over the vendored source and the manifest `pom` in a
/// container and returns the jar it packages.
async fn package(pom: Vec<u8>) -> Result<Vec<u8>, ReferenceError> {
    let repository = repository()?;
    let stage = "Maven build";
    let build = retry::start(|| {
        let request = MAVEN
            .image()
            .with_wait_for(WaitFor::exit(ExitWaitStrategy::new()))
            .with_working_dir(BUILD_DIRECTORY)
            .with_copy_to(format!("{BUILD_DIRECTORY}/pom.xml"), pom.clone())
            .with_copy_to(format!("{BUILD_DIRECTORY}/src"), Path::new(SOURCE))
            .with_cmd(maven_command())
            .with_startup_timeout(BUILD_BUDGET);
        match &repository {
            Some(path) => request.with_mount(Mount::bind_mount(path.clone(), REPOSITORY_TARGET)),
            None => request,
        }
    })
    .await
    .map_err(|source| ReferenceError::Container { stage, source })?;
    let code = build
        .exit_code()
        .await
        .map_err(|source| ReferenceError::Container { stage, source })?;
    if code != Some(0) {
        return Err(ReferenceError::Build {
            code,
            log: tail(&build).await,
        });
    }
    build
        .copy_file_from(format!("{BUILD_DIRECTORY}/{JAR}"), Vec::new())
        .await
        .map_err(|source| ReferenceError::Container { stage, source })
}

/// Returns the arguments Maven packages the reference implementation with.
///
/// Maven runs in batch mode without its download progress, so a failure's
/// tail shows Maven's own error.
fn maven_command() -> [String; 7] {
    [
        "mvn".to_owned(),
        "--batch-mode".to_owned(),
        "--no-transfer-progress".to_owned(),
        format!("-Dmaven.repo.local={REPOSITORY_TARGET}"),
        format!("-Daether.transport.http.retryHandler.count={MAVEN_RETRIES}"),
        "package".to_owned(),
        "-DskipTests".to_owned(),
    ]
}

/// Returns the host directory [`MAVEN_REPOSITORY_VARIABLE`] names, created
/// if it is missing, or `None` when the variable is unset or empty.
fn repository() -> Result<Option<String>, ReferenceError> {
    let Some(path) = std::env::var_os(MAVEN_REPOSITORY_VARIABLE).filter(|path| !path.is_empty())
    else {
        return Ok(None);
    };
    let failed = |source| ReferenceError::Repository {
        path: path.to_string_lossy().into_owned(),
        source,
    };
    std::fs::create_dir_all(&path).map_err(failed)?;
    let absolute = std::fs::canonicalize(&path).map_err(failed)?;
    Ok(Some(absolute.to_string_lossy().into_owned()))
}

/// Fetches the Maven manifest at [`REFERENCE_COMMIT`] and checks it against
/// [`POM_SHA256`].
async fn manifest() -> Result<Vec<u8>, ReferenceError> {
    let url = format!(
        "https://raw.githubusercontent.com/syntaric/openehr-federation-ref/{REFERENCE_COMMIT}/pom.xml"
    );
    let bytes = reqwest::get(url)
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(ReferenceError::Fetch)?
        .bytes()
        .await
        .map_err(ReferenceError::Fetch)?;
    let found = hex(aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, &bytes).as_ref());
    if found != POM_SHA256 {
        return Err(ReferenceError::Digest { found });
    }
    Ok(bytes.to_vec())
}

/// Returns `bytes` as lower-case hexadecimal.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // NOTE: writing into a `String` cannot fail (`std::fmt::Write` for `String`).
        let _written: std::fmt::Result = write!(text, "{byte:02x}");
    }
    text
}

/// Resolves the host origin of `server`.
async fn origin(server: &ContainerAsync<GenericImage>) -> Result<String, ReferenceError> {
    let stage = "port lookup";
    let host = server
        .get_host()
        .await
        .map_err(|source| ReferenceError::Container { stage, source })?;
    let port = server
        .get_host_port_ipv4(PORT.tcp())
        .await
        .map_err(|source| ReferenceError::Container { stage, source })?;
    Ok(format!("http://{host}:{port}"))
}

/// Returns the last [`LOG_TAIL`] bytes of `server`'s standard output and
/// error, with a note in place of a stream that could not be read.
async fn tail(server: &ContainerAsync<GenericImage>) -> String {
    let wait = Duration::from_secs(5);
    let read = async { (server.stdout_to_vec().await, server.stderr_to_vec().await) };
    let Ok((stdout, stderr)) = tokio::time::timeout(wait, read).await else {
        return "(the container's output could not be read in time)".to_owned();
    };
    let mut out = Vec::new();
    for (stream, read) in [("standard output", stdout), ("standard error", stderr)] {
        match read {
            Ok(bytes) => out.extend(bytes),
            Err(error) => out.extend(format!("({stream} unreadable: {error})\n").into_bytes()),
        }
    }
    let start = out.len().saturating_sub(LOG_TAIL);
    String::from_utf8_lossy(out.get(start..).unwrap_or(&out)).into_owned()
}
