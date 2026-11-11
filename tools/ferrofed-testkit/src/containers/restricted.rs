// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FerroEHR node with its access controls on, whose refusals the node
//! profile's access check reads (§13, N26).

use std::sync::Arc;

use http::StatusCode;
use testcontainers::ImageExt;

use super::bound::{PARTY_MAPPING, load_definitions};
use super::database::database_server;
use super::images::FERROEHR;
use super::retry;
use super::{
    FERROEHR_PRODUCT, HarnessError, HarnessUser, NODE_A_DATABASE, Node, ferroehr_request, ready,
};
use crate::issuer::{Claims, Issuer};

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

/// Where a restricted node reads its provisioning client's key set from.
const SCIM_JWKS_TARGET: &str = "/tmp/ferrofed-harness-scim-jwks.json";

/// The issuer of the provisioning client's tokens, a synthetic value.
const SCIM_ISSUER: &str = "https://scim.issuer.example.test";

/// The provisioning client of a restricted node.
const SCIM_CLIENT: &str = "harness-scim";

/// The audience of the provisioning client's tokens.
const SCIM_AUDIENCE: &str = "ferroehr-scim";

/// The scope a token of the provisioning client holds.
const SCIM_SCOPE: &str = "ferroehr/scim";

/// The configuration file of a restricted node: its two Basic users, each
/// password stored as the Argon2id hash FerroEHR requires, with cost
/// parameters above the floor it checks at boot. The users are an array of
/// tables, which only a file can carry. Both hold every clinical SMART on
/// openEHR scope, so a refusal of the clinician is the role gate's decision
/// and never a missing scope; the administrator also holds the product scope
/// of the ADMIN API.
///
/// A Basic user is a person from FerroEHR 5.7.0, bound to the `PERSON`
/// whose binding key its `[[authz.party_binding.basic_user]]` table names,
/// under the identifier issuer of a `persons` issuer that signs nothing here.
/// The harness creates both `PERSON`s through the node's SCIM endpoint, as
/// the one provisioning client of a `machines` issuer whose key set the
/// harness writes beside the file, and the Basic users' issuer reads the same
/// set.
const RESTRICTED_CONFIG: &str = r#"[[auth.issuer]]
issuer = "https://basic-users.ferroehr.invalid"
kind = "persons"
audience = ["ferroehr-basic-users"]
jwks_file = "/tmp/ferrofed-harness-scim-jwks.json"

[[auth.party]]
issuer = "https://basic-users.ferroehr.invalid"
identifier_issuer = "urn:oid:2.999.7.8"
identifier_type = "login"

[[auth.issuer]]
issuer = "https://scim.issuer.example.test"
kind = "machines"
audience = ["ferroehr-scim"]
jwks_file = "/tmp/ferrofed-harness-scim-jwks.json"

[auth.scim]
public_base_url = "http://localhost:8080"

[[auth.scim.client]]
issuer = "https://scim.issuer.example.test"
client_id = "harness-scim"
scope = "ferroehr/scim"
binding_issuer = "urn:oid:2.999.7.8"

[[auth.basic.users]]
user = "harness-admin"
password_hash = "$argon2id$v=19$m=32768,t=2,p=1$ZmVycm9mZWQtaGFybmVzcy1h$t/ra6Wz0qdF31frrVsd1pXhVK2mQdTQLx53fDeIgDS8"
scopes = ["user/composition-*.cruds", "user/template-*.cruds", "user/aql-*.cruds", "__ferroehr.platform"]
roles = ["ADMIN"]

[[auth.basic.users]]
user = "harness-clinician"
password_hash = "$argon2id$v=19$m=32768,t=2,p=1$ZmVycm9mZWQtaGFybmVzcy1j$eBo26o4ip9ZdPKtCbzkJIjvpp+SfoLqTK0a07Qw6jcU"
scopes = ["user/composition-*.cruds", "user/template-*.cruds", "user/aql-*.cruds"]
roles = ["USER"]

[[authz.party_binding.basic_user]]
user = "harness-admin"
key_issuer = "urn:oid:2.999.7.8"
key = "harness-admin"

[[authz.party_binding.basic_user]]
user = "harness-clinician"
key_issuer = "urn:oid:2.999.7.8"
key = "harness-clinician"
"#;

