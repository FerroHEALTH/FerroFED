// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! SanteMPI in the harness: a deployable PIX Manager behind the same gate.
//!
//! It answers ITI-83 `$ihe-pix` and takes the PMIR ITI-93 Mobile Patient
//! Identity Feed (PIXm 3.1.0 §2:3.83, PMIR 1.6.0 §2:3.93).
//!
//! [`santempi`] starts the pinned [`SANTEMPI`] image on the PostgreSQL
//! release line current when it was published, [`SANTEMPI_POSTGRES`], laid
//! out as SanteSuite's documented compose file lays it out, and provisions
//! it the way SanteSuite's own qualification tests do: one identity domain
//! for the patient namespace, open to every source; one protected `ehr_id`
//! domain per member, whose one authoritative source is that member's feed
//! application ([`source`]); and one application for the gateway, the PIX
//! Consumer ([`consumer`]). Each member's feed then registers the patient
//! with [`SanteMpi::feed`], carrying the patient's identifier and the
//! member's `ehr_id`, so the Manager holds every member's `ehr_id` as an
//! identifier in that member's domain.
//!
//! Every value is synthetic, inside the `urn:oid:2.999` example arc, and
//! every secret is a development value. SanteMPI is a PIX Manager here and
//! never the oracle: what it answers is evidence about SanteMPI. No
//! specification governs which products the harness runs: our own design.

use std::time::Duration;

use http::StatusCode;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use testcontainers::core::{CmdWaitFor, ExecCommand, IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use uuid::Uuid;

mod feed;

use super::{
    HarnessError, POSTGRES_PORT, SANTEMPI, SANTEMPI_POSTGRES, names, postgres_health_check,
};
use crate::seed::{EhrDomain, PatientId};
use feed::feed_message;

/// The port SanteMPI serves its REST interfaces on inside its container.
const PORT: u16 = 8080;

/// The path of the FHIR interface, the PIX Manager's FHIR base.
pub const FHIR_PATH: &str = "/fhir/";

/// The path of SanteMPI's OAuth 2.0 token endpoint.
const TOKEN_PATH: &str = "/auth/oauth2_token";

/// The database role, which also names the main database, as SanteSuite's
/// compose file names it.
const DATABASE_ROLE: &str = "santedb";

/// The role's development password.
const DATABASE_PASSWORD: &str = "santedb_example";

/// The database of the audit repository, as SanteSuite's compose file names
/// it.
const AUDIT_DATABASE: &str = "auditdb";

/// The features the container runs: SanteSuite's documented SanteMPI set,
/// without the HL7 version 2 listener the harness does not reach.
const FEATURES: &str = "LOG;DATA_POLICY;AUDIT_REPO;ADO;PUBSUB_ADO;RAMCACHE;SEC;SWAGGER;\
OPENID;FHIR;HDSI;AMI;BIS;MDM;MATCHING;IHE_PIXM;IHE_PDQM;IHE_PMIR";

/// How long [`santempi`] waits for the token endpoint to issue a token. The
/// image runs on Mono and installs its schema on first start.
const READINESS_BUDGET: Duration = Duration::from_secs(480);

/// How long the readiness poll sleeps between two probes.
const READINESS_INTERVAL: Duration = Duration::from_secs(2);

/// How many lines of the container's output a readiness failure carries.
const LOG_LINES: usize = 80;

/// How many characters of a refusing answer a provisioning error carries.
const BODY_CHARS: usize = 2048;

/// SanteMPI could not be started, did not become usable, or refused its
/// setup.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SanteMpiError {
    /// A container could not be started.
    #[error(transparent)]
    Harness(#[from] HarnessError),
    /// The token endpoint issued no token inside the budget; the last lines
    /// the container wrote are attached.
    #[error("{url} issued no token within {}s; the last output:\n{log}", budget.as_secs())]
    NotReady {
        /// The endpoint that was probed.
        url: String,
        /// How long the probe waited.
        budget: Duration,
        /// The last lines of the container's output.
        log: String,
    },
    /// A setup request could not be sent or its answer read.
    #[error("{step} could not be sent or read")]
    Exchange {
        /// The step of the setup.
        step: &'static str,
        /// What the HTTP stack reported.
        #[source]
        source: reqwest::Error,
    },
    /// A setup message could not be written, or an answer could not be read.
    #[error("{step} carried JSON that could not be written or read")]
    Json {
        /// The step of the setup.
        step: &'static str,
        /// What the JSON codec reported.
        #[source]
        source: serde_json::Error,
    },
    /// An identity domain the harness provisions is no `urn:oid`, which
    /// SanteMPI also records as an OID.
    #[error("the identity domain {system} is no urn:oid")]
    NotAnOid {
        /// The domain's system.
        system: String,
    },
    /// SanteMPI refused a setup request.
    #[error("{step} was refused with {status}: {body}")]
    Refused {
        /// The step of the setup.
        step: &'static str,
        /// The status SanteMPI answered.
        status: StatusCode,
        /// The start of the answer, which holds synthetic setup data only.
        body: String,
    },
}

/// An OAuth 2.0 client of SanteMPI: a security application's name and
/// secret, used with the client-credentials grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// The application's name, its `client_id`.
    pub name: String,
    /// The application's secret, a development value.
    pub secret: String,
}

