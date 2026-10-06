// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The container harness behind the `FERROFED_E2E` gate.
//!
//! A container-backed test asks [`e2e_enabled`] first and returns without
//! touching Docker when the gate is unset, so the ordinary suite stays
//! offline. The harness starts the two member CDRs the end-to-end lane runs
//! against (§16): two FerroEHR instances, node A and node B, each stamping its
//! own `system_id`, on one PostgreSQL server that holds a database per node,
//! and [`two_nodes`] puts a [`CapturingProxy`] in front of each. Every image
//! is pinned by tag and digest in a [`PinnedImage`](images::PinnedImage)
//! constant of [`images`], which `docs/VERSIONS.md` repeats and
//! `scripts/checks/versions.sh` compares.
//!
//! [`database`] starts the PostgreSQL server the nodes share, and the same
//! server alone for the gateway's own PostgreSQL store. [`restricted`] starts
//! a FerroEHR node with its access controls on.
//!
//! The node profile also runs against a second CDR product, EHRbase, which
//! [`ehrbase`] starts on its own pinned database image.
//!
//! The identity binding runs against a deployable PIX Manager, SanteMPI,
//! which [`santempi`](mod@santempi) starts and provisions on its own pinned
//! database image.
//!
//! The production guide's issuer recipe runs against Keycloak, which
//! [`keycloak`](mod@keycloak) starts.
//!
//! No specification governs the harness, and none governs which CDR products
//! it runs: our own design.

use crate::proxy::{CapturingProxy, ProxyError};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use testcontainers::core::{Healthcheck, IntoContainerPort};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};

use database::database_server;
use images::FERROEHR;

pub mod database;
pub mod ehrbase;
pub mod images;
pub mod keycloak;
pub mod restricted;
pub mod santempi;

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

/// What the harness needs to know of a CDR product to reach a started one.
#[derive(Debug, Clone, Copy)]
struct Product {
    /// The repository of the product's image, which names it in an error.
    image: &'static str,
    /// The path of its ITS-REST API root, the path `/v1/ehr` lives under.
    api_path: &'static str,
    /// The path that answers `200` once it serves requests.
    readiness_path: &'static str,
}

/// FerroEHR, as the harness reaches it.
const FERROEHR_PRODUCT: Product = Product {
    image: FERROEHR.repository,
    api_path: API_PATH,
    readiness_path: READINESS_PATH,
};

/// The `system_id` node A stamps into every EHR and version it creates.
pub const NODE_A_SYSTEM_ID: &str = "cdr-a.example.org";

/// The `system_id` node B stamps into every EHR and version it creates.
pub const NODE_B_SYSTEM_ID: &str = "cdr-b.example.org";

/// The database node A connects to, which is also its login role.
const NODE_A_DATABASE: &str = "ferroehr_a";

/// The database node B connects to, named as [`NODE_A_DATABASE`] is.
const NODE_B_DATABASE: &str = "ferroehr_b";

/// Returns the development password of the login role `name`, the one
/// [`database::NODE_DATABASES_SCRIPT`] gives every role it creates: the name
/// followed by `_example`.
#[must_use]
pub fn role_password(name: &str) -> String {
    format!("{name}_example")
}

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

/// A started PostgreSQL server holding one database per node, on a network
/// of its own that the nodes join.
#[derive(Debug)]
struct DatabaseServer {
    /// The server's container.
    container: ContainerAsync<GenericImage>,
    /// The network the server and its nodes share.
    network: String,
    /// The container name the nodes reach the server by.
    host: String,
}

