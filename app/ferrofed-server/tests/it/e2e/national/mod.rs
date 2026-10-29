// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national side end to end: the harness national connector of a
//! national contact point drives the FHIR face over two FerroEHR nodes
//! (Regulation (EU) 2025/327 Art 23, Annex II 2.1; Implementing Regulation
//! (EU) 2026/2099 Art 7).
//!
//! The connector confirms the patient's national identifier at the harness
//! PDQm Supplier and asks the gateway by it; the gateway resolves the patient
//! through the harness PIX Manager, asks the same Supplier for the summary
//! header, and reaches each member's own FHIR endpoint for prescriptions
//! ([`served`]). With one member restricting the patient, no answer shows a
//! trace of the restriction ([`restricted`]).
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the face's answers and the access records are read as JSON values"
)]

mod restricted;
mod served;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::eps;
use ferrofed_testkit::issuer::Issuer;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::national_connector::{CLAIMS, NationalConnector, NationalId};
use ferrofed_testkit::pdqm_stub::{self, Entry};
use ferrofed_testkit::proxy::CapturingProxy;
use tokio::net::TcpListener;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::e2e::{EHR_A, EHR_B, PATIENT, plan, registry_document};
use crate::feed_audit::audit_tables;

/// The issuer of the connector's tokens.
const NCP_ISSUER: &str = "https://ncp.example.test";

/// The audience the gateway under test is known by at that issuer.
const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// `{fhir-base}`.
const FHIR: &str = "/fhir";

/// The system each member's own FHIR endpoint names its `ehr_id` in.
const EHR_SYSTEM: &str = "urn:oid:2.999.4.1";

/// The synthetic name and birth date the stub PDQm Supplier answers the
/// patient under, the summary header.
const FAMILY: &str = "SENTINEL-FAMILY-3n8w";
const GIVEN: &str = "SENTINEL-GIVEN-6p2k";
const BIRTH_DATE: &str = "1970-01-01";

/// Node A's prescription, on its own FHIR endpoint.
const RX_A: &str = r#"{"resourceType":"MedicationRequest","id":"rx-a1","identifier":[{"system":"urn:oid:2.999.5.1","value":"SYNTHETIC-RX-A1"}],"status":"active","intent":"order","subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A"}}"#;

/// Node B's prescription, on its own FHIR endpoint.
const RX_B: &str = r#"{"resourceType":"MedicationRequest","id":"rx-b1","identifier":[{"system":"urn:oid:2.999.5.2","value":"SYNTHETIC-RX-B1"}],"status":"active","intent":"order","subject":{"reference":"Patient/p-b"},"medicationCodeableConcept":{"text":"Synthetic medicine B"}}"#;

/// The identifier value of node A's prescription.
const RX_A_IDENTIFIER: &str = "SYNTHETIC-RX-A1";

/// The member's own id of the dispensation node A creates.
const CREATED: &str = "md-1";

type TestResult = Result<(), Box<dyn Error>>;

/// The patient's national identifier, as the contact point confirmed it.
fn national(namespace: &str, value: &str) -> (String, String) {
    (namespace.to_owned(), value.to_owned())
}

/// The patient as the connector names it.
fn patient_id(id: &(String, String)) -> NationalId<'_> {
    NationalId {
        system: &id.0,
        value: &id.1,
    }
}

/// A dispensation of node A's prescription, named by its identifier, its
/// patient the test patient in a contained `Patient`.
fn dispensation() -> String {
    format!(
        r##"{{"resourceType":"MedicationDispense","meta":{{"profile":["{MPD_MEDICATION_DISPENSE}"]}},"extension":[{{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-10-24T10:00:00Z"}}],"contained":[{{"resourceType":"Patient","id":"pat","identifier":[{{"system":"{}","value":"{}"}}]}}],"status":"completed","medicationCodeableConcept":{{"text":"Synthetic medicine A"}},"identifier":[{{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-NATIONAL-1"}}],"subject":{{"reference":"#pat"}},"performer":[{{"actor":{{"display":"Synthetic pharmacist"}}}}],"authorizingPrescription":[{{"identifier":{{"system":"urn:oid:2.999.5.1","value":"{RX_A_IDENTIFIER}"}}}}],"quantity":{{"value":30,"unit":"tablet"}}}}"##,
        PATIENT.namespace(),
        PATIENT.value()
    )
}

