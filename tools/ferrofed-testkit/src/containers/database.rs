// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FerroEHR PostgreSQL server: a database per node, and a database per
//! use of the gateway's own PostgreSQL store.
//!
//! A database per node, never a schema per node: FerroEHR creates fixed
//! schema names in the database it connects to, so two nodes in one database
//! would share their tables. The database server is the same init script the
//! compose quickstart mounts, [`NODE_DATABASES_SCRIPT`]. [`postgres`] starts
//! the same server alone, its port published to the host, and
//! [`postgres_tls`] starts it serving TLS under a CA generated for the run,
//! the shape FerroFED's own store requires.

use rcgen::ExtendedKeyUsagePurpose;
use std::net::Ipv4Addr;
use std::path::Path;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{ContainerAsync, ContainerRequest, CopyTargetOptions, GenericImage, ImageExt};

use super::images::FERROEHR_POSTGRES;
use super::retry;
use super::{
    DatabaseServer, HarnessError, POSTGRES_PORT, names, postgres_health_check, role_password,
};
use crate::pki::Authority;

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

/// The directory the server certificate and its key are copied to, readable
/// by the database's own user.
const TLS_SOURCE: &str = "/ferrofed-store-tls";

/// The init script that turns TLS on: it copies the certificate and the key
/// into the data directory, the key readable by the server's user alone as
/// PostgreSQL requires, and names them in `postgresql.conf`
/// (<https://www.postgresql.org/docs/18/ssl-tcp.html#SSL-SETUP>). The
/// entrypoint sources a script that is not executable, so it sets no shell
/// option.
const TLS_SCRIPT: &str = "install -m 0600 /ferrofed-store-tls/server.key \"$PGDATA/server.key\"
install -m 0644 /ferrofed-store-tls/server.crt \"$PGDATA/server.crt\"
printf \"ssl = on\\nssl_cert_file = 'server.crt'\\nssl_key_file = 'server.key'\\n\" >> \"$PGDATA/postgresql.conf\"
";

/// Where the image's entrypoint finds [`TLS_SCRIPT`]: after every other init
/// script, so the server it restarts serves TLS.
const TLS_TARGET: &str = "/docker-entrypoint-initdb.d/30-ferrofed-store-tls.sh";

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

/// A started PostgreSQL server the host reaches, serving TLS under a CA
/// generated for the run, holding one database per name it was started
/// with, torn down when it is dropped.
#[derive(Debug)]
pub struct PostgresTls {
    /// The server.
    server: Postgres,
    /// The CA certificate, PEM, the server's certificate is verified by.
    ca: String,
}

impl PostgresTls {
    /// Returns the connection URL of the database `name`, as its own login
    /// role, requiring TLS.
    #[must_use]
    pub fn url(&self, name: &str) -> String {
        format!(
            "postgres://{name}:{}@{}:{}/{name}?sslmode=require",
            role_password(name),
            self.server.host,
            self.server.port
        )
    }

    /// Returns the connection URL of the database `name` with TLS off, which
    /// a test that reaches past the gateway uses.
    #[must_use]
    pub fn plain_url(&self, name: &str) -> String {
        self.server.url(name)
    }

    /// Returns the CA certificate, PEM, a client trusts the server by.
    #[must_use]
    pub fn ca(&self) -> &str {
        &self.ca
    }

    /// Returns the server's container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        self.server.container()
    }
}

/// Starts one PostgreSQL server the host reaches, as [`postgres`] does, and
/// serving TLS with a certificate for `localhost` and `127.0.0.1` that a CA
/// generated for the run signs.
///
/// The server still accepts a connection without TLS, so a test can reach
/// past the gateway with [`PostgresTls::plain_url`]; a client that sets
/// `sslmode=require` and trusts [`PostgresTls::ca`] reaches it over TLS.
///
/// # Errors
///
/// Returns [`HarnessError::Certificate`] when the certificates cannot be
/// generated, and [`HarnessError::Container`] when Docker refuses the
/// container or its port.
pub async fn postgres_tls(first: &str, others: &[&str]) -> Result<PostgresTls, HarnessError> {
    let authority =
        Authority::generate("FerroFED harness store CA").map_err(HarnessError::Certificate)?;
    let (certificate, key) = authority
        .issue(
            "harness store",
            vec![String::from("localhost"), Ipv4Addr::LOCALHOST.to_string()],
            ExtendedKeyUsagePurpose::ServerAuth,
        )
        .map_err(HarnessError::Certificate)?;
    let files = [
        ("server.crt", certificate.pem()),
        ("server.key", key.serialize_pem()),
    ];
    let server = published(
        database_server_with(first, others, |request| {
            let request = files.iter().fold(request, |request, (name, pem)| {
                request.with_copy_to(
                    CopyTargetOptions::new(format!("{TLS_SOURCE}/{name}")).with_mode(0o644),
                    pem.as_bytes().to_vec(),
                )
            });
            request.with_copy_to(TLS_TARGET, TLS_SCRIPT.as_bytes().to_vec())
        })
        .await?,
    )
    .await?;
    Ok(PostgresTls {
        server,
        ca: authority.certificate.pem(),
    })
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
    published(database_server(first, others).await?).await
}

/// The host and the published port of `server`.
async fn published(server: DatabaseServer) -> Result<Postgres, HarnessError> {
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
    database_server_with(first, others, |request| request).await
}

/// Starts the server [`database_server`] starts, its container request
/// first passed through `extra`.
async fn database_server_with(
    first: &str,
    others: &[&str],
    extra: impl Fn(ContainerRequest<GenericImage>) -> ContainerRequest<GenericImage>,
) -> Result<DatabaseServer, HarnessError> {
    let (network, host) = names("ferroehr");
    let container = retry::start(|| {
        let request = FERROEHR_POSTGRES
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
            .with_container_name(host.clone());
        extra(request)
    })
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
