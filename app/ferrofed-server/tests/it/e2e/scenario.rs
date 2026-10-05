// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the Connectathon scenarios of §16.3 share: a gateway over the two
//! FerroEHR nodes configured as a scenario needs it, a seed of the patient
//! at both, and the reads of an answer and of the node-side journals.
//!
//! Each track's scenarios drive the gateway over ITS-REST as a Connectathon
//! participant would, and judge what reached a node on the journal of the
//! capturing proxy in front of it (§16.3, §16.4).

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::conformance::client::{self, Gateway, GatewayError};
use ferrofed_server::conformance::fixture::{Fixture, Holding, Member, Spares, SyntheticPatient};
use ferrofed_server::state::AppState;
use ferrofed_testkit::containers::{self, ProxiedNode, TwoNodes};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, DemoComposition, PatientId};
use http::{HeaderMap, HeaderValue, Request, StatusCode, header};
use secrecy::SecretString;
use uuid::Uuid;

use crate::e2e::{EHR_A, EHR_B, PATIENT, dev_resolver, plan, registry_document};
use crate::support::settings;

/// How a scenario configures the gateway; [`Options::default`] is the
/// development cross-reference resolving the patient at both nodes, asking
/// every member, with budgets no FerroEHR answer comes near.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// The top-level keys and the resolver tables.
    pub(crate) resolver: String,
    /// `federation.node_selection`.
    pub(crate) node_selection: &'static str,
    /// `federation.per_node_timeout_ms`.
    pub(crate) per_node_ms: u64,
    /// `federation.overall_timeout_ms`.
    pub(crate) overall_ms: u64,
    /// Further `[federation]` keys, one per line.
    pub(crate) federation: String,
    /// Further keys of node B's registry endpoint entry, one per line.
    pub(crate) b_endpoint: String,
    /// Further tables after `[federation]`.
    pub(crate) tables: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            resolver: dev_resolver(),
            node_selection: "ask-all",
            per_node_ms: 20_000,
            overall_ms: 25_000,
            federation: String::new(),
            b_endpoint: String::new(),
            tables: String::new(),
        }
    }
}

/// Returns the development profile with `tables`, the `[dev]` rows of
/// [`dev_rows`].
pub(crate) fn development(tables: &[String]) -> String {
    format!("profile = \"development\"\n{}", tables.concat())
}

/// The `[dev]` rows mapping `patient` to each `(member, ehr_id)` of `rows`,
/// with the Step-1 consent pre-filter denying asking each member of `denied`
/// about `patient` (N27a).
pub(crate) fn dev_rows(patient: PatientId, rows: &[(&str, Uuid)], denied: &[&str]) -> String {
    let (namespace, value) = (patient.namespace(), patient.value());
    let crossref = rows.iter().map(|(member, ehr_id)| {
        format!(
            "\n[[dev.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        )
    });
    let consent = denied.iter().map(|member| {
        format!(
            "\n[[dev.consent_denied]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\n"
        )
    });
    crossref.chain(consent).collect::<Vec<_>>().concat()
}

/// The gateway over node A and node B as `options` configure it, its state
/// built as `ferrofed serve` builds it, mounted at `/`.
pub(crate) fn gateway_with(
    dir: &Path,
    nodes: &TwoNodes,
    options: &Options,
) -> Result<Router, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry_document(&nodes.a, &nodes.b, &options.b_endpoint),
    )?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "{}\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = {}\noverall_timeout_ms = {}\nnode_selection = \"{}\"\nid = \"example-federation\"\n{}\n\n{}\n",
        options.resolver,
        options.per_node_ms,
        options.overall_ms,
        options.node_selection,
        options.federation,
        options.tables
    );
    let resolved =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = AppState::build(&resolved)?;
    let mut server = settings();
    server.request_timeout = Duration::from_secs(60);
    server.body_limit = 256 * 1024;
    Ok(ferrofed_server::router(Arc::new(state), &server))
}

