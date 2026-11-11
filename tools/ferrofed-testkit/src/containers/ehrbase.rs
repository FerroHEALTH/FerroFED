// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! EHRbase in the harness: a second CDR product the node profile runs
//! against beside FerroEHR, behind the same `FERROFED_E2E` gate.
//!
//! Each node is the pinned [`EHRBASE`] image on a database server of its own,
//! the pinned [`EHRBASE_POSTGRES`] image EHRbase documents beside the release,
//! which creates the `ehrbase` database, its owner role and the restricted
//! role the server connects as. The `system_id` the node stamps is its
//! `server.nodename`. The node's management health endpoint, made public, is
//! the readiness probe.
//!
//! EHRbase is a node here and never the oracle: what it answers is evidence
//! about EHRbase. No specification governs which products the harness runs:
//! our own design.

use std::net::{Ipv4Addr, TcpListener};
use std::sync::Arc;

use testcontainers::core::{Host, IntoContainerPort, WaitFor};
use testcontainers::{ContainerRequest, GenericImage, ImageExt};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::images::{EHRBASE, EHRBASE_POSTGRES};
use super::retry;
use super::{
    CDR_PORT, DatabaseServer, HarnessError, Node, POSTGRES_PORT, Product, names,
    postgres_health_check, ready, role_password,
};
use crate::issuer::{Claims, Issuer, JWKS_PATH};
use crate::mock::Server;
use crate::reference::CONTAINER_HOST;

/// The path of EHRbase's ITS-REST API root, the path `/v1/ehr` lives under:
/// the servlet context path `/ehrbase` and the API context path
/// `/rest/openehr`.
pub const API_PATH: &str = "/ehrbase/rest/openehr";

/// The management health endpoint, which answers `200` once EHRbase is up.
const READINESS_PATH: &str = "/ehrbase/management/health";

/// The database EHRbase's database image creates.
const DATABASE: &str = "ehrbase";

/// The role that owns the schemas and runs the migrations.
const OWNER_ROLE: &str = "ehrbase";

/// The role the server connects as for every request.
const RESTRICTED_ROLE: &str = "ehrbase_restricted";

/// EHRbase, as the harness reaches it.
const EHRBASE_PRODUCT: Product = Product {
    image: EHRBASE.repository,
    api_path: API_PATH,
    readiness_path: READINESS_PATH,
};

/// The issuer of a restricted node's tokens, a synthetic value.
const ISSUER: &str = "https://ehrbase.issuer.example.test";

/// The audience of a restricted node's tokens.
const AUDIENCE: &str = "https://ehrbase.cdr.example.test/openehr";

/// A restricted EHRbase node, and the tokens of its administrator and its
/// user.
#[derive(Debug)]
pub struct Restricted {
    /// The node.
    pub node: Node,
    /// The administrator's token, in the node's `ADMIN` role.
    pub admin: String,
    /// The user's token, in the node's `USER` role.
    pub user: String,
    /// The server the node reads the issuer's key set from, held while the
    /// node runs.
    _keys: Server,
}

/// Starts EHRbase as `system_id` on a database server of its own, with
/// authentication off, and waits for its health endpoint.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ehrbase(system_id: &'static str) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server().await?);
    let server =
        retry::start(|| request(&database, system_id).with_env_var("SECURITY_AUTHTYPE", "NONE"))
            .await
            .map_err(|source| HarnessError::Container {
                image: EHRBASE.repository,
                source,
            })?;
    ready(EHRBASE_PRODUCT, system_id, server, database).await
}

/// Returns the request path, inside the servlet context, of the EHR
/// `ehr_id` and every resource under it, as a Spring `PathPattern`.
#[must_use]
pub fn withheld_path(ehr_id: Uuid) -> String {
    format!("/rest/openehr/v1/ehr/{ehr_id}/**")
}

