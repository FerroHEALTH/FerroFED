// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroPIX in the harness: the PIX Manager the end-to-end lane resolves
//! patients through.
//!
//! [`ferropix`] starts the pinned [`FERROPIX`] image on its pinned
//! PostgreSQL, [`FERROPIX_POSTGRES`], under FerroPIX's development profile:
//! one identifier domain per system the caller names, two Basic users, the
//! Patient Identity Source every domain is fed by ([`SOURCE`]) and the PIXm
//! Consumer the gateway asks as ([`CONSUMER`]), and no usage report. A test
//! feeds it over ITI-104 with
//! [`seed::feed_as`](crate::seed::feed_as) and points the gateway's
//! `[[pixm.manager]]` at [`FerroPix::fhir_base`] with the consumer's
//! credentials. The database role is the restricted one FerroPIX requires:
//! `CREATE` on its database and nothing else.
//!
//! Every value is synthetic, inside the `urn:oid:2.999` example arc, and
//! every secret is a development value. No specification governs which
//! products the harness runs: our own design.

use std::fmt::Write as _;
use std::time::Duration;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use super::images::{FERROPIX, FERROPIX_POSTGRES};
use super::retry;
use super::{HarnessError, POSTGRES_PORT, await_readiness, names, postgres_health_check};

/// The port FerroPIX serves HTTP on inside its container.
const PORT: u16 = 8080;

/// The path of FerroPIX's FHIR face under its base.
const FHIR_PATH: &str = "/fhir/";

/// The database's superuser, its development password, and the database.
const SUPERUSER: (&str, &str) = ("postgres", "postgres_example");

/// The database and the restricted role FerroPIX connects as.
const DATABASE: &str = "ferropix";

/// The restricted role's development password.
const DATABASE_PASSWORD: &str = "ferropix_example";

/// Where the configuration is copied in the container.
const CONFIG_TARGET: &str = "/etc/ferropix/ferropix.toml";

/// Where the sealing key is copied in the container.
const DATA_KEY_TARGET: &str = "/etc/ferropix/data-key";

/// A synthetic sealing key: the base64 of 32 bytes, a development value.
const DATA_KEY: &str = "ZmVycm9mZWQtaGFybmVzcy1zZWFsaW5nLWtleS0wMDE=";

/// The script the database image runs at its first start: the restricted
/// role, as FerroPIX's own quickstart creates it.
const ROLE_SCRIPT: &str = "CREATE ROLE ferropix LOGIN PASSWORD 'ferropix_example';\n\
GRANT CREATE ON DATABASE ferropix TO ferropix;\n";

/// Where the role script is copied in the database container.
const ROLE_SCRIPT_TARGET: &str = "/docker-entrypoint-initdb.d/10-ferropix-role.sql";

/// A Basic user of FerroPIX: its user-id and its development password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BasicUser {
    /// The user-id.
    pub user: &'static str,
    /// The password, a development value.
    pub password: &'static str,
    /// The Argon2id PHC string of the password, as FerroPIX reads it.
    phc: &'static str,
    /// The scope the user is granted.
    scope: &'static str,
}

/// The Patient Identity Source every harness domain is fed by.
pub const SOURCE: BasicUser = BasicUser {
    user: "ferrofed-source",
    password: "ferrofed-source-example",
    phc: "$argon2id$v=19$m=19456,t=2,p=1$ZmVycm9mZWQtc291cmNlLXNhbHQ$tBcj4hfLaGRGVGF9bFKn7yKB4lcUjmOTvyMeZ55pdKk",
    scope: "system/Patient.cud",
};

/// The PIXm Consumer the gateway asks FerroPIX as.
pub const CONSUMER: BasicUser = BasicUser {
    user: "ferrofed-consumer",
    password: "ferrofed-consumer-example",
    phc: "$argon2id$v=19$m=19456,t=2,p=1$ZmVycm9mZWQtY29uc3VtZXItc2FsdA$51aYstBVyvm/RYL2TNzU400Fwfl3M0V5vjybCi4FcVM",
    scope: "system/Patient.rs",
};

/// A started FerroPIX with its database, torn down when it is dropped.
#[derive(Debug)]
pub struct FerroPix {
    /// FerroPIX itself.
    server: ContainerAsync<GenericImage>,
    /// Its database server.
    database: ContainerAsync<GenericImage>,
    /// The origin FerroPIX is reachable at from the host, with no path.
    origin: String,
}

impl FerroPix {
    /// Returns the FHIR base, with its trailing slash, that the gateway's
    /// `[[pixm.manager]] url` names and the feed is sent to.
    #[must_use]
    pub fn fhir_base(&self) -> String {
        format!("{}{FHIR_PATH}", self.origin)
    }