/// Returns the debugging application a fresh SanteMPI installation carries
/// for development, which the harness provisions the installation as.
fn administrator() -> Client {
    Client {
        name: "fiddler".to_owned(),
        secret: "fiddler".to_owned(),
    }
}

/// The query that answers `1` once SanteMPI has seeded the debugging
/// application and installed the OAuth 2.0 flow policies.
const FLOWS_INSTALLED: &str = "SELECT (SELECT COUNT(*) FROM SEC_APP_TBL WHERE APP_PUB_ID = 'fiddler') \
     * (SELECT COUNT(*) FROM SEC_POL_TBL WHERE OID = '1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.1')";

/// The statements that grant the debugging application every OAuth 2.0
/// flow policy, in the form the installation's own initialization grants it every
/// other policy.
// NOTE: santedb/santedb-server@f70e765 000-OAuthPolicies.dataset adds the flow policies
// after santedb-init.sql (@011b7f0) granted `fiddler` the rest, so none is granted to it.
const GRANT_FLOWS: &str = "DELETE FROM SEC_APP_POL_ASSOC_TBL \
     WHERE APP_ID IN (SELECT APP_ID FROM SEC_APP_TBL WHERE APP_PUB_ID = 'fiddler') \
     AND POL_ID IN (SELECT POL_ID FROM SEC_POL_TBL WHERE OID LIKE '1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.%'); \
     INSERT INTO SEC_APP_POL_ASSOC_TBL (APP_ID, POL_ID, POL_ACT) SELECT APP_ID, POL_ID, 2 \
     FROM SEC_APP_TBL, SEC_POL_TBL WHERE APP_PUB_ID = 'fiddler' \
     AND OID LIKE '1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.%';";

/// The policies that admit an application to the client-credentials grant
/// with no device credential, which the Manager denies an application they
/// are not granted to.
// NOTE: santedb/santedb-restsvc@1a9fdb3 DefaultClientCredentialsTokenRequestHandler demands
// both; a policy granted in the AMI create is added to the application (ibid. @ecafe70).
const CLIENT_CREDENTIALS_POLICIES: [&str; 2] = [
    "1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.1",
    "1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.1.0",
];

/// Returns the application the gateway asks the Manager as, its PIX
/// Consumer.
#[must_use]
pub fn consumer() -> Client {
    Client {
        name: "FFD_PIX_CONSUMER".to_owned(),
        secret: "ffd-pix-consumer-example".to_owned(),
    }
}

/// Returns the feed application of the member whose `ehr_id`s are in
/// `domain`, the one source SanteMPI holds authoritative for that domain.
#[must_use]
pub fn source(domain: EhrDomain) -> Client {
    let key: String = domain
        .system()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Client {
        name: format!("FFD_SOURCE_{key}"),
        secret: format!("ffd-source-{key}-example"),
    }
}

