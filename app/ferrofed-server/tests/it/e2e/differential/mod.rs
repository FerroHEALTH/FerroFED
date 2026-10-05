// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The differential run against the Federation Tier reference
//! implementation, behind the `FERROFED_E2E` gate.
//!
//! The same requests go to FerroFED and to the reference implementation at
//! its pinned commit, over the same two FerroEHR nodes, and the answers are
//! compared: the HTTP status, the federation headers, the envelope against
//! the vendored schema, `meta.federation`, the rows, and what reached each
//! node. The reference implementation is evidence, never an oracle (§16.3,
//! §16.4): each difference is adjudicated against the specification, and
//! the verdict is held in [`register`].
//!
//! Each gateway reaches each node through a capturing proxy of its own, so a
//! journal shows what that gateway dispatched, and a fault is set on both
//! proxies of a node at once. Both gateways are configured alike: the same
//! organisations, nodes, endpoint ids and `system_id`s, the same patients
//! resolved to the same `ehr_id`s, the same per-node timeout and overall
//! budget, best-effort offered, and every member asked.
//!
//! A run writes its report to `target/differential/`, or to the directory
//! `FERROFED_DIFFERENTIAL_DIR` names, one Markdown file and one TSV of
//! differences per test.
//!
//! No specification governs the harness or the report; they are FerroFED's
//! own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::containers::{self, NODE_A_SYSTEM_ID, NODE_B_SYSTEM_ID, TwoNodes};
use ferrofed_testkit::proxy::{Capture, CapturingProxy, Fault};
use ferrofed_testkit::reference::{self, Reference};
use ferrofed_testkit::seed::PatientId;
use http::{Method, Request};

use crate::e2e::scenario::{Options, Reply, dev_rows, development, exchange, gateway_with};
use crate::e2e::{EHR_A, EHR_B, PATIENT};

mod observe;
mod register;
mod report;
mod standard;
mod twin;

use observe::{Observed, Shape};

/// A patient the gateways resolve at node A alone.
pub(crate) const AT_A_ONLY: PatientId = PatientId::new(1, 39);

/// A patient neither gateway resolves anywhere.
pub(crate) const NOWHERE: PatientId = PatientId::new(1, 40);

/// The per-node timeout both gateways run with.
pub(crate) const PER_NODE: Duration = Duration::from_secs(4);

/// The overall budget both gateways run with.
const OVERALL: Duration = Duration::from_secs(6);

/// The consent refusal code node B's refusal carries, which FerroFED's
/// registry lists for node B.
pub(crate) const REFUSAL_CODE: &str = "consent-refused";

/// Node B's consent refusal, an ITS-REST `Error` carrying [`REFUSAL_CODE`].
pub(crate) const REFUSAL: &str =
    r#"{"message":"synthetic consent refusal","validationErrors":[],"code":"consent-refused"}"#;

/// One request, sent alike to both gateways.
#[derive(Debug, Clone)]
pub(crate) struct Call {
    /// The method.
    pub(crate) method: Method,
    /// The path and query under the gateway's base.
    pub(crate) target: String,
    /// The header fields, beside the caller's `Authorization`.
    pub(crate) fields: Vec<(&'static str, String)>,
    /// The JSON body, when the request has one.
    pub(crate) body: Option<String>,
}

impl Call {
    /// `POST /v1/query/aql` with `aql`.
    pub(crate) fn aql(aql: &str) -> Result<Self, Box<dyn Error>> {
        #[derive(serde::Serialize)]
        struct Adhoc<'a> {
            q: &'a str,
        }
        Ok(Self {
            method: Method::POST,
            target: "/v1/query/aql".to_owned(),
            fields: Vec::new(),
            body: Some(serde_json::to_string(&Adhoc { q: aql })?),
        })
    }

    /// A request with no body.
    pub(crate) fn bare(method: Method, target: impl Into<String>) -> Self {
        Self {
            method,
            target: target.into(),
            fields: Vec::new(),
            body: None,
        }
    }

    /// Adds the header field `name` with `value`.
    pub(crate) fn with(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.fields.push((name, value.into()));
        self
    }