/// Seeds the patient's EHR at both nodes, one composition in each (the
/// hospital's at node A, the clinic's at node B), and clears both journals.
pub(crate) async fn seed_both(nodes: &TwoNodes) -> Result<(), Box<dyn Error>> {
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    clear(nodes);
    Ok(())
}

/// Forgets what both nodes received so far.
pub(crate) fn clear(nodes: &TwoNodes) {
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
}

/// The patient predicate over `EHR_STATUS.subject.external_ref`, for an
/// `EHR` bound to `e`.
pub(crate) fn patient_predicate() -> String {
    format!(
        "e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        PATIENT.value(),
        PATIENT.namespace()
    )
}

/// The plain patient query of a client: each composition's uid.
pub(crate) fn patient_compositions() -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    )
}

/// `POST /v1/query/aql` with `aql` and the header fields `fields`.
pub(crate) fn post_aql(
    aql: &str,
    fields: &[(&str, &str)],
) -> Result<Request<Body>, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Adhoc<'a> {
        q: &'a str,
    }
    let mut request =
        Request::post("/v1/query/aql").header(header::CONTENT_TYPE, "application/json");
    for (name, value) in fields {
        request = request.header(*name, *value);
    }
    Ok(request.body(Body::from(serde_json::to_string(&Adhoc { q: aql })?))?)
}

/// What the gateway answered, and how long it took.
#[derive(Debug)]
pub(crate) struct Reply {
    /// The status.
    pub(crate) status: StatusCode,
    /// The header fields.
    pub(crate) headers: HeaderMap,
    /// The body, as text.
    pub(crate) text: String,
    /// The time from sending the request to reading the whole answer.
    pub(crate) took: Duration,
}

impl Reply {
    /// Returns the text of header field `name`, when present.
    pub(crate) fn field(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    /// Returns the answer read as a federated `RESULT_SET`, after checking
    /// it against the vendored schema (§9, N17, CP-35).
    pub(crate) fn federated(&self) -> Result<client::Federated, Box<dyn Error>> {
        crate::facade::schema::validate(&self.text)?;
        Ok(serde_json::from_str(&self.text)?)
    }

    /// Returns the stable code of an error answer.
    pub(crate) fn code(&self) -> Result<String, Box<dyn Error>> {
        Ok(crate::support::error_body(&self.text)?.code)
    }
}

/// Sends `request` through `app` with the default caller's token and reads
/// the whole answer.
pub(crate) async fn exchange(
    app: &Router,
    request: Request<Body>,
) -> Result<Reply, Box<dyn Error>> {
    let started = Instant::now();
    let response = crate::support::send(app.clone(), request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await?;
    Ok(Reply {
        status,
        headers,
        text: String::from_utf8(bytes.to_vec())?,
        took: started.elapsed(),
    })
}

/// Returns the ITS-REST requests `node` received, as method and path.
pub(crate) fn asked(node: &ProxiedNode) -> Vec<(String, String)> {
    node.proxy
        .journal()
        .into_iter()
        .map(|capture| (capture.method, capture.path))
        .collect()
}

/// Returns the AQL queries `node` received.
pub(crate) fn queries(node: &ProxiedNode) -> Vec<Capture> {
    node.proxy
        .journal()
        .into_iter()
        .filter(|capture| capture.method == "POST" && capture.path.ends_with("/v1/query/aql"))
        .collect()
}

/// Returns the text of header field `name` in `capture`, when present.
pub(crate) fn captured_field<'c>(capture: &'c Capture, name: &str) -> Option<&'c str> {
    capture
        .headers
        .iter()
        .find(|(field, _)| field.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| std::str::from_utf8(value).ok())
}

/// Asserts that neither node received anything since the journals were last
/// cleared.
pub(crate) fn nobody_asked(nodes: &TwoNodes, why: &str) {
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&nodes.a),
        "node A: {why}"
    );
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&nodes.b),
        "node B: {why}"
    );
}

/// The gateway's router as the [`Gateway`] the conformance scenarios ask,
/// each request sent with a fresh default caller's token unless it names
/// its own.
pub(crate) struct InProcess(pub(crate) Router);

