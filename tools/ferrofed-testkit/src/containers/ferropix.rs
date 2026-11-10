// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroPIX in the harness: the PIX Manager the end-to-end lane resolves
//! patients through.
//!
//! [`start`] starts the pinned [`FERROPIX`] image on its pinned
//! PostgreSQL, [`FERROPIX_POSTGRES`], under FerroPIX's development profile:
//! the patient namespaces as `authoritative` domains and each member's
//! `ehr_id` domain as a `local` one, and no usage report. FerroPIX admits a
//! machine only with a client credentials token of an `[[auth.issuer]]`
//! (FerroPIX 0.0.21, GHSA-jhg2-jwjp-hq74) whose metadata it discovers over
//! `https` (RFC 8414 §3) and whose metadata names a token endpoint (SMART App
//! Launch 2.2.0, §Metadata). The harness issuer ([`crate::issuer::Issuer`])
//! serves its metadata, its key set and its token endpoint from a wiremock
//! server behind a TLS front ([`MutualTls::open`]), which FerroPIX reaches on
//! the Docker host gateway, as [`CONTAINER_HOST`], trusting the front's CA.
//! That issuer signs the tokens of the two machines: the Patient
//! Identity Source every domain is fed by ([`SOURCE`]) and the PIXm Consumer
//! the gateway asks as ([`CONSUMER`]). FerroPIX links two records only
//! through an identifier in an authoritative domain, so a test feeds one
//! record per member over ITI-104, each carrying the patient's identifier,
//! with [`seed::feed_records_as`](crate::seed::feed_records_as) under
//! [`FerroPix::token`] of the Source, and points the gateway's
//! `[[pixm.manager]]` at [`FerroPix::fhir_base`] with the consumer's client
//! credentials grant at [`FerroPix::token_endpoint`]. The database role is
//! the restricted one FerroPIX requires: `CREATE` on its database and
//! nothing else.
//!
//! Every value is synthetic, inside the `urn:oid:2.999` example arc, and
//! every secret is a development value. No specification governs which
//! products the harness runs: our own design.

use std::fmt::Write as _;
use std::time::Duration;

use testcontainers::core::{Host, IntoContainerPort, WaitFor};
use testcontainers::{ContainerAsync, ContainerRequest, GenericImage, ImageExt};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::issuer::{Claims, Issuer, IssuerError};
use crate::mock::Server;
use crate::reference::CONTAINER_HOST;
use crate::tls::MutualTls;

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

/// The path of the harness issuer under the front's origin.
const ISSUER_PATH: &str = "/ferrofed";

/// The audience the harness issuer's tokens name: FerroPIX's FHIR base.
const AUDIENCE: &str = "http://ferropix.example.org/fhir";

/// Where the front's CA is copied in the container.
const ROOTS_TARGET: &str = "/etc/ferropix/harness-roots.pem";

/// A machine of FerroPIX: the client the harness issuer names in its
/// tokens, and the scope it is granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Client {
    /// The client id, the token's `sub` and `client_id`.
    pub id: &'static str,
    /// The scope the client is granted.
    scope: &'static str,
}

/// The Patient Identity Source every harness domain is fed by.
pub const SOURCE: Client = Client {
    id: "ferrofed-source",
    scope: "system/Patient.cud",
};

/// The PIXm Consumer the gateway asks FerroPIX as.
pub const CONSUMER: Client = Client {
    id: "ferrofed-consumer",
    scope: "system/Patient.rs",
};

/// The development client secret the consumer's grant sends, which the
/// harness token endpoint does not check.
pub const CONSUMER_SECRET: &str = "ferrofed-consumer-example";

/// A started FerroPIX with its database, torn down when it is dropped.
#[derive(Debug)]
pub struct FerroPix {
    /// FerroPIX itself.
    server: ContainerAsync<GenericImage>,
    /// Its database server.
    database: ContainerAsync<GenericImage>,
    /// The origin FerroPIX is reachable at from the host, with no path.
    origin: String,
    /// The harness issuer FerroPIX trusts.
    issuer: Issuer,
    /// The server of the issuer's metadata, key set and token endpoint.
    _documents: Server,
    /// The TLS front of that server, which FerroPIX and the gateway reach.
    front: MutualTls,
    /// The directory holding the front's CA as a file.
    roots: tempfile::TempDir,
}