    /// Replaces the body with the JSON text `body`.
    pub(crate) fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// Which node a step's fault is set on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Faulted {
    /// Node A.
    A,
    /// Node B.
    B,
}

/// One step of a differential scenario.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// The stable id the register keys a difference by.
    pub(crate) id: &'static str,
    /// The §16.3 track the step comes from.
    pub(crate) track: &'static str,
    /// What the step exercises, for the report.
    pub(crate) what: &'static str,
    /// The request.
    pub(crate) call: Call,
    /// How the answer is read.
    pub(crate) shape: Shape,
    /// The fault set on one node for the step, when any.
    pub(crate) fault: Option<(Faulted, Fault)>,
}

/// What one gateway answered to one step, read with what its nodes received.
#[derive(Debug)]
pub(crate) struct Side {
    /// The answer.
    pub(crate) reply: Reply,
    /// The aspects the comparison reads.
    pub(crate) observed: Observed,
}

/// Every difference of a run: by step id and aspect, FerroFED's value and
/// the reference implementation's, absent as `None`.
pub(crate) type Differences = BTreeMap<(String, String), (Option<String>, Option<String>)>;

/// One step's outcome on both gateways.
#[derive(Debug)]
pub(crate) struct Outcome {
    /// The step.
    pub(crate) step: Step,
    /// FerroFED's side.
    pub(crate) ferrofed: Side,
    /// The reference implementation's side.
    pub(crate) reference: Side,
}

impl Outcome {
    /// Returns every aspect whose value differs, with FerroFED's value and
    /// the reference implementation's, absent as `None`.
    pub(crate) fn differences(&self) -> Vec<(String, Option<String>, Option<String>)> {
        let ours = &self.ferrofed.observed.aspects;
        let theirs = &self.reference.observed.aspects;
        let mut keys: Vec<&String> = ours.keys().chain(theirs.keys()).collect();
        keys.sort();
        keys.dedup();
        keys.into_iter()
            .filter_map(|key| {
                let (left, right) = (ours.get(key), theirs.get(key));
                (left != right).then(|| (key.clone(), left.cloned(), right.cloned()))
            })
            .collect()
    }
}

/// The two nodes, the reference implementation in front of them, and
/// FerroFED configured alike.
#[derive(Debug)]
pub(crate) struct Bench {
    /// The nodes, each behind the proxy FerroFED reaches it through.
    pub(crate) nodes: TwoNodes,
    /// The proxy the reference implementation reaches node A through.
    reference_a: CapturingProxy,
    /// The proxy the reference implementation reaches node B through.
    reference_b: CapturingProxy,
    /// The reference implementation.
    reference: Reference,
    /// FerroFED.
    app: Router,
    /// The client the reference implementation is driven with.
    client: reqwest::Client,
    /// Holds FerroFED's registry document for the bench's lifetime.
    _dir: tempfile::TempDir,
}