/// Starts FerroEHR as `system_id` on a database server of its own with its
/// access controls on, waits for its readiness endpoint, and provisions the
/// `PERSON` each of its Basic users binds to.
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
/// Returns [`HarnessError::Container`] when Docker refuses a container or
/// the professional definitions do not load, [`HarnessError::Issuer`] when
/// the provisioning client's key or token cannot be made,
/// [`HarnessError::NotReady`] when the CDR does not become ready in time,
/// and [`HarnessError::Unprovisioned`] or [`HarnessError::ProvisionRefused`]
/// when a user's `PERSON` cannot be created.
pub async fn ferroehr_restricted(system_id: &'static str) -> Result<Node, HarnessError> {
    let scim = Issuer::new(SCIM_ISSUER).map_err(HarnessError::Issuer)?;
    let jwks = scim.jwks_json().map_err(HarnessError::Issuer)?;
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    load_definitions(&database, NODE_A_DATABASE, system_id).await?;
    let config = format!("{RESTRICTED_CONFIG}\n{PARTY_MAPPING}");
    let server = retry::start(|| {
        ferroehr_request(&database, NODE_A_DATABASE, system_id)
            .with_copy_to(RESTRICTED_CONFIG_TARGET, config.as_bytes().to_vec())
            .with_copy_to(SCIM_JWKS_TARGET, jwks.as_bytes().to_vec())
            .with_env_var("FERROEHR_CONFIG", RESTRICTED_CONFIG_TARGET)
            .with_env_var("FERROEHR__AUTH__ENABLED", "true")
            .with_env_var("FERROEHR__AUTHZ__RBAC__ENABLED", "true")
            .with_env_var("FERROEHR__AUTHZ__RBAC__EHR_ACCESS_DEFAULT", "restricted")
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROEHR.repository,
        source,
    })?;
    let node = ready(FERROEHR_PRODUCT, system_id, server, database).await?;
    let token = provisioning_token(&scim)?;
    for user in [RESTRICTED_ADMIN, RESTRICTED_CLINICIAN] {
        provision(node.origin(), &token, user.user).await?;
    }
    Ok(node)
}

/// A token of the provisioning client: an app-only token whose `sub` is the
/// client, for the SCIM audience, holding the SCIM scope alone.
fn provisioning_token(scim: &Issuer) -> Result<String, HarnessError> {
    let mut claims = Claims::new(SCIM_ISSUER, SCIM_AUDIENCE);
    SCIM_CLIENT.clone_into(&mut claims.sub);
    SCIM_CLIENT.clone_into(&mut claims.client_id);
    claims.scope = Some(SCIM_SCOPE.to_owned());
    claims.extensions = None;
    scim.mint(&claims).map_err(HarnessError::Issuer)
}

/// Creates the `PERSON` of `user` at the node of `origin` through
/// `POST /scim/v2/Users` (RFC 7644 §3.3), its binding key the user's name
/// under [`IDENTIFIER_ISSUER`](super::bound::IDENTIFIER_ISSUER).
async fn provision(origin: &str, token: &str, user: &'static str) -> Result<(), HarnessError> {
    let body = serde_json::json!({
        "schemas": [
            "urn:ietf:params:scim:schemas:core:2.0:User",
            "urn:ietf:params:scim:schemas:extension:ferrohealth:2.0:User",
        ],
        "userName": user,
        "urn:ietf:params:scim:schemas:extension:ferrohealth:2.0:User": {
            "loginIdentifier": user,
        },
    });
    let answer = reqwest::Client::new()
        .post(format!("{}/scim/v2/Users", origin.trim_end_matches('/')))
        .bearer_auth(token)
        .header(http::header::CONTENT_TYPE, "application/scim+json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|source| HarnessError::Unprovisioned { user, source })?;
    match answer.status() {
        StatusCode::CREATED => Ok(()),
        status => Err(HarnessError::ProvisionRefused { user, status }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::bound::IDENTIFIER_ISSUER;
    use super::RESTRICTED_CONFIG;

    #[test]
    fn each_basic_user_binds_under_the_identifier_issuer_the_harness_provisions() {
        let parsed: toml::Table = toml::from_str(RESTRICTED_CONFIG).unwrap_or_default();
        let bindings = parsed
            .get("authz")
            .and_then(|authz| authz.get("party_binding"))
            .and_then(|binding| binding.get("basic_user"))
            .and_then(toml::Value::as_array)
            .map_or(0, Vec::len);
        assert_eq!(2, bindings, "{RESTRICTED_CONFIG}");
        assert!(RESTRICTED_CONFIG.contains(&format!("binding_issuer = \"{IDENTIFIER_ISSUER}\"")));
    }
}