impl FerroPix {
    /// Returns the FHIR base, with its trailing slash, that the gateway's
    /// `[[pixm.manager]] url` names and the feed is sent to.
    #[must_use]
    pub fn fhir_base(&self) -> String {
        format!("{}{FHIR_PATH}", self.origin)
    }

    /// Returns the SOAP endpoint of the XCPD Responding Gateway, when
    /// FerroPIX was started with one.
    #[must_use]
    pub fn xcpd_endpoint(&self) -> String {
        format!("{}/xcpd", self.origin)
    }

    /// Returns the origin FerroPIX is reachable at from the host.
    #[must_use]
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Returns the URL of the token endpoint the gateway's client
    /// credentials grant asks for the consumer's token.
    #[must_use]
    pub fn token_endpoint(&self) -> String {
        format!("{}{ISSUER_PATH}/token", self.front.origin())
    }

    /// Returns the file of the CA the issuer's TLS front chains to, which
    /// the gateway's grant trusts its token endpoint by.
    #[must_use]
    pub fn trust_roots_file(&self) -> std::path::PathBuf {
        self.roots.path().join("roots.pem")
    }

    /// Returns a client credentials token of the harness issuer for
    /// `client`, which FerroPIX admits as that machine.
    ///
    /// # Errors
    ///
    /// Returns [`IssuerError`] when the token cannot be signed.
    pub fn token(&self, client: Client) -> Result<String, IssuerError> {
        mint(&self.issuer, client)
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

/// FerroPIX's XCPD Responding Gateway (ITI TF-2 §3.55), which answers
/// ITI-55 at `POST {base}/xcpd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Xcpd {
    /// The gateway's `homeCommunityId`, in dotted form.
    pub home_community: String,
    /// The OID of the gateway's device.
    pub device: String,
    /// The `urn:oid:` systems of the domains whose identifiers an answer
    /// carries.
    pub domains: Vec<String>,
    /// The `homeCommunityId`s, in dotted form, of the initiating
    /// communities the gateway answers.
    pub initiating: Vec<String>,
}

impl Xcpd {
    /// The `[xcpd]` table of FerroPIX's configuration.
    fn table(&self) -> String {
        let quoted = |values: &[String], prefix: &str| {
            values
                .iter()
                .map(|value| format!("\"{prefix}{value}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        format!(
            "\n[xcpd]\nenabled = true\nhome_community_id = \"urn:oid:{}\"\ndevice = \"{}\"\n\
             domains = [{}]\ninitiating_communities = [{}]\n",
            self.home_community,
            self.device,
            quoted(&self.domains, ""),
            quoted(&self.initiating, "urn:oid:")
        )
    }
}

/// Returns `client`'s client credentials token of `issuer`: the client as
/// `sub` and `client_id`, FerroPIX's audience, and the client's scope.
fn mint(issuer: &Issuer, client: Client) -> Result<String, IssuerError> {
    let mut claims = Claims::new(issuer.name(), AUDIENCE);
    client.id.clone_into(&mut claims.sub);
    client.id.clone_into(&mut claims.client_id);
    claims.scope = Some(client.scope.to_owned());
    claims.extensions = None;
    claims.exp = claims.iat.saturating_add(3600);
    issuer.mint(&claims)
}

/// Returns FerroPIX's configuration: the development profile, its database
/// at `database_host`, the harness issuer trusted for its machines'
/// tokens, an authoritative domain per system of `authoritative` and a
/// local one per system of `local`, each fed by [`SOURCE`], and the
/// Responding Gateway `xcpd` when one is given.
fn config(
    (database_host, issuer): (&str, &str),
    authoritative: &[String],
    local: &[String],
    xcpd: Option<&Xcpd>,
) -> String {
    let mut text = format!(
        "profile = \"development\"\n\n\
         public_base_url = \"http://ferropix.example.org\"\n\n\
         [server]\nlisten = \"0.0.0.0:{PORT}\"\n\n\
         [db]\nurl = \"postgresql://{DATABASE}:{DATABASE_PASSWORD}@{database_host}:{POSTGRES_PORT}/{DATABASE}\"\n\
         data_key_file = \"{DATA_KEY_TARGET}\"\n\n\
         [usage_report]\nenabled = false\n\n\
         [dev]\nloopback = false\n"
    );
    // NOTE: writing to a String cannot fail, so each result is dropped.
    let _written: std::fmt::Result = write!(
        text,
        "\n[auth.tls]\ntrust_roots_file = \"{ROOTS_TARGET}\"\n\n\
         [[auth.issuer]]\nissuer = \"{issuer}\"\nkind = \"persons\"\nprofile = \"rfc9068\"\n\
         audience = [\"{AUDIENCE}\"]\nclient_claims = [\"client_id\"]\n\n\
         [[trust.issuer]]\nissuer = \"{issuer}\"\nclients = [\"{}\"]\n",
        SOURCE.id
    );
    let kinds = authoritative
        .iter()
        .map(|system| (system, "authoritative"))
        .chain(local.iter().map(|system| (system, "local")));
    for (index, (system, kind)) in kinds.enumerate() {
        let _written: std::fmt::Result = write!(
            text,
            "\n[[domain]]\nsystem = \"{system}\"\nname = \"Harness domain {index}\"\n\
             kind = \"{kind}\"\nsource_client_id = \"{}\"\n",
            SOURCE.id
        );
    }
    if let Some(xcpd) = xcpd {
        text.push_str(&xcpd.table());
    }
    text
}

/// Starts FerroPIX with the patient namespaces `authoritative` and the
/// member `ehr_id` domains `local`, its XCPD Responding Gateway `xcpd`
/// switched on when one is given, and waits until it reports ready.
///
/// # Errors
///
/// Returns [`HarnessError::Container`] when Docker refuses a container,
/// [`HarnessError::NotReady`] when FerroPIX does not become ready in time,
/// and [`HarnessError::Issuer`] when the harness issuer cannot be made.
pub async fn start(
    authoritative: &[String],
    local: &[String],
    xcpd: Option<&Xcpd>,
) -> Result<FerroPix, HarnessError> {
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
    let documents = Server::start().await;
    let front = MutualTls::open(&documents.uri(), &[CONTAINER_HOST]).map_err(HarnessError::Tls)?;
    let issuer = Issuer::new(format!(
        "https://{CONTAINER_HOST}:{}{ISSUER_PATH}",
        front.port()
    ))
    .map_err(HarnessError::Issuer)?;
    serve_issuer(&documents, &issuer).await?;
    let roots = tempfile::tempdir().map_err(HarnessError::Write)?;
    std::fs::write(roots.path().join("roots.pem"), front.trust_roots())
        .map_err(HarnessError::Write)?;
    let image = FERROPIX.repository;
    let configuration = config((&host, issuer.name()), authoritative, local, xcpd);
    let server =
        retry::start(|| ferropix_request(network.clone(), &configuration, front.trust_roots()))
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
        issuer,
        _documents: documents,
        front,
        roots,
    })
}

/// Mounts on `documents` what the harness issuer `issuer` serves: its
/// metadata at both well-known places (RFC 8414 §3, OpenID Connect
/// Discovery 1.0 §4), naming its token endpoint and key set, the key set,
/// and the consumer's client credentials token at the token endpoint.
async fn serve_issuer(documents: &Server, issuer: &Issuer) -> Result<(), HarnessError> {
    let name = issuer.name();
    let metadata = serde_json::json!({
        "issuer": name,
        "token_endpoint": format!("{name}/token"),
        "jwks_uri": format!("{name}/jwks"),
        "grant_types_supported": ["client_credentials"],
        "token_endpoint_auth_methods_supported": ["client_secret_basic"],
    });
    for at in [
        format!("/.well-known/oauth-authorization-server{ISSUER_PATH}"),
        format!("{ISSUER_PATH}/.well-known/openid-configuration"),
        format!("{ISSUER_PATH}/.well-known/oauth-authorization-server"),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_json(metadata.clone()))
            .mount(documents)
            .await;
    }
    let jwks = issuer.jwks_json().map_err(HarnessError::Issuer)?;
    Mock::given(method("GET"))
        .and(path(format!("{ISSUER_PATH}/jwks")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(jwks, "application/json"))
        .mount(documents)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{ISSUER_PATH}/token")))
        .and(body_string_contains("grant_type=client_credentials"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": mint(issuer, CONSUMER).map_err(HarnessError::Issuer)?,
            "token_type": "Bearer",
            "expires_in": 3600,
        })))
        .mount(documents)
        .await;
    Ok(())
}

