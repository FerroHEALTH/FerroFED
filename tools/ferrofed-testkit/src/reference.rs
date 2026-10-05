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
//! vendored provenance records. The build runs in the digest-pinned
//! [`MAVEN`] image and the gateway in the digest-pinned [`TEMURIN_JRE`]
//! image, so no published image of the upstream is needed.
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

use testcontainers::core::{Host, IntoContainerPort};
use testcontainers::runners::{AsyncBuilder, AsyncRunner};
use testcontainers::{ContainerAsync, GenericBuildableImage, GenericImage, ImageExt};

use crate::containers::{self, HarnessError, MAVEN, Postgres, TEMURIN_JRE};

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
    /// the manifest cannot be had at its pin, [`ReferenceError::Container`]
    /// when Docker refuses the build or the container,
    /// [`ReferenceError::Harness`] when the database cannot be started, and
    /// [`ReferenceError::NotReady`] when the gateway never answers.
    pub async fn start(registry: &str, settings: &str) -> Result<Self, ReferenceError> {
        let image = build().await?;
        let database = containers::postgres(DATABASE, &[])
            .await
            .map_err(|error| ReferenceError::Harness(Box::new(error)))?;
        let server = image
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
            .start()
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

/// Builds the image from the vendored source and the pinned manifest.
///
/// The build is cached by the Docker builder, so every call after the first
/// with the same inputs reuses its layers.
async fn build() -> Result<GenericImage, ReferenceError> {
    let pom = manifest().await?;
    let dockerfile = format!(
        "FROM {maven} AS build\n\
         WORKDIR /build\n\
         COPY pom.xml .\n\
         COPY src ./src\n\
         RUN mvn -B -q package -DskipTests\n\
         FROM {jre}\n\
         COPY --from=build /build/target/openehr-federation-ref-0.9.0-SNAPSHOT.jar /app/gateway.jar\n\
         EXPOSE {PORT}\n\
         ENTRYPOINT [\"java\", \"-jar\", \"/app/gateway.jar\"]\n",
        maven = MAVEN.reference(),
        jre = TEMURIN_JRE.reference(),
    );
    GenericBuildableImage::new(IMAGE, REFERENCE_COMMIT)
        .with_dockerfile_string(dockerfile)
        .with_data(pom, "./pom.xml")
        .with_file(Path::new(SOURCE), "./src")
        .build_image()
        .await
        .map_err(|source| ReferenceError::Container {
            stage: "image build",
            source,
        })
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
