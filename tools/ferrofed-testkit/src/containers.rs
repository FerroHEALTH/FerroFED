// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The container harness behind the `FERROFED_E2E` gate.
//!
//! A container-backed test asks [`e2e_enabled`] first and returns without
//! touching Docker when the gate is unset, so the ordinary suite stays
//! offline. The harness starts the two member CDRs the end-to-end lane runs
//! against (§16): two FerroEHR instances, node A
//! and node B, each on its own database and each stamping its own
//! `system_id`, and [`two_nodes`] puts a [`CapturingProxy`] in front of each.
//! Every image is pinned by tag and digest in a [`PinnedImage`] constant,
//! which `docs/VERSIONS.md` repeats and `scripts/checks/versions.sh` compares.
//!
//! No specification governs the harness; it is FerroFED's own design.

use crate::proxy::{CapturingProxy, ProxyError};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use testcontainers::core::{Healthcheck, IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The environment variable that admits the container-backed tests.
pub const E2E_GATE: &str = "FERROFED_E2E";

/// The value of [`E2E_GATE`] that admits them.
pub const E2E_GATE_VALUE: &str = "1";

/// The PostgreSQL port inside a database container.
const POSTGRES_PORT: u16 = 5432;

/// The HTTP port a CDR listens on inside its container.
const CDR_PORT: u16 = 8080;

/// The path of FerroEHR's ITS-REST API root, the path `/v1/ehr` lives under.
pub const API_PATH: &str = "/ferroehr/rest/openehr";

/// The path that answers `200` once FerroEHR serves requests.
const READINESS_PATH: &str = "/health/readiness";

/// The `system_id` node A stamps into every EHR and version it creates.
pub const NODE_A_SYSTEM_ID: &str = "cdr-a.example.org";

/// The `system_id` node B stamps into every EHR and version it creates.
pub const NODE_B_SYSTEM_ID: &str = "cdr-b.example.org";

/// How long the readiness poll of a CDR waits before it gives up. A cold
/// runner pulls both images and migrates the database first.
const READINESS_BUDGET: Duration = Duration::from_secs(240);

/// How long the readiness poll sleeps between two probes.
const READINESS_INTERVAL: Duration = Duration::from_millis(500);

/// How often a database container's own health check runs, and its per-run
/// budget.
const HEALTH_INTERVAL: Duration = Duration::from_secs(1);

/// How many consecutive health-check failures make a container unhealthy.
const HEALTH_RETRIES: u32 = 60;

/// Distinguishes the networks of two nodes in one process.
static NETWORK_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Returns whether the container-backed tests may run.
///
/// They run when `FERROFED_E2E` is exactly `1`. A test that returns early on
/// a `false` here is reported by `cargo nextest` as passed rather than
/// skipped, which is the deliberate trade: the gate keeps the ordinary suite
/// offline, and the `e2e (containers)` job of `.github/workflows/ci.yml` is
/// the run where these tests have to do their work.
#[must_use]
pub fn e2e_enabled() -> bool {
    std::env::var(E2E_GATE).is_ok_and(|value| value == E2E_GATE_VALUE)
}

/// One container image, pinned by tag and by digest.
///
/// The digest is what Docker resolves; the tag travels beside it so a reader
/// sees which release the digest is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedImage {
    /// The repository, registry included for anything but Docker Hub.
    pub repository: &'static str,
    /// The tag the digest was published under.
    pub tag: &'static str,
    /// The `sha256:` digest of the image index.
    pub digest: &'static str,
}

impl PinnedImage {
    /// Returns the reference Docker resolves this image by.
    ///
    /// # Examples
    ///
    /// ```
    /// let reference = ferrofed_testkit::containers::FERROEHR.reference();
    /// assert!(reference.starts_with("ghcr.io/rubentalstra/ferroehr:"));
    /// assert!(reference.contains("@sha256:"));
    /// ```
    #[must_use]
    pub fn reference(&self) -> String {
        format!("{}:{}@{}", self.repository, self.tag, self.digest)
    }

    /// Returns the image with the digest in the tag position, which is how
    /// `testcontainers` spells a reference (`name:tag`).
    fn image(&self) -> GenericImage {
        GenericImage::new(
            self.repository.to_owned(),
            format!("{}@{}", self.tag, self.digest),
        )
    }
}

