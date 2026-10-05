// SPDX-FileCopyrightText: Vernum Projecten B.V.
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

use std::sync::Arc;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerRequest, GenericImage, ImageExt};
use uuid::Uuid;

use super::{
    CDR_PORT, DatabaseServer, EHRBASE, EHRBASE_POSTGRES, HarnessError, HarnessUser, Node,
    POSTGRES_PORT, Product, names, postgres_health_check, ready, role_password,
};

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

/// The administrator of a restricted EHRbase node, in its `ADMIN` role.
pub const RESTRICTED_ADMIN: HarnessUser = HarnessUser {
    user: "harness-admin",
    password: "harness-admin-example",
};

/// The user of a restricted EHRbase node, in its `USER` role.
pub const RESTRICTED_USER: HarnessUser = HarnessUser {
    user: "harness-user",
    password: "harness-user-example",
};

/// Starts EHRbase as `system_id` on a database server of its own, with
/// authentication off, and waits for its health endpoint.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ehrbase(system_id: &'static str) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server().await?);
    let server = request(&database, system_id)
        .with_env_var("SECURITY_AUTHTYPE", "NONE")
        .start()
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

/// Starts EHRbase as `system_id` on a database server of its own with Basic
/// authentication on, withholding the EHR `withheld` from its user role, and
/// waits for its health endpoint.
///
/// [`RESTRICTED_ADMIN`] holds the `ADMIN` role and [`RESTRICTED_USER`] the
/// `USER` role. EHRbase 2.36.0 has no access policy over an EHR's content;
/// the narrowest refusal its configuration expresses is a role rule over a
/// request path (`security.additionalAuthorizations`), so the node admits
/// the administrator alone to [`withheld_path`] of `withheld`. The node
/// profile's access check reads what that rule withholds (§13, N26).
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ehrbase_restricted(
    system_id: &'static str,
    withheld: Uuid,
) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server().await?);
    let server = request(&database, system_id)
        .with_env_var("SECURITY_AUTHTYPE", "BASIC")
        .with_env_var("SECURITY_AUTHADMINUSER", RESTRICTED_ADMIN.user)
        .with_env_var("SECURITY_AUTHADMINPASSWORD", RESTRICTED_ADMIN.password)
        .with_env_var("SECURITY_AUTHUSER", RESTRICTED_USER.user)
        .with_env_var("SECURITY_AUTHPASSWORD", RESTRICTED_USER.password)
        .with_env_var(
            "SECURITY_ADDITIONALAUTHORIZATIONS_0_PATHPATTERN",
            withheld_path(withheld),
        )
        .with_env_var("SECURITY_ADDITIONALAUTHORIZATIONS_0_ROLES_0", "ADMIN")
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: EHRBASE.repository,
            source,
        })?;
    ready(EHRBASE_PRODUCT, system_id, server, database).await
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
    let container = EHRBASE_POSTGRES
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
        .start()
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
