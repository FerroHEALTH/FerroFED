// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FerroEHR node with its access controls on, whose refusals the node
//! profile's access check reads (§13, N26).

use std::sync::Arc;
use testcontainers::ImageExt;
use testcontainers::runners::AsyncRunner;

use super::database::database_server;
use super::images::FERROEHR;
use super::{FERROEHR_PRODUCT, HarnessError, HarnessUser, NODE_A_DATABASE, Node, ferroehr_request, ready};

/// The administrator of a restricted node, whose role reaches every EHR.
pub const RESTRICTED_ADMIN: HarnessUser = HarnessUser {
    user: "harness-admin",
    password: "harness-admin-example",
};

/// The clinician of a restricted node, whose role reaches no EHR without
/// access settings.
pub const RESTRICTED_CLINICIAN: HarnessUser = HarnessUser {
    user: "harness-clinician",
    password: "harness-clinician-example",
};

/// Where a restricted node reads [`RESTRICTED_CONFIG`] from.
const RESTRICTED_CONFIG_TARGET: &str = "/tmp/ferrofed-harness-restricted.toml";

/// The configuration file of a restricted node: its two Basic users, each
/// password stored as the Argon2id hash FerroEHR requires, with cost
/// parameters above the floor it checks at boot. The users are an array of
/// tables, which only a file can carry.
const RESTRICTED_CONFIG: &str = r#"[[auth.basic.users]]
username = "harness-admin"
password_hash = "$argon2id$v=19$m=32768,t=2,p=1$ZmVycm9mZWQtaGFybmVzcy1h$t/ra6Wz0qdF31frrVsd1pXhVK2mQdTQLx53fDeIgDS8"
roles = ["ADMIN"]

[[auth.basic.users]]
username = "harness-clinician"
password_hash = "$argon2id$v=19$m=32768,t=2,p=1$ZmVycm9mZWQtaGFybmVzcy1j$eBo26o4ip9ZdPKtCbzkJIjvpp+SfoLqTK0a07Qw6jcU"
roles = ["USER"]
"#;

/// Starts FerroEHR as `system_id` on a database server of its own with its
/// access controls on, and waits for its readiness endpoint.
///
/// Basic authentication admits [`RESTRICTED_ADMIN`] and
/// [`RESTRICTED_CLINICIAN`], the role gate is on, and
/// `authz.rbac.ehr_access_default` is `restricted`: an EHR with no access
/// settings, which every new EHR is, reaches the administrator alone, so the
/// node refuses the clinician by a decision of its own. The node profile's
/// access check reads that decision (§13, N26).
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ferroehr_restricted(system_id: &'static str) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    let server = ferroehr_request(&database, NODE_A_DATABASE, system_id)
        .with_copy_to(
            RESTRICTED_CONFIG_TARGET,
            RESTRICTED_CONFIG.as_bytes().to_vec(),
        )
        .with_env_var("FERROEHR_CONFIG", RESTRICTED_CONFIG_TARGET)
        .with_env_var("FERROEHR__AUTH__ENABLED", "true")
        .with_env_var("FERROEHR__AUTHZ__RBAC__ENABLED", "true")
        .with_env_var("FERROEHR__AUTHZ__RBAC__EHR_ACCESS_DEFAULT", "restricted")
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: FERROEHR.repository,
            source,
        })?;
    ready(FERROEHR_PRODUCT, system_id, server, database).await
}