/// The FerroPIX container on `network` with `configuration` as its file,
/// `roots`, the CA of the harness issuer's TLS front, beside it, and the
/// Docker host gateway as [`CONTAINER_HOST`].
///
/// The usage report is switched off twice: `[usage_report] enabled = false`
/// in the file, and the environment key every family launcher carries, which
/// wins over the file and is what `scripts/checks/usage-report-off.sh` reads.
fn ferropix_request(
    network: String,
    configuration: &str,
    roots: &str,
) -> ContainerRequest<GenericImage> {
    FERROPIX
        .image()
        .with_exposed_port(PORT.tcp())
        .with_network(network)
        .with_copy_to(CONFIG_TARGET, configuration.as_bytes().to_vec())
        .with_copy_to(DATA_KEY_TARGET, DATA_KEY.as_bytes().to_vec())
        .with_copy_to(ROOTS_TARGET, roots.as_bytes().to_vec())
        .with_host(CONTAINER_HOST, Host::HostGateway)
        .with_env_var("FERROPIX_CONFIG", CONFIG_TARGET)
        .with_env_var("FERROPIX__USAGE_REPORT__ENABLED", "false")
        .with_startup_timeout(Duration::from_secs(120))
}

#[cfg(test)]
mod tests {
    /// An issuer name as the harness gives one.
    const ISSUER_NAME: &str = "https://host.docker.internal:8443/ferrofed";

