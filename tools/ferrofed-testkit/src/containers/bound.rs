// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroEHR nodes with authentication on, each binding every person who
//! signs in to a demographic `PERSON`.
//!
//! From release 5.7.0 a FerroEHR node refuses a person its party store holds
//! no `PERSON` for (FerroEHR #5003, #5006), so a node with authentication on
//! stores the professional `PERSON` and `ROLE` templates its party mapping
//! names before it serves. The harness runs `ferroehr definitions
//! load` once against the node's database with the pair FerroEHR publishes,
//! vendored under `docs/specs/ferroehr-definitions/`, and [`PARTY_MAPPING`]
//! is the mapping over that pair. [`persons`] starts a node that trusts one
//! issuer of persons by its key set and creates a provisional `PERSON` at a
//! person's first request (FerroEHR's just-in-time creation), the node the
//! gateway reaches under a caller's exchanged token.
//!
//! No specification governs the harness: our own design.

use std::path::Path;
use std::sync::Arc;

use testcontainers::core::WaitFor;
use testcontainers::core::wait::ExitWaitStrategy;
use testcontainers::{ContainerRequest, GenericImage, ImageExt};

use super::database::database_server;
use super::images::FERROEHR;
use super::retry;
use super::{
    DatabaseServer, FERROEHR_PRODUCT, HarnessError, NODE_A_DATABASE, Node, ferroehr_request,
    node_settings, ready,
};

/// FerroEHR's professional `PERSON` definition, loaded first.
pub const PERSON_DEFINITION: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/ferroehr-definitions/openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0.adls"
);

/// FerroEHR's professional `ROLE` definition, loaded second.
pub const ROLE_DEFINITION: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/ferroehr-definitions/openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0.adls"
);

/// Where a container reads the two definitions from.
const PERSON_TARGET: &str = "/tmp/ferrofed-harness-person.adls";
const ROLE_TARGET: &str = "/tmp/ferrofed-harness-role.adls";

/// The identifier issuer the binding keys of a node's persons are written
/// under, a synthetic value in the example arc.
pub const IDENTIFIER_ISSUER: &str = "urn:oid:2.999.7.8";

/// The party mapping over FerroEHR's professional pair.
///
/// It names the healthcare provider every node of the harness acts for, the
/// binding key, `userName` and `externalId` on the pair's one identifier
/// element, told apart by type, the name a token's `name` claim writes, and
/// the professional `ROLE`.
pub const PARTY_MAPPING: &str = r#"[authz.party_binding]
provider = { issuer = "urn:oid:2.999.7.9", id = "ferrofed-harness" }

[[authz.party_binding.person.template]]
template_id = "openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0"
default = true
key = { path = "/details[id2]/items[id3]", type = "login" }
user_name = { path = "/details[id2]/items[id3]", type = "scim_user_name" }
external_id = { path = "/details[id2]/items[id3]", type = "scim_external_id" }

[[authz.party_binding.person.attribute]]
scim = "name.formatted"
oidc = "name"
jit = true
name = true
path = { "openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0" = "/identities[id4]/details[id5]/items[id6]" }

[authz.party_binding.professional_role]
template_id = "openEHR-DEMOGRAPHIC-ROLE.healthcare_professional.v1.0.0"
"#;

/// Where a [`persons`] node reads its configuration and its issuer's key
/// set from.
const CONFIG_TARGET: &str = "/tmp/ferrofed-harness-persons.toml";
const JWKS_TARGET: &str = "/tmp/ferrofed-harness-persons-jwks.json";

/// Stores FerroEHR's professional `PERSON` and `ROLE` definitions in the
/// database `name` of `database`, in that order, with a one-shot `ferroehr
/// definitions load` that must exit `0`.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses the container or
/// the load does not exit `0`.
pub(super) async fn load_definitions(
    database: &DatabaseServer,
    name: &str,
    system_id: &'static str,
) -> Result<(), HarnessError> {
    let loaded = retry::start(|| {
        let image = FERROEHR
            .image()
            .with_wait_for(WaitFor::exit(ExitWaitStrategy::new().with_exit_code(0)));
        let request: ContainerRequest<GenericImage> = node_settings(
            image.with_network(database.network.clone()),
            &database.host,
            name,
            system_id,
        );
        request
            .with_copy_to(PERSON_TARGET, Path::new(PERSON_DEFINITION))
            .with_copy_to(ROLE_TARGET, Path::new(ROLE_DEFINITION))
            .with_env_var("FERROEHR__AUTH__ENABLED", "false")
            .with_cmd(["definitions", "load", PERSON_TARGET, ROLE_TARGET])
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROEHR.repository,
        source,
    })?;
    drop(loaded);
    Ok(())
}