/// A started SanteMPI with its database, torn down when it is dropped.
#[derive(Debug)]
pub struct SanteMpi {
    /// SanteMPI itself.
    server: ContainerAsync<GenericImage>,
    /// Its database server.
    database: ContainerAsync<GenericImage>,
    /// The origin SanteMPI is reachable at from the host, with no path.
    origin: String,
    /// The client the harness provisions and feeds with.
    http: reqwest::Client,
}

impl SanteMpi {
    /// Returns the FHIR base the gateway's `[[pixm.manager]] url` names.
    #[must_use]
    pub fn fhir_base(&self) -> String {
        format!("{}{FHIR_PATH}", self.origin)
    }

    /// Returns the SanteMPI container.
    #[must_use]
    pub fn container(&self) -> &ContainerAsync<GenericImage> {
        &self.server
    }

    /// Returns the database container SanteMPI was started against.
    #[must_use]
    pub fn database(&self) -> &ContainerAsync<GenericImage> {
        &self.database
    }

    /// Returns a bearer token SanteMPI issues to `client` by the
    /// client-credentials grant.
    ///
    /// # Errors
    ///
    /// Returns [`SanteMpiError::Exchange`] when the token endpoint cannot be
    /// reached and [`SanteMpiError::Refused`] when it refuses or answers
    /// with no token.
    pub async fn token(&self, client: &Client) -> Result<String, SanteMpiError> {
        let form = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("grant_type", "client_credentials")
            .append_pair("scope", "*")
            .append_pair("client_id", &client.name)
            .append_pair("client_secret", &client.secret)
            .finish();
        let request = self
            .http
            .post(format!("{}{TOKEN_PATH}", self.origin))
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form);
        let step = "the client-credentials grant";
        let body = send(request, step).await?;
        serde_json::from_slice::<TokenAnswer>(&body)
            .map(|answer| answer.access_token)
            .map_err(|source| SanteMpiError::Json { step, source })
    }

    /// Registers `patient` at the Manager as known to the member whose
    /// `ehr_id`s are in `domain`, under `ehr_id` there, by one ITI-93 Mobile
    /// Patient Identity Feed message from that member's feed application
    /// (PMIR 1.6.0 §2:3.93.4.1).
    ///
    /// # Errors
    ///
    /// Returns [`SanteMpiError::Exchange`] when SanteMPI cannot be reached and
    /// [`SanteMpiError::Refused`] when it refuses the token or the message.
    pub async fn feed(
        &self,
        domain: EhrDomain,
        patient: PatientId,
        ehr_id: Uuid,
    ) -> Result<(), SanteMpiError> {
        let step = "the ITI-93 feed";
        let bearer = self.token(&source(domain)).await?;
        let body = serde_json::to_vec(&feed_message(domain, patient, ehr_id))
            .map_err(|source| SanteMpiError::Json { step, source })?;
        let request = self
            .http
            .post(format!("{}{FHIR_PATH}Bundle", self.origin))
            .header(AUTHORIZATION, format!("Bearer {bearer}"))
            .header(CONTENT_TYPE, "application/fhir+json")
            .header(ACCEPT, "application/fhir+json")
            .body(body);
        send(request, step).await.map(drop)
    }

    /// Waits until SanteMPI has installed the OAuth 2.0 flow policies,
    /// grants them to the debugging application, then polls the token
    /// endpoint with it until it issues a token, or the budget runs out.
    ///
    /// The grant precedes the first token request, so no decision SanteMPI
    /// caches predates it.
    async fn await_ready(&self) -> Result<(), SanteMpiError> {
        let started = tokio::time::Instant::now();
        let mut granted = false;
        while started.elapsed() < READINESS_BUDGET {
            if !granted {
                let installed = self.database_says(FLOWS_INSTALLED).await;
                granted = installed.as_deref() == Some("1")
                    && self.database_says(GRANT_FLOWS).await.is_some();
            }
            if granted && self.token(&administrator()).await.is_ok() {
                return Ok(());
            }
            tokio::time::sleep(READINESS_INTERVAL).await;
        }
        Err(SanteMpiError::NotReady {
            url: format!("{}{TOKEN_PATH}", self.origin),
            budget: READINESS_BUDGET,
            log: self.log_tail().await,
        })
    }

    /// Runs `sql` with `psql` in the database container and returns what it
    /// printed, trimmed, or `None` when it did not run to a clean exit.
    async fn database_says(&self, sql: &str) -> Option<String> {
        let command = ExecCommand::new([
            "psql",
            "-v",
            "ON_ERROR_STOP=1",
            "-U",
            DATABASE_ROLE,
            "-d",
            DATABASE_ROLE,
            "-tAc",
            sql,
        ])
        .with_cmd_ready_condition(CmdWaitFor::exit_code(0));
        // NOTE: no specification governs this: our own design; a database that is
        // not ready yet is legitimately polled again, so a failed run is an answer.
        let mut run = self.database.exec(command).await.ok()?;
        let out = run.stdout_to_vec().await.ok()?;
        Some(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Returns the last [`LOG_LINES`] lines the container wrote, for a
    /// readiness failure to carry.
    async fn log_tail(&self) -> String {
        let (stdout, stderr) = (
            self.server.stdout_to_vec().await,
            self.server.stderr_to_vec().await,
        );
        let mut out = Vec::new();
        for stream in [stdout, stderr] {
            match stream {
                Ok(bytes) => out.extend(bytes),
                Err(_unreadable) => out.extend(b"(a container stream could not be read)\n"),
            }
        }
        let text = String::from_utf8_lossy(&out);
        let lines: Vec<&str> = text.lines().collect();
        lines
            .iter()
            .skip(lines.len().saturating_sub(LOG_LINES))
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Creates the applications, each admitted to the client-credentials
    /// grant, and the identity domains, as the debugging application.
    async fn provision(
        &self,
        patient_namespace: &str,
        members: &[EhrDomain],
    ) -> Result<(), SanteMpiError> {
        let admin = self.token(&administrator()).await?;
        let mut applications = vec![(record_id(0, 0), consumer())];
        let mut authorities = vec![authority(
            record_id(1, 0),
            "FFD_PATIENT",
            patient_namespace,
            None,
        )?];
        for (number, &domain) in (1_u16..).zip(members) {
            let application = record_id(0, number);
            applications.push((application, source(domain)));
            authorities.push(authority(
                record_id(1, number),
                &format!("FFD_EHR_{number}"),
                &domain.system(),
                Some(application),
            )?);
        }
        for (id, client) in &applications {
            let request = self
                .http
                .post(format!("{}/ami/SecurityApplication", self.origin))
                .header(AUTHORIZATION, format!("Bearer {admin}"))
                .header(CONTENT_TYPE, "application/xml")
                .body(application(*id, client));
            send(request, "the SecurityApplication create").await?;
        }
        let bundle = format!(
            "<Bundle xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
             xmlns=\"http://santedb.org/model\">{}</Bundle>",
            authorities.concat()
        );
        let request = self
            .http
            .post(format!("{}/hdsi/Bundle", self.origin))
            .header(AUTHORIZATION, format!("Bearer {admin}"))
            .header(CONTENT_TYPE, "application/xml")
            .body(bundle);
        send(request, "the AssigningAuthority create")
            .await
            .map(drop)
    }
}

/// The part of a token endpoint answer the harness reads.
#[derive(Debug, Deserialize)]
struct TokenAnswer {
    /// The bearer token.
    access_token: String,
}

/// Starts SanteMPI on a database server of its own and provisions it.
///
/// It waits for the token endpoint, then creates the identity domain
/// `patient_namespace`, one protected `ehr_id` domain per member in
/// `members` with its feed application ([`source`]), and the gateway's
/// application ([`consumer`]).
///
/// # Errors
///
/// Returns [`SanteMpiError::Harness`] when Docker refuses a container,
/// [`SanteMpiError::NotReady`] when SanteMPI does not issue a token in
/// time, and [`SanteMpiError::Exchange`] or [`SanteMpiError::Refused`] when
/// provisioning fails.
pub async fn santempi(
    patient_namespace: &str,
    members: &[EhrDomain],
) -> Result<SanteMpi, SanteMpiError> {
    let (network, host) = names("santempi");
    let database = SANTEMPI_POSTGRES
        .image()
        .with_exposed_port(POSTGRES_PORT.tcp())
        .with_wait_for(WaitFor::healthcheck())
        .with_health_check(postgres_health_check(DATABASE_ROLE, DATABASE_ROLE))
        .with_env_var("POSTGRES_USER", DATABASE_ROLE)
        .with_env_var("POSTGRES_PASSWORD", DATABASE_PASSWORD)
        .with_network(network.clone())
        .with_container_name(host.clone())
        .start()
        .await
        .map_err(|source| HarnessError::Container {
            image: SANTEMPI_POSTGRES.repository,
            source,
        })?;
    let connection = |name: &str| {
        format!(
            "server={host};port={POSTGRES_PORT}; database={name}; user id={DATABASE_ROLE}; \
             password={DATABASE_PASSWORD}; pooling=true; MinPoolSize=5; MaxPoolSize=15; \
             Timeout=60;"
        )
    };
    let image = SANTEMPI.repository;
    let server = SANTEMPI
        .image()
        .with_exposed_port(PORT.tcp())
        .with_network(network)
        .with_env_var("SDB_FEATURE", FEATURES)
        .with_env_var("SDB_MATCHING_MODE", "WEIGHTED")
        .with_env_var(
            "SDB_MDM_RESOURCE",
            "Patient=org.santedb.matching.patient.default",
        )
        .with_env_var("SDB_MDM_AUTO_MERGE", "false")
        .with_env_var("SDB_DB_MAIN", connection(DATABASE_ROLE))
        .with_env_var("SDB_DB_AUDIT", connection(AUDIT_DATABASE))
        .with_env_var("SDB_DB_MAIN_PROVIDER", "Npgsql")
        .with_env_var("SDB_DB_AUDIT_PROVIDER", "Npgsql")
        .with_env_var("SDB_DATA_POLICY_ACTION", "HIDE")
        // NOTE: santedb/santedb-restsvc@1a9fdb3 OauthDockerFeature: the OPENID feature's
        // INSECURE_CLIENT_AUTH admits a client-credentials grant with no device credential.
        .with_env_var("SDB_OPENID_INSECURE_CLIENT_AUTH", "true")
        .with_env_var("SDB_DELAY_START", "5000")
        .start()
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let host = server
        .get_host()
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let port = server
        .get_host_port_ipv4(PORT.tcp())
        .await
        .map_err(|source| HarnessError::Container { image, source })?;
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|source| SanteMpiError::Exchange {
            step: "the harness client",
            source,
        })?;
    let mpi = SanteMpi {
        server,
        database,
        origin: format!("http://{host}:{port}"),
        http,
    };
    mpi.await_ready().await?;
    mpi.provision(patient_namespace, members).await?;
    Ok(mpi)
}

/// Returns the fixed id of a provisioned record: kind `0` is a security
/// application, kind `1` an identity domain, and `number` is `0` for the
/// consumer or the patient namespace and the member's place for the rest.
fn record_id(kind: u16, number: u16) -> Uuid {
    Uuid::from_u128(
        0x0f0f_0000_0000_4000_8000_0000_0000_0000 | (u128::from(kind) << 32) | u128::from(number),
    )
}

/// Returns the AMI request that creates the security application `client`
/// as `id`.
fn application(id: Uuid, client: &Client) -> String {
    let policies = CLIENT_CREDENTIALS_POLICIES
        .map(|oid| format!("<policy oid=\"{oid}\" grant=\"Grant\" />"))
        .concat();
    format!(
        "<SecurityApplicationInfo xmlns=\"http://santedb.org/ami\"><entity>\
         <id xmlns=\"http://santedb.org/model\">{id}</id>\
         <applicationSecret xmlns=\"http://santedb.org/model\">{}</applicationSecret>\
         <name xmlns=\"http://santedb.org/model\">{}</name>\
         </entity><id>{id}</id>{policies}</SecurityApplicationInfo>",
        client.secret, client.name
    )
}

/// Returns the HDSI Bundle entry that creates the identity domain `system`
/// as `id`: unique per patient, protected by the application `assigner`
/// when one is named and open to every source when none is.
fn authority(
    id: Uuid,
    name: &str,
    system: &str,
    assigner: Option<Uuid>,
) -> Result<String, SanteMpiError> {
    let oid = system
        .strip_prefix("urn:oid:")
        .ok_or_else(|| SanteMpiError::NotAnOid {
            system: system.to_owned(),
        })?;
    let assigner = assigner
        .map(|assigner| format!("<assigningApplication>{assigner}</assigningApplication>"))
        .unwrap_or_default();
    Ok(format!(
        "<resource xsi:type=\"AssigningAuthority\"><id>{id}</id><name>{name}</name>\
         <domainName>{name}</domainName><oid>{oid}</oid><url>{system}</url>\
         <isUnique>true</isUnique>{assigner}</resource>"
    ))
}

/// Sends `request` and returns the body of a success answer.
async fn send(
    request: reqwest::RequestBuilder,
    step: &'static str,
) -> Result<Vec<u8>, SanteMpiError> {
    let answer = request
        .send()
        .await
        .map_err(|source| SanteMpiError::Exchange { step, source })?;
    let status = answer.status();
    let body = answer
        .bytes()
        .await
        .map_err(|source| SanteMpiError::Exchange { step, source })?;
    if status.is_success() {
        Ok(body.to_vec())
    } else {
        Err(SanteMpiError::Refused {
            step,
            status,
            body: String::from_utf8_lossy(&body)
                .chars()
                .take(BODY_CHARS)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CLIENT_CREDENTIALS_POLICIES, GRANT_FLOWS, application, authority, consumer, record_id,
        source,
    };
    use crate::seed::EhrDomain;

    #[test]
    fn the_debugging_application_alone_is_granted_the_flow_policies() {
        let flows = "OID LIKE '1.3.6.1.4.1.33349.3.1.5.9.2.1.0.0.%'";
        assert_eq!(2, GRANT_FLOWS.matches(flows).count(), "{GRANT_FLOWS}");
        assert_eq!(2, GRANT_FLOWS.matches("APP_PUB_ID = 'fiddler'").count());
        assert!(GRANT_FLOWS.contains("POL_ACT) SELECT APP_ID, POL_ID, 2"));
    }

    #[test]
    fn each_member_feeds_as_an_application_of_its_own() {
        let (a, b) = (source(EhrDomain::new(1)), source(EhrDomain::new(2)));
        assert_ne!(a.name, b.name);
        assert_eq!("FFD_SOURCE_urn_oid_2_999_2_1", a.name);
    }

    #[test]
    fn each_application_is_admitted_to_the_client_credentials_grant() {
        let xml = application(record_id(0, 0), &consumer());
        for oid in CLIENT_CREDENTIALS_POLICIES {
            assert!(
                xml.contains(&format!("<policy oid=\"{oid}\" grant=\"Grant\" />")),
                "{xml}"
            );
        }
    }

    #[test]
    fn the_record_ids_never_collide() {
        assert_ne!(record_id(0, 1), record_id(1, 1));
        assert_ne!(record_id(0, 0), record_id(0, 1));
    }

    #[test]
    fn a_domain_is_named_by_its_url_and_its_oid() {
        let xml = authority(record_id(1, 1), "FFD_EHR_1", "urn:oid:2.999.2.1", None)
            .expect("an identity domain");
        assert!(xml.contains("<oid>2.999.2.1</oid><url>urn:oid:2.999.2.1</url>"));
        assert!(
            !xml.contains("assigningApplication"),
            "open to every source"
        );
        assert!(authority(record_id(1, 2), "X", "https://example.org/x", None).is_err());
    }
}