    /// Returns the origin FerroPIX is reachable at from the host.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the FerroPIX container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns the database container FerroPIX was started against.
    #[must_use]
    pub fn database(&self) -> &ContainerAsync<GenericImage> {
        &self.database
    }
}

/// Returns FerroPIX's configuration: the development profile, its database
/// at `database_host`, the two Basic users, and one local domain per system
/// of `domains`, each fed by [`SOURCE`].
fn config(database_host: &str, domains: &[String]) -> String {
    let mut text = format!(
        "profile = \"development\"\n\n\
         [server]\nlisten = \"0.0.0.0:{PORT}\"\npublic_base_url = \"http://ferropix.example.org\"\n\n\
         [db]\nurl = \"postgresql://{DATABASE}:{DATABASE_PASSWORD}@{database_host}:{POSTGRES_PORT}/{DATABASE}\"\n\
         data_key_file = \"{DATA_KEY_TARGET}\"\n\n\
         [usage_report]\nenabled = false\n\n\
         [dev]\nloopback = false\n"
    );
    for user in [SOURCE, CONSUMER] {
        // NOTE: writing to a String cannot fail, so each result is dropped.
        let _written: std::fmt::Result = write!(
            text,
            "\n[[auth.basic.users]]\nuser = \"{}\"\npassword_hash = \"{}\"\nscopes = [\"{}\"]\n",
            user.user, user.phc, user.scope
        );
    }
    for (index, system) in domains.iter().enumerate() {
        let _written: std::fmt::Result = write!(
            text,
            "\n[[domain]]\nsystem = \"{system}\"\nname = \"Harness domain {index}\"\n\
             kind = \"local\"\nsource_client_id = \"{}\"\n",
            SOURCE.user
        );
    }
    text
}

/// Starts FerroPIX with one identifier domain per system of `domains`, and
/// waits until it reports ready.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container and
/// [`HarnessError::NotReady`] when FerroPIX does not become ready in time.
pub async fn ferropix(domains: &[String]) -> Result<FerroPix, HarnessError> {
    let (network, host) = names("ferropix");
    let database = retry::start(|| {
        FERROPIX_POSTGRES
            .image()
            .with_exposed_port(POSTGRES_PORT.tcp())
            .with_wait_for(WaitFor::healthcheck())
            .with_health_check(postgres_health_check(SUPERUSER.0, DATABASE))
            .with_env_var("POSTGRES_USER", SUPERUSER.0)
            .with_env_var("POSTGRES_PASSWORD", SUPERUSER.1)
            .with_env_var("POSTGRES_DB", DATABASE)
            .with_copy_to(ROLE_SCRIPT_TARGET, ROLE_SCRIPT.as_bytes().to_vec())
            .with_network(network.clone())
            .with_container_name(host.clone())
    })
    .await
    .map_err(|source| HarnessError::Container {
        image: FERROPIX_POSTGRES.repository,
        source,
    })?;
    let image = FERROPIX.repository;
    let configuration = config(&host, domains);
    let server = retry::start(|| {
        FERROPIX
            .image()
            .with_exposed_port(PORT.tcp())
            .with_network(network.clone())
            .with_copy_to(CONFIG_TARGET, configuration.as_bytes().to_vec())
            .with_copy_to(DATA_KEY_TARGET, DATA_KEY.as_bytes().to_vec())
            .with_env_var("FERROPIX_CONFIG", CONFIG_TARGET)
            .with_startup_timeout(Duration::from_secs(120))
    })
    .await
    .map_err(|source| HarnessError::Container { image, source })?;
    let server_host = server
        .get_host()
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let port = server
        .get_host_port_ipv4(PORT.tcp())
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let origin = format!("http://{server_host}:{port}");
    await_readiness(&format!("{origin}/health/readiness")).await?;
    Ok(FerroPix {
        server,
        database,
        origin,
    })
}

#[cfg(test)]
mod tests {
    use super::{CONSUMER, SOURCE, config};

    #[test]
    fn the_configuration_names_every_domain_fed_by_the_source() {
        let text = config(
            "db",
            &["urn:oid:2.999.1".to_owned(), "urn:oid:2.999.2.1".to_owned()],
        );
        assert!(text.starts_with("profile = \"development\""));
        assert_eq!(2, text.matches("[[domain]]").count());
        assert_eq!(
            2,
            text.matches(&format!("source_client_id = \"{}\"", SOURCE.user))
                .count()
        );
        assert!(text.contains(&format!("user = \"{}\"", CONSUMER.user)));
        assert!(text.contains("@db:5432/ferropix"));
        assert!(
            !text.contains(CONSUMER.password) && !text.contains(SOURCE.password),
            "only the hashes are configured"
        );
    }
}