impl Gateway for InProcess {
    fn send(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<client::Reply, GatewayError>> + Send {
        let app = self.0.clone();
        async move {
            let mut request = request.map(Body::from);
            if !request.headers().contains_key(header::AUTHORIZATION) {
                let bearer = crate::support::bearer()
                    .map_err(|error| GatewayError::Transport(Box::new(error)))?;
                request.headers_mut().insert(
                    header::AUTHORIZATION,
                    HeaderValue::from_str(&bearer).map_err(GatewayError::Token)?,
                );
            }
            read(app, request).await
        }
    }

    fn send_anonymous(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<client::Reply, GatewayError>> + Send {
        let app = self.0.clone();
        async move { read(app, request.map(Body::from)).await }
    }
}

/// Sends `request` through `app` as it is and reads the whole answer.
async fn read(app: Router, request: Request<Body>) -> Result<client::Reply, GatewayError> {
    use tower::ServiceExt as _;
    let started = Instant::now();
    let response = match app.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .map_err(|error| GatewayError::Transport(Box::new(error)))?;
    Ok(client::Reply {
        status,
        headers,
        text: String::from_utf8(bytes.to_vec()).map_err(GatewayError::Text)?,
        took: started.elapsed(),
    })
}

/// A [`Gateway`] whose every federated answer and self-description is
/// validated against the vendored JSON Schemas (§9, §7a.2; CP-35).
pub(crate) struct Validated<G>(pub(crate) G);

impl<G: Gateway> Gateway for Validated<G> {
    fn send(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<client::Reply, GatewayError>> + Send {
        self.0.send(request)
    }

    fn send_anonymous(
        &self,
        request: Request<Vec<u8>>,
    ) -> impl Future<Output = Result<client::Reply, GatewayError>> + Send {
        self.0.send_anonymous(request)
    }

    fn check_result_set(&self, text: &str) -> Result<(), String> {
        crate::facade::schema::validate(text).map_err(|error| error.to_string())
    }

    fn check_options(&self, text: &str) -> Result<(), String> {
        crate::facade::schema::validate_options(text).map_err(|error| error.to_string())
    }
}

/// The router `app` as the schema-validated [`Gateway`] a scenario asks.
pub(crate) fn in_process(app: &Router) -> Validated<InProcess> {
    Validated(InProcess(app.clone()))
}

/// `patient` as the conformance run's synthetic patient.
pub(crate) fn synthetic(patient: PatientId) -> Result<SyntheticPatient, Box<dyn Error>> {
    Ok(SyntheticPatient::new(
        &patient.namespace(),
        SecretString::from(patient.value()),
    )?)
}

/// The fixture of the two harness nodes: node A holding `a` compositions of
/// [`PATIENT`] under [`EHR_A`], node B holding `b` under [`EHR_B`], `None`
/// where the patient does not resolve.
pub(crate) fn fixture(a: Option<usize>, b: Option<usize>) -> Result<Fixture, Box<dyn Error>> {
    let member =
        |endpoint: &str, system_id: &str, organisation: &str, held: Option<(Uuid, usize)>| Member {
            endpoint: endpoint.to_owned(),
            system_id: system_id.to_owned(),
            organisation: organisation.to_owned(),
            path: containers::API_PATH.to_owned(),
            active: true,
            holding: held.map(|(ehr_id, compositions)| Holding {
                ehr_id: ehr_id.to_string(),
                compositions,
            }),
        };
    Ok(Fixture {
        patient: synthetic(PATIENT)?,
        members: vec![
            member(
                "node-a-pub",
                containers::NODE_A_SYSTEM_ID,
                "org-a",
                a.map(|count| (EHR_A, count)),
            ),
            member(
                "node-b-pub",
                containers::NODE_B_SYSTEM_ID,
                "org-b",
                b.map(|count| (EHR_B, count)),
            ),
        ],
        spares: Spares::default(),
        shortfall: None,
    })
}