/// A started CDR with its database, torn down when it is dropped.
///
/// The CDR container is a field and the database server is shared with the
/// other nodes started beside it, so the server stops with the last of them.
#[derive(Debug)]
pub struct Node {
    /// The product the CDR runs.
    product: Product,
    /// The `system_id` the CDR was configured with.
    system_id: &'static str,
    /// The CDR itself.
    server: ContainerAsync<GenericImage>,
    /// The database server it was started against.
    database: Arc<DatabaseServer>,
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
        format!("{}{}", self.origin, self.product.api_path)
    }

    /// Returns the path of the CDR's ITS-REST API root, the path `/v1/ehr`
    /// lives under: [`API_PATH`] for FerroEHR.
    #[must_use]
    pub fn api_path(&self) -> &'static str {
        self.product.api_path
    }

    /// Returns the CDR container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns the database server container the CDR was started against,
    /// which every node started beside it shares.
    #[must_use]
    pub fn database(&self) -> &ContainerAsync<GenericImage> {
        &self.database.container
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
                image: self.product.image,
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
        format!("{}{}", self.proxy.origin(), self.node.api_path())
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

/// Starts one database server holding a database for each node, then both
/// nodes concurrently, and puts a proxy in front of each.
///
/// # Errors
///
/// Returns the first [`HarnessError`] the database server or either node
/// reports.
pub async fn two_nodes() -> Result<TwoNodes, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[NODE_B_DATABASE]).await?);
    let (a, b) = tokio::try_join!(
        ferroehr_on(&database, NODE_A_DATABASE, NODE_A_SYSTEM_ID),
        ferroehr_on(&database, NODE_B_DATABASE, NODE_B_SYSTEM_ID)
    )?;
    Ok(TwoNodes {
        a: ProxiedNode::new(a).await?,
        b: ProxiedNode::new(b).await?,
    })
}

/// Starts FerroEHR as `system_id` on a database server of its own and waits
/// for its readiness endpoint.
///
/// Authentication and role-based authorisation are switched off, the posture
/// the image documents for development, so the API root needs no credentials.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ferroehr(system_id: &'static str) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    ferroehr_on(&database, NODE_A_DATABASE, system_id).await
}

/// Starts FerroEHR as `system_id` on the database `name` of `database` and
/// waits for its readiness endpoint.
async fn ferroehr_on(
    database: &Arc<DatabaseServer>,
    name: &str,
    system_id: &'static str,
) -> Result<Node, HarnessError> {
    let server = ferroehr_request(database, name, system_id)
        .with_env_var("FERROEHR__AUTH__ENABLED", "false")
        .with_env_var("FERROEHR__AUTHZ__RBAC__ENABLED", "false")
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: FERROEHR.repository,
            source,
        })?;
    ready(FERROEHR_PRODUCT, system_id, server, Arc::clone(database)).await
}

/// The FerroEHR container as `system_id` on the database `name` of
/// `database`, before its access posture is set.
fn ferroehr_request(
    database: &DatabaseServer,
    name: &str,
    system_id: &'static str,
) -> ContainerRequest<GenericImage> {
    FERROEHR
        .image()
        .with_exposed_port(CDR_PORT.tcp())
        .with_network(database.network.clone())
        .with_env_var(
            "FERROEHR__DB__URL",
            format!(
                "postgres://{name}:{}@{}:{POSTGRES_PORT}/{name}",
                role_password(name),
                database.host
            ),
        )
        .with_env_var("FERROEHR__SERVER__SYSTEM_ID", system_id)
}

/// A synthetic user of a node [`restricted::ferroehr_restricted`] starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessUser {
    /// The Basic user name.
    pub user: &'static str,
    /// The Basic password, a synthetic development value.
    pub password: &'static str,
}

/// Returns a network name and a database server container name for a node
/// of `product`, unique to this process and call.
fn names(product: &str) -> (String, String) {
    let sequence = NETWORK_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let process = std::process::id();
    (
        format!("ferrofed-e2e-{product}-{process}-{sequence}"),
        format!("ferrofed-e2e-{product}-db-{process}-{sequence}"),
    )
}

/// Resolves the host origin of `server`, a CDR running `product`, and waits
/// until it is ready.
async fn ready(
    product: Product,
    system_id: &'static str,
    server: ContainerAsync<GenericImage>,
    database: Arc<DatabaseServer>,
) -> Result<Node, HarnessError> {
    let image = product.image;
    let host = server
        .get_host()
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let port = server
        .get_host_port_ipv4(CDR_PORT.tcp())
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let origin = format!("http://{host}:{port}");
    await_readiness(&format!("{origin}{}", product.readiness_path)).await?;
    Ok(Node {
        product,
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
pub(crate) async fn await_readiness(url: &str) -> Result<(), HarnessError> {
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