/// FerroEHR, an openEHR CDR speaking ITS-REST 1.1.0, which both nodes run.
pub const FERROEHR: PinnedImage = PinnedImage {
    repository: "ghcr.io/rubentalstra/ferroehr",
    tag: "4.3.1",
    digest: "sha256:b64f752aefe010629191f8c1d990d286c6ed28a62e457300a237a596f1116ac6",
};

/// The database image FerroEHR documents, which carries the role, the
/// database and the extensions its migrations expect.
pub const FERROEHR_POSTGRES: PinnedImage = PinnedImage {
    repository: "ghcr.io/rubentalstra/ferroehr-postgres",
    tag: "4.3.1",
    digest: "sha256:17d5772dba1c6689fccb1095a8774f3ed636f4968256a37fc505207ca75a99b9",
};

/// A container could not be started, or did not become usable.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HarnessError {
    /// Docker refused to start, inspect or stop a container.
    #[error("the {image} container could not be started or stopped")]
    Container {
        /// The image that was being handled.
        image: &'static str,
        /// What Docker reported.
        #[source]
        source: testcontainers::TestcontainersError,
    },
    /// The readiness probe could not be built.
    #[error("the readiness probe of {url} could not be built")]
    Probe {
        /// The endpoint that was probed.
        url: String,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// The service did not answer its readiness probe inside the budget.
    #[error("{url} did not become ready within {}s", budget.as_secs())]
    NotReady {
        /// The endpoint that was probed.
        url: String,
        /// How long the probe waited.
        budget: Duration,
    },
    /// The proxy in front of a node could not be started.
    #[error("the proxy in front of a node could not be started")]
    Proxy(#[source] ProxyError),
}

/// A started CDR with its own database, torn down when it is dropped.
///
/// The two containers are fields, so dropping this value stops both.
#[derive(Debug)]
pub struct Node {
    /// The `system_id` the CDR was configured with.
    system_id: &'static str,
    /// The CDR itself.
    server: ContainerAsync<GenericImage>,
    /// The database it was started against.
    database: ContainerAsync<GenericImage>,
    /// The origin the CDR is reachable at from the host, with no path.
    origin: String,
}

impl Node {
    /// Returns the `system_id` the CDR stamps into every EHR and version it
    /// creates.
    #[must_use]
    pub fn system_id(&self) -> &'static str {
        self.system_id
    }

    /// Returns the origin the CDR is reachable at, with no path.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the ITS-REST API root reached directly, bypassing any proxy.
    #[must_use]
    pub fn api_root(&self) -> String {
        format!("{}{API_PATH}", self.origin)
    }

    /// Returns the CDR container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns the database container the CDR was started against.
    #[must_use]
    pub fn database(&self) -> &ContainerAsync<GenericImage> {
        &self.database
    }

    /// Stops the CDR container, which makes the node `offline` the way an
    /// outage does rather than the way a refusing proxy does.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Container`] when Docker cannot stop it.
    pub async fn stop(&self) -> Result<(), HarnessError> {
        self.server
            .stop()
            .await
            .map_err(|source| HarnessError::Container {
                image: FERROEHR.repository,
                source,
            })
    }
}

/// A node with its capturing and fault proxy in front.
#[derive(Debug)]
pub struct ProxiedNode {
    /// The node.
    pub node: Node,
    /// The proxy every test request goes through.
    pub proxy: CapturingProxy,
}

impl ProxiedNode {
    /// Puts a fresh proxy in front of `node`.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Proxy`] when the proxy cannot be started.
    pub async fn new(node: Node) -> Result<Self, HarnessError> {
        let proxy = CapturingProxy::start(node.origin())
            .await
            .map_err(HarnessError::Proxy)?;
        Ok(Self { node, proxy })
    }

    /// Returns the ITS-REST API root reached through the proxy, the URL a
    /// gateway under test is configured with.
    #[must_use]
    pub fn api_root(&self) -> String {
        format!("{}{API_PATH}", self.proxy.origin())
    }
}

/// The two-node topology: two FerroEHR instances, node A on
/// [`NODE_A_SYSTEM_ID`] and node B on [`NODE_B_SYSTEM_ID`], each behind its
/// own proxy.
#[derive(Debug)]
pub struct TwoNodes {
    /// Node A.
    pub a: ProxiedNode,
    /// Node B.
    pub b: ProxiedNode,
}

