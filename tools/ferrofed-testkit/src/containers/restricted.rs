// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FerroEHR node with its access controls on, whose refusals the node
//! profile's access check reads (§13, N26).

use std::sync::Arc;

use testcontainers::ImageExt;

use super::bound::{IDENTIFIER_ISSUER, PARTY_MAPPING, load_definitions};
use super::database::database_server;
use super::images::FERROEHR;
use super::retry;
use super::{FERROEHR_PRODUCT, HarnessError, NODE_A_DATABASE, Node, ferroehr_request, ready};
use crate::issuer::{Claims, Issuer};

/// Where a restricted node reads its configuration from.
const RESTRICTED_CONFIG_TARGET: &str = "/tmp/ferrofed-harness-restricted.toml";

/// Where a restricted node reads its issuer's key set from.
const JWKS_TARGET: &str = "/tmp/ferrofed-harness-restricted-jwks.json";

/// The node's issuer of persons, a synthetic value.
const ISSUER: &str = "https://restricted.issuer.example.test";

/// The node's audience, which every token for it names.
const AUDIENCE: &str = "https://restricted.cdr.example.test/openehr";

/// The node's own client, through which its persons sign in.
const CLIENT: &str = "harness-client";

/// The identity provider whose persons the node creates at a first request.
const IDP: &str = "harness-idp";

/// Every clinical SMART on openEHR scope, so a refusal of the clinician is
/// the role gate's decision and never a missing scope.
const CLINICAL_SCOPES: &str = "user/composition-*.cruds user/template-*.cruds user/aql-*.cruds";

/// The configuration of a restricted node: its one issuer of persons, read
/// from [`JWKS_TARGET`], whose role claims may grant the administrator's
/// role, its persons created at the first request, the role gate on, and an
/// EHR with no access settings reaching the administrator alone.
fn config() -> String {
    format!(
        "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = \"{ISSUER}\"\nkind = \"persons\"\naudience = [\"{AUDIENCE}\"]\njwks_file = \"{JWKS_TARGET}\"\nfirst_party_clients = [\"{CLIENT}\"]\n\n[[auth.party]]\nissuer = \"{ISSUER}\"\nidentifier_issuer = \"{IDENTIFIER_ISSUER}\"\nidentifier_type = \"login\"\n\n[[authz.party_binding.issuer]]\nissuer = \"{ISSUER}\"\njit = true\nadmit_idp = [\"{IDP}\"]\n\n{PARTY_MAPPING}\n[authz.rbac]\nenabled = true\nrole_issuers = [\"{ISSUER}\"]\nehr_access_default = \"restricted\"\n"
    )
}

/// A FerroEHR node with its access controls on, and the tokens of its two
/// persons.
#[derive(Debug)]
pub struct Restricted {
    /// The node.
    pub node: Node,
    /// The administrator's token, whose role reaches every EHR.
    pub admin: String,
    /// The clinician's token, whose role reaches no EHR without access
    /// settings.
    pub clinician: String,
}

/// Starts FerroEHR as `system_id` on a database server of its own with its
/// access controls on, waits for its readiness endpoint, and mints the
/// tokens of its two persons.
///
/// The node trusts one issuer of persons, a harness [`Issuer`] whose key set
/// it reads from a file, and creates each person's `PERSON` at the person's
/// first request. The administrator's token carries the `ADMIN` role, which
/// the issuer may grant, and the clinician's the `USER` role; both hold
/// every clinical scope. The role gate is on and
/// `authz.rbac.ehr_access_default` is `restricted`: an EHR with no access
/// settings, which every new EHR is, reaches the administrator alone, so the
/// node refuses the clinician by a decision of its own. The node profile's
/// access check reads that decision (§13, N26).
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container or
/// the professional definitions do not load, [`HarnessError::Issuer`] when
/// the issuer's key or a token cannot be made, and
/// [`HarnessError::NotReady`] when the CDR does not become ready in time.
pub async fn ferroehr_restricted(system_id: &'static str) -> Result<Restricted, HarnessError> {
    let issuer = Issuer::new(ISSUER).map_err(HarnessError::Issuer)?;
    let jwks = issuer.jwks_json().map_err(HarnessError::Issuer)?;
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    load_definitions(&database, NODE_A_DATABASE, system_id).await?;
    let config = config();
    let server = retry::start(|| {
        ferroehr_request(&database, NODE_A_DATABASE, system_id)
            .with_copy_to(RESTRICTED_CONFIG_TARGET, config.as_bytes().to_vec())
            .with_copy_to(JWKS_TARGET, jwks.as_bytes().to_vec())
            .with_env_var("FERROEHR_CONFIG", RESTRICTED_CONFIG_TARGET)
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROEHR.repository,
        source,
    })?;
    let node = ready(FERROEHR_PRODUCT, system_id, server, database).await?;
    Ok(Restricted {
        node,
        admin: person_token(&issuer, "harness-admin", "ADMIN")?,
        clinician: person_token(&issuer, "harness-clinician", "USER")?,
    })
}

/// A token of the node's issuer for the person `subject` holding `role`,
/// through the node's own client, with every clinical scope and the name and
/// identity provider the node creates the person's `PERSON` from.
fn person_token(issuer: &Issuer, subject: &str, role: &str) -> Result<String, HarnessError> {
    let mut claims = Claims::new(ISSUER, AUDIENCE);
    subject.clone_into(&mut claims.sub);
    CLIENT.clone_into(&mut claims.client_id);
    claims.scope = Some(CLINICAL_SCOPES.to_owned());
    claims.extensions = None;
    claims.other.insert(String::from("roles"), role.to_owned());
    claims
        .other
        .insert(String::from("name"), format!("Synthetic {subject}"));
    claims.other.insert(String::from("idp"), IDP.to_owned());
    issuer.mint(&claims).map_err(HarnessError::Issuer)
}

#[cfg(test)]
mod tests {
    use super::{ISSUER, config};

    #[test]
    fn the_node_lets_its_issuer_grant_the_administrators_role_behind_the_role_gate() {
        let text = config();
        let parsed: toml::Table = toml::from_str(&text).unwrap_or_default();
        let rbac = parsed.get("authz").and_then(|authz| authz.get("rbac"));
        assert_eq!(
            Some(true),
            rbac.and_then(|rbac| rbac.get("enabled"))
                .and_then(toml::Value::as_bool),
            "{text}"
        );
        assert_eq!(
            Some("restricted"),
            rbac.and_then(|rbac| rbac.get("ehr_access_default"))
                .and_then(toml::Value::as_str)
        );
        assert!(text.contains(&format!("role_issuers = [\"{ISSUER}\"]")));
        assert!(!text.contains("auth.basic"), "no Basic user: {text}");
    }
}