impl Bench {
    /// Starts the reference implementation and FerroFED in front of `nodes`,
    /// which the caller has seeded.
    pub(crate) async fn start(nodes: TwoNodes) -> Result<Self, Box<dyn Error>> {
        let reference_a = CapturingProxy::start_reachable(nodes.a.node.origin()).await?;
        let reference_b = CapturingProxy::start_reachable(nodes.b.node.origin()).await?;
        let reference = Reference::start(
            &reference_registry(&reference_a, &reference_b)?,
            &reference_settings(),
        )
        .await?;
        let dir = tempfile::tempdir()?;
        let app = gateway_with(dir.path(), &nodes, &ferrofed_options()?)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            nodes,
            reference_a,
            reference_b,
            reference,
            app,
            client,
            _dir: dir,
        })
    }

    /// Sends `step` to FerroFED, then to the reference implementation, with
    /// the step's fault set on both proxies of its node.
    pub(crate) async fn run(&self, step: Step) -> Result<Outcome, Box<dyn Error>> {
        if let Some((node, fault)) = step.fault {
            for proxy in self.proxies(node) {
                proxy.set_fault(fault);
            }
        }
        let authorization = crate::support::bearer()?;
        let ferrofed = self.ferrofed(&step, &authorization).await;
        let reference = self.reference(&step, &authorization).await;
        for proxy in self
            .proxies(Faulted::A)
            .into_iter()
            .chain(self.proxies(Faulted::B))
        {
            proxy.clear_fault();
        }
        let (ferrofed, reference) = (ferrofed?, reference?);
        Ok(Outcome {
            step,
            ferrofed,
            reference,
        })
    }

    /// Returns the tail of the reference implementation's output.
    pub(crate) async fn reference_log(&self) -> String {
        self.reference.log().await
    }

    /// Returns the two proxies in front of `node`.
    fn proxies(&self, node: Faulted) -> [&CapturingProxy; 2] {
        match node {
            Faulted::A => [&self.nodes.a.proxy, &self.reference_a],
            Faulted::B => [&self.nodes.b.proxy, &self.reference_b],
        }
    }

    /// Sends `step` to FerroFED.
    async fn ferrofed(&self, step: &Step, authorization: &str) -> Result<Side, Box<dyn Error>> {
        let (a, b) = (&self.nodes.a.proxy, &self.nodes.b.proxy);
        a.clear_journal();
        b.clear_journal();
        let mut request = Request::builder()
            .method(step.call.method.clone())
            .uri(&step.call.target)
            .header(http::header::AUTHORIZATION, authorization);
        for (name, value) in &step.call.fields {
            request = request.header(*name, value);
        }
        let request = match &step.call.body {
            Some(body) => request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.clone()))?,
            None => request.body(Body::empty())?,
        };
        let reply = exchange(&self.app, request).await?;
        Ok(side(step, reply, &a.journal(), &b.journal(), authorization))
    }

    /// Sends `step` to the reference implementation.
    async fn reference(&self, step: &Step, authorization: &str) -> Result<Side, Box<dyn Error>> {
        let (a, b) = (&self.reference_a, &self.reference_b);
        a.clear_journal();
        b.clear_journal();
        let started = std::time::Instant::now();
        let mut request = self
            .client
            .request(
                step.call.method.clone(),
                format!("{}{}", self.reference.origin(), step.call.target),
            )
            .header(http::header::AUTHORIZATION, authorization);
        for (name, value) in &step.call.fields {
            request = request.header(*name, value);
        }
        if let Some(body) = &step.call.body {
            request = request
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(body.clone());
        }
        let response = request.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let text = response.text().await?;
        let reply = Reply {
            status,
            headers,
            text,
            took: started.elapsed(),
        };
        Ok(side(step, reply, &a.journal(), &b.journal(), authorization))
    }
}

/// Reads one gateway's answer and its nodes' journals into a [`Side`].
fn side(
    step: &Step,
    reply: Reply,
    node_a: &[Capture],
    node_b: &[Capture],
    authorization: &str,
) -> Side {
    let mut observed = observe::answer(&reply, step.shape);
    for (name, journal, own, other) in [
        ("node-a", node_a, EHR_A, EHR_B),
        ("node-b", node_b, EHR_B, EHR_A),
    ] {
        observe::node(&mut observed, name, journal, (own, other), authorization);
    }
    Side { reply, observed }
}

/// FerroFED's configuration: the development cross-reference resolving
/// [`PATIENT`] at both nodes and [`AT_A_ONLY`] at node A, best-effort
/// offered, node B's consent refusal code listed, and the budgets of
/// [`PER_NODE`] and [`OVERALL`].
fn ferrofed_options() -> Result<Options, Box<dyn Error>> {
    Ok(Options {
        resolver: development(&[
            dev_rows(PATIENT, &[("node-a", EHR_A), ("node-b", EHR_B)], &[]),
            dev_rows(AT_A_ONLY, &[("node-a", EHR_A)], &[]),
        ]),
        per_node_ms: crate::support::millis(PER_NODE)?,
        overall_ms: crate::support::millis(OVERALL)?,
        federation: "best_effort = true".to_owned(),
        b_endpoint: format!("consent_refusal_codes = [\"{REFUSAL_CODE}\"]\n"),
        ..Options::default()
    })
}