    use super::{
        AUDIENCE, CONSUMER, CONSUMER_SECRET, Issuer, SOURCE, config, ferropix_request, mint,
    };

    #[test]
    fn ferropix_starts_with_its_usage_report_off() {
        let request = ferropix_request("net".to_owned(), "", "");
        let usage_report = request
            .env_vars()
            .find(|(name, _)| name == "FERROPIX__USAGE_REPORT__ENABLED")
            .map(|(_, value)| value.into_owned());
        assert_eq!(
            Some("false".to_owned()),
            usage_report,
            "a test FerroPIX must not send a usage report"
        );
    }

    #[test]
    fn the_configuration_names_every_domain_fed_by_the_source() {
        let text = config(
            ("db", ISSUER_NAME),
            &["urn:oid:2.999.1".to_owned()],
            &["urn:oid:2.999.2.1".to_owned()],
            None,
        );
        assert!(text.starts_with("profile = \"development\""));
        assert_eq!(2, text.matches("[[domain]]").count());
        assert_eq!(
            2,
            text.matches(&format!("source_client_id = \"{}\"", SOURCE.id))
                .count()
        );
        assert_eq!(1, text.matches("kind = \"authoritative\"").count());
        assert_eq!(1, text.matches("kind = \"local\"").count());
        assert!(
            !text.contains("[[auth.basic.users]]"),
            "no machine is a Basic user"
        );
        assert!(text.contains(&format!("clients = [\"{}\"]", SOURCE.id)));
        assert!(text.contains(&format!("issuer = \"{ISSUER_NAME}\"")));
        assert!(text.contains("[auth.tls]\ntrust_roots_file = "));
        assert_ne!(SOURCE.id, CONSUMER.id);
        assert!(text.contains("@db:5432/ferropix"));
        assert!(
            !text.contains(CONSUMER_SECRET),
            "FerroPIX holds no client secret"
        );
    }

    #[test]
    fn a_machine_token_names_its_client_and_ferropix() {
        use base64::Engine as _;

        /// The claims FerroPIX reads of a machine's token.
        #[derive(serde::Deserialize)]
        struct Read {
            iss: String,
            sub: String,
            aud: String,
            client_id: String,
            scope: String,
        }

        let issuer = Issuer::new(ISSUER_NAME).expect("the issuer is made");
        let token = mint(&issuer, SOURCE).expect("the token is signed");
        let payload = token.split('.').nth(1).expect("a JWT");
        let claims: Read = serde_json::from_slice(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .expect("base64url"),
        )
        .expect("JSON");
        assert_eq!(SOURCE.id, claims.sub);
        assert_eq!(SOURCE.id, claims.client_id);
        assert_eq!(AUDIENCE, claims.aud);
        assert_eq!(ISSUER_NAME, claims.iss);
        assert_eq!(SOURCE.scope, claims.scope);
    }
}