/// The issuer of persons a [`persons`] node trusts.
#[derive(Debug, Clone, Copy)]
pub struct PersonsIssuer<'a> {
    /// The issuer identifier, an `https` URL its tokens name as `iss`.
    pub issuer: &'a str,
    /// The node's audience, which every token for the node names in `aud`.
    pub audience: &'a str,
    /// The issuer's public keys, a JWK Set in JSON; the node reads them from
    /// a file instead of the issuer's metadata.
    pub jwks: &'a str,
    /// The deployment's own clients, through which a person acts alone.
    pub first_party_clients: &'a [&'a str],
    /// The clients that act for a person, the gateway among them.
    pub delegates: &'a [&'a str],
    /// The identity provider a token names in its `idp` claim, whose
    /// persons the node creates at their first request.
    pub idp: &'a str,
}

impl PersonsIssuer<'_> {
    /// The node's configuration file: the issuer as its one `persons`
    /// issuer, read from a key set file, its persons bound under
    /// [`IDENTIFIER_ISSUER`] and created at their first request when their
    /// token names [`PersonsIssuer::idp`], over [`PARTY_MAPPING`], with the
    /// role gate off.
    #[must_use]
    pub fn config(&self) -> String {
        let quoted = |values: &[&str]| {
            values
                .iter()
                .map(|value| toml::Value::String((*value).to_owned()).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let delegates = self
            .delegates
            .iter()
            .map(|client| {
                format!(
                    "{{ client = {} }}",
                    toml::Value::String((*client).to_owned())
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let issuer = toml::Value::String(self.issuer.to_owned());
        format!(
            "[auth]\nenabled = true\n\n[[auth.issuer]]\nissuer = {issuer}\nkind = \"persons\"\naudience = [{audience}]\njwks_file = \"{JWKS_TARGET}\"\nfirst_party_clients = [{first_party}]\ndelegates = [{delegates}]\n\n[[auth.party]]\nissuer = {issuer}\nidentifier_issuer = \"{IDENTIFIER_ISSUER}\"\nidentifier_type = \"login\"\n\n[[authz.party_binding.issuer]]\nissuer = {issuer}\njit = true\nadmit_idp = [{idp}]\n\n{PARTY_MAPPING}\n[authz.rbac]\nenabled = false\n",
            audience = quoted(&[self.audience]),
            first_party = quoted(self.first_party_clients),
            idp = quoted(&[self.idp]),
        )
    }
}

/// Starts FerroEHR as `system_id` on a database server of its own with
/// authentication on, trusting `trust` alone, and waits for its readiness
/// endpoint.
///
/// The professional definitions are stored first. A
/// person's token of the issuer is admitted through a first-party client or
/// a delegate, and a person the node holds no `PERSON` for is given a
/// provisional one at the first request whose token names the issuer's
/// identity provider and a `name`.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container or
/// the definitions do not load, and [`HarnessError::NotReady`] when the CDR
/// does not become ready in time.
pub async fn persons(
    system_id: &'static str,
    trust: &PersonsIssuer<'_>,
) -> Result<Node, HarnessError> {
    let database = Arc::new(database_server(NODE_A_DATABASE, &[]).await?);
    load_definitions(&database, NODE_A_DATABASE, system_id).await?;
    let config = trust.config();
    let server = retry::start(|| {
        ferroehr_request(&database, NODE_A_DATABASE, system_id)
            .with_copy_to(CONFIG_TARGET, config.as_bytes().to_vec())
            .with_copy_to(JWKS_TARGET, trust.jwks.as_bytes().to_vec())
            .with_env_var("FERROEHR_CONFIG", CONFIG_TARGET)
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROEHR.repository,
        source,
    })?;
    ready(FERROEHR_PRODUCT, system_id, server, database).await
}

#[cfg(test)]
mod tests {
    use super::{PARTY_MAPPING, PERSON_DEFINITION, PersonsIssuer, ROLE_DEFINITION};

    #[test]
    fn the_definitions_the_harness_loads_are_vendored() {
        for path in [PERSON_DEFINITION, ROLE_DEFINITION] {
            let text = std::fs::read_to_string(path).unwrap_or_default();
            assert!(
                text.starts_with("archetype"),
                "{path} is an ADL 2 archetype"
            );
        }
    }

    #[test]
    fn a_persons_node_trusts_its_issuer_and_binds_under_the_mapping() {
        let config = PersonsIssuer {
            issuer: "https://issuer.example.test",
            audience: "https://cdr-a.example.org/openehr",
            jwks: "{\"keys\":[]}",
            first_party_clients: &["synthetic-seed"],
            delegates: &["synthetic-gateway"],
            idp: "synthetic-idp",
        }
        .config();
        let parsed: toml::Table = toml::from_str(&config).unwrap_or_default();
        assert!(parsed.contains_key("auth"), "{config}");
        assert!(config.contains("delegates = [{ client = \"synthetic-gateway\" }]"));
        assert!(config.contains("admit_idp = [\"synthetic-idp\"]"));
        assert!(config.contains(PARTY_MAPPING));
    }
}