/// Starts EHRbase as `system_id` on a database server of its own with
/// OAuth 2.0 bearer authentication on, withholding the EHR `withheld` from
/// its user role, and waits for its health endpoint.
///
/// The node verifies every token against the key set of a harness
/// [`Issuer`], which it reads from a server on every interface of the
/// Docker host, as [`CONTAINER_HOST`]. EHRbase takes a token's roles from
/// its `scope` (EHRbase's `SecurityConfigOAuth2`), so the administrator's
/// token states `ADMIN` and the user's `USER`. EHRbase 2.36.0 has no access
/// policy over an EHR's content; the narrowest refusal its configuration
/// expresses is a role rule over a request path
/// (`security.additionalAuthorizations`), so the node admits the
/// administrator alone to [`withheld_path`] of `withheld`. The node
/// profile's access check reads what that rule withholds (§13, N26).
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container,
/// [`HarnessError::Issuer`] when the issuer's key or a token cannot be made,
/// [`HarnessError::Write`] when the key set's listener cannot be bound, and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ehrbase_restricted(
    system_id: &'static str,
    withheld: Uuid,
) -> Result<Restricted, HarnessError> {
    let issuer = Issuer::new(ISSUER).map_err(HarnessError::Issuer)?;
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(HarnessError::Write)?;
    let port = listener.local_addr().map_err(HarnessError::Write)?.port();
    let keys = Server::from(MockServer::builder().listener(listener).start().await);
    Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(issuer.jwks_json().map_err(HarnessError::Issuer)?),
        )
        .mount(&keys)
        .await;
    let jwks_uri = format!("http://{CONTAINER_HOST}:{port}{JWKS_PATH}");
    let database = Arc::new(database_server().await?);
    let server = retry::start(|| {
        request(&database, system_id)
            .with_host(CONTAINER_HOST, Host::HostGateway)
            .with_env_var("SECURITY_AUTHTYPE", "OAUTH")
            .with_env_var(
                "SPRING_SECURITY_OAUTH2_RESOURCESERVER_JWT_JWKSETURI",
                &jwks_uri,
            )
            .with_env_var(
                "SPRING_SECURITY_OAUTH2_RESOURCESERVER_JWT_JWSALGORITHMS",
                "ES256",
            )
            .with_env_var("SECURITY_OAUTH2ADMINROLE", "ADMIN")
            .with_env_var("SECURITY_OAUTH2USERROLE", "USER")
            .with_env_var(
                "SECURITY_ADDITIONALAUTHORIZATIONS_0_PATHPATTERN",
                withheld_path(withheld),
            )
            .with_env_var("SECURITY_ADDITIONALAUTHORIZATIONS_0_ROLES_0", "ADMIN")
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: EHRBASE.repository,
        source,
    })?;
    let node = ready(EHRBASE_PRODUCT, system_id, server, database).await?;
    Ok(Restricted {
        node,
        admin: token(&issuer, "harness-admin", "ADMIN")?,
        user: token(&issuer, "harness-user", "USER")?,
        _keys: keys,
    })
}

/// A token of `issuer` for `subject` in `role`, typed `JWT`, the type a
/// Spring resource server admits by default.
fn token(issuer: &Issuer, subject: &str, role: &str) -> Result<String, HarnessError> {
    let mut claims = Claims::new(ISSUER, AUDIENCE);
    subject.clone_into(&mut claims.sub);
    claims.scope = Some(role.to_owned());
    claims.extensions = None;
    let mut header = issuer.header();
    header.typ = Some(String::from("JWT"));
    issuer
        .mint_with(&header, &claims)
        .map_err(HarnessError::Issuer)
}

/// The EHRbase container as `system_id` on `database`, before its access
/// posture is set, with its health endpoint public so the readiness probe
/// needs no credential.
fn request(database: &DatabaseServer, system_id: &'static str) -> ContainerRequest<GenericImage> {
    EHRBASE
        .image()
        .with_exposed_port(CDR_PORT.tcp())
        .with_network(database.network.clone())
        .with_env_var(
            "DB_URL",
            format!(
                "jdbc:postgresql://{}:{POSTGRES_PORT}/{DATABASE}",
                database.host
            ),
        )
        .with_env_var("DB_USER_ADMIN", OWNER_ROLE)
        .with_env_var("DB_PASS_ADMIN", role_password(OWNER_ROLE))
        .with_env_var("DB_USER", RESTRICTED_ROLE)
        .with_env_var("DB_PASS", role_password(RESTRICTED_ROLE))
        .with_env_var("SERVER_NODENAME", system_id)
        .with_env_var("MANAGEMENT_ENDPOINT_HEALTH_ACCESS", "read_only")
        .with_env_var("MANAGEMENT_ENDPOINTS_WEB_ACCESS", "PUBLIC")
}

/// Starts EHRbase's database image, whose init script creates the database,
/// its owner role and the restricted role, each with the development
/// password [`role_password`] gives it.
async fn database_server() -> Result<DatabaseServer, HarnessError> {
    let (network, host) = names("ehrbase");
    let container = retry::start(|| {
        EHRBASE_POSTGRES
            .image()
            .with_exposed_port(POSTGRES_PORT.tcp())
            .with_wait_for(WaitFor::healthcheck())
            .with_health_check(postgres_health_check(OWNER_ROLE, DATABASE))
            .with_env_var("POSTGRES_PASSWORD", "postgres")
            .with_env_var("EHRBASE_USER_ADMIN", OWNER_ROLE)
            .with_env_var("EHRBASE_PASSWORD_ADMIN", role_password(OWNER_ROLE))
            .with_env_var("EHRBASE_USER", RESTRICTED_ROLE)
            .with_env_var("EHRBASE_PASSWORD", role_password(RESTRICTED_ROLE))
            .with_network(network.clone())
            .with_container_name(host.clone())
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: EHRBASE_POSTGRES.repository,
        source,
    })?;
    Ok(DatabaseServer {
        container,
        network,
        host,
    })
}