/// Starts both nodes concurrently and puts a proxy in front of each.
///
/// # Errors
///
/// Returns the first [`HarnessError`] either node reports.
pub async fn two_nodes() -> Result<TwoNodes, HarnessError> {
    let (a, b) = tokio::try_join!(ferroehr(NODE_A_SYSTEM_ID), ferroehr(NODE_B_SYSTEM_ID))?;
    Ok(TwoNodes {
        a: ProxiedNode::new(a).await?,
        b: ProxiedNode::new(b).await?,
    })
}

/// Starts FerroEHR as `system_id` on its own database and waits for its
/// readiness endpoint.
///
/// Authentication and role-based authorisation are switched off, the posture
/// the image documents for development, so the API root needs no credentials.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ferroehr(system_id: &'static str) -> Result<Node, HarnessError> {
    let (network, database_name) = names();
    let database = FERROEHR_POSTGRES
        .image()
        .with_wait_for(WaitFor::healthcheck())
        .with_health_check(postgres_health_check("ferroehr", "ferroehr"))
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("PG_INIT_USER", "ferroehr")
        .with_env_var("PG_INIT_PASSWORD", "ferroehr")
        .with_env_var("PG_INIT_DB", "ferroehr")
        .with_network(network.clone())
        .with_container_name(database_name.clone())
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: FERROEHR_POSTGRES.repository,
            source,
        })?;
    let server = FERROEHR
        .image()
        .with_exposed_port(CDR_PORT.tcp())
        .with_network(network)
        .with_env_var(
            "FERROEHR__DB__URL",
            format!("postgres://ferroehr:ferroehr@{database_name}:{POSTGRES_PORT}/ferroehr"),
        )
        .with_env_var("FERROEHR__SERVER__SYSTEM_ID", system_id)
        .with_env_var("FERROEHR__AUTH__ENABLED", "false")
        .with_env_var("FERROEHR__AUTHZ__RBAC__ENABLED", "false")
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: FERROEHR.repository,
            source,
        })?;
    ready(system_id, server, database).await
}

/// Returns a network name and a database container name unique to this
/// process and call.
fn names() -> (String, String) {
    let sequence = NETWORK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let process = std::process::id();
    (
        format!("ferrofed-e2e-ferroehr-{process}-{sequence}"),
        format!("ferrofed-e2e-ferroehr-db-{process}-{sequence}"),
    )
}

/// Resolves the host origin of `server` and waits until it is ready.
async fn ready(
    system_id: &'static str,
    server: ContainerAsync<GenericImage>,
    database: ContainerAsync<GenericImage>,
) -> Result<Node, HarnessError> {
    let image = FERROEHR.repository;
    let host = server
        .get_host()
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let port = server
        .get_host_port_ipv4(CDR_PORT.tcp())
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let origin = format!("http://{host}:{port}");
    await_readiness(&format!("{origin}{READINESS_PATH}")).await?;
    Ok(Node {
        system_id,
        server,
        database,
        origin,
    })
}

/// Returns the health check that answers when PostgreSQL accepts TCP
/// connections.
///
/// `pg_isready` is PostgreSQL's own readiness utility, and the host is named
/// explicitly because the entrypoint's initialisation server listens on the
/// Unix socket alone; a socket probe would report ready before the port the
/// CDR connects to serves anything.
fn postgres_health_check(role: &str, database: &str) -> Healthcheck {
    Healthcheck::cmd(["pg_isready", "-h", "127.0.0.1", "-U", role, "-d", database])
        .with_interval(HEALTH_INTERVAL)
        .with_timeout(HEALTH_INTERVAL)
        .with_retries(HEALTH_RETRIES)
}

/// Polls `url` until it answers `200`, or the budget runs out.
async fn await_readiness(url: &str) -> Result<(), HarnessError> {
    let client = reqwest::Client::builder()
        .timeout(READINESS_INTERVAL.saturating_mul(8))
        .build()
        .map_err(|source| HarnessError::Probe {
            url: url.to_owned(),
            source,
        })?;
    let started = tokio::time::Instant::now();
    while started.elapsed() < READINESS_BUDGET {
        if let Ok(answer) = client.get(url).send().await
            && answer.status().is_success()
        {
            return Ok(());
        }
        tokio::time::sleep(READINESS_INTERVAL).await;
    }
    Err(HarnessError::NotReady {
        url: url.to_owned(),
        budget: READINESS_BUDGET,
    })
}
