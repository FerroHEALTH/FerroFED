// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroEHR PostgreSQL server: a database per node, and a database per
//! use of the gateway's own PostgreSQL store.
//!
//! A database per node, never a schema per node: FerroEHR creates fixed
//! schema names in the database it connects to, so two nodes in one database
//! would share their tables. The database server is the same init script the
//! compose quickstart mounts, [`NODE_DATABASES_SCRIPT`]. [`postgres`] starts
//! the same server alone, its port published to the host.

use std::path::Path;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, CopyTargetOptions, GenericImage, ImageExt};

use super::images::FERROEHR_POSTGRES;
use super::{
    DatabaseServer, HarnessError, POSTGRES_PORT, names, postgres_health_check, role_password,
};

/// The init script that adds a database per node to the FerroEHR PostgreSQL
/// image, shared with the compose quickstart.
pub const NODE_DATABASES_SCRIPT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docker/postgres/20-ferrofed-node-databases.sh"
);

/// Where the image's entrypoint finds [`NODE_DATABASES_SCRIPT`]: after the
/// image's own `10-ferroehr-init.sh`, because the entrypoint runs the init
/// scripts in sorted order.
const NODE_DATABASES_TARGET: &str = "/docker-entrypoint-initdb.d/20-ferrofed-node-databases.sh";

/// A started PostgreSQL server the host reaches, holding one database per
/// name it was started with, torn down when it is dropped.
#[derive(Debug)]
pub struct Postgres {
    /// The server.
    server: DatabaseServer,
    /// The host the server's port is published on.
    host: String,
    /// The published port.
    port: u16,
}

impl Postgres {
    /// Returns the connection URL of the database `name`, as its own login
    /// role, with TLS off: the harness server has no certificate.
    #[must_use]
    pub fn url(&self, name: &str) -> String {
        format!(
            "postgres://{name}:{}@{}:{}/{name}?sslmode=disable",
            role_password(name),
            self.host,
            self.port
        )
    }

    /// Returns the server's container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server.container
    }

    /// Returns the host port the server is published on, which a container
    /// reaches through the Docker host gateway.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

/// Starts one PostgreSQL server the host reaches, holding a database per
/// name.
///
/// It holds the database `first` and one more for each of `others`, each
/// owned by a login role of the same name whose development password is
/// [`role_password`]. The server is the FerroEHR database image the nodes run, built on
/// PostgreSQL 18.6, so a test of FerroFED's own PostgreSQL use runs on the
/// release `docs/VERSIONS.md` pins, one database per use.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses the container or
/// its port.
pub async fn postgres(first: &str, others: &[&str]) -> Result<Postgres, HarnessError> {
    let server = database_server(first, others).await?;
    let image = FERROEHR_POSTGRES.repository;
    let host = server
        .container
        .get_host()
        .await
        .map_err(|source| HarnessError::Container { image, source })?
        .to_string();
    let port = server
        .container
        .get_host_port_ipv4(POSTGRES_PORT.tcp())
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    Ok(Postgres { server, host, port })
}

/// Starts the FerroEHR PostgreSQL image with the database `first` and one
/// more for each of `others`, each owned by a login role of the same name
/// whose development password is [`role_password`].
///
/// The image's own init script creates `first`, and
/// [`NODE_DATABASES_SCRIPT`] runs it once more for each of `others`.
pub(super) async fn database_server(
    first: &str,
    others: &[&str],
) -> Result<DatabaseServer, HarnessError> {
    let (network, host) = names("ferroehr");
    let container = FERROEHR_POSTGRES
        .image()
        .with_exposed_port(POSTGRES_PORT.tcp())
        .with_wait_for(WaitFor::healthcheck())
        .with_health_check(postgres_health_check(first, first))
        .with_copy_to(
            CopyTargetOptions::new(NODE_DATABASES_TARGET).with_mode(0o755),
            Path::new(NODE_DATABASES_SCRIPT),
        )
        .with_env_var("POSTGRES_PASSWORD", "postgres")
        .with_env_var("PG_INIT_USER", first)
        .with_env_var("PG_INIT_PASSWORD", role_password(first))
        .with_env_var("PG_INIT_DB", first)
        .with_env_var("FERROFED_NODE_DATABASES", others.join(" "))
        .with_network(network.clone())
        .with_container_name(host.clone())
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: FERROEHR_POSTGRES.repository,
            source,
        })?;
    Ok(DatabaseServer {
        container,
        network,
        host,
    })
}