/// A `searchset` of the prescriptions `resources`, each under a `fullUrl`
/// at `base`.
fn searchset(base: &str, resources: &[&str]) -> String {
    let entries: Vec<String> = resources
        .iter()
        .enumerate()
        .map(|(index, resource)| {
            format!(
                r#"{{"fullUrl":"{base}/fhir/MedicationRequest/entry-{index}","resource":{resource},"search":{{"mode":"match"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","total":{},"entry":[{}]}}"#,
        resources.len(),
        entries.join(",")
    )
}

/// A member's own FHIR endpoint holding `prescription`, its id `id`, holding
/// no dispensation of the patient yet, and creating a dispensation as
/// [`CREATED`].
async fn own_endpoint(prescription: &'static str, id: &str) -> Server {
    let server = Server::start().await;
    let json = |body: String| {
        ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/fhir+json")
    };
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationRequest"))
        .respond_with(json(searchset(&server.uri(), &[prescription])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fhir/MedicationRequest/{id}")))
        .respond_with(json(prescription.to_owned()))
        .mount(&server)
        .await;
    // The patient's dispensations the duplicate check reads: none yet.
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(json(searchset(&server.uri(), &[])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header(
                    "location",
                    format!(
                        "{}/fhir/MedicationDispense/{CREATED}/_history/1",
                        server.uri()
                    ),
                )
                .insert_header("etag", "W/\"1\""),
        )
        .mount(&server)
        .await;
    server
}

/// The `[[endpoint]]` of `node`'s own FHIR endpoint `id` at `base`, serving
/// prescriptions and dispensations.
fn own_entry(id: &str, (node, organisation): (&str, &str), base: &str) -> String {
    format!(
        "\n[[endpoint]]\nid = \"{id}\"\nnode = \"{node}\"\nurl = \"{base}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"{organisation}\"\n\
         categories = [\"Electronic-Prescriptions\", \"Electronic-Dispensations\"]\n\
         profiles = [\"{MPD_MEDICATION_REQUEST}\", \"{MPD_MEDICATION_DISPENSE}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n"
    )
}

/// The services every gateway of a test shares: the two seeded FerroEHR
/// nodes, each member's own FHIR endpoint behind a capturing proxy, the
/// stub PDQm Supplier answering with the patient, and the Audit Record
/// Repository the access records reach.
struct Harness {
    nodes: TwoNodes,
    pdq: Server,
    own_a: Server,
    own_b: Server,
    proxy_a: CapturingProxy,
    proxy_b: CapturingProxy,
    repository: FeedRepository,
}

impl Harness {
    /// Returns the FHIR base of the stub PDQm Supplier.
    pub(crate) fn supplier_url(&self) -> String {
        pdqm_stub::base_url(&self.pdq)
    }

    /// Starts the services, the patient's record seeded at both nodes.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let nodes = containers::two_nodes().await?;
        ferrofed_testkit::seed::seed(
            &nodes.a.api_root(),
            &plan(
                EHR_A,
                ferrofed_testkit::seed::DemoComposition::FirstHospital,
            ),
        )
        .await?;
        ferrofed_testkit::seed::seed(
            &nodes.b.api_root(),
            &plan(EHR_B, ferrofed_testkit::seed::DemoComposition::FirstClinic),
        )
        .await?;
        nodes.a.proxy.clear_journal();
        nodes.b.proxy.clear_journal();
        let pdq = pdqm_stub::supplier(&[Entry::new(&[(&PATIENT.namespace(), &PATIENT.value())])
            .named((FAMILY, GIVEN), Some(BIRTH_DATE))])
        .await;
        let own_a = own_endpoint(RX_A, "rx-a1").await;
        let own_b = own_endpoint(RX_B, "rx-b1").await;
        let proxy_a = CapturingProxy::start(own_a.uri()).await?;
        let proxy_b = CapturingProxy::start(own_b.uri()).await?;
        Ok(Self {
            nodes,
            pdq,
            own_a,
            own_b,
            proxy_a,
            proxy_b,
            repository: FeedRepository::start().await,
        })
    }

    /// Clears every journal the test reads.
    fn clear_journals(&self) {
        for proxy in [
            &self.nodes.a.proxy,
            &self.nodes.b.proxy,
            &self.proxy_a,
            &self.proxy_b,
        ] {
            proxy.clear_journal();
        }
    }

    /// Starts a gateway over the services on a loopback port, resolving the
    /// patient through `identity`, with the connector that calls it.
    ///
    /// `identity` is the configuration text of the identity binding and
    /// carries the top-level `profile` key.
    async fn gateway(&self, dir: &Path, identity: &str) -> Result<Gateway, Box<dyn Error>> {
        let connector_issuer = Issuer::new(NCP_ISSUER)?;
        let jwks = dir.join("ncp-jwks.json");
        connector_issuer.write_jwks(&jwks)?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}", listener.local_addr()?);

        let own = format!(
            "{}{}",
            own_entry("node-a-mpd", ("node-a", "org-a"), self.proxy_a.origin()),
            own_entry("node-b-mpd", ("node-b", "org-b"), self.proxy_b.origin())
        );
        let document = dir.join("registry.toml");
        std::fs::write(
            &document,
            registry_document(&self.nodes.a, &self.nodes.b, &own),
        )?;
        let path_of = |value: &Path| toml::Value::String(value.display().to_string());
        let [model, context] = eps::mapping_files();
        let namespace = PATIENT.namespace();
        let text = format!(
            "{identity}\n\n[gateway]\npublic_url = \"{origin}\"\n\n\
             [registry]\ndocument = {}\n\n[federation]\nid = \"example-federation\"\n\
             node_selection = \"ask-all\"\nper_node_timeout_ms = 20000\noverall_timeout_ms = 25000\n\n\
             [auth]\naudience = [\"{AUDIENCE}\"]\n\n[[auth.issuer]]\nissuer = \"{NCP_ISSUER}\"\n\
             jwks_file = {}\n\n[[trust.issuer]]\nissuer = \"{NCP_ISSUER}\"\n{}\n\
             [fhir]\nbase = \"{FHIR}\"\nreceive_namespaces = [\"{namespace}\"]\n\n\
             [fhir.operator]\nname = \"Synthetic Operator\"\nidentifier_system = \"urn:oid:2.999.9\"\n\
             identifier_value = \"operator-1\"\n\n[[fhir.mapping]]\nsection = \"allergies-and-intolerances\"\n\
             template = {}\nfiles = [{}, {}]\ncontext = \"{}\"\n\n\
             [[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\"node-a\", \"node-b\"]\n\n\
             [[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [\"node-a\", \"node-b\"]\n\n\
             [pdqm]\nurl = \"{}\"\ntransaction = \"iti-78\"\nmaster = \"{namespace}\"\ntimeout_ms = 5000\n\n\
             [pdqm.namespaces]\n\"urn:oid:2.999.7\" = \"urn:oid:2.999.7\"\n\n\
             [signing]\nkey_file = {}\njwks_uri = \"{origin}/.well-known/jwks.json\"\n{}",
            path_of(&document),
            path_of(&jwks),
            CLAIMS.toml(),
            path_of(&eps::opt()),
            path_of(&model),
            path_of(&context),
            eps::CONTEXT,
            pdqm_stub::base_url(&self.pdq),
            toml::Value::String(crate::support::signing_key_file().to_owned()),
            audit_tables(&self.repository, ""),
        );
        let settings =
            Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?
                .resolve()?;
        let state = Arc::new(AppState::build(&settings)?);
        state.lifecycle().booted();
        let app = ferrofed_server::router(Arc::clone(&state), &settings.server);
        let task = tokio::spawn(crate::support::serve_until(
            listener,
            app,
            Duration::ZERO,
            std::future::pending(),
        ));
        Ok(Gateway {
            connector: NationalConnector::new(
                connector_issuer,
                AUDIENCE,
                format!("{origin}{FHIR}"),
            )?,
            task,
        })
    }
}

/// A gateway serving on a loopback port, and the connector that calls it.
struct Gateway {
    connector: NationalConnector,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}