/// The reference implementation's registry document: the organisations,
/// nodes and endpoint ids FerroFED's registry names, each endpoint at the
/// proxy the reference implementation reaches its node through.
fn reference_registry(a: &CapturingProxy, b: &CapturingProxy) -> Result<String, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Document {
        organisations: [Organisation; 2],
        nodes: [Member; 2],
        endpoints: [Route; 2],
    }
    #[derive(serde::Serialize)]
    struct Organisation {
        id: &'static str,
        name: &'static str,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Member {
        node_id: &'static str,
        system_id: &'static str,
        organisation_id: &'static str,
        product: &'static str,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Route {
        endpoint_id: &'static str,
        node_id: &'static str,
        base_url: String,
        connection_type: &'static str,
        status: &'static str,
    }
    let endpoint = |endpoint_id, node_id, proxy: &CapturingProxy| Route {
        endpoint_id,
        node_id,
        base_url: format!(
            "{}{}",
            reference::container_origin(proxy.port()),
            containers::API_PATH
        ),
        connection_type: "openehr-rest-query",
        status: "active",
    };
    let member = |node_id, system_id, organisation_id| Member {
        node_id,
        system_id,
        organisation_id,
        product: "FerroEHR",
    };
    let document = Document {
        organisations: [
            Organisation {
                id: "org-a",
                name: "Organisation A",
            },
            Organisation {
                id: "org-b",
                name: "Organisation B",
            },
        ],
        nodes: [
            member("node-a", NODE_A_SYSTEM_ID, "org-a"),
            member("node-b", NODE_B_SYSTEM_ID, "org-b"),
        ],
        endpoints: [
            endpoint("node-a-pub", "node-a", a),
            endpoint("node-b-pub", "node-b", b),
        ],
    };
    Ok(serde_json::to_string_pretty(&document)?)
}

/// The reference implementation's configuration, alike to
/// [`ferrofed_options`].
fn reference_settings() -> String {
    let seconds = |duration: Duration| duration.as_secs();
    format!(
        "federation:\n  federation:\n    id: example-federation\n  timeouts:\n    per-node: {}s\n    overall-budget: {}s\n  completeness:\n    offer-best-effort: true\n  identity:\n    mode: static\n    default-namespace: \"{}\"\n    static-mappings:\n      \"{}\":\n        node-a-pub: \"{EHR_A}\"\n        node-b-pub: \"{EHR_B}\"\n      \"{}\":\n        node-a-pub: \"{EHR_A}\"\n",
        seconds(PER_NODE),
        seconds(OVERALL),
        PATIENT.namespace(),
        PATIENT.value(),
        AT_A_ONLY.value(),
    )
}

/// Returns the directory the report is written to.
fn report_dir() -> PathBuf {
    std::env::var_os("FERROFED_DIFFERENTIAL_DIR").map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/differential"
            ))
        },
        PathBuf::from,
    )
}

/// Runs `steps` on `bench` in order, writes the report named `name`, and
/// checks every difference against the register.
pub(crate) async fn differ(
    bench: &Bench,
    name: &str,
    steps: Vec<Step>,
) -> Result<(), Box<dyn Error>> {
    let mut outcomes = Vec::with_capacity(steps.len());
    for step in steps {
        let id = step.id;
        match bench.run(step).await {
            Ok(outcome) => outcomes.push(outcome),
            Err(error) => {
                return Err(format!(
                    "step {id} could not be run: {error}\nthe reference implementation's output ends:\n{}",
                    bench.reference_log().await
                )
                .into());
            }
        }
    }
    let found: Differences = outcomes
        .iter()
        .flat_map(|outcome| {
            outcome
                .differences()
                .into_iter()
                .map(|(aspect, ours, theirs)| {
                    ((outcome.step.id.to_owned(), aspect), (ours, theirs))
                })
        })
        .collect();
    report::write(&report_dir(), name, &outcomes)?;
    register::check(name, &outcomes, &found)
}
